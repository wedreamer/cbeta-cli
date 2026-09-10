//! Search errors.

use thiserror::Error;

/// Failures opening an index or running a query.
#[derive(Debug, Error)]
pub enum Error {
    /// No usable artifact under the index root.
    #[error("no index found under {0}; run: cbeta fetch --release 2026R2 --scope taisho")]
    NoIndex(String),
    /// Underlying Tantivy failure.
    #[error("tantivy: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    /// Directory open failure.
    #[error("open directory: {0}")]
    OpenDirectory(#[from] tantivy::directory::error::OpenDirectoryError),
    /// Filesystem I/O.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Schema field missing (corrupt or foreign index).
    #[error("schema: {0}")]
    Schema(String),
    /// Index path helper failure.
    #[error("{0}")]
    Path(String),
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
