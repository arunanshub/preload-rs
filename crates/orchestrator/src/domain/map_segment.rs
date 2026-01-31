#![forbid(unsafe_code)]

use super::MapKey;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapSegment {
    pub path: PathBuf,
    pub offset: u64,
    pub length: u64,
    pub update_time: u64,
}

impl MapSegment {
    pub fn new(path: impl Into<PathBuf>, offset: u64, length: u64, update_time: u64) -> Self {
        Self {
            path: path.into(),
            offset,
            length,
            update_time,
        }
    }

    pub fn key(&self) -> MapKey {
        MapKey::new(self.path.clone(), self.offset, self.length)
    }
}
