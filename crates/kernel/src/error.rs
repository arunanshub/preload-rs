#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to load config: {0}")]
    ConfigLoadFailed(#[from] config::Error),

    #[error("Failed to read procfs info: {0}")]
    ProcfsReadFailed(#[from] procfs::ProcError),
}
