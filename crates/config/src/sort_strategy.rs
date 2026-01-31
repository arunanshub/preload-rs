#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortStrategy {
    None,
    Path,
    Block,
    Inode,
}

impl Default for SortStrategy {
    fn default() -> Self {
        SortStrategy::Block
    }
}
