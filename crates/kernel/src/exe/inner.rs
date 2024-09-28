#![allow(clippy::mutable_key_type)]

use crate::ExeMap;
use educe::Educe;
use std::{collections::HashSet, path::PathBuf};

#[derive(Default, Clone, Educe)]
#[educe(Debug)]
pub struct ExeInner {
    pub path: PathBuf,

    #[educe(Debug(ignore))]
    pub exemaps: HashSet<ExeMap>,

    pub size: usize,

    pub seq: u64,

    pub time: u64,

    pub update_time: Option<u64>,

    pub running_timestamp: Option<u64>,

    pub change_timestamp: u64,
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

    pub fn with_exemaps(&mut self, exemaps: HashSet<ExeMap>) -> &mut Self {
        self.exemaps = exemaps;
        let size: usize = self.exemaps.iter().map(|m| m.map.length()).sum();
        self.size += size;
        self
    }

    pub const fn is_running(&self, last_running_timestamp: u64) -> bool {
        if let Some(running_timestamp) = self.running_timestamp {
            running_timestamp >= last_running_timestamp
        } else {
            0 == last_running_timestamp
        }
    }
}
