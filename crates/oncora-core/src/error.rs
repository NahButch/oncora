//! Library error type. Binaries layer `anyhow` on top of this.

use thiserror::Error;

/// The crate-wide error type returned by provider traits.
#[derive(Error, Debug)]
pub enum OncoraError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error("provider error: {0}")]
    Provider(String),

    #[error("tool error: {0}")]
    Tool(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, OncoraError>;
