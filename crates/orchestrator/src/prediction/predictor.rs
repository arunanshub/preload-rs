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

        if t == 0 || a_time == 0 || b_time == 0 || a_time >= t || b_time >= t {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ExeKey, MapSegment, MarkovState};
    use crate::stores::{EdgeKey, Stores};
    use config::Config;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn predictor_scores_are_bounded(
            exe_count in 0usize..8,
            map_count in 0usize..8,
            model_time in 0u64..1_000,
            use_correlation in any::<bool>(),
            edges in prop::collection::vec(edge_strategy(), 0..20),
            attachments in prop::collection::vec((0u8..16, 0u8..16), 0..30),
        ) {
            let mut stores = Stores {
                model_time,
                ..Default::default()
            };

            let exe_ids: Vec<_> = (0..exe_count)
                .map(|i| {
                    let id = stores.ensure_exe(ExeKey::new(format!("/exe/{i}")));
                    if let Some(exe) = stores.exes.get_mut(id) {
                        exe.running = i % 2 == 0;
                        exe.total_running_time = (i as u64) * 10;
                        exe.last_seen_time = Some(model_time);
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

            let mut config = Config::default();
            config.model.use_correlation = use_correlation;
            let predictor = MarkovPredictor::new(&config);
            let prediction = predictor.predict(&stores);

            for score in prediction.exe_scores.values() {
                prop_assert!(!score.is_nan());
                prop_assert!(*score >= 0.0 && *score <= 1.0);
            }

            for score in prediction.map_scores.values() {
                prop_assert!(!score.is_nan());
                prop_assert!(*score >= 0.0 && *score <= 1.0);
            }
        }
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
