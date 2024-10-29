mod error;
mod model;
mod sort_strategy;
mod system;

pub use error::Error;
use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};
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

    /// Load the configuration file from a path. The file **must exist**.
    ///
    /// Any missing fields are filled with default values. If you want to load
    /// and merge multiple files at once, or if the file(s) may or may not exist,
    /// use [`load_multiple`](Self::load_multiple).
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Figment::from(Serialized::defaults(Self::new()))
            .merge(Toml::file_exact(path))
            .extract()?)
    }

    /// Load configuration from multiple paths and merge them together.
    ///
    /// If the file does not exist, it is skipped.
    pub fn load_multiple<T, U>(paths: U) -> Result<Self, Error>
    where
        T: AsRef<Path>,
        U: IntoIterator<Item = T>,
    {
        let mut partial = Figment::from(Serialized::defaults(Self::new()));
        for path in paths {
            partial = partial.merge(Toml::file(path));
        }
        Ok(partial.extract()?)
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
    use std::{fs::write, time::Duration};
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

    #[test]
    fn load_partial() {
        let dir = tempdir().unwrap();
        let mut partial1 = dir.path().join("config1.toml");
        let mut partial2 = dir.path().join("config2.toml");
        let existent_but_empty = dir.path().join("existent_but_empty.toml");

        write(
            &mut partial1,
            r#"
        [model]
        cycle = 42069
        usecorrelation = true
        "#,
        )
        .unwrap();
        write(
            &mut partial2,
            r#"
        [system]
        sortstrategy = "path"
        "#,
        )
        .unwrap();

        let conf = Config::load_multiple(&[
            partial1,
            "nonexistent.toml".into(),
            existent_but_empty,
            partial2,
        ])
        .unwrap();
        assert_eq!(conf.model.cycle, Duration::from_secs(42069));
        assert_eq!(conf.system.sortstrategy, Some(SortStrategy::Path));
    }
}
