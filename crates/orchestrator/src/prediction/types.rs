#![forbid(unsafe_code)]

use crate::domain::{ExeId, MapId};
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct Prediction {
    pub exe_scores: HashMap<ExeId, f32>,
    pub map_scores: HashMap<MapId, f32>,
}

#[derive(Debug, Default, Clone)]
pub struct PredictionSummary {
    pub num_exes_scored: usize,
    pub num_maps_scored: usize,
}

impl Prediction {
    pub fn summarize(&self) -> PredictionSummary {
        PredictionSummary {
            num_exes_scored: self.exe_scores.len(),
            num_maps_scored: self.map_scores.len(),
        }
    }
}
