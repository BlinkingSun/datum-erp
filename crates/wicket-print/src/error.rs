//! Crate errors.

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures from rendering, templates, and archival print.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented.
    #[error("unimplemented")]
    Unimplemented,
    /// Core error.
    #[error(transparent)]
    Core(#[from] wicket_core::Error),
    /// Database error.
    #[error(transparent)]
    Db(#[from] wicket_db::Error),
    /// Documents / blob store.
    #[error(transparent)]
    Documents(#[from] wicket_documents::Error),
    /// E-sign read path.
    #[error(transparent)]
    Esign(#[from] wicket_esign::Error),
    /// Template or record payload missing.
    #[error("not found")]
    NotFound,
    /// Unknown template id.
    #[error("unknown template: {0}")]
    UnknownTemplate(String),
    /// Unsupported output format.
    #[error("unsupported format")]
    UnsupportedFormat,
    /// Blob root missing for archive.
    #[error("WICKET_BLOB_ROOT is not set")]
    BlobRootMissing,
    /// Installation profile id is not a known profile.
    #[error("unknown installation profile: {0}")]
    UnknownProfile(String),
}
