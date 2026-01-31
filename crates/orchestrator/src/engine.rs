#![forbid(unsafe_code)]

use crate::clock::Clock;
use crate::domain::{ExeKey, MapSegment, MarkovState, MemStat};
use crate::error::Error;
use crate::observation::{AdmissionPolicy, ModelDelta, ModelUpdater, ObservationEvent, Scanner};
use crate::persistence::{
    ExeMapRecord, ExeRecord, MapRecord, MarkovRecord, SNAPSHOT_SCHEMA_VERSION, SnapshotMeta,
    StateRepository, StateSnapshot, StoresSnapshot,
};
use crate::prediction::{Prediction, Predictor};
use crate::prefetch::{PrefetchPlanner, PrefetchReport, Prefetcher};
use crate::stores::Stores;
use config::Config;
use std::time::{Instant, SystemTime};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub struct Services {
    pub scanner: Box<dyn Scanner + Send + Sync>,
    pub admission: Box<dyn AdmissionPolicy + Send + Sync>,
    pub updater: Box<dyn ModelUpdater + Send + Sync>,
    pub predictor: Box<dyn Predictor + Send + Sync>,
    pub planner: Box<dyn PrefetchPlanner + Send + Sync>,
    pub prefetcher: Box<dyn Prefetcher + Send + Sync>,
    pub repo: Box<dyn StateRepository + Send + Sync>,
    pub clock: Box<dyn Clock + Send + Sync>,
}

pub struct ReloadBundle {
    pub config: Config,
    pub admission: Box<dyn AdmissionPolicy + Send + Sync>,
    pub updater: Box<dyn ModelUpdater + Send + Sync>,
    pub predictor: Box<dyn Predictor + Send + Sync>,
    pub planner: Box<dyn PrefetchPlanner + Send + Sync>,
    pub prefetcher: Box<dyn Prefetcher + Send + Sync>,
}

pub enum ControlEvent {
    Reload(Box<ReloadBundle>),
    DumpStatus,
    SaveNow,
}

#[derive(Debug, Clone)]
pub struct TickReport {
    pub scan_id: u64,
    pub model_delta: ModelDelta,
    pub prediction: crate::prediction::PredictionSummary,
    pub prefetch: PrefetchReport,
    pub memstat: Option<MemStat>,
}

pub struct PreloadEngine {
    config: Config,
    services: Services,
    stores: Stores,
    scan_id: u64,
    last_save: Instant,
}

impl PreloadEngine {
    /// Create a new engine with empty state. No persistence is read.
    pub async fn new(config: Config, services: Services) -> Result<Self, Error> {
        Ok(Self {
            config,
            services,
            stores: Stores::default(),
            scan_id: 0,
            last_save: Instant::now(),
        })
    }

    /// Load state from the configured repository and build the engine.
    pub async fn load(config: Config, services: Services) -> Result<Self, Error> {
        let snapshot = services.repo.load().await?;
        let stores = Self::stores_from_snapshot(snapshot, config.model.active_window.as_secs())?;
        Ok(Self {
            config,
            services,
            stores,
            scan_id: 0,
            last_save: Instant::now(),
        })
    }

