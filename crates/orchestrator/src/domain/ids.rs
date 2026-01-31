#![forbid(unsafe_code)]

use slotmap::new_key_type;
use std::{fmt, path::PathBuf};

new_key_type! { pub struct ExeId; }
new_key_type! { pub struct MapId; }

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExeKey(PathBuf);

impl ExeKey {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn path(&self) -> &PathBuf {
        &self.0
    }
}

impl fmt::Debug for ExeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ExeKey").field(&self.0).finish()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MapKey {
    pub path: PathBuf,
    pub offset: u64,
    pub length: u64,
}

impl MapKey {
    pub fn new(path: impl Into<PathBuf>, offset: u64, length: u64) -> Self {
        Self {
            path: path.into(),
            offset,
            length,
        }
    }
}

impl fmt::Debug for MapKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MapKey")
            .field("path", &self.path)
            .field("offset", &self.offset)
            .field("length", &self.length)
            .finish()
    }
}
