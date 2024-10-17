#![allow(clippy::mutable_key_type)]

pub(crate) mod database;
mod inner;

use crate::{extract_exe, Error, ExeMap, Markov};
use inner::ExeInner;
use parking_lot::Mutex;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Weak},
};

#[derive(Debug, Default, Clone)]
pub struct Exe(pub(crate) Arc<Mutex<ExeInner>>);

#[derive(Debug, Default, Clone)]
pub struct ExeForMarkov(pub(crate) Weak<Mutex<ExeInner>>);

impl Exe {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(Arc::new(Mutex::new(ExeInner::new(path))))
    }

    /// Sequence number of the Exe assigned by [`State`](crate::State) during
    /// runtime.
    ///
    /// By default it is zero.
    pub fn seq(&self) -> Option<u64> {
        self.0.lock().seq
    }

    pub(crate) fn for_markov(&self) -> ExeForMarkov {
        ExeForMarkov(Arc::downgrade(&self.0))
    }

    pub fn build_markov_chain_with(
        &self,
        other_exe: &Exe,
        state_time: u64,
        last_running_timestamp: u64,
    ) -> Result<Option<Markov>, Error> {
        if self.path() == other_exe.path() {
            return Ok(None);
        }
        let markov = Markov::new(self.for_markov(), other_exe.for_markov())
            .with_initialize(state_time, last_running_timestamp)?;
        Ok(Some(markov))
    }

    pub fn markov_bid_in_exes(
        &self,
        use_correlation: bool,
        state_time: u64,
        cycle: f32,
    ) -> Result<(), Error> {
        let markovs = std::mem::take(&mut self.0.lock().markovs);
        let path = self.path();
        let res = markovs.iter().try_for_each(|markov| {
            if extract_exe!(markov.0.lock().exe_a).path == path {
                markov.bid_in_exes(use_correlation, state_time, cycle)?;
            }
            Ok(())
        });
        self.0.lock().markovs = markovs;
        res
    }

    pub fn markov_state_changed(
        &self,
        state_time: u64,
        last_running_timestamp: u64,
    ) -> Result<(), Error> {
        // extract the markovs from the exe because markov might lock the exe
        // back this is to prevent a deadlock
        let markovs = std::mem::take(&mut self.0.lock().markovs);
        let res = markovs
            .iter()
            .try_for_each(|markov| markov.state_changed(state_time, last_running_timestamp));
        self.0.lock().markovs = markovs;
        res
    }

    pub fn increase_markov_time(&self, time: u64) -> Result<(), Error> {
        // same as markov_state_changed. Take them out to prevent deadlock
        let markovs = std::mem::take(&mut self.0.lock().markovs);
        let path = self.path();
        let res = markovs.iter().try_for_each(|markov| {
            if extract_exe!(markov.0.lock().exe_a).path == path {
                markov.increase_time(time);
            }
            Ok(())
        });
        self.0.lock().markovs = markovs;
        res
    }

    pub fn change_timestamp(&self) -> u64 {
        self.0.lock().change_timestamp
    }

    pub fn add_markov(&self, markov: Markov) {
        self.0.lock().markovs.push(markov);
    }

    pub fn with_change_timestamp(self, change_timestamp: u64) -> Self {
        self.0.lock().with_change_timestamp(change_timestamp);
        self
    }

    pub fn with_running(self, last_running_timestamp: u64) -> Self {
        self.0.lock().with_running(last_running_timestamp);
        self
    }

    pub fn try_with_exemaps(self, exemaps: HashSet<ExeMap>) -> Result<Self, Error> {
        self.0.lock().try_with_exemaps(exemaps)?;
        Ok(self)
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

    /// Set the sequence number of the Exe.
    ///
    /// This is called by [`State`](crate::State) during runtime.
    pub fn set_seq(&self, seq: u64) {
        self.0.lock().seq.replace(seq);
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
            offset in 0..=u64::MAX,
            length in 0..=u64::MAX,
            update_time in 0..=u64::MAX,
        ) -> Map {
            Map::new(path, offset, length, update_time)
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
            let exe = Exe::new("foo");
            exe.set_seq(1);
            let exe = exe.try_with_exemaps(exemaps).unwrap();
            let exe_size = exe.size();

            assert_eq!(exe_size, map_sizes);
        }
    }
}
