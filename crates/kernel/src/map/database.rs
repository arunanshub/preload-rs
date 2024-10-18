use super::Map;
use crate::{database::DatabaseWriteExt, Error};
use sqlx::SqlitePool;

#[async_trait::async_trait]
impl DatabaseWriteExt for Map {
    type Error = Error;

    async fn write(&self, pool: &SqlitePool) -> Result<u64, Error> {
        let mut tx = pool.begin().await?;

        let seq = if let Some(val) = self.seq() {
            val as i64
        } else {
            return Err(Error::MapSeqNotAssigned {
                path: self.path().into(),
                offset: self.offset(),
                length: self.length(),
            });
        };
        let update_time = self.update_time() as i64;
        let offset = self.offset() as i64;
        let length = self.length() as i64;
        let path = self
            .path()
            .to_str()
            .ok_or_else(|| Error::InvalidPath(self.path().into()))?;

        let rows_affected = sqlx::query!(
            "
            INSERT INTO maps
                (id, update_time, offset, length, path)
            VALUES
                (?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                update_time = excluded.update_time,
                offset = excluded.offset,
                length = excluded.length,
                path = excluded.path
        ",
            seq,
            update_time,
            offset,
            length,
            path
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

    #[sqlx::test]
    fn write_map(pool: SqlitePool) {
        let map = Map::new("a/b/c", 12, 13, 14);
        map.set_seq(1);
        let result = map.write(&pool).await.unwrap();
        assert_eq!(result, 1);
    }

    #[sqlx::test]
    fn write_map_fails_without_seq_number(pool: SqlitePool) {
        let map = Map::new("a/b/c", 12, 13, 14);
        let result = map.write(&pool).await;
        assert!(result.is_err());
    }
}
