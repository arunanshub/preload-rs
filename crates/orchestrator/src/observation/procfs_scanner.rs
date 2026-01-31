#![forbid(unsafe_code)]

use crate::domain::{MapSegment, MemStat};
use crate::error::Error;
use crate::observation::{Observation, ObservationEvent, ScanWarning, Scanner};
use procfs::process::MMapPath;
use procfs::{Current, Meminfo, page_size, vmstat};
use std::path::{Path, PathBuf};
use tracing::{trace, warn};

#[derive(Debug, Default)]
pub struct ProcfsScanner;

impl ProcfsScanner {
    fn sanitize_path(path: &Path) -> Option<PathBuf> {
        if !path.has_root() {
            return None;
        }
        let path_str = path.to_str()?;
        if path_str.contains("(deleted)") {
            return None;
        }
        let trimmed = path_str.split(".#prelink#.").next()?;
        Some(PathBuf::from(trimmed))
    }

    fn read_memstat() -> Result<MemStat, Error> {
        let mem = Meminfo::current()?;
        let vm = vmstat()?;
        let page = page_size() as i64;
        let pagein = vm.get("pgpgin").map(|v| v * page / 1024).unwrap_or(0);
        let pageout = vm.get("pgpgout").map(|v| v * page / 1024).unwrap_or(0);

        Ok(MemStat {
            total: mem.mem_total,
            free: mem.mem_free,
            cached: mem.cached,
            pagein,
            pageout,
        })
    }
}

impl Scanner for ProcfsScanner {
    fn scan(&mut self, time: u64, scan_id: u64) -> Result<Observation, Error> {
        let mut events = Vec::new();
        let mut warnings = Vec::new();
        events.push(ObservationEvent::ObsBegin { time, scan_id });

        for process in procfs::process::all_processes()? {
            let process = match process {
                Ok(p) => p,
                Err(err) => {
                    warn!(?err, "failed to read process entry");
                    continue;
                }
            };
            let pid = process.pid as u32;
            let exe_path = match process.exe() {
                Ok(path) => path,
                Err(err) => {
                    warn!(pid, ?err, "failed to read exe path");
                    continue;
                }
            };
            let Some(exe_path) = Self::sanitize_path(&exe_path) else {
                continue;
            };

            events.push(ObservationEvent::ExeSeen {
                path: exe_path.clone(),
                pid,
            });

            match process.maps() {
                Ok(maps) => {
                    for map in maps {
                        let MMapPath::Path(path) = map.pathname else {
                            continue;
                        };
                        let Some(path) = Self::sanitize_path(&path) else {
                            continue;
                        };
                        let (start, end) = map.address;
                        let length = end.saturating_sub(start);
                        let segment = MapSegment::new(path, map.offset, length, time);
                        events.push(ObservationEvent::MapSeen {
                            exe_path: exe_path.clone(),
                            map: segment,
                        });
                    }
                }
                Err(err) => {
                    warnings.push(ScanWarning::MapScanFailed {
                        pid,
                        reason: err.to_string(),
                    });
                }
            }
        }

        if let Ok(mem) = Self::read_memstat() {
            events.push(ObservationEvent::MemStat { mem });
        }

        events.push(ObservationEvent::ObsEnd {
            time,
            scan_id,
            warnings,
        });

        trace!(scan_id, event_count = events.len(), "observation collected");
        Ok(events)
    }
}
