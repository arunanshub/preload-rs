#![forbid(unsafe_code)]

use crate::observation::CandidateExe;
use config::Config;
use moka::policy::EvictionPolicy;
use moka::sync::Cache;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completeness {
    Full,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    TooSmall,
    ExePrefixDenied,
    MapPrefixDenied,
    MissingMaps,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionDecision {
    Accept { completeness: Completeness },
    Reject { reason: RejectReason },
    Defer,
}

pub trait AdmissionPolicy: Send + Sync {
    /// Determine whether an exe path is eligible for tracking.
    fn allow_exe(&self, path: &Path) -> bool;
    /// Determine whether a map path is eligible for tracking.
    fn allow_map(&self, path: &Path) -> bool;
    /// Decide whether a candidate exe should be admitted into the model.
    fn decide(&self, candidate: &CandidateExe) -> AdmissionDecision;
    /// Optional stats for diagnostics.
    fn stats(&self) -> Option<AdmissionPolicyStats> {
        None
    }
}

#[derive(Debug)]
pub struct DefaultAdmissionPolicy {
    minsize: u64,
    exeprefix: Vec<String>,
    mapprefix: Vec<String>,
    cache_ttl: Duration,
    cache_capacity: usize,
    cache: Option<Cache<PathBuf, RejectReason>>,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    cache_inserts: AtomicU64,
    cache_invalidations: AtomicU64,
}

impl DefaultAdmissionPolicy {
    pub fn new(config: &Config) -> Self {
        let cache_ttl = config.system.policy_cache_ttl;
        let cache_capacity = config.system.policy_cache_capacity;
        let cache = if cache_capacity == 0 || cache_ttl.is_zero() {
            None
        } else {
            Some(
                Cache::builder()
                    .max_capacity(cache_capacity as u64)
                    .time_to_live(cache_ttl)
                    .eviction_policy(EvictionPolicy::lru())
                    .build(),
            )
        };
        Self {
            minsize: config.model.minsize,
            exeprefix: config.system.exeprefix.clone(),
            mapprefix: config.system.mapprefix.clone(),
            cache_ttl,
            cache_capacity,
            cache,
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            cache_inserts: AtomicU64::new(0),
            cache_invalidations: AtomicU64::new(0),
        }
    }

    fn accept_path<T: AsRef<str>>(path: &Path, prefixes: &[T]) -> bool {
        let mut best: Option<(bool, usize)> = None;
        let path_str = match path.to_str() {
            Some(s) => s,
            None => return false,
        };
        for prefix in prefixes {
            let prefix = prefix.as_ref();
            let (neg, p) = prefix
                .strip_prefix('!')
                .map(|p| (true, p))
                .unwrap_or((false, prefix));
            if path_str.starts_with(p) {
                let len = p.len();
                if best.map(|(_, l)| l).unwrap_or(0) < len {
                    best = Some((!neg, len));
                }
            }
        }
        best.map(|(accept, _)| accept).unwrap_or(true)
    }
}

impl Clone for DefaultAdmissionPolicy {
    fn clone(&self) -> Self {
        Self {
            minsize: self.minsize,
            exeprefix: self.exeprefix.clone(),
            mapprefix: self.mapprefix.clone(),
            cache_ttl: self.cache_ttl,
            cache_capacity: self.cache_capacity,
            cache: self.cache.clone(),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            cache_inserts: AtomicU64::new(0),
            cache_invalidations: AtomicU64::new(0),
        }
    }
}

impl AdmissionPolicy for DefaultAdmissionPolicy {
    fn allow_exe(&self, path: &Path) -> bool {
        Self::accept_path(path, &self.exeprefix)
    }

    fn allow_map(&self, path: &Path) -> bool {
        Self::accept_path(path, &self.mapprefix)
    }

    fn decide(&self, candidate: &CandidateExe) -> AdmissionDecision {
        if let Some(cache) = &self.cache
            && let Some(reason) = cache.get(&candidate.path)
        {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            return AdmissionDecision::Reject { reason };
        }
        if self.cache.is_some() {
            self.cache_misses.fetch_add(1, Ordering::Relaxed);
        }

        if !self.allow_exe(&candidate.path) {
            let reason = RejectReason::ExePrefixDenied;
            self.cache_reject(&candidate.path, reason.clone());
            return AdmissionDecision::Reject { reason };
        }
        if candidate.maps.is_empty() {
            let reason = if candidate.rejected_maps.is_empty() {
                RejectReason::MissingMaps
            } else {
                RejectReason::MapPrefixDenied
            };
            self.cache_reject(&candidate.path, reason.clone());
            return AdmissionDecision::Reject { reason };
        }
        if candidate.total_size < self.minsize {
            let reason = RejectReason::TooSmall;
            self.cache_reject(&candidate.path, reason.clone());
            return AdmissionDecision::Reject { reason };
        }

        let completeness = if candidate.rejected_maps.is_empty() {
            Completeness::Full
        } else {
            Completeness::Partial
        };
        self.cache_clear(&candidate.path);
        AdmissionDecision::Accept { completeness }
    }

    fn stats(&self) -> Option<AdmissionPolicyStats> {
        let (enabled, entries) = match &self.cache {
            Some(cache) => {
                cache.run_pending_tasks();
                (true, cache.entry_count())
            }
            None => (false, 0),
        };
        Some(AdmissionPolicyStats {
            cache_enabled: enabled,
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.cache_misses.load(Ordering::Relaxed),
            cache_inserts: self.cache_inserts.load(Ordering::Relaxed),
            cache_invalidations: self.cache_invalidations.load(Ordering::Relaxed),
            cache_entries: entries,
            cache_capacity: self.cache_capacity,
            cache_ttl: self.cache_ttl,
        })
    }
}

impl DefaultAdmissionPolicy {
    fn cache_reject(&self, path: &Path, reason: RejectReason) {
        if let Some(cache) = &self.cache {
            cache.insert(path.to_path_buf(), reason);
            self.cache_inserts.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn cache_clear(&self, path: &Path) {
        if let Some(cache) = &self.cache {
            cache.invalidate(path);
            self.cache_invalidations.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdmissionPolicyStats {
    pub cache_enabled: bool,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_inserts: u64,
    pub cache_invalidations: u64,
    pub cache_entries: u64,
    pub cache_capacity: usize,
    pub cache_ttl: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MapSegment;
    use proptest::prelude::*;
    use std::path::PathBuf;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn prefix_matching_respects_negation() {
        let prefixes = ["/usr/bin", "!/usr/bin/deny"];
        let path_allow = Path::new("/usr/bin/ok");
        let path_deny = Path::new("/usr/bin/deny/app");
        assert!(DefaultAdmissionPolicy::accept_path(path_allow, &prefixes));
        assert!(!DefaultAdmissionPolicy::accept_path(path_deny, &prefixes));
    }

    #[test]
    fn decision_rejects_small() {
        let config = Config::default();
        let policy = DefaultAdmissionPolicy::new(&config);
        let mut exe = CandidateExe::new(PathBuf::from("/usr/bin/app"), 1);
        exe.maps.push(MapSegment::new("/usr/lib/lib.so", 0, 1, 0));
        exe.total_size = 1;
        let decision = policy.decide(&exe);
        assert!(matches!(
            decision,
            AdmissionDecision::Reject {
                reason: RejectReason::TooSmall
            }
        ));
    }

    #[test]
    fn decision_rejects_map_prefix_when_all_maps_denied() {
        let config = Config::default();
        let policy = DefaultAdmissionPolicy::new(&config);
        let mut exe = CandidateExe::new(PathBuf::from("/usr/bin/app"), 1);
        exe.rejected_maps.push(PathBuf::from("/opt/secret.so"));
        let decision = policy.decide(&exe);
        assert!(matches!(
            decision,
            AdmissionDecision::Reject {
                reason: RejectReason::MapPrefixDenied
            }
        ));
    }

    #[test]
    fn policy_cache_ttl_expires_entries() {
        let mut config = Config::default();
        config.model.minsize = 100;
        config.system.exeprefix = vec!["!/".into(), "/tmp/".into()];
        config.system.mapprefix = vec!["!/".into(), "/tmp/".into()];
        config.system.policy_cache_ttl = Duration::from_millis(50);
        config.system.policy_cache_capacity = 8;

        let policy = DefaultAdmissionPolicy::new(&config);
        let mut exe = CandidateExe::new(PathBuf::from("/tmp/app"), 1);
        exe.maps.push(MapSegment::new("/tmp/lib.so", 0, 1, 0));
        exe.total_size = 1;

        let decision = policy.decide(&exe);
        assert!(matches!(
            decision,
            AdmissionDecision::Reject {
                reason: RejectReason::TooSmall
            }
        ));

        exe.total_size = 200;
        let decision = policy.decide(&exe);
        assert!(matches!(
            decision,
            AdmissionDecision::Reject {
                reason: RejectReason::TooSmall
            }
        ));

        sleep(Duration::from_millis(80));
        let decision = policy.decide(&exe);
        assert!(matches!(
            decision,
            AdmissionDecision::Accept {
                completeness: Completeness::Full
            }
        ));
    }

    #[test]
    fn policy_cache_respects_lru_eviction() {
        let mut config = Config::default();
        config.model.minsize = 100;
        config.system.exeprefix = vec!["!/".into(), "/tmp/".into()];
        config.system.mapprefix = vec!["!/".into(), "/tmp/".into()];
        config.system.policy_cache_ttl = Duration::from_secs(60);
        config.system.policy_cache_capacity = 1;

        let policy = DefaultAdmissionPolicy::new(&config);
        let mut exe_a = CandidateExe::new(PathBuf::from("/tmp/a"), 1);
        exe_a.maps.push(MapSegment::new("/tmp/a.so", 0, 1, 0));
        exe_a.total_size = 1;

        let mut exe_b = CandidateExe::new(PathBuf::from("/tmp/b"), 1);
        exe_b.maps.push(MapSegment::new("/tmp/b.so", 0, 1, 0));
        exe_b.total_size = 1;

        let decision = policy.decide(&exe_a);
        assert!(matches!(
            decision,
            AdmissionDecision::Reject {
                reason: RejectReason::TooSmall
            }
        ));

        let decision = policy.decide(&exe_b);
        assert!(matches!(
            decision,
            AdmissionDecision::Reject {
                reason: RejectReason::TooSmall
            }
        ));

        if let Some(cache) = &policy.cache {
            cache.run_pending_tasks();
            assert!(cache.get(&exe_b.path).is_some());
            assert!(cache.get(&exe_a.path).is_none());
        }

        exe_a.total_size = 200;
        let decision = policy.decide(&exe_a);
        assert!(matches!(
            decision,
            AdmissionDecision::Accept {
                completeness: Completeness::Full
            }
        ));
    }

    #[test]
    fn policy_cache_stats_track_hits_and_misses() {
        let mut config = Config::default();
        config.model.minsize = 100;
        config.system.exeprefix = vec!["!/".into(), "/tmp/".into()];
        config.system.mapprefix = vec!["!/".into(), "/tmp/".into()];
        config.system.policy_cache_ttl = Duration::from_secs(60);
        config.system.policy_cache_capacity = 8;

        let policy = DefaultAdmissionPolicy::new(&config);
        let mut exe = CandidateExe::new(PathBuf::from("/tmp/app"), 1);
        exe.maps.push(MapSegment::new("/tmp/lib.so", 0, 1, 0));
        exe.total_size = 1;

        let _ = policy.decide(&exe);
        let _ = policy.decide(&exe);

        let stats = policy.stats().expect("stats");
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
        assert!(stats.cache_inserts >= 1);
        assert!(stats.cache_entries >= 1);
    }

    proptest! {
        #[test]
        fn accept_path_matches_reference(
            prefixes in prop::collection::vec(prefix_strategy(), 0..10),
            path in path_strategy(),
        ) {
            let expected = reference_accept_path(&path, &prefixes);
            let actual = DefaultAdmissionPolicy::accept_path(Path::new(&path), &prefixes);
            prop_assert_eq!(actual, expected);
        }
    }

    fn path_strategy() -> impl Strategy<Value = String> {
        prop::collection::vec(segment_strategy(), 1..6)
            .prop_map(|segments| format!("/{}", segments.join("/")))
    }

    fn prefix_strategy() -> impl Strategy<Value = String> {
        (
            any::<bool>(),
            prop::collection::vec(segment_strategy(), 1..6),
        )
            .prop_map(|(negate, segments)| {
                let prefix = format!("/{}", segments.join("/"));
                if negate { format!("!{prefix}") } else { prefix }
            })
    }

    fn segment_strategy() -> impl Strategy<Value = String> {
        prop::collection::vec(97u8..=122, 1..8)
            .prop_map(|bytes| bytes.into_iter().map(|b| b as char).collect())
    }

    fn reference_accept_path(path: &str, prefixes: &[String]) -> bool {
        let mut best: Option<(bool, usize)> = None;
        for prefix in prefixes {
            let (neg, raw) = prefix
                .strip_prefix('!')
                .map(|p| (true, p))
                .unwrap_or((false, prefix.as_str()));
            if path.starts_with(raw) {
                let len = raw.len();
                if best.map(|(_, l)| l).unwrap_or(0) < len {
                    best = Some((!neg, len));
                }
            }
        }
        best.map(|(accept, _)| accept).unwrap_or(true)
    }
}
