use crate::{
    utils::{accept_file, sanitize_file},
    Error,
};
use config::Config;
use libc::pid_t;
use std::{
    collections::{HashMap, VecDeque},
    mem,
    path::{Path, PathBuf},
};
use sysinfo::{ProcessRefreshKind, RefreshKind, UpdateKind};
use tracing::{debug, warn};

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

    new_exes: HashMap<PathBuf, pid_t>,

    exes: HashMap<PathBuf, ()>,

    bad_exes: HashMap<PathBuf, ()>,
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
            exes: Default::default(),
            bad_exes: Default::default(),
        }
    }

    fn running_process_callback(&mut self, pid: pid_t, exe_path: impl Into<PathBuf>) {
        let exe_path = exe_path.into();

        if let Some(_exe) = self.exes.get(&exe_path) {
            // TODO: !exe_is_running(exe);
            if true {
                self.new_running_exes.push_back(());
                self.state_changed_exes.push_back(());
            }
            // TODO: exe.running_timestamp = self.time;
        } else if !self.bad_exes.contains_key(&exe_path) {
            self.new_exes.insert(exe_path, pid);
        }
    }

    /// Update the exe list by its running status.
    ///
    /// If the exe is running, it is considered to be newly running, otherwise
    /// it is considered to have changed state.
    fn update_exe_list(&mut self, exe: ()) {
        // TODO: exe_is_running(exe);
        if true {
            self.new_running_exes.push_back(exe);
        } else {
            self.state_changed_exes.push_back(exe);
        }
    }

    /// scan processes, see which exes started running, which are not running
    /// anymore, and what new exes are around.
    fn spy_scan(&mut self) {
        self.state_changed_exes.clear();
        self.new_running_exes.clear();
        self.new_exes.clear();

        // HACK: self.running_process_callback is used as a closure that has a
        // mutable reference to self. Now, config is also a part of self, and by
        // rust's rules we cannot have an immutable reference to self while we
        // have a mutable reference to self.
        let exeprefix = std::mem::take(&mut self.config.system.exeprefix);
        proc_foreach(
            |pid, exe_path| self.running_process_callback(pid as i32, exe_path),
            &exeprefix,
        );
        self.config.system.exeprefix = exeprefix;
        // mark each running exe with fresh timestamp
        self.last_running_timestamp = self.time;

        // figure out who's not running by checking their timestamp
        let running_exes = mem::take(&mut self.running_exes);
        for exe in running_exes {
            self.update_exe_list(exe);
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

// TODO: make this a method of StateInner
fn proc_foreach<T, U>(mut callback: T, exeprefixes: &[U])
where
    T: FnMut(u32, &Path),
    U: AsRef<str>,
{
    let refreshes = RefreshKind::new().with_processes(
        ProcessRefreshKind::new()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_memory(),
    );
    let mut system = sysinfo::System::new_with_specifics(refreshes);
    system.refresh_specifics(refreshes);

    for (pid, process) in system.processes() {
        let pid = pid.as_u32();
        if pid == std::process::id() {
            continue;
        }

        let Some(exe_path) = process.exe().map(PathBuf::from) else {
            warn!("exe path not found for pid={pid}. Am I running as root?");
            continue;
        };

        let Some(exe_path) = sanitize_file(&exe_path) else {
            continue;
        };

        if !accept_file(exe_path, exeprefixes) {
            continue;
        }
        callback(pid, exe_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_foreach_atleast_one_process() {
        let mut count = 0;
        proc_foreach::<_, &str>(|_, _| count += 1, &[]);
        assert!(count > 0);
    }
}
