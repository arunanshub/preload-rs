#![allow(clippy::mutable_key_type)]

use super::inner::StateInner;
use crate::{database::DatabaseWriteExt, exe::database::write_bad_exe, Error};
use sqlx::SqlitePool;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tracing::trace;

#[async_trait::async_trait]
impl DatabaseWriteExt for StateInner {
    type Error = Error;

    #[tracing::instrument(skip_all)]
    async fn write(&self, pool: &SqlitePool) -> Result<u64, Self::Error> {
        let mut joinset = tokio::task::JoinSet::new();
        // number of database operations performed
        let num_ops = Arc::new(AtomicU64::new(0));

        trace!("Writing maps");
        for map in &self.maps {
            let map = map.clone();
            let pool = pool.clone();
            let num_ops = num_ops.clone();

            joinset.spawn(async move {
                match map.write(&pool).await {
                    Ok(num) => {
                        num_ops.fetch_add(num, Ordering::Relaxed);
                        Ok(num)
                    }
                    Err(e) => Err(e),
                }
            });
        }

        trace!("Writing badexes");
        for (path, &size) in &self.bad_exes {
            let pool = pool.clone();
            let path = path.to_path_buf();
            let num_ops = num_ops.clone();

            joinset.spawn(async move {
                match write_bad_exe(path, size, &pool).await {
                    Ok(num) => {
                        num_ops.fetch_add(num, Ordering::Relaxed);
                        Ok(num)
                    }
                    Err(e) => Err(e),
                }
            });
        }

        trace!("Writing exes");
        // NOTE: we write the exes first because they have a foreign key
        // reference to the exemaps and markovs
        for exe in self.exes.values() {
            let exe = exe.clone();
            let pool = pool.clone();
            let num_ops = num_ops.clone();

            joinset.spawn(async move {
                match exe.write(&pool).await {
                    Ok(num) => {
                        num_ops.fetch_add(num, Ordering::Relaxed);
                        Ok(num)
                    }
                    Err(e) => Err(e),
                }
            });
        }

        trace!("Waiting for maps, badexes, and exes to finish writing");
        while let Some(res) = joinset.join_next().await {
            res??;
        }

        trace!("Writing exemaps and markovs");
        self.exes.iter().for_each(|(_, exe)| {
            // take exemaps out to prevent any deadlocks
            let exemaps = std::mem::take(&mut exe.0.lock().exemaps);
            exemaps.iter().for_each(|exemap| {
                let pool = pool.clone();
                let exemap = exemap.clone();
                let num_ops = num_ops.clone();

                joinset.spawn(async move {
                    match exemap.write(&pool).await {
                        Ok(num) => {
                            num_ops.fetch_add(num, Ordering::Relaxed);
                            Ok(num)
                        }
                        Err(e) => Err(e),
                    }
                });
            });
            // NOTE: falsely flagged as "mutable_key_type" by clippy. Only the
            // `map` of `exemap` contributes to the hashset's key, and it is
            // immutable.
            exe.0.lock().exemaps = exemaps;

            // take markovs out to prevent any deadlocks
            let markovs = std::mem::take(&mut exe.0.lock().markovs);
            markovs.iter().for_each(|markov| {
                let pool = pool.clone();
                let markov = markov.clone();
                let num_ops = num_ops.clone();

                joinset.spawn(async move {
                    match markov.write(&pool).await {
                        Ok(num) => {
                            num_ops.fetch_add(num, Ordering::Relaxed);
                            Ok(num)
                        }
                        Err(e) => Err(e),
                    }
                });
            });
            exe.0.lock().markovs = markovs;
        });

        trace!("Waiting for exemaps and markovs to finish writing");
        while let Some(res) = joinset.join_next().await {
            res??;
        }
        trace!("Finished writing");

        Ok(num_ops.load(Ordering::Relaxed))
    }
}
