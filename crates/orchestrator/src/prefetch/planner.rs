#![forbid(unsafe_code)]

use crate::domain::{MapId, MemStat};
use crate::prediction::Prediction;
use crate::prefetch::PrefetchPlan;
use crate::stores::Stores;
use config::{Config, SortStrategy};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::os::linux::fs::MetadataExt;
use std::sync::Mutex;
use tracing::trace;

pub trait PrefetchPlanner: Send + Sync {
    /// Create a prefetch plan from prediction scores and memory stats.
    fn plan(&self, prediction: &Prediction, stores: &Stores, memstat: &MemStat) -> PrefetchPlan;
}

#[derive(Debug)]
pub struct GreedyPrefetchPlanner {
    sort: SortStrategy,
    memtotal: i32,
    memfree: i32,
    memcached: i32,
    sort_cache: Mutex<HashMap<MapId, Option<MapSortMeta>>>,
}

impl GreedyPrefetchPlanner {
    pub fn new(config: &Config) -> Self {
        let policy = config.model.memory.clamp();
        Self {
            sort: config.system.sortstrategy,
            memtotal: policy.memtotal,
            memfree: policy.memfree,
            memcached: policy.memcached,
            sort_cache: Mutex::new(HashMap::new()),
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

    fn sort_meta(&self, map_id: MapId, map: &crate::domain::MapSegment) -> Option<MapSortMeta> {
        if let Ok(cache) = self.sort_cache.lock()
            && let Some(meta) = cache.get(&map_id)
        {
            return *meta;
        }

        let meta = fs::metadata(&map.path).ok().map(|metadata| {
            let block_size = metadata.st_blksize();
            let block_size = if block_size > 0 { block_size } else { 4096 };
            let block = map.offset / block_size;
            MapSortMeta {
                device: metadata.st_dev(),
                inode: metadata.st_ino(),
                block,
            }
        });

        if let Ok(mut cache) = self.sort_cache.lock() {
            cache.insert(map_id, meta);
        }

        meta
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

        for (map_id, score) in items {
            let Some(map) = stores.maps.get(map_id) else {
                continue;
            };
            let map_kb = Self::kb(map.length);
            if map_kb > budget_kb {
                continue;
            }
            budget_kb = budget_kb.saturating_sub(map_kb);
            total_bytes = total_bytes.saturating_add(map.length);
            selected.push(SelectedMap {
                id: map_id,
                score,
                index: selected.len(),
            });
        }

        // Sort selected maps based on strategy for I/O efficiency.
        match self.sort {
            SortStrategy::None => {}
            SortStrategy::Path => {
                let mut keyed: Vec<SelectedWithKey<std::path::PathBuf>> = selected
                    .into_iter()
                    .map(|item| {
                        let key = stores.maps.get(item.id).map(|m| m.path.clone());
                        SelectedWithKey { item, key }
                    })
                    .collect();
                sort_by_score_and_key(&mut keyed);
                selected = keyed.into_iter().map(|entry| entry.item).collect();
            }
            SortStrategy::Block | SortStrategy::Inode => {
                let mut keyed: Vec<SelectedWithKey<SortKey>> = selected
                    .into_iter()
                    .map(|item| {
                        let key = stores.maps.get(item.id).and_then(|map| {
                            self.sort_meta(item.id, map).map(|meta| match self.sort {
                                SortStrategy::Block => SortKey::Block(BlockKey {
                                    device: meta.device,
                                    block: meta.block,
                                    offset: map.offset,
                                }),
                                SortStrategy::Inode => SortKey::Inode(InodeKey {
                                    device: meta.device,
                                    inode: meta.inode,
                                    offset: map.offset,
                                }),
                                _ => SortKey::Block(BlockKey {
                                    device: meta.device,
                                    block: meta.block,
                                    offset: map.offset,
                                }),
                            })
                        });
                        SelectedWithKey { item, key }
                    })
                    .collect();
                sort_by_score_and_key(&mut keyed);
                selected = keyed.into_iter().map(|entry| entry.item).collect();
            }
        }

        trace!(
            selected = selected.len(),
            total_bytes, "prefetch plan created"
        );

        PrefetchPlan {
            maps: selected.into_iter().map(|item| item.id).collect(),
            total_bytes,
            budget_bytes: self.available_kb(memstat) * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct MapSortMeta {
    device: u64,
    inode: u64,
    block: u64,
}

#[derive(Debug, Clone)]
struct SelectedMap {
    id: MapId,
    score: f32,
    index: usize,
}

#[derive(Debug, Clone)]
struct SelectedWithKey<K> {
    item: SelectedMap,
    key: Option<K>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct BlockKey {
    device: u64,
    block: u64,
    offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct InodeKey {
    device: u64,
    inode: u64,
    offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SortKey {
    Block(BlockKey),
    Inode(InodeKey),
}

fn sort_by_score_and_key<K: Ord>(items: &mut [SelectedWithKey<K>]) {
    items.sort_by(|a, b| {
        let score_cmp = b
            .item
            .score
            .partial_cmp(&a.item.score)
            .unwrap_or(Ordering::Equal);
        if score_cmp != Ordering::Equal {
            return score_cmp;
        }
        match (&a.key, &b.key) {
            (Some(a_key), Some(b_key)) => a_key
                .cmp(b_key)
                .then_with(|| a.item.index.cmp(&b.item.index)),
            _ => a.item.index.cmp(&b.item.index),
        }
    });
}
