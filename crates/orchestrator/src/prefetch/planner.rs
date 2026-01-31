#![forbid(unsafe_code)]

use crate::domain::{MapId, MemStat};
use crate::prediction::Prediction;
use crate::prefetch::PrefetchPlan;
use crate::stores::Stores;
use config::{Config, SortStrategy};
use std::cmp::Ordering;
use std::os::linux::fs::MetadataExt;
use tracing::trace;

pub trait PrefetchPlanner: Send + Sync {
    /// Create a prefetch plan from prediction scores and memory stats.
    fn plan(&self, prediction: &Prediction, stores: &Stores, memstat: &MemStat) -> PrefetchPlan;
}

#[derive(Debug, Clone)]
pub struct GreedyPrefetchPlanner {
    sort: SortStrategy,
    memtotal: i32,
    memfree: i32,
    memcached: i32,
}

impl GreedyPrefetchPlanner {
    pub fn new(config: &Config) -> Self {
        let policy = config.model.memory.clamp();
        Self {
            sort: config.system.sortstrategy,
            memtotal: policy.memtotal,
            memfree: policy.memfree,
            memcached: policy.memcached,
        }
    }

    fn available_kb(&self, mem: &MemStat) -> u64 {
        let mut available = self.memtotal as i64 * mem.total as i64 / 100;
        available += self.memfree as i64 * mem.free as i64 / 100;
        available = available.max(0);
        available += self.memcached as i64 * mem.cached as i64 / 100;
        available.max(0) as u64
    }

    fn kb(bytes: u64) -> u64 {
        bytes.div_ceil(1024)
    }
}

impl PrefetchPlanner for GreedyPrefetchPlanner {
    fn plan(&self, prediction: &Prediction, stores: &Stores, memstat: &MemStat) -> PrefetchPlan {
        let mut items: Vec<(MapId, f32)> = prediction
            .map_scores
            .iter()
            .map(|(id, score)| (*id, *score))
            .collect();
        items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

        let mut budget_kb = self.available_kb(memstat);
        let mut selected = Vec::new();
        let mut total_bytes: u64 = 0;

        for (map_id, _) in items {
            let Some(map) = stores.maps.get(map_id) else {
                continue;
            };
            let map_kb = Self::kb(map.length);
            if map_kb > budget_kb {
                continue;
            }
            budget_kb = budget_kb.saturating_sub(map_kb);
            total_bytes = total_bytes.saturating_add(map.length);
            selected.push(map_id);
        }

        // Sort selected maps based on strategy for I/O efficiency.
        match self.sort {
            SortStrategy::None => {}
            SortStrategy::Path => {
                selected.sort_by(|a, b| {
                    let a_path = stores.maps.get(*a).map(|m| &m.path);
                    let b_path = stores.maps.get(*b).map(|m| &m.path);
                    a_path.cmp(&b_path)
                });
            }
            SortStrategy::Block | SortStrategy::Inode => {
                selected.sort_by(|a, b| {
                    let a_inode = stores
                        .maps
                        .get(*a)
                        .and_then(|m| m.path.metadata().ok())
                        .map(|m| m.st_ino());
                    let b_inode = stores
                        .maps
                        .get(*b)
                        .and_then(|m| m.path.metadata().ok())
                        .map(|m| m.st_ino());
                    a_inode.cmp(&b_inode)
                });
            }
        }

        trace!(
            selected = selected.len(),
            total_bytes, "prefetch plan created"
        );

        PrefetchPlan {
            maps: selected,
            total_bytes,
            budget_bytes: self.available_kb(memstat) * 1024,
        }
    }
}
