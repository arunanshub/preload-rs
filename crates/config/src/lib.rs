#![forbid(unsafe_code)]

mod error;
mod memory_policy;
mod model;
mod persistence;
mod sort_strategy;
mod system;

pub use error::Error;
pub use memory_policy::MemoryPolicy;
pub use model::Model;
pub use persistence::Persistence;
pub use sort_strategy::SortStrategy;
pub use system::System;

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub model: Model,
    pub system: System,
    pub persistence: Persistence,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: Model::default(),
            system: System::default(),
            persistence: Persistence::default(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file. Missing fields are filled with defaults.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let text = std::fs::read_to_string(path)?;
        let mut config: Config = toml_edit::de::from_str(&text)?;
        config.apply_defaults();
        Ok(config)
    }

    /// Save configuration to a TOML file.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let toml = toml_edit::ser::to_string_pretty(self)?;
        std::fs::write(path, toml)?;
        Ok(())
    }

    /// Load configuration from multiple TOML files. Later files override earlier ones.
    pub fn load_multiple<T, U>(paths: U) -> Result<Self, Error>
    where
        T: AsRef<Path>,
        U: IntoIterator<Item = T>,
    {
        let mut merged = toml_edit::DocumentMut::new();
        for path in paths {
            let path = path.as_ref();
            if !path.exists() {
                continue;
            }
            let text = std::fs::read_to_string(path)?;
            let doc: toml_edit::DocumentMut = text.parse()?;
            merge_document(&mut merged, doc);
        }
        let mut config: Config = toml_edit::de::from_str(&merged.to_string())?;
        config.apply_defaults();
        Ok(config)
    }

    fn apply_defaults(&mut self) {
        // Ensure vectors are sorted for prefix matching semantics.
        self.system.exeprefix.sort();
        self.system.mapprefix.sort();
    }
}

fn merge_document(target: &mut toml_edit::DocumentMut, source: toml_edit::DocumentMut) {
    for (key, item) in source.iter() {
        merge_item(
            target.entry(key).or_insert(toml_edit::Item::None),
            item.clone(),
        );
    }
}

fn merge_item(target: &mut toml_edit::Item, source: toml_edit::Item) {
    use toml_edit::Item;
    match (target, source) {
        (Item::Table(target_table), Item::Table(source_table)) => {
            for (key, item) in source_table.iter() {
                merge_item(target_table.entry(key).or_insert(Item::None), item.clone());
            }
        }
        (Item::ArrayOfTables(target_array), Item::ArrayOfTables(source_array)) => {
            for table in source_array.iter() {
                target_array.push(table.clone());
            }
        }
        (target_item, source_item) => {
            *target_item = source_item;
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
        let path = dir.path().join("config.toml");

        let mut config = Config::default();
        config.apply_defaults();
        config.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();

        assert_eq!(config, loaded);
    }

    #[test]
    fn load_multiple_merges() {
        let dir = tempdir().unwrap();
        let path1 = dir.path().join("a.toml");
        let path2 = dir.path().join("b.toml");

        std::fs::write(&path1, "[model]\ncycle = 60\n[system]\ndoscan = false\n").unwrap();
        std::fs::write(&path2, "[persistence]\nautosave_interval = 120\n").unwrap();

        let cfg = Config::load_multiple([path1, path2]).unwrap();
        assert_eq!(cfg.model.cycle, Duration::from_secs(60));
        assert!(!cfg.system.doscan);
        assert_eq!(
            cfg.persistence.autosave_interval,
            Some(Duration::from_secs(120))
        );
    }
}
