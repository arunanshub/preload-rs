#![allow(clippy::mutable_key_type)]

use super::ExeMap;
use crate::{database::DatabaseWriteExt, Error, Exe, Map};
use sqlx::SqlitePool;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

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
        // TODO: return error if exe_seq is not set
        let exe_id = self.exe_seq.map(|v| v as i64).unwrap_or_default();

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

#[async_trait::async_trait]
pub trait ExeMapDatabaseReadExt: Sized {
    /// Read all exe maps from the database.
    ///
    /// # Arguments
    ///
    /// - `maps`: A map of all [`Map`](crate::Map) keyed by its sequence number.
    ///
    /// # Note
    ///
    /// Ideally you would call this function after you have read the maps from
    /// the database.
    async fn read_all(
        pool: &SqlitePool,
        maps: &HashMap<u64, Map>,
        exes: &HashMap<PathBuf, Exe>,
    ) -> Result<HashSet<Self>, Error>;
}

#[async_trait::async_trait]
impl ExeMapDatabaseReadExt for ExeMap {
    async fn read_all(
        pool: &SqlitePool,
        maps: &HashMap<u64, Map>,
        exes: &HashMap<PathBuf, Exe>,
    ) -> Result<HashSet<Self>, Error> {
        let records = sqlx::query!(
            r#"
            SELECT
                exes.path as exe_path,
                map_id as "map_id: u64",
                prob as "prob: f32"
            FROM
                exemaps
            JOIN
                exes
            ON
                exe_id = exes.id
            "#
        )
        .fetch_all(pool)
        .await?;

        let mut exemaps = HashSet::new();
        for record in records {
            let map_id = record.map_id;
            let prob = record.prob;
            let map = maps
                .get(&map_id)
                .ok_or_else(|| Error::MapDoesNotExist(map_id))?;
            let exe = exes
                .get(Path::new(&record.exe_path))
                .ok_or_else(|| Error::ExeDoesNotExist(record.exe_path.into()))?;

            let exemap = ExeMap::new(map.clone()).with_prob(prob);
            // register exemap with exe
            {
                let mut lock = exe.0.lock();
                lock.exemaps.insert(exemap.clone());
                lock.size += exemap.map.length();
            }
            exemaps.insert(exemap);
        }
        Ok(exemaps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Exe, Map, MapDatabaseReadExt};
    use itertools::Itertools;
    use pretty_assertions::assert_eq;

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

    #[sqlx::test]
    fn read_exemap(pool: SqlitePool) {
        // write the map to the database with their sequence numbers
        let mut maps = HashMap::new();
        for i in 0..10 {
            let map = Map::new(format!("a/b/{i}"), 1 + i, 2 + i, 3 + i);
            map.set_seq(i);
            map.write(&pool).await.unwrap();
            maps.insert(i, map);
        }

        // write the exes to the database with their sequence numbers
        let mut exes = HashMap::new();
        for i in 0..10 {
            let path = PathBuf::from(format!("foo/bar/{i}"));
            let exe = Exe::new(&path);
            exe.set_seq(i);
            exe.write(&pool).await.unwrap();
            exes.insert(path, exe);
        }

        // write the exemaps to the database
        let mut exe_maps = HashSet::new();
        for (map, exe) in maps.values().zip_eq(exes.values()) {
            // we are bound to have the sequence number for the exe
            let exemap = ExeMap::new(map.clone()).with_exe_seq(exe.seq().unwrap());
            exemap.write(&pool).await.unwrap();
            exe_maps.insert(exemap);
        }

        // maps are needed to read the exemaps
        let maps_read = Map::read_all(&pool).await.unwrap();
        // read the exemaps from the database
        let exe_maps_read = ExeMap::read_all(&pool, &maps_read, &exes).await.unwrap();

        assert_eq!(exe_maps, exe_maps_read);
    }
}
