#![forbid(unsafe_code)]

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config error: {0}")]
    Config(#[from] config::Error),

    #[error("procfs error: {0}")]
    Procfs(#[from] procfs::ProcError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("serialization error: {0}")]
    RkyvSerialize(String),

    #[error("deserialization error: {0}")]
    RkyvDeserialize(String),

    #[error("invalid path: {0}")]
    InvalidPath(PathBuf),

    #[error("missing exe: {0}")]
    ExeMissing(PathBuf),

    #[error("missing map: {0:?}")]
    MapMissing(PathBuf),
}
