#![forbid(unsafe_code)]

use config::{Config, MemoryPolicy, SortStrategy};
use orchestrator::clock::SystemClock;
use orchestrator::domain::{MapSegment, MemStat};
use orchestrator::observation::{
    DefaultAdmissionPolicy, DefaultModelUpdater, Observation, ObservationEvent, Scanner,
};
use orchestrator::persistence::{NoopRepository, SqliteRepository};
use orchestrator::prediction::{Prediction, Predictor};
use orchestrator::prefetch::{
    GreedyPrefetchPlanner, NoopPrefetcher, PrefetchPlan, PrefetchReport, Prefetcher,
};
use orchestrator::{PreloadEngine, Services};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

#[derive(Debug)]
struct StaticScanner {
    observation: Observation,
}

impl Scanner for StaticScanner {
    fn scan(
        &mut self,
        _time: u64,
        _scan_id: u64,
    ) -> Result<Observation, orchestrator::error::Error> {
        Ok(self.observation.clone())
    }
}

#[derive(Debug)]
struct PathScorePredictor {
    scores: Vec<(PathBuf, f32)>,
}

impl Predictor for PathScorePredictor {
    fn predict(&self, stores: &orchestrator::stores::Stores) -> Prediction {
        let mut prediction = Prediction::default();
        for (map_id, map) in stores.maps.iter() {
            if let Some(score) = self
                .scores
                .iter()
                .find(|(path, _)| path == &map.path)
                .map(|(_, score)| *score)
            {
                prediction.map_scores.insert(map_id, score);
            }
        }
        prediction
    }
}

#[derive(Debug, Default)]
struct SpyPrefetcher {
    plans: Arc<Mutex<Vec<PrefetchPlan>>>,
}

impl SpyPrefetcher {
    fn take_inner(plans: &Arc<Mutex<Vec<PrefetchPlan>>>) -> Vec<PrefetchPlan> {
        let mut guard = plans.lock().unwrap();
        std::mem::take(&mut *guard)
    }
}

#[async_trait::async_trait]
impl Prefetcher for SpyPrefetcher {
    async fn execute(
        &self,
        plan: &PrefetchPlan,
        _stores: &orchestrator::stores::Stores,
    ) -> PrefetchReport {
        self.plans.lock().unwrap().push(plan.clone());
        PrefetchReport {
            num_maps: plan.maps.len(),
            total_bytes: plan.total_bytes,
            failures: Vec::new(),
        }
    }
}

#[tokio::test]
async fn engine_tick_flows_through_pipeline() {
    let exe_path = PathBuf::from("/test/exe");
    let map_a = PathBuf::from("/test/map-a");
    let map_b = PathBuf::from("/test/map-b");

    let observation = vec![
        ObservationEvent::ObsBegin {
            time: 0,
            scan_id: 1,
        },
        ObservationEvent::ExeSeen {
            path: exe_path.clone(),
            pid: 1234,
        },
        ObservationEvent::MapSeen {
            exe_path: exe_path.clone(),
            map: MapSegment::new(map_a.clone(), 0, 2048, 0),
        },
        ObservationEvent::MapSeen {
            exe_path: exe_path.clone(),
            map: MapSegment::new(map_b.clone(), 0, 1024, 0),
        },
        ObservationEvent::MemStat {
            mem: MemStat {
                total: 0,
                free: 64,
                cached: 0,
                pagein: 0,
                pageout: 0,
            },
        },
        ObservationEvent::ObsEnd {
            time: 0,
            scan_id: 1,
            warnings: Vec::new(),
        },
    ];

    let mut config = Config::default();
    config.model.minsize = 1;
    config.model.memory = MemoryPolicy {
        memtotal: 0,
        memfree: 100,
        memcached: 0,
    };
    config.system.exeprefix = vec!["!/".into(), "/test/".into()];
    config.system.mapprefix = vec!["!/".into(), "/test/".into()];
    config.system.sortstrategy = SortStrategy::None;

    let spy = SpyPrefetcher::default();
    let spy_handle = spy.plans.clone();

    let services = Services {
        scanner: Box::new(StaticScanner { observation }),
        admission: Box::new(DefaultAdmissionPolicy::new(&config)),
        updater: Box::new(DefaultModelUpdater::new(&config)),
        predictor: Box::new(PathScorePredictor {
            scores: vec![(map_a.clone(), 0.9), (map_b.clone(), 0.1)],
        }),
        planner: Box::new(GreedyPrefetchPlanner::new(&config)),
        prefetcher: Box::new(spy),
        repo: Box::new(NoopRepository),
        clock: Box::new(SystemClock),
    };

    let mut engine = PreloadEngine::new(config, services).await.unwrap();
    let _ = engine.tick().await.unwrap();

    let plans = SpyPrefetcher::take_inner(&spy_handle);
    assert_eq!(plans.len(), 1);
    let plan = &plans[0];
    assert_eq!(plan.maps.len(), 2);
    assert_eq!(plan.total_bytes, 2048 + 1024);

    let stores = engine.stores();
    let map_ids = plan.maps.clone();
    let first_path = stores.maps.get(map_ids[0]).unwrap().path.clone();
    let second_path = stores.maps.get(map_ids[1]).unwrap().path.clone();
    assert_eq!(first_path, map_a);
    assert_eq!(second_path, map_b);
}

