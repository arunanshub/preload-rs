use super::ExeMap;
use crate::{database::DatabaseWriteExt, Error};
use sqlx::SqlitePool;

#[async_trait::async_trait]
impl DatabaseWriteExt for ExeMap {
    type Error = Error;

    async fn write(&self, pool: &SqlitePool) -> Result<u64, Self::Error> {
        let map_id = self.map.seq() as i64;
        let rows_affected = sqlx::query!(
            r#"
            INSERT INTO exemaps (map_id, prob)
            VALUES (?, ?)
            "#,
            map_id,
            self.prob
        )
        .execute(pool)
        .await?
        .rows_affected();
        Ok(rows_affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Map;

    #[sqlx::test]
    fn write_exemap(pool: SqlitePool) {
        let map = Map::new("ab/c", 1, 2, 3);
        map.write(&pool).await.unwrap();
        let mut exemap = ExeMap::new(map.clone());
        exemap.prob = 2.3;
        exemap.write(&pool).await.unwrap();
    }
}
