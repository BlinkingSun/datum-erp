//! Dimensioned quantity. No operator traits, no serde, no `PartialOrd`.

use crate::residual::Scaled;
use crate::units::{Dimension, DimensionKind, UnitId, UnitRef};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Maximum decimal places any quantity may carry. Matches the `numeric(24,8)` posting
/// column. A quantity with more precision is an error, never a truncation.
pub const QUANTITY_MAX_SCALE: u32 = 8;

/// Integer digits stored by PostgreSQL `numeric(24,8)`. `|amount| >= 10^16` is overflow.
const QUANTITY_MAX_INTEGER_DIGITS: u32 = 16;

/// True when `|amount|` has more than `max_integer_digits` digits before the decimal.
pub(crate) fn exceeds_integer_width(amount: Decimal, max_integer_digits: u32) -> bool {
    let Some(limit_i) = 10i128.checked_pow(max_integer_digits) else {
        return true;
    };
    match Decimal::try_from_i128_with_scale(limit_i, 0) {
        Ok(limit) => amount.abs() >= limit,
        Err(_) => true,
    }
}

/// Errors from quantity construction and same-unit arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum QuantityError {
    /// The catalog said this unit is a different dimension than the type parameter.
    #[error("unit {unit:?} is dimension {actual:?}, expected {expected:?}")]
    DimensionMismatch {
        /// Unit that failed the check.
        unit: UnitId,
        /// Dimension the type parameter required.
        expected: DimensionKind,
        /// Dimension the catalog reported.
        actual: DimensionKind,
    },
    /// Same dimension, different catalog unit; convert first.
    #[error("cannot combine quantities in units {left:?} and {right:?} without conversion")]
    UnitMismatch {
        /// Left-hand unit.
        left: UnitId,
        /// Right-hand unit.
        right: UnitId,
    },
    /// Caller supplied more fractional digits than [`QUANTITY_MAX_SCALE`].
    #[error("scale {found} exceeds maximum {max}")]
    ScaleExceeded {
        /// Scale on the input.
        found: u32,
        /// Maximum permitted scale.
        max: u32,
    },
    /// Amount exceeds `numeric(24,8)` (`|amount| >= 10^16`) or `Decimal` could not
    /// represent the result.
    #[error("arithmetic overflow")]
    Overflow,
    /// Division by a zero quantity.
    #[error("division by zero")]
    DivideByZero,
    /// An operation that must be exact would have rounded.
    #[error("operation would not be exact")]
    Inexact,
    /// No conversion path in the catalog for this item.
    #[error("no conversion path from {from:?} to {to:?} for item {item:?}")]
    NoConversionPath {
        /// Source unit.
        from: UnitId,
        /// Destination unit.
        to: UnitId,
        /// Item whose factor was requested.
        item: crate::id::ItemId,
    },
    /// The catalog does not know this unit.
    #[error("unknown unit {0:?}")]
    UnknownUnit(UnitId),
}

/// A signed decimal amount in a specific unit of a specific dimension.
///
/// Signed because postings are signed. Non-negativity is a business rule, not a type.
/// This type is not `Serialize`: a type parameter cannot round-trip through JSON.
/// Use [`AnyQuantity`] at the wire and database boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Quantity<D: Dimension> {
    amount: Decimal,
    unit: UnitRef<D>,
}

impl<D: Dimension> Quantity<D> {
    pub(crate) fn from_raw(amount: Decimal, unit: UnitRef<D>) -> Self {
        Self { amount, unit }
    }

    /// Rejects scale > [`QUANTITY_MAX_SCALE`] and overflowing input.
    /// Never rounds. Never truncates.
    ///
    /// Overflow is the PostgreSQL `numeric(24,8)` integer-width bound: more than
    /// 16 integer digits (`|amount| >= 10^16`) returns [`QuantityError::Overflow`].
    /// The value is never truncated to fit.
    pub fn new(amount: Decimal, unit: UnitRef<D>) -> Result<Self, QuantityError> {
        if amount.scale() > QUANTITY_MAX_SCALE {
            return Err(QuantityError::ScaleExceeded {
                found: amount.scale(),
                max: QUANTITY_MAX_SCALE,
            });
        }
        if exceeds_integer_width(amount, QUANTITY_MAX_INTEGER_DIGITS) {
            return Err(QuantityError::Overflow);
        }
        Ok(Self { amount, unit })
    }

    /// Zero in `unit`. A zero still has a unit; an empty sum does not.
    pub fn zero(unit: UnitRef<D>) -> Self {
        Self {
            amount: Decimal::ZERO,
            unit,
        }
    }

