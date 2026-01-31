#![forbid(unsafe_code)]

use crate::sort_strategy::SortStrategy;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::time::Duration;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct System {
    pub doscan: bool,
    pub dopredict: bool,

    /// Autosave interval for state persistence.
    #[serde_as(as = "serde_with::DurationSeconds")]
    pub autosave: Duration,

    /// Exe path prefixes ("!" means deny).
    pub exeprefix: Vec<String>,

    /// Map path prefixes ("!" means deny).
    pub mapprefix: Vec<String>,

    /// Prefetch sort strategy.
    pub sortstrategy: SortStrategy,

    /// Max number of concurrent prefetch workers. None means auto (CPU cores).
    /// 0 disables prefetch entirely.
    pub prefetch_concurrency: Option<usize>,

    /// How long to cache admission rejections.
    #[serde_as(as = "serde_with::DurationSeconds")]
    pub policy_cache_ttl: Duration,

    /// Maximum number of cached admission rejections. 0 disables caching.
    pub policy_cache_capacity: usize,
}

impl Default for System {
    fn default() -> Self {
        Self {
            doscan: true,
            dopredict: true,
            autosave: Duration::from_secs(3600),
            mapprefix: vec![
                "/usr/".into(),
                "/lib/".into(),
                "/var/cache/".into(),
                "!/".into(),
            ],
            exeprefix: vec![
                "!/usr/sbin/".into(),
                "!/usr/local/sbin/".into(),
                "/usr/".into(),
                "!/".into(),
            ],
            sortstrategy: SortStrategy::Block,
            prefetch_concurrency: None,
            policy_cache_ttl: Duration::from_secs(300),
            policy_cache_capacity: 1024,
        }
    }
}

impl System {}
