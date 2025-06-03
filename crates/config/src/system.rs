use crate::sort_strategy::SortStrategy;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::{path::PathBuf, time::Duration};

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct System {
    /// Whether preload should monitor running processes and update its model
    /// state. Normally you do want that, that's all preload is about, but you
    /// may want to temporarily turn it off for various reasons like testing
    /// and only make predictions.
    ///
    /// # Note
    ///
    /// If scanning is off, predictions are made based on whatever processes
    /// have been running when preload started and the list of running
    /// processes is not updated at all.
    pub doscan: bool,

    /// Whether preload should make prediction and prefetch anything off the
    /// disk. Quite like doscan, you normally want that, that's the other half
    /// of what preload is about, but you may want to temporarily turn it off,
    /// to only train the model for example.
    ///
    /// # Note
    ///
    /// This allows you to turn scan/predict or or off on the fly, by modifying
    /// the config file and signalling the daemon.
    pub dopredict: bool,

    /// Preload will automatically save the state to disk every autosave
    /// period. This is only relevant if doscan is set to true. The period is
    /// measured in **seconds**.
    ///
    /// # Note
    ///
    /// Some janitory work on the model, like removing entries for files that
    /// no longer exist happen at state save time. So, turning off autosave
    /// completely is not advised.
    #[serde_as(as = "serde_with::DurationSeconds")]
    pub autosave: Duration,

    /// A list of path prefixes that control which mapped file are to be
    /// considered by preload and which not. The list items are separated by
    /// semicolons. Matching will be stopped as soon as the first item is
    /// matched. For each item, if item appears at the beginning of the path
    /// of the file, then a match occurs, and the file is accepted. If on the
    /// other hand, the item has a exclamation mark as its first character,
    /// then the rest of the item is considered, and if a match happens, the
    /// file is rejected. For example a value of !/lib/modules;/ means that
    /// every file other than those in /lib/modules should be accepted. In
    /// this case, the trailing item can be removed, since if no match occurs,
    /// the file is accepted. It's advised to make sure /dev is rejected,
    /// since preload doesn't special-handle device files internally.
    ///
    /// # Note
    ///
    /// If /lib matches all of /lib, /lib64, and even /libexec if there was
    /// one. If one really meant /lib only, they should use /lib/ instead.
    pub mapprefix: Vec<PathBuf>,

    /// The syntax for this is exactly the same as for mapprefix. The only
    /// difference is that this is used to accept or reject binary executable
    /// files instead of maps.
    pub exeprefix: Vec<String>,

    /// Maximum number of processes to use to do parallel readahead. If
    /// equal to 0, no parallel processing is done and all readahead is
    /// done in-process. Parallel readahead supposedly gives a better I/O
    /// performance as it allows the kernel to batch several I/O requests
    /// of nearby blocks.
    pub processes: u32,

    /// The I/O sorting strategy. Ideally this should be automatically
    /// decided, but it's not currently.
    ///
    /// See [`SortStrategy`] for possible values.
    pub sortstrategy: Option<SortStrategy>, // we need an enum
}

impl Default for System {
    fn default() -> Self {
        Self {
            doscan: true,
            dopredict: true,
            autosave: Duration::from_secs(3600),
            // TODO: can use mapexclude and exeexclude
            mapprefix: vec![
                PathBuf::from("/usr/"),
                PathBuf::from("/lib/"),
                PathBuf::from("/var/cache/"),
                PathBuf::from("!/"),
            ],
            exeprefix: vec![
                "!/usr/sbin/".into(),
                "!/usr/local/sbin/".into(),
                "/usr/".into(),
                "!/".into(),
            ],
            processes: 30,
            sortstrategy: Some(SortStrategy::Block),
        }
    }
}
