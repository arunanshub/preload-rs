mod database;
mod inner;
mod markov_state;

use crate::{exe::ExeForMarkov, extract_exe, Error};
use inner::MarkovInner;
pub use markov_state::MarkovState;
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Markov(pub(crate) Arc<Mutex<MarkovInner>>);

impl Markov {
    pub fn new(exe_a: ExeForMarkov, exe_b: ExeForMarkov) -> Self {
        Self(Arc::new(Mutex::new(MarkovInner::new(exe_a, exe_b))))
    }

    pub fn with_initialize(
        self,
        state_time: u64,
        last_runnging_timestamp: u64,
    ) -> Result<Markov, Error> {
        {
            let lock = &mut self.0.lock();
            lock.with_initialize(state_time, last_runnging_timestamp)?;
            extract_exe!(lock.exe_a).markovs.push(self.clone());
            extract_exe!(lock.exe_b).markovs.push(self.clone());
        }

        Ok(self)
    }

    pub fn state_changed(&self, state_time: u64, last_running_timestamp: u64) -> Result<(), Error> {
        self.0
            .lock()
            .state_changed(state_time, last_running_timestamp)
    }

    pub fn increase_time(&self, time: u64) {
        let mut markov = self.0.lock();
        if markov.state == MarkovState::BothRunning {
            markov.time += time;
        }
    }

    pub fn bid_in_exes(
        &self,
        use_correlation: bool,
        state_time: u64,
        cycle: f32,
    ) -> Result<(), Error> {
        self.0
            .lock()
            .bid_in_exes(use_correlation, state_time, cycle)
    }
}

#[cfg(test)]
mod tests {
    use core::panic;

    use super::Markov;
    use crate::{Error, Exe};

    #[test]
    fn build_markov_with_two_exes() {
        let exe_a = Exe::new("foo");
        let exe_b = Exe::new("bar");

        exe_a
            .build_markov_chain_with(&exe_b, 1, 1)
            .unwrap()
            .unwrap();
    }

    #[test]
    #[should_panic]
    fn cannot_build_markov_with_same_exe() {
        let exe_a = Exe::new("foo");
        exe_a
            .build_markov_chain_with(&exe_a, 1, 1)
            .unwrap()
            .unwrap();
    }

    #[test]
    fn cannot_build_markov_if_exe_dropped() {
        let exe_a = Exe::new("foo");
        let exe_b = Exe::new("bar");

        let markov = Markov::new(exe_a.for_markov(), exe_b.for_markov());
        drop(exe_a);
        if let Err(err) = markov.with_initialize(1, 1) {
            assert!(matches!(err, Error::ExeDoesNotExist));
        } else {
            panic!()
        };
    }
}
