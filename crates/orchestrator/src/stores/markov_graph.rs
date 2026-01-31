#![forbid(unsafe_code)]

use crate::domain::{ExeId, MarkovEdge, MarkovState};
use crate::stores::EdgeKey;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct MarkovGraph {
    edges: HashMap<EdgeKey, MarkovEdge>,
}

impl MarkovGraph {
    pub fn ensure_edge(&mut self, a: ExeId, b: ExeId, now: u64, state: MarkovState) -> bool {
        let key = EdgeKey::new(a, b);
        if self.edges.contains_key(&key) {
            return false;
        }
        self.edges.insert(key, MarkovEdge::new(state, now));
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = (EdgeKey, &MarkovEdge)> {
        self.edges.iter().map(|(k, v)| (*k, v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (EdgeKey, &mut MarkovEdge)> {
        self.edges.iter_mut().map(|(k, v)| (*k, v))
    }

    pub fn get_mut(&mut self, key: EdgeKey) -> Option<&mut MarkovEdge> {
        self.edges.get_mut(&key)
    }

    pub fn prune_inactive(&mut self, active: &HashSet<ExeId>) {
        self.edges
            .retain(|key, _| active.contains(&key.0) && active.contains(&key.1));
    }
}
