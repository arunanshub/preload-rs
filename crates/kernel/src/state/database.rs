use super::inner::StateInner;
use crate::{database::DatabaseWriteExt, exe::database::write_bad_exe, Error};
use sqlx::SqlitePool;
use tracing::trace;

#[async_trait::async_trait]
impl DatabaseWriteExt for StateInner {
    type Error = Error;

    async fn write(&self, pool: &SqlitePool) -> Result<u64, Self::Error> {
        let mut js = tokio::task::JoinSet::new();

        trace!("Writing maps");
        for map in &self.maps {
            let map = map.clone();
            let pool = pool.clone();
            js.spawn(async move { map.write(&pool).await });
        }

        trace!("Writing badexes");
        for (path, &size) in &self.bad_exes {
            let pool = pool.clone();
            let path = path.to_path_buf();
            js.spawn(async move { write_bad_exe(path, size, &pool).await });
        }

        trace!("Writing exes");
        for exe in self.exes.values() {
            let exe = exe.clone();
            let pool = pool.clone();
            js.spawn(async move { exe.write(&pool).await });
        }

        // TODO: exemaps
        while let Some(res) = js.join_next().await {
            res??;
        }
        trace!("All written");

        Ok(1)
    }
}
