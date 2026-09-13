//! Crate errors.

use datum_core::SignatureError;

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures from documents, blobs, numbering, and the approval machine.
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
    /// Numbering error.
    #[error(transparent)]
    Numbering(#[from] datum_numbering::Error),
    /// State-machine / executor error.
    #[error(transparent)]
    StateMachine(datum_statemachine::Error),
    /// Identity / RBAC error.
    #[error(transparent)]
    Identity(#[from] datum_identity::Error),
    /// Signature gate refused a Required edge.
    #[error(transparent)]
    Signature(#[from] SignatureError),
    /// Filesystem blob store I/O.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Document, revision, attachment, or blob was not found.
    #[error("not found")]
    NotFound,
    /// `DATUM_BLOB_ROOT` was missing or empty.
    #[error("DATUM_BLOB_ROOT is not set")]
    BlobRootMissing,
    /// On-disk bytes do not match the stored hash.
    #[error("blob corrupt: {hash}")]
    BlobCorrupt {
        /// Hex-encoded sha-256.
        hash: String,
    },
    /// Blob file is absent from the store.
    #[error("blob missing: {hash}")]
    BlobMissing {
        /// Hex-encoded sha-256.
        hash: String,
    },
    /// Write-once path already holds different bytes.
    #[error("blob write-once collision: {hash}")]
    BlobWriteOnce {
        /// Hex-encoded sha-256.
        hash: String,
    },
    /// Overlapping effectivity windows on one document.
    #[error("overlapping effectivity")]
    OverlappingEffectivity,
    /// `legal_hold` refuses Obsolete and retention actions.
    #[error("legal hold refuses this action")]
    LegalHold,
    /// Unknown installation profile.
    #[error("unknown profile: {0}")]
    UnknownProfile(String),
    /// Kind is empty or illegal for numbering.
    #[error("invalid document kind: {0}")]
    InvalidKind(String),
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}

impl From<datum_statemachine::Error> for Error {
    fn from(err: datum_statemachine::Error) -> Self {
        match err {
            datum_statemachine::Error::Signature(s) => Error::Signature(s),
            other => Error::StateMachine(other),
        }
    }
}
