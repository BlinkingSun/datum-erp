//! Residual types. The value cannot be read without `into_exact` or `split`.

use crate::money::Money;
use crate::quantity::Quantity;
use crate::units::{CurrencyId, Dimension, UnitId, UnitRef};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

/// Rounding rule applied only at a residual-type exit. Core has no `round` on
/// [`Quantity`], [`Money`], or [`crate::UnitCost`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Rounding {
    /// Default for money settlement: 0.5 away from zero.
    HalfUp,
    /// Bankers' rounding; cost proration across many lines, avoids drift.
    HalfEven,
    /// Only where a physical count cannot exceed what exists.
    TowardZero,
    /// Directed rounding away from zero.
    AwayFromZero,
}

impl Rounding {
    fn strategy(self) -> RoundingStrategy {
        match self {
            Rounding::HalfUp => RoundingStrategy::MidpointAwayFromZero,
            Rounding::HalfEven => RoundingStrategy::MidpointNearestEven,
            Rounding::TowardZero => RoundingStrategy::ToZero,
            Rounding::AwayFromZero => RoundingStrategy::AwayFromZero,
        }
    }
}

/// A conversion or scaling did not divide evenly at the requested scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ResidualError {
    /// Use [`Converted::split`] (or the matching exit) and post the residual, or reject.
    #[error(
        "operation left residual {residual} in unit {unit:?}; use split() and post \
             the residual, or reject the transaction"
    )]
    NotExact {
        /// Leftover at the working precision.
        residual: Decimal,
        /// Unit of a quantity residual. `UnitId(0)` for money residuals (D1 froze this field).
        unit: UnitId,
    },
}

pub(crate) fn apply_rounding(amount: Decimal, scale: u32, rule: Rounding) -> Decimal {
    amount.round_dp_with_strategy(scale, rule.strategy())
}

fn remainder_toward_zero(amount: Decimal, scale: u32) -> Decimal {
    amount - apply_rounding(amount, scale, Rounding::TowardZero)
}

fn is_exact_at_scale(amount: Decimal, scale: u32) -> bool {
    remainder_toward_zero(amount, scale).is_zero()
}

/// The result of an operation that may not divide evenly.
///
/// Private fields, no `Deref`, no `Into`, no getter that yields the value alone. The
/// only two exits are [`Converted::into_exact`] (typed error if a residual exists) and
/// [`Converted::split`] (hands you both halves). `#[must_use]` so discarding the whole
/// value is a lint fail.
#[must_use = "a conversion residual must be proven zero or given a destination"]
#[derive(Debug, Clone, Copy)]
pub struct Converted<D: Dimension> {
    value: Decimal,
    residual: Decimal,
    unit: UnitRef<D>,
    scale: u32,
    overflow: bool,
}

impl<D: Dimension> Converted<D> {
    /// Package an unrounded converted amount. `datum-uom` is the intended caller.
    ///
    /// `amount` is the converted figure before rounding to `scale`. Rounding is applied
    /// only by [`Self::split`].
    pub fn new(amount: Decimal, unit: UnitRef<D>, scale: u32) -> Self {
        Self::from_unrounded(amount, unit, scale)
    }

    pub(crate) fn from_unrounded(amount: Decimal, unit: UnitRef<D>, scale: u32) -> Self {
        Self {
            value: amount,
            residual: Decimal::ZERO,
            unit,
            scale,
            overflow: false,
        }
    }

    fn exact(self) -> Decimal {
        self.value + self.residual
    }

    /// Succeeds only when the operation divided evenly at the target scale.
    pub fn into_exact(self) -> Result<Quantity<D>, ResidualError> {
        if self.overflow {
            return Err(ResidualError::NotExact {
                residual: Decimal::ZERO,
                unit: self.unit.id(),
            });
        }
        let exact = self.exact();
        if is_exact_at_scale(exact, self.scale) {
            Ok(Quantity::from_raw(
                apply_rounding(exact, self.scale, Rounding::TowardZero),
                self.unit,
            ))
        } else {
            Err(ResidualError::NotExact {
                residual: remainder_toward_zero(exact, self.scale),
                unit: self.unit.id(),
            })
        }
    }

