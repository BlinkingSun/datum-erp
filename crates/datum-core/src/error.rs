//! Shared crate error. Downstream crates wrap this with `From`.

use crate::money::MoneyError;
use crate::posting::PostingError;
use crate::quantity::QuantityError;
use crate::residual::ResidualError;
use crate::signature::SignatureError;

/// Shared kernel error. Downstream crates add their own `Error` and `From<datum_core::Error>`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// An invariant the caller violated, with a human-readable reason.
    #[error("invariant violated: {0}")]
    Invariant(String),
    /// Arithmetic that `Decimal` cannot represent.
    #[error("arithmetic overflow")]
    Overflow,
    /// Quantity construction or arithmetic failed.
    #[error(transparent)]
    Quantity(#[from] QuantityError),
    /// Money construction or arithmetic failed.
    #[error(transparent)]
    Money(#[from] MoneyError),
    /// A residual was demanded to be zero and was not.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// A posting sink rejected a contribution or finalize.
    #[error(transparent)]
    Posting(#[from] PostingError),
    /// A signature gate rejected a token.
    #[error(transparent)]
    Signature(#[from] SignatureError),
    /// The called operation is not implemented in this crate.
    #[error("unimplemented")]
    Unimplemented,
}

/// Result alias used by kernel-facing APIs that share [`Error`].
pub type Result<T> = core::result::Result<T, Error>;
