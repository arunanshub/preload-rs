use crate::{
    Error, Exe, ExeMap, Map, MemStat,
    utils::{accept_file, kb, readahead, sanitize_file},
};
use config::{Config, Model, SortStrategy};
use humansize::{DECIMAL, format_size_i};
use itertools::Itertools;
use libc::pid_t;
use procfs::process::MMapPath;
use rayon::prelude::*;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet, VecDeque},
    mem,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicU64},
};
use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};
use tracing::{Level, debug, enabled, error, trace, warn};

#[derive(Debug, Default)]
pub(crate) struct StateInner {
    /// Configuration is created by the user and (probably) loaded from a file.
    pub(crate) config: Config,

    pub(crate) dirty: bool,

    pub(crate) model_dirty: bool,

    pub(crate) time: u64,

    pub(crate) last_running_timestamp: u64,

    pub(crate) last_accounting_timestamp: u64,

    pub(crate) map_seq: u64,

    pub(crate) maps: HashSet<Map>,

    pub(crate) exe_seq: u64,

    pub(crate) state_changed_exes: VecDeque<Exe>,

    pub(crate) running_exes: VecDeque<Exe>,

    pub(crate) new_running_exes: VecDeque<Exe>,

    pub(crate) new_exes: HashMap<PathBuf, pid_t>,

    pub(crate) exes: HashMap<PathBuf, Exe>,
    /// Exes that are too small to be considered. Value is the size of the exe maps.
    pub(crate) bad_exes: HashMap<PathBuf, u64>,

    pub(crate) sysinfo: System,

    pub(crate) system_refresh_kind: RefreshKind,

    pub(crate) memstat_timestamp: u64,
}

impl StateInner {
    #[tracing::instrument(skip_all)]
    pub fn new(mut config: Config) -> Self {
        let system_refresh_kind = RefreshKind::nothing().with_processes(
            ProcessRefreshKind::nothing()
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
            sysinfo,
            system_refresh_kind,
            ..Default::default()
        }
    }

