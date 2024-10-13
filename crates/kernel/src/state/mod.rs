mod database;
mod inner;

use crate::Error;
use config::Config;
use inner::StateInner;
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{sync::RwLock, time};

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct State(Arc<RwLock<StateInner>>);

impl State {
    pub fn new(config: Config) -> Self {
        Self(Arc::new(RwLock::new(StateInner::new(config))))
    }

    pub async fn dump_info(&self) {
        self.0.read().await.dump_info();
    }

    pub async fn reload_config(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        self.0.write().await.reload_config(path)
    }

    pub async fn update(&self) -> Result<(), Error> {
        self.0.write().await.update()
    }

    pub async fn scan_and_predict(&self) -> Result<(), Error> {
        self.0.write().await.scan_and_predict()
    }

    pub async fn start(self) -> Result<(), Error> {
        let state = self.0;
        loop {
            state.write().await.scan_and_predict()?;
            time::sleep(Duration::from_secs(
                state.read().await.config.model.cycle as u64 / 2,
            ))
            .await;
            state.write().await.update()?;
            time::sleep(Duration::from_secs(
                (state.read().await.config.model.cycle + 1) as u64 / 2,
            ))
            .await;
        }
    }
}
