use crate::Error;
use config::Config;
use std::path::Path;
use tracing::debug;

#[derive(Debug)]
pub(crate) struct StateInner {
    /// Configuration is created by the user and (probably) loaded from a file.
    pub(crate) config: Config,

    pub(crate) dirty: bool,

    pub(crate) model_dirty: bool,

    pub(crate) time: u64,
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

    pub fn scan_and_predict(&mut self) -> Result<(), Error> {
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

    pub fn update(&mut self) -> Result<(), Error> {
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