    /// Value at the target scale under `rule`, plus the residual as a quantity in the
    /// same unit. `value + residual` reconstructs the pre-rounding amount exactly.
    ///
    /// ```
    /// use datum_core::{Converted, CountDim, DimensionKind, Rounding, UnitId, UnitRef};
    /// use rust_decimal::Decimal;
    /// let unit = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).unwrap();
    /// let converted = Converted::<CountDim>::new(Decimal::new(106, 1), unit, 0);
    /// let (value, residual) = converted.split(Rounding::HalfUp);
    /// assert_eq!(
    ///     value.try_add(residual).unwrap().amount(),
    ///     Decimal::new(106, 1)
    /// );
    /// ```
    pub fn split(self, rule: Rounding) -> (Quantity<D>, Quantity<D>) {
        if self.overflow {
            return (Quantity::zero(self.unit), Quantity::zero(self.unit));
        }
        let exact = self.exact();
        let rounded = apply_rounding(exact, self.scale, rule);
        let leftover = exact - rounded;
        (
            Quantity::from_raw(rounded, self.unit),
            Quantity::from_raw(leftover, self.unit),
        )
    }

    /// Whether a residual exists at `scale` (any rounding rule would move the value).
    pub fn has_residual(self) -> bool {
        self.overflow || !is_exact_at_scale(self.exact(), self.scale)
    }

    /// Inspection only: reveals the residual without releasing the value.
    pub fn peek_residual(self) -> Decimal {
        if self.overflow {
            Decimal::ZERO
        } else {
            remainder_toward_zero(self.exact(), self.scale)
        }
    }
}

/// Scalar-scaling result. Same discipline as [`Converted`].
#[must_use = "a scaling residual must be proven zero or given a destination"]
#[derive(Debug, Clone, Copy)]
pub struct Scaled<D: Dimension> {
    inner: Converted<D>,
}

impl<D: Dimension> Scaled<D> {
    pub(crate) fn from_unrounded(amount: Decimal, unit: UnitRef<D>, scale: u32) -> Self {
        Self {
            inner: Converted::from_unrounded(amount, unit, scale),
        }
    }

    pub(crate) fn overflow(unit: UnitRef<D>, scale: u32) -> Self {
        Self {
            inner: Converted {
                value: Decimal::ZERO,
                residual: Decimal::ZERO,
                unit,
                scale,
                overflow: true,
            },
        }
    }

    /// Succeeds only when scaling divided evenly at `to_scale`.
    ///
    /// On `Decimal` overflow from [`crate::Quantity::scale`], this returns
    /// [`ResidualError::NotExact`].
    pub fn into_exact(self) -> Result<Quantity<D>, ResidualError> {
        self.inner.into_exact()
    }

    /// Value at `to_scale` under `rule`, plus residual. Sum reconstructs the product
    /// when the product fit in `Decimal`.
    ///
    /// On overflow from [`crate::Quantity::scale`], both halves are zero: `split`
    /// returns a tuple, so there is no `Err` path.
    pub fn split(self, rule: Rounding) -> (Quantity<D>, Quantity<D>) {
        self.inner.split(rule)
    }

    /// Whether a residual exists at the target scale.
    pub fn has_residual(self) -> bool {
        self.inner.has_residual()
    }

    /// Inspection only: leftover at the target scale toward zero.
    pub fn peek_residual(self) -> Decimal {
        self.inner.peek_residual()
    }
}

/// Settlement result. Same discipline as [`Converted`]; `split()` uses the rule
/// captured by [`Money::settle`](crate::Money::settle).
#[must_use = "a settlement residual must be proven zero or given a destination"]
#[derive(Debug, Clone, Copy)]
pub struct Settled {
    value: Decimal,
    currency: CurrencyId,
    scale: u32,
    rule: Rounding,
    overflow: bool,
}

impl Settled {
    pub(crate) fn from_unrounded(
        amount: Decimal,
        currency: CurrencyId,
        scale: u32,
        rule: Rounding,
    ) -> Self {
        Self {
            value: amount,
            currency,
            scale,
            rule,
            overflow: false,
        }
    }

