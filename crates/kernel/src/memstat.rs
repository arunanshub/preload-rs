use crate::Error;
use procfs::Current;

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
        let mem_info = procfs::Meminfo::current()?;
        let vm_info = procfs::vmstat()?;

        let mut pagein = *vm_info
            .get("pgpgin")
            .ok_or(Error::ProcfsFieldDoesNotExist("pgpgin".into()))?;
        let mut pageout = *vm_info
            .get("pgpgout")
            .ok_or(Error::ProcfsFieldDoesNotExist("pgpgout".into()))?;
        let pagesize = procfs::page_size();

        pagein *= pagesize as i64 / 1024;
        pageout *= pagesize as i64 / 1024;

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
