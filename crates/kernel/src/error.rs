use std::path::PathBuf;

/// Represents all possible errors that can occur in this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Error occurred while loading the configuration file.
    #[error("Failed to load config: {0}")]
    ConfigLoadFailed(#[from] config::Error),

    /// Error occurred while reading data from procfs.
    #[error("Failed to read procfs info: {0}")]
    ProcfsReadFailed(#[from] procfs::ProcError),

    /// A field does not exist in procfs.
    #[error("Procfs field does not exist: {0}")]
    ProcfsFieldDoesNotExist(String),

    /// Error occurred while performing I/O operation on a file.
    #[error("Failed to perform I/O operation on file: {0}")]
    FileIOFailed(#[from] std::io::Error),

    /// Error occurred while performing a POSIX fadvise operation.
    ///
    /// # See Also
    ///
    /// [`readahead`](crate::utils::readahead)
    #[error("Failed to readahead: {0}")]
    ReadaheadFailed(#[from] nix::Error),

    /// The exe associated with markov has been deallocated.
    #[error("Exe associated with markov has been deallocated")]
    ExeMarkovDeallocated,

    /// The path is invalid.
    #[error("Path is invalid: {0}")]
    InvalidPath(PathBuf),

    /// Error occurred while performing a database operation.
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    /// Error occurred while performing a database migration operation.
    #[error("Failed to run database migration: {0}")]
    MigrationFailed(#[from] sqlx::migrate::MigrateError),

    /// Error occurred during performing a bincode serialization operation.
    #[error("Failed to serialize to bincode: {0}")]
    BincodeSerializeFailed(#[from] bincode::Error),

    /// Error occurred during joining async tasks.
    #[error("Failed to join async tasks: {0}")]
    JoinError(#[from] tokio::task::JoinError),

    /// Exe does not exist or it has not been assigned a sequence number.
    #[error("Exe {0:?} has not been assigned a sequence number")]
    ExeSeqNotAssigned(PathBuf),

    /// Exe does not exist
    #[error("Exe {0:?} does not exist")]
    ExeDoesNotExist(PathBuf),

    /// Map has not been assigned a sequence number.
    #[error("Map {path:?} has not been assigned a sequence number")]
    MapSeqNotAssigned {
        /// Path of the map.
        path: PathBuf,

        /// Offset of the map.
        offset: u64,

        /// Length of the map.
        length: u64,
    },

    /// Map with the given sequence number does not exist
    #[error("Map with sequence number {0:?} does not exist")]
    MapDoesNotExist(u64),
}
