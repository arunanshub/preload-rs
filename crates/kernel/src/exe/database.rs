use super::Exe;
use crate::{database::DatabaseWriteExt, Error};
use sqlx::SqlitePool;
use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
