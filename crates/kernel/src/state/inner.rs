use crate::Error;
use config::Config;
use std::{
    collections::{HashMap, VecDeque},
    mem,
    path::{Path, PathBuf},
};
use tracing::debug;

#[derive(Debug)]
pub(crate) struct StateInner {
    /// Configuration is created by the user and (probably) loaded from a file.
    pub(crate) config: Config,

    pub(crate) dirty: bool,

    pub(crate) model_dirty: bool,

    pub(crate) time: u64,

    pub(crate) last_running_timestamp: u64,

    state_changed_exes: VecDeque<()>,

    running_exes: VecDeque<()>,

    new_running_exes: VecDeque<()>,

    new_exes: HashMap<PathBuf, ()>,
}

impl StateInner {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            dirty: false,
            model_dirty: false,
            time: 0,
            last_running_timestamp: 0,
            state_changed_exes: Default::default(),
            running_exes: Default::default(),
            new_running_exes: Default::default(),
            new_exes: Default::default(),
        }
    }

    /// scan processes, see which exes started running, which are not running
    /// anymore, and what new exes are around.
    fn spy_scan(&mut self) {
        self.state_changed_exes.clear();
        self.new_running_exes.clear();
        self.new_exes.clear();

        // mark each running exe with fresh timestamp
        // TODO: proc_foreach((GHFunc)G_CALLBACK(running_process_callback), data);
        self.last_running_timestamp = self.time;

        // figure out who's not running by checking their timestamp
        let running_exes = mem::take(&mut self.running_exes);
        for _ in running_exes {
            // TODO: g_slist_foreach(state->running_exes, (GFunc)G_CALLBACK(already_running_exe_callback), data);
        }

        self.running_exes = mem::take(&mut self.new_running_exes);
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
            self.spy_scan();
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
