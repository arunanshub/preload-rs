use crate::Error;
use config::Config;
use std::{path::Path, sync::Arc};
use tokio::sync::RwLock;
use tracing::debug;

#[derive(Debug)]
struct StateInner {
    /// Configuration is created by the user and (probably) loaded from a file.
    pub config: Config,

    dirty: bool,

    model_dirty: bool,

    time: u64,
}

impl StateInner {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            dirty: false,
            model_dirty: false,
            time: 0,
        }
    }

    pub fn dump_info(&self) {
        let span = tracing::info_span!("state dump");
        let _enter = span.enter();
        debug!(?self.config, ?self.time, ?self.dirty, "current config");
    }

    pub fn reload_config(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        self.config = Config::load(path)?;
        debug!(?self.config, "loaded new config");
        Ok(())
    }

    fn scan_and_predict(&mut self) -> Result<(), Error> {
        let span = tracing::debug_span!("state_scan");
        let _enter = span.enter();

        debug!("scanning and predicting");
        if self.config.system.doscan {
            // TODO: preload_spy_scan(data);
            self.model_dirty = true;
            self.dirty = true;
        }
        if self.config.system.dopredict {
            // TODO: preload_prophet_predict(data);
        }

        self.time += self.config.model.cycle as u64 / 2;
        Ok(())
    }

    fn update(&mut self) -> Result<(), Error> {
        let span = tracing::debug_span!("state_update");
        let _enter = span.enter();

        debug!("updating state");
        if self.model_dirty {
            // TODO: preload_spy_update_model(data);
            self.model_dirty = false;
        }

        self.time += (self.config.model.cycle as u64 + 1) / 2;
        Ok(())
    }
}

#[derive(Debug, Clone)]
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
}
