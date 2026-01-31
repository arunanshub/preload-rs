#![forbid(unsafe_code)]

use crate::domain::MapKey;
use std::path::PathBuf;
use std::time::SystemTime;

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct StoresSnapshot {
    pub meta: SnapshotMeta,
    pub state: StateSnapshot,
}

#[derive(Debug, Clone)]
pub struct SnapshotMeta {
    pub schema_version: u32,
    pub app_version: Option<String>,
    pub created_at: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub model_time: u64,
    pub last_accounting_time: u64,
    pub exes: Vec<ExeRecord>,
    pub maps: Vec<MapRecord>,
    pub exe_maps: Vec<ExeMapRecord>,
    pub markov_edges: Vec<MarkovRecord>,
}

#[derive(Debug, Clone)]
pub struct ExeRecord {
    pub path: PathBuf,
    pub total_running_time: u64,
    pub last_seen_time: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct MapRecord {
    pub path: PathBuf,
    pub offset: u64,
    pub length: u64,
    pub update_time: u64,
}

#[derive(Debug, Clone)]
pub struct ExeMapRecord {
    pub exe_path: PathBuf,
    pub map_key: MapKey,
    pub prob: f32,
}

#[derive(Debug, Clone)]
pub struct MarkovRecord {
    pub exe_a: PathBuf,
    pub exe_b: PathBuf,
    pub time_to_leave: [f32; 4],
    pub transition_prob: [[f32; 4]; 4],
    pub both_running_time: u64,
}
