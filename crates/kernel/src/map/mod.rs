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

    pub fn block(&self) -> Option<u64> {
        self.inner.runtime.lock().block
    }

    pub fn length(&self) -> usize {
        self.inner.length
    }

    pub fn set_seq(&self, seq: u64) {
        self.inner.runtime.lock().seq = seq;
    }

    pub fn zero_lnprob(&self) {
        self.inner.runtime.lock().lnprob = 0.0;
    }

    pub fn increase_lnprob(&self, lnprob: f32) {
        self.inner.runtime.lock().lnprob += lnprob;
    }

    pub fn set_lnprob(&self, lnprob: f32) {
        self.inner.runtime.lock().lnprob = lnprob;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prop::collection::vec;
    use proptest::prelude::*;

    prop_compose! {
        fn arbitrary_map()(
            path in ".*",
            offset in 0..usize::MAX,
            length in 0..usize::MAX,
            lnprob: f32,
            seq in 0..u64::MAX,
        ) -> Map {
            let map = Map::new(path, offset, length);
            map.set_lnprob(lnprob);
            map.set_seq(seq);
            map
        }
    }

    proptest! {
        #[test]
        fn map_is_sortable(mut map in vec(arbitrary_map(), 1..3000)) {
            map.sort();
            for map_l_r in map.chunks_exact(2) {
                let map_left = &map_l_r[0];
                let map_right = &map_l_r[1];
                assert!(map_left < map_right);
            }
        }
    }
}
