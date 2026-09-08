//! Parse errors for TEI streaming.

use thiserror::Error;

/// Failure while streaming CBETA TEI P5.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Underlying XML reader failure.
    #[error("xml: {0}")]
    Xml(String),
    /// Required TEI attribute or structure is missing.
    #[error("missing {0}")]
    Missing(&'static str),
}

impl From<quick_xml::Error> for Error {
    fn from(value: quick_xml::Error) -> Self {
        Self::Xml(value.to_string())
    }
}

/// Convenient result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