    /// Execute a single scan/update/predict/prefetch cycle without sleeping.
    pub async fn tick(&mut self) -> Result<TickReport, Error> {
        self.scan_id = self.scan_id.saturating_add(1);
        let now = self.stores.model_time;

        let observation = if self.config.system.doscan {
            self.services.scanner.scan(now, self.scan_id)?
        } else {
            vec![
                ObservationEvent::ObsBegin {
                    time: now,
                    scan_id: self.scan_id,
                },
                ObservationEvent::ObsEnd {
                    time: now,
                    scan_id: self.scan_id,
                    warnings: Vec::new(),
                },
            ]
        };

        let memstat = observation.iter().find_map(|event| match event {
            ObservationEvent::MemStat { mem } => Some(*mem),
            _ => None,
        });

        let model_delta = if self.config.system.doscan {
            self.services.updater.apply(
                &mut self.stores,
                &observation,
                self.services.admission.as_ref(),
            )?
        } else {
            ModelDelta::default()
        };

        let prediction = if self.config.system.dopredict {
            self.services.predictor.predict(&self.stores)
        } else {
            Prediction::default()
        };

        let plan = if self.config.system.dopredict {
            if let Some(mem) = memstat {
                self.services.planner.plan(&prediction, &self.stores, &mem)
            } else {
                crate::prefetch::PrefetchPlan {
                    maps: Vec::new(),
                    total_bytes: 0,
                    budget_bytes: 0,
                }
            }
        } else {
            crate::prefetch::PrefetchPlan {
                maps: Vec::new(),
                total_bytes: 0,
                budget_bytes: 0,
            }
        };

        let prefetch = self.services.prefetcher.execute(&plan, &self.stores).await;

        // Advance model time by one cycle.
        self.stores.model_time = self
            .stores
            .model_time
            .saturating_add(self.config.model.cycle.as_secs());

        Ok(TickReport {
            scan_id: self.scan_id,
            model_delta,
            prediction: prediction.summarize(),
            prefetch,
            memstat,
        })
    }

    /// Run ticks until the cancellation token is triggered. Handles autosave.
    pub async fn run_until(
        &mut self,
        cancel: CancellationToken,
        mut control_rx: mpsc::UnboundedReceiver<ControlEvent>,
    ) -> Result<(), Error> {
        loop {
            let tick_start = self.services.clock.now();
            let mut did_tick = false;
            tokio::select! {
                _ = cancel.cancelled() => {
                    if self.config.persistence.save_on_shutdown {
                        let _ = self.save().await;
                    }
                    info!("shutdown requested");
                    break;
                }
                Some(event) = control_rx.recv() => {
                    self.handle_control(event).await?;
                }
                result = self.tick() => {
                    result?;
                    did_tick = true;
                }
            }

            let autosave = self
                .config
                .persistence
                .autosave_interval
                .unwrap_or(self.config.system.autosave);

            if autosave.as_secs() > 0 {
                let elapsed = self.last_save.elapsed();
                if elapsed >= autosave {
                    self.save().await?;
                    self.last_save = Instant::now();
                }
            }

            if did_tick {
                let elapsed = tick_start.elapsed();
                if elapsed < self.config.model.cycle {
                    let sleep_for = self.config.model.cycle - elapsed;
                    self.services.clock.sleep(sleep_for).await;
                }
            }
        }

        Ok(())
    }

    /// Persist current state via the configured repository.
    pub async fn save(&self) -> Result<(), Error> {
        let snapshot = Self::snapshot_from_stores(&self.stores);
        self.services.repo.save(&snapshot).await
    }

    /// Read-only access to in-memory stores (useful for tests).
    pub fn stores(&self) -> &Stores {
        &self.stores
    }

    async fn handle_control(&mut self, event: ControlEvent) -> Result<(), Error> {
        match event {
            ControlEvent::Reload(bundle) => {
                self.apply_reload(*bundle);
                info!("config reloaded");
            }
            ControlEvent::DumpStatus => {
                self.dump_status();
            }
            ControlEvent::SaveNow => {
                self.save().await?;
                self.last_save = Instant::now();
                info!("state saved");
            }
        }
        Ok(())
    }

    fn apply_reload(&mut self, mut bundle: ReloadBundle) {
        if bundle.config.persistence.state_path != self.config.persistence.state_path {
            warn!(
                current = ?self.config.persistence.state_path,
                requested = ?bundle.config.persistence.state_path,
                "ignoring state_path change during reload"
            );
            bundle.config.persistence.state_path = self.config.persistence.state_path.clone();
        }

        self.config = bundle.config;
        self.services.admission = bundle.admission;
        self.services.updater = bundle.updater;
        self.services.predictor = bundle.predictor;
        self.services.planner = bundle.planner;
        self.services.prefetcher = bundle.prefetcher;
    }

    fn dump_status(&self) {
        let exe_count = self.stores.exes.iter().count();
        let map_count = self.stores.maps.iter().count();
        let edge_count = self.stores.markov.iter().count();
        let active_count = self.stores.active.exes().len();

        info!(?self.config, "current config");
        info!(
            exe_count,
            map_count,
            edge_count,
            active_count,
            model_time = self.stores.model_time,
            "state summary"
        );
        if let Some(stats) = self.services.admission.stats() {
            info!(?stats, "admission policy stats");
        }
    }