    /// Signed amount. Scale is at most [`QUANTITY_MAX_SCALE`] when built via [`Self::new`].
    pub fn amount(self) -> Decimal {
        self.amount
    }

    /// Proven unit of this quantity.
    pub fn unit(self) -> UnitRef<D> {
        self.unit
    }

    /// Catalog id of the unit.
    pub fn unit_id(self) -> UnitId {
        self.unit.id()
    }

    /// Dimension of `D`.
    pub fn kind(self) -> DimensionKind {
        D::KIND
    }

    /// Whether the amount is zero. The unit is still present.
    pub fn is_zero(self) -> bool {
        self.amount.is_zero()
    }

    /// `-1`, `0`, or `1` according to the sign of the amount.
    pub fn signum(self) -> i8 {
        if self.amount.is_zero() {
            0
        } else if self.amount.is_sign_negative() {
            -1
        } else {
            1
        }
    }

    /// Add two quantities of the same unit. Different units are [`QuantityError::UnitMismatch`].
    pub fn try_add(self, rhs: Self) -> Result<Self, QuantityError> {
        if self.unit.id() != rhs.unit.id() {
            return Err(QuantityError::UnitMismatch {
                left: self.unit.id(),
                right: rhs.unit.id(),
            });
        }
        let amount = self
            .amount
            .checked_add(rhs.amount)
            .ok_or(QuantityError::Overflow)?;
        Ok(Self {
            amount,
            unit: self.unit,
        })
    }

    /// Subtract two quantities of the same unit.
    pub fn try_sub(self, rhs: Self) -> Result<Self, QuantityError> {
        self.try_add(rhs.negate())
    }

    /// Add in place. On error `self` is unchanged.
    pub fn try_add_assign(&mut self, rhs: Self) -> Result<(), QuantityError> {
        *self = self.try_add(rhs)?;
        Ok(())
    }

    /// Arithmetic negation. Unit is unchanged.
    pub fn negate(self) -> Self {
        Self {
            amount: -self.amount,
            unit: self.unit,
        }
    }

    /// Absolute value. Unit is unchanged.
    pub fn abs(self) -> Self {
        Self {
            amount: self.amount.abs(),
            unit: self.unit,
        }
    }

    /// Ordering must be fallible: `10 ft > 36 in` compared by `amount` is a bug that
    /// would otherwise compile and return a wrong `bool`.
    pub fn try_cmp(self, rhs: Self) -> Result<core::cmp::Ordering, QuantityError> {
        if self.unit.id() != rhs.unit.id() {
            return Err(QuantityError::UnitMismatch {
                left: self.unit.id(),
                right: rhs.unit.id(),
            });
        }
        Ok(self.amount.cmp(&rhs.amount))
    }

    /// Scalar scaling (BOM multiplier, yield factor). Exact or error.
    ///
    /// Exact means the product divided by `factor` recovers `self` when `factor` is
    /// non-zero, and the product's scale is at most [`QUANTITY_MAX_SCALE`].
    pub fn try_scale_exact(self, factor: Decimal) -> Result<Self, QuantityError> {
        let product = self
            .amount
            .checked_mul(factor)
            .ok_or(QuantityError::Overflow)?;
        if !factor.is_zero() {
            let recovered = product.checked_div(factor).ok_or(QuantityError::Overflow)?;
            if recovered != self.amount {
                return Err(QuantityError::Inexact);
            }
        }
        Self::new(product, self.unit)
    }

    /// Scalar scaling that hands you the residual. [`Scaled<D>`] is un-ignorable.
    ///
    /// Overflow exits (when `amount.checked_mul(factor)` is `None`):
    /// [`Scaled::into_exact`] returns [`crate::residual::ResidualError::NotExact`]
    /// with residual `0` and this quantity's unit; [`Scaled::split`] returns
    /// `(zero, zero)` in this unit (`split` is a tuple, so there is no `Err` path;
    /// the zeros do not reconstruct the pre-rounding amount, D1 §2.6);
    /// [`Scaled::has_residual`] is `true`. This is not an exact success.
    pub fn scale(self, factor: Decimal, to_scale: u32) -> Scaled<D> {
        match self.amount.checked_mul(factor) {
            Some(product) => Scaled::from_unrounded(product, self.unit, to_scale),
            None => Scaled::overflow(self.unit, to_scale),
        }
    }

