use crate::Error;
use educe::Educe;
use parking_lot::Mutex;
use std::{os::linux::fs::MetadataExt, path::PathBuf};

/// Runtime statistics related to the map.
#[derive(Debug, Default, Clone, Copy)]
pub struct RuntimeStats {
    /// Log probability of the map NOT being needed in the next period.
    pub lnprob: f32,

    /// Unique map sequence number.
    pub seq: Option<u64>,

    /// On-disk location of the start of the map.
    pub block: Option<u64>,
    // private: u64,
}

#[derive(Debug, Default, Educe)]
#[educe(Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct MapInner {
    /// Absolute path to the mapped file.
    pub path: PathBuf,

    /// Offset of the mapped section in bytes.
    pub offset: u64,

    /// Length of the mapped section in bytes.
    pub length: u64,

    /// Last time the map was probed.
    pub update_time: u64,

    /// Runtime statistics related to the map.
    #[educe(Eq(ignore), Ord(ignore), Hash(ignore))]
    pub runtime: Mutex<RuntimeStats>,
}

impl MapInner {
    pub fn new(path: impl Into<PathBuf>, offset: u64, length: u64, update_time: u64) -> Self {
        Self {
            path: path.into(),
            length,
            offset,
            update_time,
            ..Default::default()
        }
    }

    pub fn set_block(&self, use_inode: bool) -> Result<(), Error> {
        use std::os::unix::io::AsRawFd;

        // in case we can get block, set 0 to not retry
        self.runtime.lock().block = Some(0);

        let file = std::fs::File::open(&self.path)?;
        let meta = file.metadata()?;

        #[cfg(feature = "fiemap")]
        {
            // TODO: if (!use_inode) { ... }
        }

        let block = if use_inode {
            meta.st_ino()
        } else {
            #[cfg(target_os = "linux")]
            {
                const FIBMAP: libc::c_ulong = 1;
                let mut block = (self.offset / meta.st_blksize() as u64) as libc::c_long;
                unsafe {
                    if libc::ioctl(file.as_raw_fd(), FIBMAP, &mut block) < 0 {
                        meta.st_ino()
                    } else {
                        block as u64
                    }
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                meta.st_ino()
            }
        };

        self.runtime.lock().block = Some(block);
        Ok(())
    }
}
