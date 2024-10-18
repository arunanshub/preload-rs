mod database;
mod inner;

use crate::{
    database::{create_database_pool, DatabaseWriteExt},
    Error, MIGRATOR,
};
use config::Config;
use inner::StateInner;
use sqlx::SqlitePool;
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{sync::RwLock, time};
use tracing::debug;

#[derive(Debug, Clone)]
pub struct State {
    inner: Arc<RwLock<StateInner>>,
    pool: SqlitePool,
}

impl State {
    pub async fn try_new(
        config: Config,
        statefile: Option<impl AsRef<Path>>,
    ) -> Result<Self, Error> {
        Ok(Self {
            inner: Arc::new(RwLock::new(StateInner::new(config))),
            pool: create_database_pool(statefile).await?,
        })
    }

    pub async fn dump_info(&self) {
        self.inner.read().await.dump_info();
    }

    pub async fn reload_config(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        self.inner.write().await.reload_config(path)
    }

    pub async fn update(&self) -> Result<(), Error> {
        self.inner.write().await.update()
    }

    pub async fn scan_and_predict(&self) -> Result<(), Error> {
        self.inner.write().await.scan_and_predict()
    }

    #[tracing::instrument(skip_all)]
    pub async fn write(&self) -> Result<u64, Error> {
        self.inner.write().await.write(&self.pool).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn start(self) -> Result<(), Error> {
        debug!("Running migrations");
        MIGRATOR.run(&self.pool).await?;

        let state = self.inner;
        loop {
            state.write().await.scan_and_predict()?;
            time::sleep(state.read().await.config.model.cycle / 2).await;
            state.write().await.update()?;
            time::sleep((state.read().await.config.model.cycle + Duration::from_secs(1)) / 2).await;
        }
    }
}
