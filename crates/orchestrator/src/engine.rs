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
    Reload(ReloadBundle),
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
                self.apply_reload(bundle);
                info!("config reloaded");
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
