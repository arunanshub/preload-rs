use crate::Error;
use config::Config;
use std::path::Path;
use tracing::{debug, info};

pub struct State {
    /// Configuration is created by the user and (probably) loaded from a file.
    pub config: Config,

    dirty: bool,

    model_dirty: bool,

    time: u64,
}

impl State {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            dirty: false,
            model_dirty: false,
            time: 0,
        }
    }

    pub fn dump_info(&self) {
        // TODO: dump state info
        info!("{:?}", self.config);
    }

    pub fn reload_config(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        self.config = Config::load(path)?;
        debug!(?self.config, "loaded new config");
        Ok(())
    }

    pub fn tick(&mut self) {
        let span = tracing::info_span!("state scan");
        let _enter = span.enter();

        info!("begin");
        if self.config.system.doscan {
            self.spy_scan();
            self.model_dirty = true;
            self.dirty = true;
        }
        if self.config.system.dopredict {
            // TODO: preload_prophet_predict(data);
        }

        self.time += self.config.model.cycle as u64 / 2;
        // TODO: g_timeout_add_seconds(conf->model.cycle / 2, preload_state_tick2, data);
    }

    pub fn spy_scan(&self) {}
}
