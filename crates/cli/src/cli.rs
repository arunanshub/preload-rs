use clap::Parser;
use clap_verbosity_flag::{Verbosity, WarnLevel};
use std::path::{Path, PathBuf};

/// preload-rs: The memory safe system optimizer
///
/// preload-rs is an adaptive readahead daemon that prefetches files mapped by
/// applications from the disk to reduce application startup time.
#[derive(Debug, Parser, Clone)]
#[command(about, long_about, version)]
pub struct Cli {
    /// Path to configuration file.
    ///
    /// If not provided, the default locations are checked. They are
    /// `/etc/preload-rs/config.toml` and `/etc/preload-rs/config.d/*.toml`,
    /// where the latter being a glob pattern. If they don't exist, the default
    /// configuration is used.
    #[arg(short, long, value_parser = validate_file)]
    pub conffile: Option<PathBuf>,

    /// File to load and save application state to.
    ///
    /// Empty string means state is stored in memory.
    #[arg(short, long)]
    pub statefile: Option<String>,

    /// Path to log file.
    ///
    /// Empty string means log to stderr.
    #[arg(short, long)]
    pub logfile: Option<PathBuf>,

    /// Run in foreground, do not daemonize.
    #[arg(short, long)]
    pub foreground: bool,

    /// Nice level.
    #[arg(short, long, default_value_t = 2)]
    #[arg(value_parser = validate_nice)]
    _nice: i8,

    #[command(flatten)]
    pub verbosity: Verbosity<WarnLevel>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn nice_candidates() -> impl Strategy<Value = String> {
        prop_oneof![
            2 => (-50..50).prop_map(|i| format!("{}", i)),
            1 => (-1000..=1000).prop_map(|i| format!("{}", i)),
            1 => ".*",
        ]
    }

    proptest! {
        #[test]
        fn test_validate_nice(nice in nice_candidates()) {
            let result = validate_nice(&nice);
            match result {
                Ok(n) => prop_assert!((-20..=19).contains(&n)),
                Err(err) => {
                    let error_msg = format!("`{}` is not a valid nice number", nice);
                    prop_assert!(
                        err == error_msg || err == "Nice level must be between -20 and 19"
                    );
                },
            }
        }
    }
}
