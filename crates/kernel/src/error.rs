#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to load config: {0}")]
    ConfigLoadFailed(#[from] config::Error),
}
