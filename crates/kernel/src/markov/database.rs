use super::Markov;
use crate::{database::DatabaseWriteExt, extract_exe, Error, Exe};
use bincode::serialize;
use sqlx::SqlitePool;
use std::{collections::HashMap, path::PathBuf};

#[async_trait::async_trait]
impl DatabaseWriteExt for Markov {
    type Error = Error;

    async fn write(&self, pool: &SqlitePool) -> Result<u64, Self::Error> {
        let exe_a_seq;
        let exe_b_seq;
        let ttl;
        let weight;
        let time;
        {
            let markov = self.0.lock();
            exe_a_seq = if let Some(val) = extract_exe!(markov.exe_a).seq {
                val as i64
            } else {
                let path = extract_exe!(markov.exe_a).path.clone();
                return Err(Error::ExeSeqNotAssigned(path));
            };
            exe_b_seq = if let Some(val) = extract_exe!(markov.exe_b).seq {
                val as i64
            } else {
                let path = extract_exe!(markov.exe_b).path.clone();
                return Err(Error::ExeSeqNotAssigned(path));
            };
            ttl = serialize(&markov.time_to_leave)?;
            weight = serialize(&markov.weight)?;
            time = markov.time as i64;
        }

        let mut tx = pool.begin().await?;
        let rows_affected = sqlx::query!(
            r#"
            INSERT INTO markovs
                (exe_a, exe_b, time, time_to_leave, weight)
            VALUES
                (?, ?, ?, ?, ?)
            ON CONFLICT(exe_a, exe_b) DO UPDATE SET
                time = excluded.time,
                time_to_leave = excluded.time_to_leave,
                weight = excluded.weight
            "#,
            exe_a_seq,
            exe_b_seq,
            time,
            ttl,
            weight,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;

        Ok(rows_affected)
    }
}

#[async_trait::async_trait]
pub trait MarkovDatabaseReadExt: Sized {
    /// Read all markovs from the database.
    ///
    /// # Args
    ///
    /// * `exes` - A map of exes keyed by the exe path. Ideally you would get
    /// this by calling [`ExeDatabaseReadExt::read_all`](crate::ExeDatabaseReadExt::read_all).
    /// * `state_time` - Can be obtained from [`State`](crate::State).
    /// * `last_running_timestamp` - This value can be obtained from [`State`](crate::State).
    async fn read_all(
        pool: &SqlitePool,
        exes: &HashMap<PathBuf, Exe>,
        state_time: u64,
        last_running_timestamp: u64,
    ) -> Result<Vec<Self>, Error>;
}

#[async_trait::async_trait]
impl MarkovDatabaseReadExt for Markov {
    async fn read_all(
        pool: &SqlitePool,
        exes: &HashMap<PathBuf, Exe>,
        state_time: u64,
        last_running_timestamp: u64,
    ) -> Result<Vec<Self>, Error> {
        let records = sqlx::query!(
            r#"
            SELECT
                exe_a.path AS exe_a_path,
                exe_b.path AS exe_b_path,
                markovs.time,
                markovs.time_to_leave,
                markovs.weight
            FROM
                markovs, exes AS exe_a, exes AS exe_b
            WHERE
                exe_a.id = markovs.exe_a AND exe_b.id = markovs.exe_b
        "#
        )
        .fetch_all(pool)
        .await?;

        let mut markovs = Vec::with_capacity(records.len());
        for record in records {
            let exe_a_path = PathBuf::from(record.exe_a_path);
            let exe_b_path = PathBuf::from(record.exe_b_path);

            let exe_a = exes
                .get(&exe_a_path)
                .ok_or_else(|| Error::ExeSeqNotAssigned(exe_a_path))?;
            let exe_b = exes
                .get(&exe_b_path)
                .ok_or_else(|| Error::ExeSeqNotAssigned(exe_b_path))?;
            let time_to_leave: [f32; 4] = bincode::deserialize(&record.time_to_leave)?;
            let weight: [[u32; 4]; 4] = bincode::deserialize(&record.weight)?;

            let Some(markov) =
                exe_a.build_markov_chain_with(exe_b, state_time, last_running_timestamp)?
            else {
                unreachable!("both exes should have different path");
            };
            {
                let mut lock = markov.0.lock();
                lock.time_to_leave = time_to_leave;
                lock.weight = weight;
            }
            markovs.push(markov);
        }

        Ok(markovs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Exe;

    #[sqlx::test]
    async fn write_markov(pool: SqlitePool) {
        let exe_a = Exe::new("a/b/c");
        exe_a.set_seq(0);
        exe_a.write(&pool).await.unwrap();

        let exe_b = Exe::new("d/e/f");
        exe_b.set_seq(2);
        exe_b.write(&pool).await.unwrap();

        let markov = exe_a
            .build_markov_chain_with(&exe_b, 1, 2)
            .unwrap()
            .expect("both exes should have different path");
        let rows = markov.write(&pool).await.unwrap();
        assert_eq!(rows, 1);
    }
}
