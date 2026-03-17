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
    /// A state machine error.
    State(#[from] StateError),
    /// A transaction failure.
    Txn(#[from] TxnError),
}

/// An I/O error.
#[derive(Debug, Error)]
#[error("IO error: {0}")]
pub struct IoError(#[from] std::io::Error);

/// A state machine error.
#[derive(Debug, Error)]
#[error("state machine error: {0}")]
pub struct StateError(String);

/// A transaction error.
#[derive(Debug, Error)]
#[error("transaction error: {0}")]
pub struct TxnError(String);
