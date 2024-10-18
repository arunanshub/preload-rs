mod error;
mod model;
mod sort_strategy;
mod system;

pub use error::Error;
pub use model::Model;
use serde::{Deserialize, Serialize};
pub use sort_strategy::SortStrategy;
use std::path::Path;
pub use system::System;

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub model: Model,
    pub system: System,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the configuration file from a path.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let config = std::fs::read_to_string(path)?;
        Ok(toml_edit::de::from_str(&config)?)
    }

    /// Save configuration to a file.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let config = toml_edit::ser::to_string_pretty(self)?;
        std::fs::write(path, config)?;
        Ok(())
    }

    /// Save the configuration to a file if it doesn't exist, otherwise load it.
    pub fn save_and_load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        if path.exists() {
            Self::load(path)
        } else {
            let config = Self::default();
            config.save(path)?;
            Ok(config)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn roundtrip() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("config.toml");

        let config = Config::new();
        config.save(&file).unwrap();
        let config2 = Config::load(file).unwrap();
        assert_eq!(config, config2);
    }

    #[test]
    fn save_and_load() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("config.toml");
        assert!(!file.exists());

        // file does not exist yet: it will be created on first call
        let config = Config::save_and_load(&file).unwrap();
        assert!(file.exists());
        // now it exists, so it will be loaded
        let mut config2 = Config::save_and_load(&file).unwrap();
        assert_eq!(config, config2);
        // modify the config and save it
        config2.model.cycle = Duration::from_secs(124);
        config2.save(&file).unwrap();
        // the loaded config should match the modified one
        let config3 = Config::save_and_load(&file).unwrap();
        assert_eq!(config2, config3);
    }
}
