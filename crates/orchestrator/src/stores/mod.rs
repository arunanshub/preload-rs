#![forbid(unsafe_code)]

mod active_set;
mod edge_key;
mod exe_map_index;
mod exe_store;
mod map_store;
mod markov_graph;

pub use active_set::ActiveSet;
pub use edge_key::EdgeKey;
pub use exe_map_index::ExeMapIndex;
pub use exe_store::ExeStore;
pub use map_store::MapStore;
pub use markov_graph::MarkovGraph;

use crate::domain::{ExeId, ExeKey, MapId, MapSegment, MarkovState};
use std::collections::HashSet;

#[derive(Debug, Default)]
pub struct Stores {
    pub exes: ExeStore,
    pub maps: MapStore,
    pub exe_maps: ExeMapIndex,
    pub markov: MarkovGraph,
    pub active: ActiveSet,
    pub model_time: u64,
    pub last_accounting_time: u64,
}

impl Stores {
    pub fn ensure_exe(&mut self, key: ExeKey) -> ExeId {
        self.exes.ensure(key)
    }

    pub fn ensure_map(&mut self, segment: MapSegment) -> MapId {
        self.maps.ensure(segment)
    }

    pub fn ensure_map_with_flag(&mut self, segment: MapSegment) -> (MapId, bool) {
        self.maps.ensure_with_flag(segment)
    }

    pub fn attach_map(&mut self, exe_id: ExeId, map_id: MapId) {
        self.exe_maps.attach(exe_id, map_id);
    }

    pub fn ensure_markov_edge(&mut self, a: ExeId, b: ExeId, now: u64, state: MarkovState) -> bool {
        self.markov.ensure_edge(a, b, now, state)
    }

    pub fn active_exes(&self) -> HashSet<ExeId> {
        self.active.exes()
    }
}
