#![forbid(unsafe_code)]

use crate::domain::{ExeId, MarkovState};
use crate::prediction::Prediction;
use crate::stores::Stores;
use config::Config;
use std::collections::HashMap;

pub trait Predictor: Send + Sync {
    /// Produce exe and map scores for the next cycle.
    fn predict(&self, stores: &Stores) -> Prediction;
}

#[derive(Debug, Clone)]
pub struct MarkovPredictor {
    use_correlation: bool,
    cycle_secs: f32,
}

impl MarkovPredictor {
    pub fn new(config: &Config) -> Self {
        Self {
            use_correlation: config.model.use_correlation,
            cycle_secs: config.model.cycle.as_secs_f32(),
        }
    }

    fn correlation(&self, stores: &Stores, a: ExeId, b: ExeId, ab_time: u64) -> f32 {
        let t = stores.model_time;
        let a_time = stores
            .exes
            .get(a)
            .map(|e| e.total_running_time)
            .unwrap_or(0);
        let b_time = stores
            .exes
            .get(b)
            .map(|e| e.total_running_time)
            .unwrap_or(0);

        if t == 0 || a_time == 0 || b_time == 0 || a_time == t || b_time == t {
            return 0.0;
        }

        let numerator = (t as f32 * ab_time as f32) - (a_time as f32 * b_time as f32);
        let denom =
            (a_time as f32 * b_time as f32 * (t - a_time) as f32 * (t - b_time) as f32).sqrt();
        if denom == 0.0 { 0.0 } else { numerator / denom }
    }

    fn p_needed(
        edge: &crate::domain::MarkovEdge,
        state: MarkovState,
        target_state: MarkovState,
        cycle: f32,
    ) -> f32 {
        let state_ix = state.index();
        let tt = edge.time_to_leave[state_ix];
        if tt <= 0.0 {
            return 0.0;
        }
        let p_state_change = 1.0 - (-cycle / tt).exp();
        let target_ix = target_state.index();
        let both_ix = MarkovState::Both.index();
        let p_runs_next =
            edge.transition_prob[state_ix][target_ix] + edge.transition_prob[state_ix][both_ix];
        (p_state_change * p_runs_next).clamp(0.0, 1.0)
    }
}

impl Predictor for MarkovPredictor {
    fn predict(&self, stores: &Stores) -> Prediction {
        let mut not_needed: HashMap<ExeId, f32> = HashMap::new();

        for (key, edge) in stores.markov.iter() {
            let a = key.a();
            let b = key.b();
            let a_running = stores.exes.get(a).map(|e| e.running).unwrap_or(false);
            let b_running = stores.exes.get(b).map(|e| e.running).unwrap_or(false);

            let state = MarkovState::from_running(a_running, b_running);

            if !a_running {
                let base = Self::p_needed(edge, state, MarkovState::AOnly, self.cycle_secs);
                let corr = if self.use_correlation {
                    self.correlation(stores, a, b, edge.both_running_time).abs()
                } else {
                    1.0
                };
                let p = (base * corr).clamp(0.0, 1.0);
                let entry = not_needed.entry(a).or_insert(1.0);
                *entry *= 1.0 - p;
            }
            if !b_running {
                let base = Self::p_needed(edge, state, MarkovState::BOnly, self.cycle_secs);
                let corr = if self.use_correlation {
                    self.correlation(stores, a, b, edge.both_running_time).abs()
                } else {
                    1.0
                };
                let p = (base * corr).clamp(0.0, 1.0);
                let entry = not_needed.entry(b).or_insert(1.0);
                *entry *= 1.0 - p;
            }
        }

        let mut prediction = Prediction::default();

        for (exe_id, exe) in stores.exes.iter() {
            if exe.running {
                prediction.exe_scores.insert(exe_id, 0.0);
            } else {
                let not_needed_prob = not_needed.get(&exe_id).copied().unwrap_or(1.0);
                let needed = (1.0 - not_needed_prob).clamp(0.0, 1.0);
                prediction.exe_scores.insert(exe_id, needed);
            }
        }

        // Map scores derived from exe scores (Pr map needed).
        for (map_id, _map) in stores.maps.iter() {
            let mut not_needed_prob = 1.0;
            for exe_id in stores.exe_maps.exes_for_map(map_id) {
                let exe_score = prediction.exe_scores.get(&exe_id).copied().unwrap_or(0.0);
                not_needed_prob *= 1.0 - exe_score;
            }
            let needed = (1.0 - not_needed_prob).clamp(0.0, 1.0);
            prediction.map_scores.insert(map_id, needed);
        }

        prediction
    }
}
