pub(crate) mod database;
mod inner;

use crate::Error;
use inner::MapInner;
pub use inner::RuntimeStats;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Default, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Map {
    inner: Arc<MapInner>,
}

impl Map {
    pub fn new(path: impl Into<PathBuf>, offset: u64, length: u64, update_time: u64) -> Self {
        Self {
            inner: Arc::new(MapInner::new(path, offset, length, update_time)),
        }
    }

    pub fn lnprob(&self) -> f32 {
        self.inner.runtime.lock().lnprob
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    pub fn seq(&self) -> Option<u64> {
        self.inner.runtime.lock().seq
    }

    pub fn update_time(&self) -> u64 {
        self.inner.update_time
    }

    pub fn block(&self) -> Option<u64> {
        self.inner.runtime.lock().block
    }

    pub fn length(&self) -> u64 {
        self.inner.length
    }

    pub fn offset(&self) -> u64 {
        self.inner.offset
    }

    pub fn set_seq(&self, seq: u64) {
        self.inner.runtime.lock().seq.replace(seq);
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

    pub fn set_block(&self) -> Result<(), Error> {
        self.inner.set_block()
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
            offset in 0..=u64::MAX,
            length in 0..=u64::MAX,
            update_time in 0..=u64::MAX,
            lnprob: f32,
            seq in 0..=u64::MAX,
        ) -> Map {
            let map = Map::new(path, offset, length, update_time);
            map.set_lnprob(lnprob);
            map.set_seq(seq);
            map
        }
    }

    proptest! {
        #[test]
        fn map_is_sortable(mut map in vec(arbitrary_map(), 1..3000)) {
            map.sort();
            map.chunks_exact(2).for_each(|map_l_r| {
                let map_left = &map_l_r[0];
                let map_right = &map_l_r[1];
                assert!(map_left < map_right);
            });
        }
    }
}
