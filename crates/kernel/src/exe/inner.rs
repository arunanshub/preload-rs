use crate::{Error, ExeMap, Markov};
use educe::Educe;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use std::{collections::HashSet, path::PathBuf};

#[derive(Default, Clone, Educe)]
#[educe(Debug)]
pub struct ExeInner {
    pub path: PathBuf,

    #[educe(Debug(ignore))]
    pub exemaps: HashSet<ExeMap>,

    pub size: u64,

    pub seq: Option<u64>,

    pub time: u64,

    pub update_time: Option<u64>,

    pub running_timestamp: Option<u64>,

    pub change_timestamp: u64,

    pub lnprob: f32,

    pub markovs: Vec<Markov>,
}

impl ExeInner {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            ..Default::default()
        }
    }

    pub fn with_change_timestamp(&mut self, change_timestamp: u64) -> &mut Self {
        self.change_timestamp = change_timestamp;
        self
    }

    pub fn with_running(&mut self, last_running_timestamp: u64) -> &mut Self {
        self.update_time.replace(last_running_timestamp);
        self.running_timestamp.replace(last_running_timestamp);
        self
    }

    pub fn try_with_exemaps(&mut self, exemaps: HashSet<ExeMap>) -> Result<&mut Self, Error> {
        let exe_seq = self
            .seq
            .ok_or_else(|| Error::ExeSeqNotAssigned(self.path.clone()))?;

        // NOTE: why are we doing this? This is because, during the `write to
        // the db` phase, exemap needs to know the exe_seq that it is related
        // to. Ideally we could have given exemap a weakref to exe, but that
        // would have made the code more complex. Maybe that's a todo for some
        // other time.
        self.exemaps = exemaps
            .into_par_iter()
            .map(|v| v.with_exe_seq(exe_seq))
            .collect();
        let size = self
            .exemaps
            .par_iter()
            .map(|map| map.map.length())
            .reduce(|| 0, |acc, x| acc.wrapping_add(x));
        self.size = self.size.wrapping_add(size);
        Ok(self)
    }

    pub const fn is_running(&self, last_running_timestamp: u64) -> bool {
        if let Some(running_timestamp) = self.running_timestamp {
            running_timestamp >= last_running_timestamp
        } else {
            0 == last_running_timestamp
        }
    }

    pub fn bid_in_maps(&self, last_running_timestamp: u64) {
        if self.is_running(last_running_timestamp) {
            self.exemaps
                .par_iter()
                .for_each(|v| v.map.increase_lnprob(1.));
        } else {
            self.exemaps
                .par_iter()
                .for_each(|v| v.map.set_lnprob(self.lnprob));
        }
    }
}
