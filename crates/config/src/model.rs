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

    /// Half-life for exponentially-fading means.
    #[serde_as(as = "Option<serde_with::DurationSeconds>")]
    pub half_life: Option<Duration>,

    /// Decay factor (1/sec) for exponentially-fading means. Ignored if half_life is set.
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
            half_life: None,
            decay: 0.01,
            memory: MemoryPolicy::default(),
        }
    }
}

impl Model {
    pub fn decay_factor(&self) -> f32 {
        if let Some(half_life) = self.half_life {
            let secs = half_life.as_secs_f32();
            if secs > 0.0 {
                return (2.0_f32.ln()) / secs;
            }
            return 0.0;
        }
        self.decay.max(0.0)
    }
}