    /// Succeeds only when the amount is already exact at the minor-unit scale.
    pub fn into_exact(self) -> Result<Money, ResidualError> {
        if self.overflow || !is_exact_at_scale(self.value, self.scale) {
            return Err(ResidualError::NotExact {
                residual: remainder_toward_zero(self.value, self.scale),
                unit: UnitId(0),
            });
        }
        Ok(Money::from_raw(
            apply_rounding(self.value, self.scale, Rounding::TowardZero),
            self.currency,
        ))
    }

    /// Value at the minor-unit scale under the settle rule, plus residual money.
    /// `value + residual` reconstructs the pre-settlement amount exactly.
    pub fn split(self) -> (Money, Money) {
        if self.overflow {
            return (Money::zero(self.currency), Money::zero(self.currency));
        }
        let rounded = apply_rounding(self.value, self.scale, self.rule);
        let leftover = self.value - rounded;
        (
            Money::from_raw(rounded, self.currency),
            Money::from_raw(leftover, self.currency),
        )
    }

    /// Whether settlement would move the amount.
    pub fn has_residual(self) -> bool {
        self.overflow || !is_exact_at_scale(self.value, self.scale)
    }

    /// Inspection only: leftover toward zero at the minor-unit scale.
    pub fn peek_residual(self) -> Decimal {
        remainder_toward_zero(self.value, self.scale)
    }
}

/// Extension (rate × quantity) result. Same discipline as [`Converted`].
#[must_use = "an extension residual must be proven zero or given a destination"]
#[derive(Debug, Clone, Copy)]
pub struct Extended {
    value: Decimal,
    currency: CurrencyId,
    scale: u32,
    overflow: bool,
}

impl Extended {
    pub(crate) fn from_unrounded(amount: Decimal, currency: CurrencyId, scale: u32) -> Self {
        Self {
            value: amount,
            currency,
            scale,
            overflow: false,
        }
    }

    /// Succeeds only when the product is exact at `to_scale`.
    pub fn into_exact(self) -> Result<Money, ResidualError> {
        if self.overflow || !is_exact_at_scale(self.value, self.scale) {
            return Err(ResidualError::NotExact {
                residual: remainder_toward_zero(self.value, self.scale),
                unit: UnitId(0),
            });
        }
        Ok(Money::from_raw(
            apply_rounding(self.value, self.scale, Rounding::TowardZero),
            self.currency,
        ))
    }

    /// Value at `to_scale` under `rule`, plus residual money.
    pub fn split(self, rule: Rounding) -> (Money, Money) {
        if self.overflow {
            return (Money::zero(self.currency), Money::zero(self.currency));
        }
        let rounded = apply_rounding(self.value, self.scale, rule);
        let leftover = self.value - rounded;
        (
            Money::from_raw(rounded, self.currency),
            Money::from_raw(leftover, self.currency),
        )
    }

    /// Whether a residual exists at `to_scale`.
    pub fn has_residual(self) -> bool {
        self.overflow || !is_exact_at_scale(self.value, self.scale)
    }

    /// Inspection only: leftover toward zero at `to_scale`.
    pub fn peek_residual(self) -> Decimal {
        remainder_toward_zero(self.value, self.scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantity::Quantity;
    use crate::units::{CountDim, DimensionKind, UnitId, UnitRef};
    use rust_decimal::Decimal;

    #[test]
    fn converted_into_exact_not_exact() {
        let unit = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).unwrap();
        let converted = Converted::<CountDim>::new(Decimal::new(1, 0) / Decimal::from(3), unit, 4);
        let err = converted.into_exact().unwrap_err();
        assert!(matches!(err, ResidualError::NotExact { .. }));
    }

    #[test]
    fn split_reconstructs() {
        let unit = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).unwrap();
        let qty = Quantity::new(Decimal::from(1), unit).unwrap();
        let scaled = qty.scale(Decimal::new(1, 0) / Decimal::from(3), 4);
        for rule in [
            Rounding::HalfUp,
            Rounding::HalfEven,
            Rounding::TowardZero,
            Rounding::AwayFromZero,
        ] {
            let (v, r) = scaled.split(rule);
            assert_eq!(v.try_add(r).unwrap().amount(), scaled.inner.exact());
        }
    }
}
