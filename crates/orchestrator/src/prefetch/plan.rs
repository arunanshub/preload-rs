#![forbid(unsafe_code)]

use crate::domain::MapId;
use crate::domain::MapKey;

#[derive(Debug, Clone)]
pub struct PrefetchPlan {
    pub maps: Vec<MapId>,
    pub total_bytes: u64,
    pub budget_bytes: u64,
}

#[derive(Debug, Default, Clone)]
pub struct PrefetchReport {
    pub num_maps: usize,
    pub total_bytes: u64,
    pub failures: Vec<MapKey>,
}
