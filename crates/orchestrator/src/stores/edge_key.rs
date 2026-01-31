#![forbid(unsafe_code)]

use crate::domain::ExeId;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeKey(pub ExeId, pub ExeId);

impl EdgeKey {
    pub fn new(a: ExeId, b: ExeId) -> Self {
        if a < b { EdgeKey(a, b) } else { EdgeKey(b, a) }
    }

    pub fn a(self) -> ExeId {
        self.0
    }

    pub fn b(self) -> ExeId {
        self.1
    }
}

impl Hash for EdgeKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        self.1.hash(state);
    }
}
