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
}