    #[tracing::instrument(skip_all)]
    fn proc_get_maps(
        &mut self,
        pid: pid_t,
        with_exemaps: bool,
    ) -> Result<(u64, Option<HashSet<ExeMap>>), Error> {
        let mut size = 0;
        let mut exemaps = if with_exemaps {
            Some(HashSet::new())
        } else {
            None
        };

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

                if let Some(exemaps) = &mut exemaps {
                    let mut map = Map::new(path, map.offset, length, self.time);
                    if let Some(existing_map) = self.maps.get(&map) {
                        map = existing_map.clone();
                    }
                    exemaps.insert(ExeMap::new(map.clone()));
                    self.register_map(map);
                }
            }
        }

        Ok((size, exemaps))
    }

    pub(crate) fn register_map(&mut self, map: Map) {
        if self.maps.contains(&map) {
            return;
        }
        map.set_seq(self.map_seq);
        self.map_seq += 1;
        self.maps.insert(map);
    }

    #[tracing::instrument(skip(self))]
    pub(crate) fn proc_foreach(&mut self) {
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

        if let Some(exe) = self.exes.get(&exe_path) {
            if !exe.is_running(self.last_running_timestamp) {
                self.new_running_exes.push_back(exe.clone());
                self.state_changed_exes.push_back(exe.clone());
            }
            exe.set_running_timestamp(self.time);
        } else if !self.bad_exes.contains_key(&exe_path) {
            self.new_exes.insert(exe_path, pid);
        }
    }

    #[tracing::instrument(skip(self, path))]
    fn new_exe_callback(&mut self, path: impl Into<PathBuf>, pid: pid_t) -> Result<(), Error> {
        let path = path.into();
        let (size, _) = self.proc_get_maps(pid, false)?;
        trace!(?path, size, "gathered new exe");

        // exe is too small to be considered
        if size < self.config.model.minsize as u64 {
            trace!(?path, size, "exe is too small to be considered");
            self.bad_exes.insert(path, size);
            return Ok(());
        }

        let (size, exemaps) = self.proc_get_maps(pid, true)?;
        if size == 0 {
            warn!(?path, "exe has no maps. Maybe the process died?");
            return Ok(());
        }
        let Some(exemaps) = exemaps else {
            unreachable!("exemaps should be Some because we explicitly asked for it");
        };

        let exe = Exe::new(path).with_running(self.last_running_timestamp);
        self.register_exe(exe.clone(), true);
        // NOTE: we can only register the exemaps after we have been assigned an
        // exe_seq
        let exe = exe.try_with_exemaps(exemaps)?;
        self.running_exes.push_front(exe);

        Ok(())
    }

    #[tracing::instrument(skip(self, exe))]
    pub(crate) fn register_exe(&mut self, exe: Exe, create_markovs: bool) {
        exe.set_seq(self.exe_seq);
        self.exe_seq += 1;
        trace!(?exe, "registering exe");
        if create_markovs {
            self.exes.iter().for_each(|(_, other_exe)| {
                let Ok(_) =
                    exe.build_markov_chain_with(other_exe, self.time, self.last_running_timestamp)
                else {
                    unreachable!("Both exes are present which is enough to build the markov chain");
                };
            });
        }
        self.exes.insert(exe.path(), exe);
    }

    /// Update the exe list by its running status.
    ///
    /// If the exe is running, it is considered to be newly running, otherwise
    /// it is considered to have changed state.
    fn update_exe_list(&mut self, exe: Exe) {
        if exe.is_running(self.last_running_timestamp) {
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
        self.state_changed_exes.clear();
        self.new_exes.clear();

        self.proc_foreach();
        // mark each running exe with fresh timestamp
        self.last_running_timestamp = self.time;

        // figure out who's not running by checking their timestamp
        let running_exes = mem::take(&mut self.running_exes);
        trace!(
            num_running_exes = running_exes.len(),
            "running exes found during scan"
        );
        for exe in running_exes {
            self.update_exe_list(exe);
        }

        trace!(num_new_running_exes = self.new_running_exes.len());
        self.running_exes = mem::take(&mut self.new_running_exes);
    }

    fn exe_changed_callback(&self, exe: &Exe) -> Result<(), Error> {
        exe.set_change_timestamp(self.time);
        exe.markov_state_changed(self.time, self.last_running_timestamp)
    }

    #[tracing::instrument(skip(self))]
    fn spy_update_model(&mut self) -> Result<(), Error> {
        // register newly discovered exes
        let new_exes = mem::take(&mut self.new_exes);
        debug!(?new_exes, "preparing to register exes");
        trace!(bad_exes=?self.bad_exes, "bad exes");
        for (path, pid) in new_exes {
            self.new_exe_callback(path, pid)?;
        }

        // adjust state for exes that changed state
        let state_changed_exes = mem::take(&mut self.state_changed_exes);
        trace!(num = state_changed_exes.len(), "Exes that changed state");
        state_changed_exes
            .iter()
            .try_for_each(|exe| self.exe_changed_callback(exe))?;
        trace!("Exes state changed");

        // do some accounting
        let period = self.time - self.last_accounting_timestamp;
        self.exes.iter().for_each(|(_, exe)| {
            if exe.is_running(self.last_running_timestamp) {
                exe.set_time(period);
            }
        });
        trace!("Exe time updated");

        self.exes
            .iter()
            .try_for_each(|(_, exe)| exe.increase_markov_time(period))?;
        trace!("Markov time updated");

        self.last_accounting_timestamp = self.time;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn dump_info(&self) {
        debug!(?self.config, ?self.time, ?self.dirty, "current config");
    }

    #[tracing::instrument(skip(self))]
    fn prophet_predict(&mut self) -> Result<(), Error> {
        // reset probabilities that we are going to compute
        self.exes.iter().for_each(|(_, exe)| exe.zero_lnprob());
        self.maps.par_iter().for_each(|map| map.zero_lnprob());

        self.exes.iter().try_for_each(|(_, exe)| {
            exe.markov_bid_in_exes(
                self.config.model.usecorrelation,
                self.time,
                self.config.model.cycle.as_secs_f32(),
            )
        })?;
        trace!("Markov is done bidding in exes");

        if enabled!(Level::TRACE) {
            self.exes.iter().for_each(|(_, exe)| {
                trace!(lnprob=exe.lnprob(), path=?exe.path(), "lnprob of exes");
            });
        }

        // exes bid in maps
        self.exes
            .iter()
            .for_each(|(_, exe)| exe.bid_in_maps(self.last_running_timestamp));

        // may not be required if maps stored as BTreeMap
        // XXX: g_ptr_array_sort(state->maps_arr, (GCompareFunc)map_prob_compare);

        self.prophet_readahead()?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn prophet_readahead(&mut self) -> Result<(), Error> {
        let memstat = MemStat::try_new()?;
        let Model {
            memtotal,
            memcached,
            memfree,
            ..
        } = self.config.model;

        // amount of memory we are allowed to use for readahead
        let mut memavail = {
            let mut temp = memtotal.clamp(-100, 100) as i64 * (memstat.total / 100) as i64
                + memfree.clamp(-100, 100) as i64 * (memstat.free / 100) as i64;
            temp = temp.max(0);
            temp += memcached.clamp(-100, 100) as i64 * (memstat.cached as i64 / 100);
            temp
        };
        let memavailtotal = memavail;
        self.memstat_timestamp = self.time;

        // XXX: we only readahead a subset of all maps. Maybe find a better way
        // to select maps to readahead without additional vec allocation.
        let mut maps_to_readahead = vec![];
        let mut num_maps_to_readahead = 0;

        let mut maps_iter = self.maps.iter().sorted_by(|a, b| {
            a.lnprob()
                .partial_cmp(&b.lnprob())
                .unwrap_or(Ordering::Equal)
        });
        // XXX: clean up the loop
        while num_maps_to_readahead < maps_iter.len() {
            let Some(map) = maps_iter.nth(num_maps_to_readahead) else {
                error!("Map not found. Please report a bug!");
                break;
            };
            let map_length = kb(map.length()) as i64;
            if map.lnprob() < 0.0 && map_length <= memavail {
                continue;
            }

            memavail -= map_length;
            if enabled!(Level::TRACE) {
                trace!(lnprob = map.lnprob(), "lnprob of map");
                trace!(
                    memavailtotal,
                    memallowed = memavailtotal - memavail,
                    "{} available for preloading, using {} of it.",
                    format_size_i(memavailtotal, DECIMAL),
                    format_size_i(memavailtotal - memavail, DECIMAL),
                );
            }

            num_maps_to_readahead += 1;
            maps_to_readahead.push(map);
        }

        if num_maps_to_readahead > 0 {
            let num_maps_readahead = self.preload_readahead(&mut maps_to_readahead);
            let num_maps = self.maps.len();
            debug!(
                num_maps_readahead,
                num_maps, "Have {num_maps} maps, readahead {num_maps_readahead} maps."
            );
        } else {
            debug!("Nothing to readahead.");
        }

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    fn preload_readahead(&self, maps: &mut [&Map]) -> u64 {
        // sort files
        if let Some(sort_strategy) = self.config.system.sortstrategy {
            trace!("Sorting {} maps by {:?}.", maps.len(), sort_strategy);
            match sort_strategy {
                SortStrategy::Path => {
                    maps.par_sort_by(|a, b| a.path().cmp(b.path()));
                }
                SortStrategy::Block | SortStrategy::Inode => {
                    let need_block = maps.par_iter().any(|map| map.block().is_none());
                    if need_block {
                        trace!("Some maps don't have block.");
                        // sorting by path to make stat fast
                        maps.par_sort_by(|a, b| a.path().cmp(b.path()));
                        // set block if using inode
                        maps.par_iter()
                            .filter_map(|map| match map.block() {
                                Some(_) => None,
                                None => Some(map),
                            })
                            .for_each(|map| {
                                // TODO: strategy == Inode
                                if let Err(err) = map.set_block() {
                                    trace!(?err, "Failed to set block for map")
                                }
                            });
                    }
                    // sort by block
                    maps.par_sort_by(|a, b| a.block().cmp(&b.block()));
                }
            }
        }

        let num_readahead = Arc::new(AtomicU64::new(0));
        maps.par_iter()
            .for_each_with(num_readahead.clone(), |counter, map| {
                // TODO: if (path && offset <= files[i]->offset ...) {}
                if let Err(error) = readahead(map.path(), map.offset() as i64, map.length() as i64)
                {
                    warn!(path=?map.path(), %error, "Failed to readahead");
                } else {
                    counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    trace!(?map, "Readahead done.");
                }
            });
        num_readahead.load(std::sync::atomic::Ordering::Relaxed)
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

    #[tracing::instrument(skip_all)]
    fn dump_log(&self) {
        debug!(
            time = self.time,
            exe_seq = self.exe_seq,
            map_seq = self.map_seq,
            num_exes = self.exes.len(),
            num_bad_exes = self.bad_exes.len(),
            num_maps = self.maps.len(),
            num_running_exes = self.running_exes.len(),
            "Dump log:",
        )
    }

    #[tracing::instrument(skip(self))]
    pub fn scan_and_predict(&mut self) -> Result<(), Error> {
        if self.config.system.doscan {
            self.spy_scan();
            self.model_dirty = true;
            self.dirty = true;
        }
        if enabled!(Level::DEBUG) {
            self.dump_log();
        }
        if self.config.system.dopredict {
            self.prophet_predict()?;
        }

        // TODO: the actual sleep takes place outside the function by the
        // caller. This leads to some duplication.
        self.time += self.config.model.cycle.as_secs() / 2;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn update(&mut self) -> Result<(), Error> {
        if self.model_dirty {
            self.spy_update_model()?;
            self.model_dirty = false;
        }

        // TODO: the actual sleep takes place outside the function by the
        // caller. This leads to some duplication.
        self.time += self.config.model.cycle.as_secs().div_ceil(2);
        Ok(())
    }
}
