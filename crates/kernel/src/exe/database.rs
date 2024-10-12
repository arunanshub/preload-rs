use super::Exe;
use crate::{database::DatabaseWriteExt, Error};
use sqlx::SqlitePool;

#[async_trait::async_trait]
impl DatabaseWriteExt for Exe {
    type Error = Error;
    async fn write(&self, pool: &SqlitePool) -> Result<u64, Self::Error> {
        let mut tx = pool.begin().await?;

        let path = {
            let temp = self.path();
            temp.to_str()
                .ok_or_else(|| Error::InvalidPath(temp.to_path_buf()))?
                .to_owned()
        };
        let seq = self.0.lock().seq as i64;
        let update_time = self.0.lock().update_time.map(|t| t as i64);
        let time = self.0.lock().time as i64;

        let rows_affected = sqlx::query!(
            r#"
            INSERT INTO exes
                (id, path, update_time, time)
            VALUES
                (?, ?, ?, ?)
            "#,
            seq,
            path,
            update_time,
            time
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();

        Ok(rows_affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    fn write_exe(pool: SqlitePool) {
        let exe = Exe::new("a/b/c").with_change_timestamp(2).with_running(3);
        let rows = exe.write(&pool).await.unwrap();
        assert_eq!(rows, 1);
    }
}
