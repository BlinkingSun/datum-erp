//! Crate error type.

use datum_core::SignatureError;

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Crate error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented.
    #[error("unimplemented")]
    Unimplemented,
    /// Core error.
    #[error(transparent)]
    Core(#[from] datum_core::Error),
    /// Database error.
    #[error(transparent)]
    Db(#[from] datum_db::Error),
    /// Identity / credential error.
    #[error(transparent)]
    Identity(#[from] datum_identity::Error),
    /// Signature gate refusal.
    #[error(transparent)]
    Signature(#[from] SignatureError),
    /// Required identification is missing (401 `SIGNATURE_REQUIRED`).
    #[error("signature required: {field}")]
    SignatureRequired {
        /// JSON field path (`identification.secret`).
        field: String,
    },
    /// Request failed validation (400 `VALIDATION`).
    #[error("{message}")]
    Validation {
        /// JSON field path, when the error is about one field.
        field: Option<String>,
        /// Human message.
        message: String,
    },
    /// Conflict (409 `CONFLICT`).
    #[error("{message}")]
    Conflict {
        /// Human message.
        message: String,
    },
    /// Signature row was not found.
    #[error("signature not found")]
    NotFound,
    /// Invariant violated.
    #[error("invariant violated: {0}")]
    Invariant(String),
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Invariant(err.to_string())
    }
}

pub(crate) fn map_tx(err: datum_db::Error) -> Error {
    match err {
        datum_db::Error::Sqlx(e) => Error::from(e),
        other => Error::Db(other),
    }
}
