use super::Exe;
use crate::{database::DatabaseWriteExt, Error};
use sqlx::SqlitePool;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[async_trait::async_trait]
impl DatabaseWriteExt for Exe {
    type Error = Error;

    async fn write(&self, pool: &SqlitePool) -> Result<u64, Self::Error> {
        let path;
        let seq;
        let update_time;
        let time;

        // cannot lock across await so we need to extract the values first
        {
            let exe = self.0.lock();

            path = exe
                .path
                .to_str()
                .ok_or_else(|| Error::InvalidPath(exe.path.clone()))?
                .to_owned();
            seq = exe
                .seq
                .ok_or_else(|| Error::ExeSeqNotAssigned(exe.path.clone()))?
                as i64;
            update_time = exe.update_time.map(|v| v as i64);
            time = exe.time as i64;
        };

        let mut tx = pool.begin().await?;
        let rows_affected = sqlx::query!(
            r#"
            INSERT INTO exes
                (id, path, update_time, time)
            VALUES
                (?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                path = excluded.path,
                update_time = excluded.update_time,
                time = excluded.time
            "#,
            seq,
            path,
            update_time,
            time
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;

        Ok(rows_affected)
    }
}

/// Write bad exes to the database.
pub async fn write_bad_exe(
    path: impl AsRef<Path>,
    size: u64,
    pool: &SqlitePool,
) -> Result<u64, Error> {
    let path = path.as_ref();
    let path_str = path
        .to_str()
        .ok_or_else(|| Error::InvalidPath(path.to_path_buf()))?;
    let size = size as i64;

    let mut tx = pool.begin().await?;
    let rows_affected = sqlx::query!(
        r#"
        INSERT INTO badexes
            (path, update_time)
        VALUES
            (?, ?)
        "#,
        path_str,
        size
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(rows_affected)
}

#[async_trait::async_trait]
pub trait ExeDatabaseReadExt: Sized {
    /// Read exes from the database. The returned exes are stripped of their
    /// sequence numbers.
    ///
    /// Once read, make sure [`State`](crate::State) registers these.
    async fn read_all(pool: &SqlitePool) -> Result<HashMap<PathBuf, Self>, Error>;
}

#[async_trait::async_trait]
impl ExeDatabaseReadExt for Exe {
    async fn read_all(pool: &SqlitePool) -> Result<HashMap<PathBuf, Self>, Error> {
        let records = sqlx::query!(
            r#"
            SELECT
                id as "id: u64", path, update_time as "update_time: u64", time as "time: u64"
            FROM
                exes
            "#
        )
        .fetch_all(pool)
        .await?;

        let exes = records
            .into_iter()
            .map(|row| {
                let exe = Exe::new(&row.path);
                if let Some(update_time) = row.update_time {
                    exe.set_update_time(update_time);
                }
                // TODO: do we need exe.set_seq(row.id);
                exe.set_time(row.time);
                (PathBuf::from(row.path), exe)
            })
            .collect();
        Ok(exes)
    }
}

/// Read bad exes from the database.
///
/// Returned value is a vector of tuples where the first element is the path of
/// the bad exe and the second element is the update time.
pub async fn read_bad_exes(pool: &SqlitePool) -> Result<Vec<(PathBuf, u64)>, Error> {
    let mut tx = pool.begin().await?;
    let bad_exes = sqlx::query!(
        r#"
        SELECT
            path, update_time
        FROM
            badexes
        "#
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(bad_exes
        .into_iter()
        .map(|row| (PathBuf::from(row.path), row.update_time as u64))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[sqlx::test]
    async fn write_exe(pool: SqlitePool) {
        let exe = Exe::new("a/b/c").with_change_timestamp(2).with_running(3);
        exe.set_seq(1);
        let rows = exe.write(&pool).await.unwrap();
        assert_eq!(rows, 1);
    }

    #[sqlx::test]
    async fn test_write_bad_exe(pool: SqlitePool) {
        let path = "a/b/c";
        let size = 2;
        let rows = write_bad_exe(path, size, &pool).await.unwrap();
        assert_eq!(rows, 1);
    }

    #[sqlx::test]
    fn test_read_bad_exes(pool: SqlitePool) {
        let mut bad_exes = vec![];
        for i in 0..3 {
            let path = PathBuf::from(format!("a/b/c/{i}"));
            let size = i as u64;
            write_bad_exe(&path, size, &pool).await.unwrap();
            bad_exes.push((path, size));
        }

        let bad_exes_read = read_bad_exes(&pool).await.unwrap();
        assert_eq!(bad_exes, bad_exes_read);
    }

    #[sqlx::test]
    fn test_read_exes(pool: SqlitePool) {
        let mut exes = HashMap::new();
        for i in 0..10 {
            let path = PathBuf::from(format!("a/b/c/{i}"));
            let exe = Exe::new(&path).with_change_timestamp(i).with_running(i + 1);
            exe.set_seq(i);
            exe.write(&pool).await.unwrap();
            exes.insert(path.clone(), exe);
        }

        let exes_read = Exe::read_all(&pool).await.unwrap();
        // assert exes and exes_read are equal
        assert_eq!(exes.len(), exes_read.len());
        for (path, exe) in exes {
            let read_exe = exes_read.get(&path).expect("Exe not found");
            assert!(read_exe.seq().is_none());
            assert_eq!(exe.0.lock().time, read_exe.0.lock().time);
            assert_eq!(exe.0.lock().update_time, read_exe.0.lock().update_time);
        }
    }
}
