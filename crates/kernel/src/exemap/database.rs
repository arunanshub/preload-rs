use super::ExeMap;
use crate::{database::DatabaseWriteExt, Error};
use sqlx::SqlitePool;

#[async_trait::async_trait]
impl DatabaseWriteExt for ExeMap {
    type Error = Error;

    async fn write(&self, pool: &SqlitePool) -> Result<u64, Self::Error> {
        let map_id = if let Some(val) = self.map.seq() {
            val as i64
        } else {
            return Err(Error::MapSeqNotAssigned {
                path: self.map.path().into(),
                offset: self.map.offset(),
                length: self.map.length(),
            });
        };
        let exe_id = self.exe_seq as i64;

        let mut tx = pool.begin().await?;
        let rows_affected = sqlx::query!(
            r#"
            INSERT INTO exemaps
                (exe_id, map_id, prob)
            VALUES
                (?, ?, ?)
            ON CONFLICT(exe_id, map_id) DO UPDATE SET
                prob = excluded.prob
            "#,
            exe_id,
            map_id,
            self.prob
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
    use crate::{Exe, Map};

    #[sqlx::test]
    fn write_exemap(pool: SqlitePool) {
        let map = Map::new("ab/c", 1, 2, 3);
        map.set_seq(1);
        map.write(&pool).await.unwrap();
        let exe = Exe::new("foo/bar");
        exe.set_seq(1);
        exe.write(&pool).await.unwrap();

        let exemap = ExeMap::new(map.clone());
        let mut exemap = exemap.with_exe_seq(exe.seq().unwrap());
        exemap.prob = 2.3;
        exemap.write(&pool).await.unwrap();
    }
}
