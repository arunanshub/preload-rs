#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid sort strategy: {0}")]
    InvalidSortStrategy(u8),

    #[error("Failed to parse TOML file: {0}")]
    DeserializeTOML(#[from] toml::de::Error),

    #[error("Failed to serialize TOML: {0}")]
    SerializeTOML(#[from] toml::ser::Error),

    #[error("Failed to read file: {0}")]
    Io(#[from] std::io::Error),
}
