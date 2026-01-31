#![forbid(unsafe_code)]

use crate::observation::CandidateExe;
use config::Config;
use std::path::Path;

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
}

#[derive(Debug, Clone)]
pub struct DefaultAdmissionPolicy {
    minsize: u64,
    exeprefix: Vec<String>,
    mapprefix: Vec<String>,
}

impl DefaultAdmissionPolicy {
    pub fn new(config: &Config) -> Self {
        Self {
            minsize: config.model.minsize,
            exeprefix: config.system.exeprefix.clone(),
            mapprefix: config.system.mapprefix.clone(),
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

impl AdmissionPolicy for DefaultAdmissionPolicy {
    fn allow_exe(&self, path: &Path) -> bool {
        Self::accept_path(path, &self.exeprefix)
    }

    fn allow_map(&self, path: &Path) -> bool {
        Self::accept_path(path, &self.mapprefix)
    }

    fn decide(&self, candidate: &CandidateExe) -> AdmissionDecision {
        if !self.allow_exe(&candidate.path) {
            return AdmissionDecision::Reject {
                reason: RejectReason::ExePrefixDenied,
            };
        }
        if candidate.maps.is_empty() {
            return AdmissionDecision::Reject {
                reason: RejectReason::MissingMaps,
            };
        }
        if candidate.total_size < self.minsize {
            return AdmissionDecision::Reject {
                reason: RejectReason::TooSmall,
            };
        }

        let completeness = if candidate.rejected_maps.is_empty() {
            Completeness::Full
        } else {
            Completeness::Partial
        };
        AdmissionDecision::Accept { completeness }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MapSegment;
    use proptest::prelude::*;
    use std::path::PathBuf;

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
