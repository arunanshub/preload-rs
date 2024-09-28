mod inner;

use inner::MapInner;
pub use inner::RuntimeStats;
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Default, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Map {
    inner: Arc<MapInner>,
}

impl Map {
    pub fn new(path: impl Into<PathBuf>, offset: usize, length: usize) -> Self {
        Self {
            inner: Arc::new(MapInner::new(path, offset, length)),
        }
    }

    pub fn lnprob(&self) -> f32 {
        self.inner.runtime.lock().lnprob
    }

    pub fn seq(&self) -> u64 {
        self.inner.runtime.lock().seq
    }

    pub fn block(&self) -> u64 {
        self.inner.runtime.lock().block
    }

    pub fn length(&self) -> usize {
        self.inner.length
    }

    pub fn set_seq(&self, seq: u64) {
        self.inner.runtime.lock().seq = seq;
    }
}
