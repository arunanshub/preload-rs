#![allow(clippy::mutable_key_type)]

mod inner;

use crate::ExeMap;
use inner::ExeInner;
use parking_lot::Mutex;
use std::{collections::HashSet, path::PathBuf, sync::Arc};

#[derive(Debug, Default, Clone)]
pub struct Exe(Arc<Mutex<ExeInner>>);

impl Exe {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(Arc::new(Mutex::new(ExeInner::new(path))))
    }
    pub fn with_change_timestamp(self, change_timestamp: u64) -> Self {
        self.0.lock().with_change_timestamp(change_timestamp);
        self
    }

    pub fn with_running(self, last_running_timestamp: u64) -> Self {
        self.0.lock().with_running(last_running_timestamp);
        self
    }

    pub fn with_exemaps(self, exemaps: HashSet<ExeMap>) -> Self {
        self.0.lock().with_exemaps(exemaps);
        self
    }

    pub fn path(&self) -> PathBuf {
        self.0.lock().path.clone()
    }

    pub fn lnprob(&self) -> f32 {
        self.0.lock().lnprob
    }

    pub fn zero_lnprob(&self) {
        self.0.lock().lnprob = 0.0;
    }

    pub fn size(&self) -> u64 {
        self.0.lock().size
    }

    pub fn is_running(&self, last_running_timestamp: u64) -> bool {
        self.0.lock().is_running(last_running_timestamp)
    }

    pub fn update_running_timestamp(&self, running_timestamp: u64) {
        self.0.lock().running_timestamp.replace(running_timestamp);
    }

    pub fn update_change_timestamp(&self, change_timestamp: u64) {
        self.0.lock().change_timestamp = change_timestamp;
    }

    pub fn update_time(&self, time: u64) {
        self.0.lock().time = time;
    }

    pub fn set_seq(&self, seq: u64) {
        self.0.lock().seq = seq;
    }

    pub fn bid_in_maps(&self, last_running_timestamp: u64) {
        self.0.lock().bid_in_maps(last_running_timestamp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExeMap, Map};
    use pretty_assertions::assert_eq;
    use prop::collection::hash_set;
    use proptest::prelude::*;

    prop_compose! {
        fn arbitrary_map()(
            path in ".*",
            offset in 0..u64::MAX,
            length in 0..u64::MAX,
        ) -> Map {
            Map::new(path, offset, length)
        }
    }

    prop_compose! {
        // create arbitrary ExeMap from arbitrary Map
        fn arbitrary_exemap()(map in arbitrary_map()) -> ExeMap {
            ExeMap::new(map)
        }
    }

    proptest! {
        #[test]
        fn exe_sums_map_sizes(exemaps in hash_set(arbitrary_exemap(), 0..2000)) {
            let map_sizes: u64 = exemaps
                .iter()
                .map(|m| m.map.length())
                .fold(0, |acc, x| acc.wrapping_add(x));
            let exe = Exe::new("foo").with_exemaps(exemaps);
            let exe_size = exe.size();

            assert_eq!(exe_size, map_sizes);
        }
    }
}
