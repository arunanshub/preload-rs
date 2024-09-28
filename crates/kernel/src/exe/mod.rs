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
