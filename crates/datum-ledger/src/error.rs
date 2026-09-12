//! Crate errors, including the ZL000–ZL007 mapping (D2 §5.4).

use datum_core::{PostingError, PostingHandle, PostingId};

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures from posting, allocation, projections, and the deferred trigger.
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
    Db(datum_db::Error),
    /// A posting sink rejected a contribution or finalize.
    #[error(transparent)]
    Posting(#[from] PostingError),
    /// Units conversion failed.
    #[error(transparent)]
    Uom(#[from] datum_uom::Error),
    /// `ZL000`: postings exist without a group header.
    #[error("ledger: group has postings but no header")]
    GroupHasNoHeader,
    /// `ZL001`: a later transaction extended a settled group (P0).
    #[error("ledger: group was extended by a later transaction")]
    GroupExtended,
    /// `ZL002`: quantity slice does not net to zero (P1).
    #[error("ledger: quantity not conserved")]
    QuantityNotConserved,
    /// `ZL003`: value slice does not net to zero (P2-A).
    #[error("ledger: value not conserved")]
    ValueNotConserved,
    /// `ZL004`: a MOVEMENT reclassified cost elements (P2-B).
    #[error("ledger: MOVEMENT reclassifies a cost element")]
    CostElementReclassified,
    /// `ZL005`: consumption does not reproduce a withdrawal (P3).
    #[error("ledger: withdrawal is not covered by cost layers")]
    AllocationIncomplete,
    /// `ZL006`: a reversal is not the exact negation of its target (P4).
    #[error("ledger: reversal is not the exact negation")]
    ReversalNotExact,
    /// `ZL007`: a reversal does not restore the consumed layers (P4).
    #[error("ledger: reversal does not restore cost layers")]
    LayersNotRestored,
    /// Partial unique index `posting_group_reversed_once`.
    #[error("ledger: group has already been reversed")]
    AlreadyReversed,
    /// Sink took contributions and was dropped without [`crate::GroupBuilder`] finalize.
    #[error("posting sink dropped without finalize")]
    Unfinalized,
    /// Projection disagrees with the ledger fold.
    #[error("projection divergence: {0}")]
    ProjectionDivergence(String),
    /// Named group is missing.
    #[error("unknown posting group")]
    UnknownGroup,
    /// `parent_group_id` does not reference an existing `ledger.posting_group` row.
    #[error("ledger: parent_group_id must reference an existing posting group")]
    ParentMustExist,
    /// A required registry row is missing.
    #[error("unknown registry row: {0}")]
    UnknownRegistry(String),
}

impl From<datum_db::Error> for Error {
    fn from(err: datum_db::Error) -> Self {
        map_db(err)
    }
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::from(datum_db::Error::from(err))
    }
}

/// Map a PostgreSQL SQLSTATE in class `ZL` (and the reversed-once unique) onto
/// distinct [`Error`] variants.
pub fn map_sqlstate(code: &str, message: &str) -> Option<Error> {
    match code {
        "ZL000" => Some(Error::GroupHasNoHeader),
        "ZL001" => Some(Error::GroupExtended),
        "ZL002" => Some(Error::QuantityNotConserved),
        "ZL003" => Some(Error::ValueNotConserved),
        "ZL004" => Some(Error::CostElementReclassified),
        "ZL005" => Some(Error::AllocationIncomplete),
        "ZL006" => Some(Error::ReversalNotExact),
        "ZL007" => Some(Error::LayersNotRestored),
        "23505" if message.contains("posting_group_reversed_once") => Some(Error::AlreadyReversed),
        _ => None,
    }
}

fn map_db(err: datum_db::Error) -> Error {
    match &err {
        datum_db::Error::Sqlx(sqlx_err) => {
            let code = sqlx_err
                .as_database_error()
                .and_then(|d| d.code().map(|c| c.into_owned()))
                .unwrap_or_default();
            let message = sqlx_err.to_string();
            if let Some(mapped) = map_sqlstate(&code, &message) {
                return mapped;
            }
            if code == "23503" && message.contains("parent_group_id") {
                return Error::ParentMustExist;
            }
            Error::Db(err)
        }
        _ => Error::Db(err),
    }
}

impl Error {
    /// Convert a [`PostingError`] that this crate's `post` wrapper surfaces as [`Error`].
    pub(crate) fn from_posting(err: PostingError) -> Error {
        match err {
            PostingError::Unfinalized => Error::Unfinalized,
            other => Error::Posting(other),
        }
    }
}

/// Helper so tests can name the handle-bearing variants without importing core.
#[allow(dead_code)]
pub fn allocation_required(handle: PostingHandle) -> Error {
    Error::Posting(PostingError::AllocationRequired(handle))
}

/// Helper so tests can name ineligible-layer failures.
#[allow(dead_code)]
pub fn ineligible_layer(consuming: PostingHandle, consumed: PostingId) -> Error {
    Error::Posting(PostingError::IneligibleLayer {
        consuming,
        consumed,
    })
}
