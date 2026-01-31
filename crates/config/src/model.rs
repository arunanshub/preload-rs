#![forbid(unsafe_code)]

use crate::memory_policy::MemoryPolicy;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::time::Duration;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Model {
    /// Cycle length in seconds.
    #[serde_as(as = "serde_with::DurationSeconds")]
    pub cycle: Duration,

    /// Whether to use correlation in prediction.
    pub use_correlation: bool,

    /// Minimum total map size (bytes) to track an exe.
    pub minsize: u64,

    /// Active-set window for lazy Markov edges.
    #[serde_as(as = "serde_with::DurationSeconds")]
    pub active_window: Duration,

    /// Decay factor for exponentially-fading means.
    pub decay: f32,

    pub memory: MemoryPolicy,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            cycle: Duration::from_secs(20),
            use_correlation: true,
            minsize: 2_000_000,
            active_window: Duration::from_secs(6 * 60 * 60),
            decay: 0.01,
            memory: MemoryPolicy::default(),
        }
    }
}

impl Model {}
