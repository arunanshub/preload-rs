#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid sort strategy: {0}")]
    InvalidSortStrategy(u8),

    #[error("Failed to serialize TOML: {0}")]
    SerializeTOML(#[from] toml_edit::ser::Error),

    #[error("Failed to read configuration: {0}")]
    Extract(#[from] figment::Error),

    #[error("Failed to read file: {0}")]
    Io(#[from] std::io::Error),
}