    /// Dimensionless ratio. There is deliberately no `Div` impl on `Quantity`.
    pub fn try_ratio(self, rhs: Self) -> Result<Decimal, QuantityError> {
        if self.unit.id() != rhs.unit.id() {
            return Err(QuantityError::UnitMismatch {
                left: self.unit.id(),
                right: rhs.unit.id(),
            });
        }
        if rhs.amount.is_zero() {
            return Err(QuantityError::DivideByZero);
        }
        self.amount
            .checked_div(rhs.amount)
            .ok_or(QuantityError::Overflow)
    }

    /// Summation. `None` for an empty iterator, because a zero has no unit.
    pub fn try_sum<I: IntoIterator<Item = Self>>(items: I) -> Result<Option<Self>, QuantityError> {
        let mut iter = items.into_iter();
        let Some(first) = iter.next() else {
            return Ok(None);
        };
        let mut acc = first;
        for item in iter {
            acc = acc.try_add(item)?;
        }
        Ok(Some(acc))
    }
}

/// The boundary type. What crosses HTTP. What `datum-db` maps to a row. The only
/// serde-bearing quantity representation in the kernel.
///
/// `amount` is a JSON string, never a float.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnyQuantity {
    /// Amount serialized with `rust_decimal` `serde-with-str`.
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    /// Catalog unit.
    pub unit: UnitId,
    /// Denormalized so the boundary can check itself without a catalog round-trip.
    pub dimension: DimensionKind,
}

impl<D: Dimension> From<Quantity<D>> for AnyQuantity {
    fn from(q: Quantity<D>) -> Self {
        Self {
            amount: q.amount(),
            unit: q.unit_id(),
            dimension: q.kind(),
        }
    }
}

impl AnyQuantity {
    /// Runtime -> compile time. The one place a dimension error can occur on read.
    pub fn downcast<D: Dimension>(self) -> Result<Quantity<D>, QuantityError> {
        let unit = UnitRef::<D>::checked(self.unit, self.dimension)?;
        Quantity::new(self.amount, unit)
    }

    /// Same-unit arithmetic on erased values, for generic ledger code that must not
    /// know the dimension. Requires equal `unit` AND equal `dimension`.
    pub fn try_add(self, rhs: Self) -> Result<Self, QuantityError> {
        if self.dimension != rhs.dimension {
            return Err(QuantityError::DimensionMismatch {
                unit: rhs.unit,
                expected: self.dimension,
                actual: rhs.dimension,
            });
        }
        if self.unit != rhs.unit {
            return Err(QuantityError::UnitMismatch {
                left: self.unit,
                right: rhs.unit,
            });
        }
        let amount = self
            .amount
            .checked_add(rhs.amount)
            .ok_or(QuantityError::Overflow)?;
        Ok(Self {
            amount,
            unit: self.unit,
            dimension: self.dimension,
        })
    }

    /// Sum erased quantities. `None` if the iterator is empty.
    pub fn try_sum<I: IntoIterator<Item = Self>>(i: I) -> Result<Option<Self>, QuantityError> {
        let mut iter = i.into_iter();
        let Some(first) = iter.next() else {
            return Ok(None);
        };
        let mut acc = first;
        for item in iter {
            acc = acc.try_add(item)?;
        }
        Ok(Some(acc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{CountDim, LengthDim};
    use rust_decimal::Decimal;

    fn count(n: i64, scale: u32) -> Quantity<CountDim> {
        let unit = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).unwrap();
        Quantity::new(Decimal::new(n, scale), unit).unwrap()
    }

    #[test]
    fn quantity_new_never_truncates() {
        let unit = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).unwrap();
        let over = Decimal::new(1, QUANTITY_MAX_SCALE + 1);
        assert!(over.scale() > QUANTITY_MAX_SCALE);
        let err = Quantity::new(over, unit).unwrap_err();
        assert_eq!(
            err,
            QuantityError::ScaleExceeded {
                found: QUANTITY_MAX_SCALE + 1,
                max: QUANTITY_MAX_SCALE,
            }
        );
        assert_eq!(over.scale(), QUANTITY_MAX_SCALE + 1);
    }

    #[test]
    fn quantity_new_rejects_integer_overflow() {
        let unit = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).unwrap();
        let mut accepted = Decimal::from_i128_with_scale(10i128.pow(16) - 1, 0);
        accepted.rescale(QUANTITY_MAX_SCALE);
        assert_eq!(accepted.scale(), QUANTITY_MAX_SCALE);
        let q = Quantity::new(accepted, unit).expect("10^16 - 1 at scale 8 fits numeric(24,8)");
        assert_eq!(q.amount(), accepted);

        let rejected = Decimal::from_i128_with_scale(10i128.pow(16), 0);
        assert_eq!(
            Quantity::new(rejected, unit).unwrap_err(),
            QuantityError::Overflow
        );
        let mut rejected_at_scale = rejected;
        rejected_at_scale.rescale(QUANTITY_MAX_SCALE);
        assert_eq!(
            Quantity::new(rejected_at_scale, unit).unwrap_err(),
            QuantityError::Overflow
        );
        assert_eq!(
            Quantity::new(-rejected, unit).unwrap_err(),
            QuantityError::Overflow
        );
    }