#[tokio::test]
async fn engine_persists_and_loads_state() {
    let exe_path = PathBuf::from("/test/exe");
    let map_a = PathBuf::from("/test/map-a");
    let map_b = PathBuf::from("/test/map-b");

    let observation = vec![
        ObservationEvent::ObsBegin {
            time: 10,
            scan_id: 1,
        },
        ObservationEvent::ExeSeen {
            path: exe_path.clone(),
            pid: 1234,
        },
        ObservationEvent::MapSeen {
            exe_path: exe_path.clone(),
            map: MapSegment::new(map_a.clone(), 0, 2048, 10),
        },
        ObservationEvent::MapSeen {
            exe_path: exe_path.clone(),
            map: MapSegment::new(map_b.clone(), 0, 1024, 10),
        },
        ObservationEvent::ObsEnd {
            time: 10,
            scan_id: 1,
            warnings: Vec::new(),
        },
    ];

    let mut config = Config::default();
    config.model.minsize = 1;
    config.system.exeprefix = vec!["!/".into(), "/test/".into()];
    config.system.mapprefix = vec!["!/".into(), "/test/".into()];
    config.system.dopredict = false;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let repo = SqliteRepository::new(db_path.clone()).await.unwrap();

    let services = Services {
        scanner: Box::new(StaticScanner { observation }),
        admission: Box::new(DefaultAdmissionPolicy::new(&config)),
        updater: Box::new(DefaultModelUpdater::new(&config)),
        predictor: Box::new(PathScorePredictor { scores: Vec::new() }),
        planner: Box::new(GreedyPrefetchPlanner::new(&config)),
        prefetcher: Box::new(NoopPrefetcher),
        repo: Box::new(repo),
        clock: Box::new(SystemClock),
    };

    let mut engine = PreloadEngine::new(config.clone(), services).await.unwrap();
    let _ = engine.tick().await.unwrap();
    engine.save().await.unwrap();

    let repo = SqliteRepository::new(db_path).await.unwrap();
    let services = Services {
        scanner: Box::new(StaticScanner {
            observation: Vec::new(),
        }),
        admission: Box::new(DefaultAdmissionPolicy::new(&config)),
        updater: Box::new(DefaultModelUpdater::new(&config)),
        predictor: Box::new(PathScorePredictor { scores: Vec::new() }),
        planner: Box::new(GreedyPrefetchPlanner::new(&config)),
        prefetcher: Box::new(NoopPrefetcher),
        repo: Box::new(repo),
        clock: Box::new(SystemClock),
    };

    let engine = PreloadEngine::load(config, services).await.unwrap();
    let stores = engine.stores();

    let exe_id = stores
        .exes
        .iter()
        .find(|(_, exe)| exe.key.path() == &exe_path)
        .map(|(id, _)| id)
        .expect("exe missing after reload");

    let map_paths: std::collections::HashSet<_> = stores
        .exe_maps
        .maps_for_exe(exe_id)
        .filter_map(|map_id| stores.maps.get(map_id))
        .map(|map| map.path.clone())
        .collect();

    let expected: std::collections::HashSet<_> = [map_a, map_b].into_iter().collect();
    assert_eq!(map_paths, expected);
}
