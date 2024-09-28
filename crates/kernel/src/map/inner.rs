use educe::Educe;
use parking_lot::Mutex;
use std::path::PathBuf;

/// Runtime statistics related to the map.
#[derive(Debug, Default, Clone, Copy)]
pub struct RuntimeStats {
    /// Log probability of the map NOT being needed in the next period.
    pub lnprob: f32,

    /// Unique map sequence number.
    pub seq: u64,

    /// On-disk location of the start of the map.
    pub block: u64,
    // private: u64,
}

#[derive(Debug, Default, Educe)]
#[educe(Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct MapInner {
    /// Absolute path to the mapped file.
    pub path: PathBuf,

    /// Offset of the mapped section in bytes.
    pub offset: usize,

    /// Length of the mapped section in bytes.
    pub length: usize,

    /// Runtime statistics related to the map.
    #[educe(Eq(ignore), Ord(ignore), Hash(ignore))]
    pub runtime: Mutex<RuntimeStats>,
}

impl MapInner {
    pub fn new(path: impl Into<PathBuf>, offset: usize, length: usize) -> Self {
        Self {
            path: path.into(),
            length,
            offset,
            ..Default::default()
        }
    }
}
