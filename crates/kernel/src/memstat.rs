use crate::Error;
use procfs::{Current, Meminfo, page_size, vmstat};

#[derive(Debug, Clone, Copy)]
pub struct MemStat {
    pub total: u64,
    pub free: u64,
    pub buffers: u64,
    pub cached: u64,
    pub pagein: i64,
    pub pageout: i64,
}

impl MemStat {
    pub fn try_new() -> Result<Self, Error> {
        let mem_info = Meminfo::current()?;
        let vm_info = vmstat()?;

        let page_size = page_size();
        let pagein = vm_info
            .get("pgpgin")
            .map(|val| val * page_size as i64 / 1024)
            .ok_or(Error::ProcfsFieldDoesNotExist("pgpgin".into()))?;
        let pageout = vm_info
            .get("pgpgout")
            .map(|val| val * page_size as i64 / 1024)
            .ok_or(Error::ProcfsFieldDoesNotExist("pgpgout".into()))?;

        Ok(Self {
            total: mem_info.mem_total,
            free: mem_info.mem_free,
            buffers: mem_info.buffers,
            cached: mem_info.cached,
            pagein,
            pageout,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memstat() {
        let memstat = MemStat::try_new().unwrap();
        assert!(memstat.total > 0);
        assert!(memstat.free > 0);
        assert!(memstat.buffers > 0);
        assert!(memstat.cached > 0);
        assert!(memstat.pagein >= 0);
        assert!(memstat.pageout >= 0);
    }
}
