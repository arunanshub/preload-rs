use crate::{
    utils::{accept_file, sanitize_file},
    Error,
};
use config::Config;
use libc::pid_t;
use procfs::process::MMapPath;
use std::{
    collections::{HashMap, VecDeque},
    mem,
    path::{Path, PathBuf},
};
use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};
use tracing::{debug, enabled, trace, warn, Level};

#[derive(Debug)]
pub(crate) struct StateInner {
    /// Configuration is created by the user and (probably) loaded from a file.
    pub(crate) config: Config,

    pub(crate) dirty: bool,

    pub(crate) model_dirty: bool,

    pub(crate) time: u64,

    pub(crate) last_running_timestamp: u64,

    last_accounting_timestamp: u64,

    state_changed_exes: VecDeque<()>,

    running_exes: VecDeque<()>,

    new_running_exes: VecDeque<()>,

    new_exes: HashMap<PathBuf, pid_t>,

    exes: HashMap<PathBuf, ()>,
    /// Exes that are too small to be considered. Value is the size of the exe maps.
    bad_exes: HashMap<PathBuf, u64>,

    sysinfo: System,

    system_refresh_kind: RefreshKind,
}

impl StateInner {
    #[tracing::instrument(skip_all)]
    pub fn new(mut config: Config) -> Self {
        let system_refresh_kind = RefreshKind::new().with_processes(
            ProcessRefreshKind::new()
                .with_exe(UpdateKind::OnlyIfNotSet)
                .with_memory(),
        );
        debug!(?system_refresh_kind);
        let sysinfo = System::new_with_specifics(system_refresh_kind);
        // sort map and exeprefixes ahead of time: see `utils::accept_file` for
        // more info
        config.system.mapprefix.sort();
        config.system.exeprefix.sort();

        Self {
            config,
            dirty: false,
            model_dirty: false,
            time: 0,
            last_running_timestamp: 0,
            last_accounting_timestamp: 0,
            state_changed_exes: Default::default(),
            running_exes: Default::default(),
            new_running_exes: Default::default(),
            new_exes: Default::default(),
            exes: Default::default(),
            bad_exes: Default::default(),
            sysinfo,
            system_refresh_kind,
        }
    }

    // TODO: needed by new_exe_callback(...). Work asap.
    #[tracing::instrument(skip_all)]
    fn proc_get_maps(&mut self, pid: pid_t) -> Result<u64, Error> {
        let mut size = 0;

        let processes = procfs::process::all_processes()?;
        for map_res in processes.flat_map(|p| p.map(|p| p.maps())) {
            let Ok(maps) = map_res else {
                warn!("Failed to get maps for pid={pid}. Am I running as root?");
                continue;
            };

            for map in maps
                .into_iter()
                .filter(|v| matches!(v.pathname, MMapPath::Path(_)))
            {
                let MMapPath::Path(path) = map.pathname else {
                    unreachable!("This is not possible");
                };

                let Some(path) = sanitize_file(&path) else {
                    continue;
                };
                if !accept_file(path, &self.config.system.exeprefix) {
                    continue;
                }
                let (start, end) = map.address;
                let length = end - start;
                size += length;
            }
        }
        Ok(size)
    }

