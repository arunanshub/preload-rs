#![allow(clippy::mutable_key_type)]

use super::inner::StateInner;
use crate::{
    database::DatabaseWriteExt,
    exe::database::{read_bad_exes, write_bad_exe},
    Error, Exe, ExeDatabaseReadExt, ExeMap, ExeMapDatabaseReadExt, Map, MapDatabaseReadExt, Markov,
    MarkovDatabaseReadExt,
};
use sqlx::SqlitePool;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tracing::{debug, info, trace};

#[async_trait::async_trait]
impl DatabaseWriteExt for StateInner {
    type Error = Error;

    #[tracing::instrument(skip_all)]
    async fn write(&self, pool: &SqlitePool) -> Result<u64, Self::Error> {
        let mut joinset = tokio::task::JoinSet::new();
        // number of database operations performed
        let num_ops = Arc::new(AtomicU64::new(0));

        trace!("Writing state");
        {
            let mut tx = pool.begin().await?;
            let time = self.time as i64;
            let version = env!("CARGO_PKG_VERSION");
            let rows_affected = sqlx::query!(
                "
            INSERT INTO state
                (version, time)
            VALUES
                (?, ?)
            ON CONFLICT(version) DO UPDATE SET
                time = excluded.time
        ",
                version,
                time
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            tx.commit().await?;
            num_ops.fetch_add(rows_affected, Ordering::Relaxed);
        }

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
        trace!(?num_ops, "database operations performed");
        Ok(num_ops.load(Ordering::Relaxed))
    }
}

#[async_trait::async_trait]
pub trait StateDatabaseReadExt {
    /// Read the state from the database with the given SQLite pool.
    async fn read_all(&mut self, pool: &SqlitePool) -> Result<(), Error>;
}

#[async_trait::async_trait]
impl StateDatabaseReadExt for StateInner {
    async fn read_all(&mut self, pool: &SqlitePool) -> Result<(), Error> {
        let record = sqlx::query!(
            r#"
            SELECT
                version as "version: u64", time as "time: u64"
            FROM
                state
            WHERE
                version = ?
        "#,
            env!("CARGO_PKG_VERSION")
        )
        .fetch_optional(pool)
        .await?;
        let Some(record) = record else {
            info!("No state found in the database. Looks like we are starting from scratch.");
            return Ok(());
        };

        self.last_accounting_timestamp = record.time;
        self.time = record.time;

        debug!("Reading maps, exes, and bad exes from the database");
        let map_fut = tokio::spawn({
            let pool = pool.clone();
            async move { Map::read_all(&pool).await }
        });
        let exes_fut = tokio::spawn({
            let pool = pool.clone();
            async move { Exe::read_all(&pool).await }
        });
        let bad_exes_fut = tokio::spawn({
            let pool = pool.clone();
            async move { read_bad_exes(&pool).await }
        });
        let (maps, exes, bad_exes) = tokio::try_join!(map_fut, exes_fut, bad_exes_fut)?;
        let (maps, exes, bad_exes) = (maps?, exes?, bad_exes?);
        debug!("Finished reading maps, exes, and bad_exes from the database");

        // register maps, exes, and bad exes
        for map in maps.values() {
            self.register_map(map.clone());
        }
        self.bad_exes = bad_exes.into_iter().collect();
        for exe in exes.values() {
            self.register_exe(exe.clone(), false);
        }
        // markovs and exemaps are implicitly registered by the exes
        Markov::read_all(pool, &exes, self.time, self.last_running_timestamp).await?;
        ExeMap::read_all(pool, &maps, &exes).await?;

        self.proc_foreach();
        self.last_running_timestamp = self.time;
        for exe in exes.values() {
            exe.set_markov_state(self.last_running_timestamp)?;
        }

        Ok(())
    }
}
