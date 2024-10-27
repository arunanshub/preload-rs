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
    /// Read all markovs from the database and registers them with [`Exe`s](crate::Exe).
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
                markovs
            INNER JOIN
                exes AS exe_a, exes AS exe_b
            ON
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
                .ok_or_else(|| Error::ExeDoesNotExist(exe_a_path))?;
            let exe_b = exes
                .get(&exe_b_path)
                .ok_or_else(|| Error::ExeDoesNotExist(exe_b_path))?;
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
    use futures::future::join_all;
    use itertools::Itertools;
    use pretty_assertions::assert_eq;

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

    #[sqlx::test]
    fn read_markovs(pool: SqlitePool) {
        // let there be a given number of exes
        let mut exes = HashMap::new();
        for i in 0..10 {
            let path = PathBuf::from(format!("path/a/b/{i}"));
            exes.insert(path.clone(), Exe::new(path));
        }
        // set the sequence number for each exe and write them to db
        join_all(exes.values().enumerate().map(|(i, exe)| {
            exe.set_seq(i as u64);
            exe.write(&pool)
        }))
        .await;

        // build markov chains with adjacent exes: (1, 2), (2, 3), (3, 4), ... (n-1, n), (n, 1)
        // where n is the number of exes
        let mut markovs = vec![];
        for (exe_a, exe_b) in exes.values().circular_tuple_windows() {
            let markov = exe_a.build_markov_chain_with(exe_b, 1, 2).unwrap().unwrap();
            markov.write(&pool).await.unwrap();
            markovs.push(markov);
        }

        // read the markovs back from the db and assert
        let markovs_read = Markov::read_all(&pool, &exes, 1, 2).await.unwrap();
        assert_eq!(markovs.len(), markovs_read.len());
        for (markov, markov_read) in markovs.iter().zip(markovs_read.iter()) {
            assert_eq!(
                markov.0.lock().time_to_leave,
                markov_read.0.lock().time_to_leave
            );
            assert_eq!(markov.0.lock().weight, markov_read.0.lock().weight);
        }
    }
}
