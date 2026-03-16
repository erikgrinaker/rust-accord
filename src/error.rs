//! Accord error types.

use thiserror::Error;

/// An Accord result.
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level Accord error type.
#[derive(Debug, Error)]
#[error(transparent)]
pub enum Error {
    /// An I/O operation failed.
    Io(#[from] IoError),
}

/// An I/O error.
#[derive(Debug, Error)]
#[error("IO error: {0}")]
pub struct IoError(#[from] std::io::Error);
