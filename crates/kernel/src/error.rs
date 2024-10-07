#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to load config: {0}")]
    ConfigLoadFailed(#[from] config::Error),

    #[error("Failed to read procfs info: {0}")]
    ProcfsReadFailed(#[from] procfs::ProcError),

    #[error("Procfs field does not exist: {0}")]
    ProcfsFieldDoesNotExist(String),

    #[error("Failed to read file: {0}")]
    FileReadFailed(#[from] std::io::Error),

    #[error("Failed to readahead: {0}")]
    ReadaheadFailed(#[from] nix::Error),

    #[error("Exe associated with markov has been deallocated")]
    ExeDoesNotExist,
}
