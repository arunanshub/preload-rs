#![forbid(unsafe_code)]

use crate::prefetch::{PrefetchPlan, PrefetchReport};
use crate::stores::Stores;
use async_trait::async_trait;
use nix::fcntl::PosixFadviseAdvice;
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::warn;

#[async_trait]
pub trait Prefetcher: Send + Sync {
    /// Execute the prefetch plan (side effects only).
    async fn execute(&self, plan: &PrefetchPlan, stores: &Stores) -> PrefetchReport;
}

#[derive(Debug, Default)]
pub struct NoopPrefetcher;

#[async_trait]
impl Prefetcher for NoopPrefetcher {
    async fn execute(&self, _plan: &PrefetchPlan, _stores: &Stores) -> PrefetchReport {
        PrefetchReport::default()
    }
}

#[derive(Debug, Clone)]
pub struct PosixFadvisePrefetcher {
    concurrency: usize,
}

impl PosixFadvisePrefetcher {
    pub fn new(concurrency: usize) -> Self {
        Self { concurrency }
    }

    fn readahead(path: &std::path::Path, offset: i64, length: i64) -> Result<(), std::io::Error> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOCTTY | libc::O_NOATIME)
            .open(path)?;
        nix::fcntl::posix_fadvise(
            file.as_raw_fd(),
            offset,
            length,
            PosixFadviseAdvice::POSIX_FADV_WILLNEED,
        )
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
        Ok(())
    }
}

#[async_trait]
impl Prefetcher for PosixFadvisePrefetcher {
    async fn execute(&self, plan: &PrefetchPlan, stores: &Stores) -> PrefetchReport {
        let mut report = PrefetchReport::default();
        let semaphore = Arc::new(Semaphore::new(self.concurrency.max(1)));
        let mut handles = Vec::new();

        for map_id in &plan.maps {
            let Some(map) = stores.maps.get(*map_id) else {
                continue;
            };
            let permit = match semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(err) => {
                    warn!(%err, "prefetch semaphore closed");
                    report.failures.push(map.key());
                    continue;
                }
            };
            let path = map.path.clone();
            let offset = map.offset as i64;
            let length = map.length as i64;
            let map_key = map.key();

            let handle = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                (map_key, Self::readahead(&path, offset, length))
            });
            handles.push(handle);
        }

        for handle in handles {
            match handle.await {
                Ok((_map_key, Ok(()))) => {
                    report.num_maps += 1;
                }
                Ok((map_key, Err(err))) => {
                    warn!(?map_key, %err, "prefetch failed");
                    report.failures.push(map_key);
                }
                Err(err) => {
                    warn!(%err, "prefetch task join failed");
                }
            }
        }

        report.total_bytes = plan.total_bytes;
        report
    }
}
