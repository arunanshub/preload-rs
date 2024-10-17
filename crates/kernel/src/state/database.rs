#![allow(clippy::mutable_key_type)]

use super::inner::StateInner;
use crate::{database::DatabaseWriteExt, exe::database::write_bad_exe, Error};
use sqlx::SqlitePool;
use tracing::trace;

#[async_trait::async_trait]
impl DatabaseWriteExt for StateInner {
    type Error = Error;

    #[tracing::instrument(skip_all)]
    async fn write(&self, pool: &SqlitePool) -> Result<u64, Self::Error> {
        let mut joinset = tokio::task::JoinSet::new();

        trace!("Writing maps");
        for map in &self.maps {
            let map = map.clone();
            let pool = pool.clone();
            joinset.spawn(async move { map.write(&pool).await });
        }

        trace!("Writing badexes");
        for (path, &size) in &self.bad_exes {
            let pool = pool.clone();
            let path = path.to_path_buf();
            joinset.spawn(async move { write_bad_exe(path, size, &pool).await });
        }

        trace!("Writing exes");
        // NOTE: we write the exes first because they have a foreign key
        // reference to the exemaps and markovs
        for exe in self.exes.values() {
            let exe = exe.clone();
            let pool = pool.clone();
            joinset.spawn(async move { exe.write(&pool).await });
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
                joinset.spawn(async move { exemap.write(&pool).await });
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
                joinset.spawn(async move { markov.write(&pool).await });
            });
            exe.0.lock().markovs = markovs;
        });

        trace!("Waiting for exemaps and markovs to finish writing");
        while let Some(res) = joinset.join_next().await {
            res??;
        }
        trace!("Finished writing");

        Ok(1)
    }
}
