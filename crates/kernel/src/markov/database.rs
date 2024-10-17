use super::Markov;
use crate::{database::DatabaseWriteExt, extract_exe, Error};
use bincode::serialize;
use sqlx::SqlitePool;

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