    fn snapshot_from_stores(stores: &Stores) -> StoresSnapshot {
        let mut exes = Vec::new();
        for (_, exe) in stores.exes.iter() {
            exes.push(ExeRecord {
                path: exe.key.path().clone(),
                total_running_time: exe.total_running_time,
                last_seen_time: exe.last_seen_time,
            });
        }

        let mut maps = Vec::new();
        for (_, map) in stores.maps.iter() {
            maps.push(MapRecord {
                path: map.path.clone(),
                offset: map.offset,
                length: map.length,
                update_time: map.update_time,
            });
        }

        let mut exe_maps = Vec::new();
        for (exe_id, exe) in stores.exes.iter() {
            for map_id in stores.exe_maps.maps_for_exe(exe_id) {
                if let Some(map) = stores.maps.get(map_id) {
                    exe_maps.push(ExeMapRecord {
                        exe_path: exe.key.path().clone(),
                        map_key: map.key(),
                        prob: 1.0,
                    });
                }
            }
        }

        let mut markov_edges = Vec::new();
        for (key, edge) in stores.markov.iter() {
            let Some(exe_a) = stores.exes.get(key.a()) else {
                continue;
            };
            let Some(exe_b) = stores.exes.get(key.b()) else {
                continue;
            };
            markov_edges.push(MarkovRecord {
                exe_a: exe_a.key.path().clone(),
                exe_b: exe_b.key.path().clone(),
                time_to_leave: edge.time_to_leave,
                transition_prob: edge.transition_prob,
                both_running_time: edge.both_running_time,
            });
        }

        StoresSnapshot {
            meta: SnapshotMeta {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                app_version: None,
                created_at: Some(SystemTime::now()),
            },
            state: StateSnapshot {
                model_time: stores.model_time,
                last_accounting_time: stores.last_accounting_time,
                exes,
                maps,
                exe_maps,
                markov_edges,
            },
        }
    }

