//! Kernel primitives for Wicket: identifiers, quantities, money, and inversion traits.
//!
//! This crate has no database dependency and no async. Domain math uses [`Quantity`];
//! plumbing uses [`AnyQuantity`]. Residuals are un-ignorable: read them with `into_exact`
//! or `split`, never by field access.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

#[cfg(test)]
use proptest as _;
#[cfg(test)]
use serde_json as _;
#[cfg(test)]
use trybuild as _;

pub mod actor;
pub mod convert;
pub mod error;
pub mod id;
pub mod money;
pub mod posting;
pub mod quantity;
pub mod residual;
pub mod signature;
pub mod units;

pub use actor::{Actor, ActorKind};
pub use convert::{ConversionContext, UnitCatalog, UnitConverter};
pub use error::{Error, Result};
pub use id::{Identifier, ItemId, LocationId, LotId, SerialId, SignatureId, UserId};
pub use money::{MONEY_MAX_SCALE, Money, MoneyError, MoneyWire, RATE_MAX_SCALE, UnitCost};
pub use posting::{
    Boundary, ConsumptionPosting, CostElement, GroupKind, NoPostings, PostingError,
    PostingGroupHeader, PostingHandle, PostingId, PostingIntent, PostingSink, QuantityPosting,
    ValueAccount, ValuePosting,
};
pub use quantity::{AnyQuantity, QUANTITY_MAX_SCALE, Quantity, QuantityError};
pub use residual::{Converted, Extended, ResidualError, Rounding, Scaled, Settled};
pub use signature::{
    NoSignatures, PermissionKey, RecordRef, SignatureError, SignatureGate, SignatureMeaning,
    SignatureRequirement, SignatureToken,
};
pub use units::{
    AreaDim, CountDim, CurrencyId, Dimension, DimensionKind, LengthDim, MassDim, TimeDim, UnitId,
    UnitRef, VolumeDim,
};
