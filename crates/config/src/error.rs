#![forbid(unsafe_code)]

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse TOML: {0}")]
    TomlDe(#[from] toml_edit::de::Error),

    #[error("failed to parse TOML document: {0}")]
    TomlParse(#[from] toml_edit::TomlError),

    #[error("failed to serialize TOML: {0}")]
    TomlSer(#[from] toml_edit::ser::Error),

    #[error("invalid path: {0}")]
    InvalidPath(PathBuf),
}
