use clap::Parser;
use std::path::{Path, PathBuf};

/// Command line interface for preload-rs.
#[derive(Debug, Parser, Clone)]
#[command(about, long_about, version)]
pub struct Cli {
    /// Path to a configuration file.
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Path to a directory containing additional TOML config files.
    #[arg(long, value_name = "DIR")]
    pub config_dir: Option<PathBuf>,

    /// Path to the state database.
    #[arg(short, long, value_name = "FILE")]
    pub state: Option<PathBuf>,

    /// Run a single tick and exit.
    #[arg(long)]
    pub once: bool,

    /// Disable persistence entirely.
    #[arg(long)]
    pub no_persist: bool,

    /// Disable prefetch I/O (observe/predict only).
    #[arg(long)]
    pub no_prefetch: bool,
}

impl Cli {
    /// Resolve configuration paths in precedence order (earlier overridden by later).
    pub fn resolve_config_paths(&self) -> Result<Vec<PathBuf>, std::io::Error> {
        let mut paths = Vec::new();

        if let Some(config) = &self.config {
            ensure_file_exists(config)?;
            paths.push(config.clone());

            if let Some(dir) = &self.config_dir {
                paths.extend(collect_toml(dir, true)?);
            }

            return Ok(paths);
        }

        if let Some(path) = system_config_path()
            && path.exists()
        {
            paths.push(path);
        }

        if let Some(dir) = system_config_dir()
            && dir.is_dir()
        {
            paths.extend(collect_toml(&dir, false)?);
        }

        if let Some(path) = user_config_path()
            && path.exists()
        {
            paths.push(path);
        }

        let local = PathBuf::from("config.toml");
        if local.exists() {
            paths.push(local);
        }

        if let Some(dir) = &self.config_dir {
            paths.extend(collect_toml(dir, true)?);
        }

        Ok(paths)
    }
}

fn ensure_file_exists(path: &Path) -> Result<(), std::io::Error> {
    if path.exists() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("config file not found: {}", path.display()),
        ))
    }
}

fn collect_toml(dir: &Path, strict: bool) -> Result<Vec<PathBuf>, std::io::Error> {
    if !dir.exists() {
        if strict {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("config directory not found: {}", dir.display()),
            ));
        }
        return Ok(Vec::new());
    }
    if !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("config directory is not a directory: {}", dir.display()),
        ));
    }

    let mut paths = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn system_config_path() -> Option<PathBuf> {
    Some(PathBuf::from("/etc/preload-rs/config.toml"))
}

fn system_config_dir() -> Option<PathBuf> {
    Some(PathBuf::from("/etc/preload-rs/config.d"))
}

fn user_config_path() -> Option<PathBuf> {
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    xdg.map(|dir| dir.join("preload-rs").join("config.toml"))
}
