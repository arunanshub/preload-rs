use clap::Parser;
use clap_verbosity_flag::{Verbosity, WarnLevel};
use std::path::{Path, PathBuf};

/// Drutyka: The memory safe system optimizer
///
/// Drutyka is an adaptive readahead daemon that prefetches files mapped by
/// applications from the disk to reduce application startup time.
#[derive(Debug, Parser, Clone)]
#[command(about, long_about, version)]
pub(crate) struct Cli {
    /// Path to configuration file.
    ///
    /// Empty string means no conf file.
    #[arg(short, long, value_parser = validate_file)]
    pub(crate) conffile: Option<PathBuf>,

    /// File to load and save application state to.
    ///
    /// Empty string means state is stored in memory.
    #[arg(short, long)]
    pub(crate) statefile: Option<String>,

    /// Path to log file.
    ///
    /// Empty string means log to stderr.
    #[arg(short, long)]
    pub(crate) logfile: Option<PathBuf>,

    /// Run in foreground, do not daemonize.
    #[arg(short, long)]
    pub(crate) foreground: bool,

    /// Nice level.
    #[arg(short, long, default_value_t = 2)]
    #[arg(value_parser = validate_nice)]
    _nice: i8,

    #[command(flatten)]
    pub(crate) verbosity: Verbosity<WarnLevel>,
}

/// Check if the file exists.
#[inline(always)]
fn validate_file(file: &str) -> Result<PathBuf, String> {
    let path = Path::new(file);
    if path.exists() {
        Ok(path.to_owned())
    } else {
        Err(format!("File not found: {:?}", path))
    }
}

/// Validate niceness level
#[inline(always)]
fn validate_nice(nice: &str) -> Result<i8, String> {
    let nice: i8 = nice
        .parse()
        .map_err(|_| format!("`{nice}` is not a valid nice number"))?;
    if (-20..=19).contains(&nice) {
        Ok(nice)
    } else {
        Err("Nice level must be between -20 and 19".to_string())
    }
}
