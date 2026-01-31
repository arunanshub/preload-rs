#![forbid(unsafe_code)]

use crate::domain::{ExeKey, MapKey, MarkovState};
use crate::error::Error;
use crate::observation::{
    AdmissionDecision, AdmissionPolicy, CandidateExe, Completeness, Observation, ObservationEvent,
};
use crate::stores::Stores;
use config::Config;
use std::collections::{HashMap, HashSet};
use tracing::{debug, trace};

#[derive(Debug, Default, Clone)]
pub struct ModelDelta {
    pub new_exes: Vec<ExeKey>,
    pub new_maps: Vec<MapKey>,
    pub new_edges: Vec<(ExeKey, ExeKey)>,
    pub running_now: Vec<ExeKey>,
    pub stopped_now: Vec<ExeKey>,
    pub rejected: Vec<(ExeKey, super::RejectReason)>,
    pub partial_exes: Vec<ExeKey>,
}

pub trait ModelUpdater: Send + Sync {
    fn apply(
        &mut self,
        stores: &mut Stores,
        observation: &Observation,
        policy: &dyn AdmissionPolicy,
    ) -> Result<ModelDelta, Error>;
}

#[derive(Debug, Clone)]
pub struct DefaultModelUpdater {
    active_window: u64,
    decay: f32,
}

impl DefaultModelUpdater {
    pub fn new(config: &Config) -> Self {
        Self {
            active_window: config.model.active_window.as_secs(),
            decay: config.model.decay_factor(),
        }
    }
}

impl ModelUpdater for DefaultModelUpdater {
    fn apply(
        &mut self,
        stores: &mut Stores,
        observation: &Observation,
        policy: &dyn AdmissionPolicy,
    ) -> Result<ModelDelta, Error> {
        let mut candidates: HashMap<std::path::PathBuf, CandidateExe> = HashMap::new();
        let mut running_paths: HashSet<std::path::PathBuf> = HashSet::new();
        let mut now = stores.model_time;

        for event in observation {
            match event {
                ObservationEvent::ObsBegin { time, .. } => {
                    now = *time;
                }
                ObservationEvent::ExeSeen { path, pid } => {
                    running_paths.insert(path.clone());
                    candidates
                        .entry(path.clone())
                        .or_insert_with(|| CandidateExe::new(path.clone(), *pid));
                }
                ObservationEvent::MapSeen { exe_path, map } => {
                    let candidate = candidates
                        .entry(exe_path.clone())
                        .or_insert_with(|| CandidateExe::new(exe_path.clone(), 0));
                    if policy.allow_map(&map.path) {
                        candidate.total_size = candidate.total_size.saturating_add(map.length);
                        candidate.maps.push(map.clone());
                    } else {
                        candidate.rejected_maps.push(map.path.clone());
                    }
                }
                ObservationEvent::MemStat { .. } => {}
                ObservationEvent::ObsEnd { .. } => {}
            }
        }

        let mut delta = ModelDelta::default();
        let mut active_exe_ids = HashSet::new();

        for (_, candidate) in candidates.into_iter() {
            match policy.decide(&candidate) {
                AdmissionDecision::Reject { reason } => {
                    delta
                        .rejected
                        .push((ExeKey::new(candidate.path.clone()), reason));
                }
                AdmissionDecision::Defer => {}
                AdmissionDecision::Accept { completeness } => {
                    let exe_key = ExeKey::new(candidate.path.clone());
                    let is_new_exe = stores.exes.id_by_key(&exe_key).is_none();
                    let exe_id = stores.ensure_exe(exe_key.clone());
                    if is_new_exe {
                        delta.new_exes.push(exe_key.clone());
                    }

                    if let Some(exe) = stores.exes.get_mut(exe_id) {
                        exe.last_seen_time = Some(now);
                    }

                    if completeness == Completeness::Partial {
                        delta.partial_exes.push(exe_key.clone());
                    }

                    for map in candidate.maps {
                        let map_key = map.key();
                        let (map_id, is_new) = stores.ensure_map_with_flag(map);
                        if is_new {
                            delta.new_maps.push(map_key);
                        }
                        stores.attach_map(exe_id, map_id);
                    }

                    if running_paths.contains(&candidate.path) {
                        active_exe_ids.insert(exe_id);
                    }
                }
            }
        }

        // Update running flags and transitions.
        let exe_ids: Vec<_> = stores.exes.iter().map(|(id, _)| id).collect();
        for exe_id in exe_ids {
            if let Some(exe_mut) = stores.exes.get_mut(exe_id) {
                let is_running = running_paths.contains(exe_mut.key.path());
                if exe_mut.running != is_running {
                    exe_mut.change_time = now;
                    if is_running {
                        delta.running_now.push(exe_mut.key.clone());
                    } else {
                        delta.stopped_now.push(exe_mut.key.clone());
                    }
                }
                exe_mut.running = is_running;
                if is_running {
                    active_exe_ids.insert(exe_id);
                }
            }
        }

        // Update active set (lazy Markov edges).
        stores.active.update(active_exe_ids.iter().copied(), now);
        let _removed = stores.active.prune(now, self.active_window);
        let active = stores.active.exes();
        stores.markov.prune_inactive(&active);

        // Ensure edges among active exes.
        let active_vec: Vec<_> = active.iter().copied().collect();
        for i in 0..active_vec.len() {
            for j in (i + 1)..active_vec.len() {
                let a = active_vec[i];
                let b = active_vec[j];
                let state = {
                    let a_running = stores.exes.get(a).map(|e| e.running).unwrap_or(false);
                    let b_running = stores.exes.get(b).map(|e| e.running).unwrap_or(false);
                    MarkovState::from_running(a_running, b_running)
                };
                if stores.ensure_markov_edge(a, b, now, state)
                    && let (Some(a_exe), Some(b_exe)) = (stores.exes.get(a), stores.exes.get(b))
                {
                    delta.new_edges.push((a_exe.key.clone(), b_exe.key.clone()));
                }
            }
        }

        // Accounting time updates.
        let period = now.saturating_sub(stores.last_accounting_time);
        if period > 0 {
            let exe_ids: Vec<_> = stores.exes.iter().map(|(id, _)| id).collect();
            for exe_id in exe_ids {
                if let Some(exe_mut) = stores.exes.get_mut(exe_id)
                    && exe_mut.running
                {
                    exe_mut.total_running_time = exe_mut.total_running_time.saturating_add(period);
                }
            }
            for (key, edge) in stores.markov.iter_mut() {
                let a_running = stores.exes.get(key.a()).map(|e| e.running).unwrap_or(false);
                let b_running = stores.exes.get(key.b()).map(|e| e.running).unwrap_or(false);
                if a_running && b_running {
                    edge.both_running_time = edge.both_running_time.saturating_add(period);
                }
            }
        }
        stores.last_accounting_time = now;

        // Update Markov transitions.
        for (key, edge) in stores.markov.iter_mut() {
            let a_running = stores.exes.get(key.a()).map(|e| e.running).unwrap_or(false);
            let b_running = stores.exes.get(key.b()).map(|e| e.running).unwrap_or(false);
            let new_state = MarkovState::from_running(a_running, b_running);
            edge.update_state(new_state, now, self.decay);
        }

        stores.model_time = now;

        trace!(?delta, "model delta computed");
        debug!(active_count = active.len(), "active set updated");

        Ok(delta)
    }
}