    #[test]
    fn scale_overflow_exits() {
        let unit = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).unwrap();
        let q = Quantity::from_raw(Decimal::MAX, unit);
        let scaled = q.scale(Decimal::from(2u8), QUANTITY_MAX_SCALE);
        assert!(scaled.has_residual());
        assert!(matches!(
            scaled.into_exact(),
            Err(crate::residual::ResidualError::NotExact { .. })
        ));
        let (value, residual) = q
            .scale(Decimal::from(2u8), QUANTITY_MAX_SCALE)
            .split(crate::residual::Rounding::HalfUp);
        assert!(value.is_zero());
        assert!(residual.is_zero());
        assert_eq!(value.unit(), unit);
        assert_eq!(residual.unit(), unit);
    }

    #[test]
    fn try_add_unit_mismatch() {
        let a_unit = UnitRef::<LengthDim>::checked(UnitId(1), DimensionKind::Length).unwrap();
        let b_unit = UnitRef::<LengthDim>::checked(UnitId(2), DimensionKind::Length).unwrap();
        let a = Quantity::new(Decimal::from(10), a_unit).unwrap();
        let b = Quantity::new(Decimal::from(36), b_unit).unwrap();
        let err = a.try_add(b).unwrap_err();
        assert_eq!(
            err,
            QuantityError::UnitMismatch {
                left: UnitId(1),
                right: UnitId(2),
            }
        );
    }

    #[test]
    fn try_cmp_unit_mismatch() {
        let a_unit = UnitRef::<LengthDim>::checked(UnitId(1), DimensionKind::Length).unwrap();
        let b_unit = UnitRef::<LengthDim>::checked(UnitId(2), DimensionKind::Length).unwrap();
        let a = Quantity::new(Decimal::from(10), a_unit).unwrap();
        let b = Quantity::new(Decimal::from(36), b_unit).unwrap();
        assert!(matches!(
            a.try_cmp(b),
            Err(QuantityError::UnitMismatch { .. })
        ));
    }

    #[test]
    fn try_sum_empty() {
        let items: Vec<Quantity<CountDim>> = Vec::new();
        assert_eq!(Quantity::try_sum(items), Ok(None));
    }

    #[test]
    fn try_sum_same_unit() {
        let a = count(1, 0);
        let b = count(2, 0);
        let sum = Quantity::try_sum([a, b]).unwrap().unwrap();
        assert_eq!(sum.amount(), Decimal::from(3));
    }

    #[test]
    fn try_add_overflow() {
        let unit = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).unwrap();
        let a = Quantity::from_raw(Decimal::MAX, unit);
        let b = Quantity::from_raw(Decimal::MAX, unit);
        assert_eq!(a.try_add(b).unwrap_err(), QuantityError::Overflow);
    }

    #[test]
    fn try_ratio_divide_by_zero() {
        let a = count(1, 0);
        let z = count(0, 0);
        assert_eq!(a.try_ratio(z).unwrap_err(), QuantityError::DivideByZero);
    }

    #[test]
    fn negate_involution() {
        let a = count(-5, 2);
        assert_eq!(a.negate().negate(), a);
    }

    #[test]
    fn dimension_mismatch_on_downcast() {
        let q = AnyQuantity {
            amount: Decimal::from(1),
            unit: UnitId(1),
            dimension: DimensionKind::Mass,
        };
        let err = q.downcast::<LengthDim>().unwrap_err();
        assert!(matches!(err, QuantityError::DimensionMismatch { .. }));
    }

    #[test]
    fn any_quantity_try_add_requires_unit_and_dimension() {
        let a = AnyQuantity {
            amount: Decimal::from(1),
            unit: UnitId(1),
            dimension: DimensionKind::Count,
        };
        let b = AnyQuantity {
            amount: Decimal::from(1),
            unit: UnitId(1),
            dimension: DimensionKind::Mass,
        };
        assert!(matches!(
            a.try_add(b),
            Err(QuantityError::DimensionMismatch { .. })
        ));
        let c = AnyQuantity {
            amount: Decimal::from(1),
            unit: UnitId(2),
            dimension: DimensionKind::Count,
        };
        assert!(matches!(
            a.try_add(c),
            Err(QuantityError::UnitMismatch { .. })
        ));
    }
}
