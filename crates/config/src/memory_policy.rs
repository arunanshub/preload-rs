#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MemoryPolicy {
    /// Percentage of total memory (clamped to -100..=100).
    pub memtotal: i32,
    /// Percentage of free memory (clamped to -100..=100).
    pub memfree: i32,
    /// Percentage of cached memory (clamped to -100..=100).
    pub memcached: i32,
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            memtotal: -10,
            memfree: 50,
            memcached: 0,
        }
    }
}

impl MemoryPolicy {
    pub fn clamp(self) -> Self {
        Self {
            memtotal: self.memtotal.clamp(-100, 100),
            memfree: self.memfree.clamp(-100, 100),
            memcached: self.memcached.clamp(-100, 100),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn clamp_limits_values(a in -1000i32..1000, b in -1000i32..1000, c in -1000i32..1000) {
            let policy = MemoryPolicy { memtotal: a, memfree: b, memcached: c }.clamp();
            prop_assert!((-100..=100).contains(&policy.memtotal));
            prop_assert!((-100..=100).contains(&policy.memfree));
            prop_assert!((-100..=100).contains(&policy.memcached));
        }
    }
}
