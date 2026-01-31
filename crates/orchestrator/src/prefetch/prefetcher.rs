#![forbid(unsafe_code)]

use crate::prefetch::{PrefetchPlan, PrefetchReport};
use crate::stores::Stores;
use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use nix::fcntl::PosixFadviseAdvice;
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
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
            &file,
            offset,
            length,
            PosixFadviseAdvice::POSIX_FADV_WILLNEED,
        )
        .map_err(std::io::Error::other)?;
        Ok(())
    }
}

#[async_trait]
impl Prefetcher for PosixFadvisePrefetcher {
    async fn execute(&self, plan: &PrefetchPlan, stores: &Stores) -> PrefetchReport {
        let mut report = PrefetchReport::default();

        let concurrency = self.concurrency.max(1);
        let tasks: Vec<(crate::domain::MapKey, std::path::PathBuf, i64, i64)> = plan
            .maps
            .iter()
            .filter_map(|map_id| {
                let map = stores.maps.get(*map_id)?;
                Some((
                    map.key(),
                    map.path.clone(),
                    map.offset as i64,
                    map.length as i64,
                ))
            })
            .collect();

        let mut stream = stream::iter(tasks).map(|(map_key, path, offset, length)| async move {
            let join =
                tokio::task::spawn_blocking(move || Self::readahead(&path, offset, length)).await;
            match join {
                Ok(result) => (map_key, result),
                Err(err) => {
                    let err = std::io::Error::other(err);
                    (map_key, Err(err))
                }
            }
        });

        while let Some((map_key, result)) =
            stream.by_ref().buffer_unordered(concurrency).next().await
        {
            match result {
                Ok(()) => report.num_maps += 1,
                Err(err) => {
                    warn!(?map_key, %err, "prefetch failed");
                    report.failures.push(map_key);
                }
            }
        }

        report.total_bytes = plan.total_bytes;
        report
    }
}
