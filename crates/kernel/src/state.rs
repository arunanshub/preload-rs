use crate::Error;
use config::Config;
use std::{path::Path, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tokio::time;
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
        debug!(?self.config, "current config");
        debug!(?self.time);
        debug!(?self.dirty);
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

    pub async fn start(&mut self) -> Result<(), Error> {
        let mut i1 = time::interval(Duration::from_millis(self.config.model.cycle as u64 / 2));
        let mut i2 = time::interval(Duration::from_millis(
            (self.config.model.cycle + 1) as u64 / 2,
        ));

        loop {
            self.scan_and_predict()?;
            i1.tick().await;
            self.update()?;
            i2.tick().await;
        }
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

    pub async fn start(&self) -> Result<(), Error> {
        self.0.write().await.start().await
    }
}
