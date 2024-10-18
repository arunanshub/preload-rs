use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::time::Duration;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Model {
    /// This is the quantum of time for preload. Preload performs data gathering
    /// and predictions every cycle. Use an even number. **Measured in
    /// seconds**.
    ///
    /// ## Note
    ///
    /// Setting this parameter too low may reduce system performance and
    /// stability.
    #[serde_as(as = "serde_with::DurationSeconds")]
    pub cycle: Duration,

    /// Whether correlation coefficient should be used in the prediction
    /// algorithm. There are arguments both for and against using it.
    /// Currently it's believed that using it results in more accurate
    /// prediction. The option may be removed in the future.
    pub usecorrelation: bool,

    /// Minimum sum of the length of maps of the process for preload to
    /// consider tracking the application.
    ///
    /// ## Note
    ///
    /// Setting this parameter too high will make preload less effective,
    /// while setting it too low will make it eat quadratically more resources,
    /// as it tracks more processes.
    pub minsize: u32,

    /// The following control how much memory preload is allowed to use for
    /// preloading in each cycle. All values are percentages and are clamped
    /// to -100 to 100.
    ///
    /// The total memory preload uses for prefetching is then computed using
    /// the following formulae:
    ///
    /// ```text
    /// max(0, TOTAL * memtotal + FREE * memfree) + CACHED * memcached
    /// ```
    ///
    /// where TOTAL, FREE, and CACHED are the respective values read at runtime
    /// from `/proc/meminfo`.
    pub memtotal: i32,

    /// Percentage of free memory.
    pub memfree: i32,

    /// Percentage of cached memory.
    pub memcached: i32,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            cycle: Duration::from_secs(2),
            usecorrelation: true,
            minsize: 2_000_000,
            memtotal: -10,
            memfree: 50,
            memcached: 0,
        }
    }
}
