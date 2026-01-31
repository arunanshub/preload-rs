#![forbid(unsafe_code)]

use super::ExeKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exe {
    pub key: ExeKey,
    pub total_running_time: u64,
    pub last_seen_time: Option<u64>,
    pub running: bool,
    pub change_time: u64,
}

impl Exe {
    pub fn new(key: ExeKey) -> Self {
        Self {
            key,
            total_running_time: 0,
            last_seen_time: None,
            running: false,
            change_time: 0,
        }
    }
}
