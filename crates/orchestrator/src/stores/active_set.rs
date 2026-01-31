#![forbid(unsafe_code)]

use crate::domain::ExeId;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct ActiveSet {
    last_seen: HashMap<ExeId, u64>,
}

impl ActiveSet {
    pub fn update(&mut self, active_now: impl IntoIterator<Item = ExeId>, now: u64) {
        for exe_id in active_now {
            self.last_seen.insert(exe_id, now);
        }
    }

    pub fn prune(&mut self, now: u64, window: u64) -> HashSet<ExeId> {
        let mut removed = HashSet::new();
        self.last_seen.retain(|exe_id, last| {
            if now.saturating_sub(*last) > window {
                removed.insert(*exe_id);
                false
            } else {
                true
            }
        });
        removed
    }

    pub fn exes(&self) -> HashSet<ExeId> {
        self.last_seen.keys().copied().collect()
    }
}
