#![forbid(unsafe_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemStat {
    pub total: u64,
    pub free: u64,
    pub cached: u64,
    pub pagein: i64,
    pub pageout: i64,
}
