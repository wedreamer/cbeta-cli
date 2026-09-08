//! Index build and path errors.

use thiserror::Error;

/// Failures while building or locating a CBETA Tantivy index.
#[derive(Debug, Error)]
pub enum Error {
    /// Underlying Tantivy failure.
    #[error("tantivy: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    /// Directory open failure from Tantivy.
    #[error("open directory: {0}")]
    OpenDirectory(#[from] tantivy::directory::error::OpenDirectoryError),
    /// Filesystem I/O while creating or renaming the artifact dir.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Required environment or home directory is missing.
    #[error("{0}")]
    Path(String),
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