    #[tracing::instrument(skip(self))]
    fn proc_foreach(&mut self) {
        trace!("Refresh system info");
        self.sysinfo.refresh_specifics(self.system_refresh_kind);
        // NOTE: we `take` the sysinfo to avoid borrowing issues when looping.
        // Because `running_process_callback` borrows `self` mutably, we can't
        // borrow `self` immutably in the loop.
        let sysinfo = std::mem::take(&mut self.sysinfo);

        for (pid, process) in sysinfo.processes() {
            let pid = pid.as_u32();
            if pid == std::process::id() {
                continue;
            }

            let Some(exe_path) = process.exe() else {
                warn!("Cannot get exe path for pid={pid}. Am I running as root?");
                continue;
            };

            let Some(exe_path) = sanitize_file(exe_path) else {
                continue;
            };

            if !accept_file(exe_path, &self.config.system.exeprefix) {
                continue;
            }
            self.running_process_callback(pid as i32, exe_path)
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

    #[tracing::instrument(skip(self, path))]
    fn new_exe_callback(&mut self, path: impl Into<PathBuf>, pid: pid_t) -> Result<(), Error> {
        let path = path.into();
        // TODO: proc_get_maps(pid, maps: NULL, exemaps: NULL)
        let size = self.proc_get_maps(pid)?;
        trace!(?path, size, "gathered new exe");

        // exe is too small to be considered
        if size < self.config.model.minsize as u64 {
            trace!(?path, size, "exe is too small to be considered");
            self.bad_exes.insert(path, size);
            return Ok(());
        }
        self.proc_get_maps(pid)?;

        // TODO: exe = preload_exe_new(path, TRUE, exemaps);
        // TODO: preload_state_register_exe(exe, TRUE);
        self.running_exes.insert(0, ());

        Ok(())
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
    #[tracing::instrument(skip(self))]
    fn spy_scan(&mut self) {
        self.new_running_exes.clear();
        self.new_exes.clear();

        self.proc_foreach();
        // mark each running exe with fresh timestamp
        self.last_running_timestamp = self.time;

        // figure out who's not running by checking their timestamp
        let running_exes = mem::take(&mut self.running_exes);
        for exe in running_exes {
            self.update_exe_list(exe);
        }

        self.running_exes = mem::take(&mut self.new_running_exes);
    }

    #[tracing::instrument(skip(self))]
    fn spy_update_model(&mut self) -> Result<(), Error> {
        // register newly discovered exes
        let new_exes = mem::take(&mut self.new_exes);
        debug!(?new_exes, "preparing to register exes");
        for (path, pid) in new_exes {
            self.new_exe_callback(path, pid)?;
        }

        // adjust state for exes that changed state
        let state_changed_exes = mem::take(&mut self.state_changed_exes);
        for _exe in state_changed_exes {
            // TODO: (GFunc)G_CALLBACK(exe_changed_callback), data
        }

        // do some accounting
        let _period = self.time - self.last_accounting_timestamp;
        for _exe in self.exes.values_mut() {
            // TODO: running_exe_inc_time(exe, period);
        }
        // TODO: preload_markov_foreach((GFunc)G_CALLBACK(running_markov_inc_time), GINT_TO_POINTER(period));

        self.last_accounting_timestamp = self.time;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn dump_info(&self) {
        debug!(?self.config, ?self.time, ?self.dirty, "current config");
    }

    #[tracing::instrument(skip(self))]
    fn prophet_predict(&mut self) {
        for _exe in self.exes.values_mut() {
            // TODO: exe_zero_prob(exe);
        }

        // TODO: g_ptr_array_foreach(state->maps_arr, (GFunc)G_CALLBACK(map_zero_prob), data);

        // TODO: preload_markov_foreach((GFunc)G_CALLBACK(markov_bid_in_exes), data);

        if enabled!(Level::TRACE) {
            for _exe in self.exes.values() {
                // TODO: exe_prob_print(exe);
            }
        }

        // TODO: preload_exemap_foreach((GHFunc)G_CALLBACK(exemap_bid_in_maps), data);

        // may not be required if maps stored as BTreeMap
        // XXX: g_ptr_array_sort(state->maps_arr, (GCompareFunc)map_prob_compare);

        // TODO: preload_prophet_readahead(state->maps_arr);
    }

    #[tracing::instrument(skip_all)]
    pub fn reload_config(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        self.config = Config::load(path)?;
        // sort map and exeprefixes ahead of time: see `utils::accept_file` for
        // more info
        self.config.system.mapprefix.sort();
        self.config.system.exeprefix.sort();
        debug!(?self.config, "loaded new config");
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn scan_and_predict(&mut self) -> Result<(), Error> {
        if self.config.system.doscan {
            self.spy_scan();
            self.model_dirty = true;
            self.dirty = true;
        }
        if self.config.system.dopredict {
            self.prophet_predict();
        }

        self.time += self.config.model.cycle as u64 / 2;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn update(&mut self) -> Result<(), Error> {
        if self.model_dirty {
            self.spy_update_model()?;
            self.model_dirty = false;
        }

        self.time += (self.config.model.cycle as u64 + 1) / 2;
        Ok(())
    }
}