    fn stores_from_snapshot(snapshot: StoresSnapshot, active_window: u64) -> Result<Stores, Error> {
        let mut stores = Stores {
            model_time: snapshot.state.model_time,
            last_accounting_time: snapshot.state.last_accounting_time,
            ..Default::default()
        };

        for map in snapshot.state.maps {
            let segment = MapSegment::new(map.path, map.offset, map.length, map.update_time);
            stores.ensure_map(segment);
        }

        for exe in snapshot.state.exes {
            let exe_key = ExeKey::new(exe.path);
            let exe_id = stores.ensure_exe(exe_key);
            if let Some(exe_mut) = stores.exes.get_mut(exe_id) {
                exe_mut.total_running_time = exe.total_running_time;
                exe_mut.last_seen_time = exe.last_seen_time;
            }
        }

        // Rebuild active set based on last_seen_time and window.
        for (exe_id, exe) in stores.exes.iter() {
            if let Some(last_seen) = exe.last_seen_time
                && stores.model_time.saturating_sub(last_seen) <= active_window
            {
                stores.active.update([exe_id], stores.model_time);
            }
        }

        for record in snapshot.state.exe_maps {
            let exe_key = ExeKey::new(record.exe_path);
            let map_key = record.map_key;
            let exe_id = stores
                .exes
                .id_by_key(&exe_key)
                .ok_or_else(|| Error::ExeMissing(exe_key.path().clone()))?;
            let map_id = stores
                .maps
                .id_by_key(&map_key)
                .ok_or_else(|| Error::MapMissing(map_key.path.clone()))?;
            stores.attach_map(exe_id, map_id);
        }

        for record in snapshot.state.markov_edges {
            let exe_a_key = ExeKey::new(record.exe_a);
            let exe_b_key = ExeKey::new(record.exe_b);
            let a = stores
                .exes
                .id_by_key(&exe_a_key)
                .ok_or_else(|| Error::ExeMissing(exe_a_key.path().clone()))?;
            let b = stores
                .exes
                .id_by_key(&exe_b_key)
                .ok_or_else(|| Error::ExeMissing(exe_b_key.path().clone()))?;
            let state = MarkovState::Neither;
            let key = crate::stores::EdgeKey::new(a, b);
            if stores.ensure_markov_edge(a, b, stores.model_time, state)
                && let Some(edge) = stores.markov.get_mut(key)
            {
                edge.time_to_leave = record.time_to_leave;
                edge.transition_prob = record.transition_prob;
                edge.both_running_time = record.both_running_time;
            }
        }

        let active = stores.active.exes();
        stores.markov.prune_inactive(&active);

        Ok(stores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ExeKey, MapKey, MapSegment, MarkovState, MemStat};
    use crate::observation::{AdmissionDecision, AdmissionPolicy, CandidateExe, Completeness};
    use crate::observation::{ModelUpdater, Observation, ObservationEvent, Scanner};
    use crate::persistence::NoopRepository;
    use crate::prediction::{Prediction, Predictor};
    use crate::prefetch::{PrefetchPlan, PrefetchPlanner, PrefetchReport, Prefetcher};
    use crate::stores::EdgeKey;
    use async_trait::async_trait;
    use proptest::prelude::*;
    use std::collections::{HashMap, HashSet};
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    #[derive(Debug, Clone)]
    struct Recording {
        id: u32,
        hits: Arc<AtomicU32>,
    }

    impl Recording {
        fn record(&self) {
            self.hits.store(self.id, Ordering::SeqCst);
        }
    }

    #[derive(Debug, Default)]
    struct StaticScanner;

    impl Scanner for StaticScanner {
        fn scan(&mut self, time: u64, scan_id: u64) -> Result<Observation, Error> {
            Ok(vec![
                ObservationEvent::ObsBegin { time, scan_id },
                ObservationEvent::MemStat {
                    mem: MemStat {
                        total: 1,
                        free: 1,
                        cached: 1,
                        pagein: 0,
                        pageout: 0,
                    },
                },
                ObservationEvent::ObsEnd {
                    time,
                    scan_id,
                    warnings: Vec::new(),
                },
            ])
        }
    }

    impl AdmissionPolicy for Recording {
        fn allow_exe(&self, _path: &Path) -> bool {
            self.record();
            true
        }

        fn allow_map(&self, _path: &Path) -> bool {
            self.record();
            true
        }

        fn decide(&self, _candidate: &CandidateExe) -> AdmissionDecision {
            self.record();
            AdmissionDecision::Accept {
                completeness: Completeness::Full,
            }
        }
    }

    impl ModelUpdater for Recording {
        fn apply(
            &mut self,
            _stores: &mut Stores,
            _observation: &Observation,
            policy: &dyn AdmissionPolicy,
        ) -> Result<ModelDelta, Error> {
            self.record();
            let candidate = CandidateExe::new(std::path::PathBuf::from("/bin/test"), 0);
            let _ = policy.decide(&candidate);
            Ok(ModelDelta::default())
        }
    }

    impl Predictor for Recording {
        fn predict(&self, _stores: &Stores) -> Prediction {
            self.record();
            Prediction::default()
        }
    }

    impl PrefetchPlanner for Recording {
        fn plan(
            &self,
            _prediction: &Prediction,
            _stores: &Stores,
            _memstat: &MemStat,
        ) -> PrefetchPlan {
            self.record();
            PrefetchPlan {
                maps: Vec::new(),
                total_bytes: 0,
                budget_bytes: 0,
            }
        }
    }

    #[async_trait]
    impl Prefetcher for Recording {
        async fn execute(&self, _plan: &PrefetchPlan, _stores: &Stores) -> PrefetchReport {
            self.record();
            PrefetchReport::default()
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct EdgeData {
        time_to_leave: [f32; 4],
        transition_prob: [[f32; 4]; 4],
        both_running_time: u64,
    }

    proptest! {
        #[test]
        fn snapshot_roundtrip_preserves_keys(
            exe_count in 0usize..8,
            map_count in 0usize..8,
            attachments in prop::collection::vec((0u8..16, 0u8..16), 0..30),
            edges in prop::collection::vec(edge_strategy(), 0..20),
            model_time in 0u64..1_000,
        ) {
            let mut stores = Stores {
                model_time,
                last_accounting_time: model_time,
                ..Default::default()
            };

            let exe_ids: Vec<_> = (0..exe_count)
                .map(|i| {
                    let id = stores.ensure_exe(ExeKey::new(format!("/exe/{i}")));
                    if let Some(exe) = stores.exes.get_mut(id) {
                        exe.last_seen_time = Some(model_time);
                        exe.total_running_time = (i as u64) * 10;
                        exe.running = i % 2 == 0;
                    }
                    id
                })
                .collect();

            let map_ids: Vec<_> = (0..map_count)
                .map(|i| {
                    stores.ensure_map(MapSegment::new(
                        format!("/map/{i}"),
                        (i as u64) * 4096,
                        1024,
                        model_time,
                    ))
                })
                .collect();

            if !exe_ids.is_empty() && !map_ids.is_empty() {
                for (e, m) in attachments {
                    let exe = exe_ids[e as usize % exe_ids.len()];
                    let map = map_ids[m as usize % map_ids.len()];
                    stores.attach_map(exe, map);
                }
            }

            if exe_ids.len() >= 2 {
                for (a_idx, b_idx, ttl, tp, both_time) in edges {
                    let a = exe_ids[a_idx as usize % exe_ids.len()];
                    let b = exe_ids[b_idx as usize % exe_ids.len()];
                    if a == b {
                        continue;
                    }
                    let state = MarkovState::Neither;
                    stores.ensure_markov_edge(a, b, model_time, state);
                    if let Some(edge) = stores.markov.get_mut(EdgeKey::new(a, b)) {
                        edge.time_to_leave = ttl;
                        edge.transition_prob = tp;
                        edge.both_running_time = both_time;
                    }
                }
            }

            let snapshot = PreloadEngine::snapshot_from_stores(&stores);
            let restored = PreloadEngine::stores_from_snapshot(snapshot.clone(), 1_000_000)
                .expect("rehydrate failed");

            let exe_set: HashSet<_> = snapshot
                .state
                .exes
                .iter()
                .map(|exe| exe.path.clone())
                .collect();
            let map_set: HashSet<_> = snapshot
                .state
                .maps
                .iter()
                .map(|map| MapKey::new(map.path.clone(), map.offset, map.length))
                .collect();
            let exe_map_set: HashSet<_> = snapshot
                .state
                .exe_maps
                .iter()
                .map(|record| (record.exe_path.clone(), record.map_key.clone()))
                .collect();
            let mut markov_map: HashMap<(std::path::PathBuf, std::path::PathBuf), EdgeData> =
                HashMap::new();
            for record in snapshot.state.markov_edges.iter() {
                let key = (record.exe_a.clone(), record.exe_b.clone());
                markov_map.insert(
                    key,
                    EdgeData {
                        time_to_leave: record.time_to_leave,
                        transition_prob: record.transition_prob,
                        both_running_time: record.both_running_time,
                    },
                );
            }

            let restored_exes: HashSet<_> = restored
                .exes
                .iter()
                .map(|(_, exe)| exe.key.path().clone())
                .collect();
            let restored_maps: HashSet<_> = restored
                .maps
                .iter()
                .map(|(_, map)| map.key())
                .collect();

            prop_assert_eq!(restored_exes, exe_set);
            prop_assert_eq!(restored_maps, map_set);

            let restored_exe_maps: HashSet<_> = restored
                .exes
                .iter()
                .flat_map(|(exe_id, exe)| {
                    restored
                        .exe_maps
                        .maps_for_exe(exe_id)
                        .filter_map(|map_id| restored.maps.get(map_id))
                        .map(move |map| (exe.key.path().clone(), map.key()))
                })
                .collect();

            prop_assert_eq!(restored_exe_maps, exe_map_set);

            let restored_edges: HashMap<(std::path::PathBuf, std::path::PathBuf), EdgeData> =
                restored
                    .markov
                    .iter()
                    .filter_map(|(key, edge)| {
                        let a = restored.exes.get(key.a())?.key.path().clone();
                        let b = restored.exes.get(key.b())?.key.path().clone();
                        Some((
                            (a, b),
                    EdgeData {
                        time_to_leave: edge.time_to_leave,
                        transition_prob: edge.transition_prob,
                        both_running_time: edge.both_running_time,
                    },
                ))
            })
            .collect();

            let original_keys: HashSet<_> = markov_map.keys().cloned().collect();
            let restored_keys: HashSet<_> = restored_edges.keys().cloned().collect();
            prop_assert_eq!(restored_keys, original_keys);

            for (key, original) in markov_map {
                if let Some(restored_record) = restored_edges.get(&key) {
                    prop_assert_eq!(original.time_to_leave, restored_record.time_to_leave);
                    prop_assert_eq!(original.transition_prob, restored_record.transition_prob);
                    prop_assert_eq!(original.both_running_time, restored_record.both_running_time);
                }
            }
        }
    }

    #[tokio::test]
    async fn reload_swaps_runtime_services() {
        let mut config = Config::default();
        config.system.doscan = true;
        config.system.dopredict = true;
        config.model.cycle = Duration::from_secs(1);

        let admission_hits = Arc::new(AtomicU32::new(0));
        let updater_hits = Arc::new(AtomicU32::new(0));
        let predictor_hits = Arc::new(AtomicU32::new(0));
        let planner_hits = Arc::new(AtomicU32::new(0));
        let prefetcher_hits = Arc::new(AtomicU32::new(0));

        let services = Services {
            scanner: Box::new(StaticScanner),
            admission: Box::new(Recording {
                id: 1,
                hits: admission_hits.clone(),
            }),
            updater: Box::new(Recording {
                id: 1,
                hits: updater_hits.clone(),
            }),
            predictor: Box::new(Recording {
                id: 1,
                hits: predictor_hits.clone(),
            }),
            planner: Box::new(Recording {
                id: 1,
                hits: planner_hits.clone(),
            }),
            prefetcher: Box::new(Recording {
                id: 1,
                hits: prefetcher_hits.clone(),
            }),
            repo: Box::new(NoopRepository),
            clock: Box::new(crate::clock::SystemClock),
        };

        let mut engine = PreloadEngine::new(config.clone(), services)
            .await
            .expect("engine");
        engine.tick().await.expect("tick");

        assert_eq!(admission_hits.load(Ordering::SeqCst), 1);
        assert_eq!(updater_hits.load(Ordering::SeqCst), 1);
        assert_eq!(predictor_hits.load(Ordering::SeqCst), 1);
        assert_eq!(planner_hits.load(Ordering::SeqCst), 1);
        assert_eq!(prefetcher_hits.load(Ordering::SeqCst), 1);

        let bundle = ReloadBundle {
            config: config.clone(),
            admission: Box::new(Recording {
                id: 2,
                hits: admission_hits.clone(),
            }),
            updater: Box::new(Recording {
                id: 2,
                hits: updater_hits.clone(),
            }),
            predictor: Box::new(Recording {
                id: 2,
                hits: predictor_hits.clone(),
            }),
            planner: Box::new(Recording {
                id: 2,
                hits: planner_hits.clone(),
            }),
            prefetcher: Box::new(Recording {
                id: 2,
                hits: prefetcher_hits.clone(),
            }),
        };

        engine.apply_reload(bundle);
        engine.tick().await.expect("tick");

        assert_eq!(admission_hits.load(Ordering::SeqCst), 2);
        assert_eq!(updater_hits.load(Ordering::SeqCst), 2);
        assert_eq!(predictor_hits.load(Ordering::SeqCst), 2);
        assert_eq!(planner_hits.load(Ordering::SeqCst), 2);
        assert_eq!(prefetcher_hits.load(Ordering::SeqCst), 2);
    }

    fn edge_strategy() -> impl Strategy<Value = (u8, u8, [f32; 4], [[f32; 4]; 4], u64)> {
        (
            0u8..16,
            0u8..16,
            prop::array::uniform4(0f32..100f32),
            prop::array::uniform4(prop::array::uniform4(0f32..1f32)),
            0u64..10_000,
        )
    }
}
