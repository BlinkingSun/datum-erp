//! Money and unit cost. No shared trait with [`crate::Quantity`].

use crate::quantity::{Quantity, exceeds_integer_width};
use crate::residual::{Extended, Settled};
use crate::units::{CurrencyId, Dimension, UnitId, UnitRef};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Money carries up to 6 decimal places: enough for 4 sub-minor digits on a 2-minor
/// currency, which is what extended amounts and landed-cost proration actually need.
/// Storage column: numeric(24,6).
pub const MONEY_MAX_SCALE: u32 = 6;
/// Integer digits stored by PostgreSQL `numeric(24,6)`. `|amount| >= 10^18` is overflow.
const MONEY_MAX_INTEGER_DIGITS: u32 = 18;
/// Unit costs and rates carry 8. Storage column: numeric(24,8).
pub const RATE_MAX_SCALE: u32 = 8;

/// Errors from money construction, arithmetic, and allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MoneyError {
    /// Different currencies cannot be added.
    #[error("cannot combine currency {left:?} with {right:?}")]
    CurrencyMismatch {
        /// Left-hand currency.
        left: CurrencyId,
        /// Right-hand currency.
        right: CurrencyId,
    },
    /// [`UnitCost::extend`] requires the quantity's unit to match the quoted `per`.
    #[error("cost is quoted per {cost_unit:?} but quantity is in {qty_unit:?}")]
    RateUnitMismatch {
        /// Unit the cost is quoted in.
        cost_unit: UnitId,
        /// Unit of the quantity.
        qty_unit: UnitId,
    },
    /// Caller supplied more fractional digits than the type allows.
    #[error("scale {found} exceeds maximum {max}")]
    ScaleExceeded {
        /// Scale on the input.
        found: u32,
        /// Maximum permitted scale.
        max: u32,
    },
    /// Amount exceeds `numeric(24,6)` (`|amount| >= 10^18`) or `Decimal` could not
    /// represent the result.
    #[error("arithmetic overflow")]
    Overflow,
    /// Allocation weights sum to zero (including an empty slice). Never panics.
    #[error("allocation weights sum to zero")]
    EmptyAllocation,
}

/// A signed decimal amount in a currency. Not a quantity; they share no arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Money {
    amount: Decimal,
    currency: CurrencyId,
}

impl Money {
    pub(crate) fn from_raw(amount: Decimal, currency: CurrencyId) -> Self {
        Self { amount, currency }
    }

    /// Rejects scale > [`MONEY_MAX_SCALE`] and overflowing input.
    /// Never rounds. Never truncates.
    ///
    /// Overflow is the PostgreSQL `numeric(24,6)` integer-width bound: more than
    /// 18 integer digits (`|amount| >= 10^18`) returns [`MoneyError::Overflow`].
    /// The value is never truncated to fit.
    pub fn new(amount: Decimal, currency: CurrencyId) -> Result<Self, MoneyError> {
        if amount.scale() > MONEY_MAX_SCALE {
            return Err(MoneyError::ScaleExceeded {
                found: amount.scale(),
                max: MONEY_MAX_SCALE,
            });
        }
        if exceeds_integer_width(amount, MONEY_MAX_INTEGER_DIGITS) {
            return Err(MoneyError::Overflow);
        }
        Ok(Self { amount, currency })
    }

    /// Zero in `currency`.
    pub fn zero(currency: CurrencyId) -> Self {
        Self {
            amount: Decimal::ZERO,
            currency,
        }
    }

    /// Signed amount.
    pub fn amount(self) -> Decimal {
        self.amount
    }

    /// Currency of this amount.
    pub fn currency(self) -> CurrencyId {
        self.currency
    }

    /// Add two amounts of the same currency.
    pub fn try_add(self, rhs: Self) -> Result<Self, MoneyError> {
        if self.currency != rhs.currency {
            return Err(MoneyError::CurrencyMismatch {
                left: self.currency,
                right: rhs.currency,
            });
        }
        let amount = self
            .amount
            .checked_add(rhs.amount)
            .ok_or(MoneyError::Overflow)?;
        Ok(Self {
            amount,
            currency: self.currency,
        })
    }

    /// Subtract two amounts of the same currency.
    pub fn try_sub(self, rhs: Self) -> Result<Self, MoneyError> {
        self.try_add(rhs.negate())
    }

    /// Arithmetic negation.
    pub fn negate(self) -> Self {
        Self {
            amount: -self.amount,
            currency: self.currency,
        }
    }

    /// Fallible ordering; mixed currencies are an error, not a `bool`.
    pub fn try_cmp(self, rhs: Self) -> Result<core::cmp::Ordering, MoneyError> {
        if self.currency != rhs.currency {
            return Err(MoneyError::CurrencyMismatch {
                left: self.currency,
                right: rhs.currency,
            });
        }
        Ok(self.amount.cmp(&rhs.amount))
    }

    /// Sum. `None` for an empty iterator, because a zero has no currency.
    pub fn try_sum<I: IntoIterator<Item = Self>>(i: I) -> Result<Option<Self>, MoneyError> {
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

    /// Bring an amount to a currency's minor-unit scale. Returns value AND residual;
    /// the caller decides where the residual goes. [`Settled`] is un-ignorable.
    pub fn settle(self, minor_exponent: u32, rule: crate::residual::Rounding) -> Settled {
        Settled::from_unrounded(self.amount, self.currency, minor_exponent, rule)
    }

    /// Exact split by integer weights, largest-remainder. The sum of the output equals
    /// `self` exactly. No residual exists, therefore none can be lost. This is the
    /// ergonomic answer for landed cost, freight, and overhead proration.
    ///
    /// `scale` is the working quantum (`10^{-scale}`). Weights that sum to zero,
    /// including an empty slice, return [`MoneyError::EmptyAllocation`] and never panic.
    pub fn allocate(self, weights: &[u64], scale: u32) -> Result<Vec<Money>, MoneyError> {
        if scale > MONEY_MAX_SCALE {
            return Err(MoneyError::ScaleExceeded {
                found: scale,
                max: MONEY_MAX_SCALE,
            });
        }
        let total: u128 = weights.iter().copied().map(u128::from).sum();
        if total == 0 {
            return Err(MoneyError::EmptyAllocation);
        }
        let total_dec = u128_to_decimal(total)?;
        let sign = if self.amount.is_sign_negative() {
            Decimal::NEGATIVE_ONE
        } else {
            Decimal::ONE
        };
        let abs_amount = self.amount.abs();
        let quantum = Decimal::new(1, scale);

        let mut floors: Vec<Decimal> = Vec::with_capacity(weights.len());
        let mut remainders: Vec<(usize, Decimal)> = Vec::with_capacity(weights.len());
        for (i, &w) in weights.iter().enumerate() {
            if w == 0 {
                floors.push(Decimal::ZERO);
                remainders.push((i, Decimal::ZERO));
                continue;
            }
            let w_dec = Decimal::from(w);
            let exact = abs_amount
                .checked_mul(w_dec)
                .ok_or(MoneyError::Overflow)?
                .checked_div(total_dec)
                .ok_or(MoneyError::Overflow)?;
            let floored =
                exact.round_dp_with_strategy(scale, rust_decimal::RoundingStrategy::ToZero);
            floors.push(floored);
            remainders.push((i, exact - floored));
        }

        let mut sum_floored = Decimal::ZERO;
        for f in &floors {
            sum_floored = sum_floored.checked_add(*f).ok_or(MoneyError::Overflow)?;
        }
        let leftover = abs_amount
            .checked_sub(sum_floored)
            .ok_or(MoneyError::Overflow)?;
        let n_quanta = if quantum.is_zero() {
            Decimal::ZERO
        } else {
            leftover
                .checked_div(quantum)
                .ok_or(MoneyError::Overflow)?
                .round_dp_with_strategy(0, rust_decimal::RoundingStrategy::ToZero)
        };
        let n = i128::try_from(n_quanta).map_err(|_| MoneyError::Overflow)?;
        let n = u128::try_from(n).map_err(|_| MoneyError::Overflow)?;

        remainders.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let assign = core::cmp::min(n, remainders.len() as u128);
        for &(i, _) in remainders.iter().take(assign as usize) {
            let slot = floors.get_mut(i).ok_or(MoneyError::Overflow)?;
            *slot = slot.checked_add(quantum).ok_or(MoneyError::Overflow)?;
        }

        let mut sum_parts = Decimal::ZERO;
        for f in &floors {
            sum_parts = sum_parts.checked_add(*f).ok_or(MoneyError::Overflow)?;
        }
        let dust = abs_amount
            .checked_sub(sum_parts)
            .ok_or(MoneyError::Overflow)?;
        if !dust.is_zero() {
            let target = remainders.first().map(|(i, _)| *i).unwrap_or(0);
            let slot = floors.get_mut(target).ok_or(MoneyError::Overflow)?;
            *slot = slot.checked_add(dust).ok_or(MoneyError::Overflow)?;
        }

        floors
            .into_iter()
            .map(|part| {
                let signed = part.checked_mul(sign).ok_or(MoneyError::Overflow)?;
                Ok(Self::from_raw(signed, self.currency))
            })
            .collect()
    }
}

fn u128_to_decimal(n: u128) -> Result<Decimal, MoneyError> {
    if n > i128::MAX as u128 {
        return Err(MoneyError::Overflow);
    }
    Ok(Decimal::from_i128_with_scale(n as i128, 0))
}

/// Price or cost per one unit of a dimensioned quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitCost<D: Dimension> {
    amount: Decimal,
    currency: CurrencyId,
    per: UnitRef<D>,
}

impl<D: Dimension> UnitCost<D> {
    /// Construct a rate. Scale may not exceed [`RATE_MAX_SCALE`].
    pub fn new(amount: Decimal, currency: CurrencyId, per: UnitRef<D>) -> Result<Self, MoneyError> {
        if amount.scale() > RATE_MAX_SCALE {
            return Err(MoneyError::ScaleExceeded {
                found: amount.scale(),
                max: RATE_MAX_SCALE,
            });
        }
        Ok(Self {
            amount,
            currency,
            per,
        })
    }

    /// cost x quantity. Requires `qty.unit() == self.per` (typed error otherwise) and
    /// yields an [`Extended`], carrying the residual the multiplication created.
    pub fn extend(self, qty: Quantity<D>, to_scale: u32) -> Result<Extended, MoneyError> {
        if qty.unit().id() != self.per.id() {
            return Err(MoneyError::RateUnitMismatch {
                cost_unit: self.per.id(),
                qty_unit: qty.unit_id(),
            });
        }
        match self.amount.checked_mul(qty.amount()) {
            Some(product) => Ok(Extended::from_unrounded(product, self.currency, to_scale)),
            None => Err(MoneyError::Overflow),
        }
    }
}

/// Wire and database representation of [`Money`]. `Decimal` is a JSON string.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MoneyWire {
    /// Amount serialized with `rust_decimal` `serde-with-str`.
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    /// Currency of the amount.
    pub currency: CurrencyId,
}

impl From<Money> for MoneyWire {
    fn from(m: Money) -> Self {
        Self {
            amount: m.amount(),
            currency: m.currency(),
        }
    }
}

impl TryFrom<MoneyWire> for Money {
    type Error = MoneyError;

    fn try_from(w: MoneyWire) -> Result<Self, MoneyError> {
        Money::new(w.amount, w.currency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{CountDim, DimensionKind, UnitRef};

    #[test]
    fn money_new_never_truncates() {
        let over = Decimal::new(1, MONEY_MAX_SCALE + 1);
        assert!(over.scale() > MONEY_MAX_SCALE);
        let err = Money::new(over, CurrencyId(840)).unwrap_err();
        assert_eq!(
            err,
            MoneyError::ScaleExceeded {
                found: MONEY_MAX_SCALE + 1,
                max: MONEY_MAX_SCALE,
            }
        );
        assert_eq!(over.scale(), MONEY_MAX_SCALE + 1);
    }

    #[test]
    fn money_new_rejects_integer_overflow() {
        let mut accepted = Decimal::from_i128_with_scale(10i128.pow(18) - 1, 0);
        accepted.rescale(MONEY_MAX_SCALE);
        assert_eq!(accepted.scale(), MONEY_MAX_SCALE);
        let m =
            Money::new(accepted, CurrencyId(840)).expect("10^18 - 1 at scale 6 fits numeric(24,6)");
        assert_eq!(m.amount(), accepted);

        let rejected = Decimal::from_i128_with_scale(10i128.pow(18), 0);
        assert_eq!(
            Money::new(rejected, CurrencyId(840)).unwrap_err(),
            MoneyError::Overflow
        );
        assert_eq!(
            Money::new(-rejected, CurrencyId(840)).unwrap_err(),
            MoneyError::Overflow
        );
    }

    #[test]
    fn try_add_currency_mismatch() {
        let a = Money::new(Decimal::from(1), CurrencyId(840)).unwrap();
        let b = Money::new(Decimal::from(1), CurrencyId(978)).unwrap();
        assert_eq!(
            a.try_add(b).unwrap_err(),
            MoneyError::CurrencyMismatch {
                left: CurrencyId(840),
                right: CurrencyId(978),
            }
        );
    }

    #[test]
    fn allocate_empty_and_zero_weights() {
        let m = Money::new(Decimal::new(1000, 2), CurrencyId(840)).unwrap();
        assert_eq!(m.allocate(&[], 2).unwrap_err(), MoneyError::EmptyAllocation);
        assert_eq!(
            m.allocate(&[0, 0], 2).unwrap_err(),
            MoneyError::EmptyAllocation
        );
    }

    #[test]
    fn allocate_sums_exactly() {
        let m = Money::new(Decimal::new(4720, 2), CurrencyId(840)).unwrap();
        let parts = m.allocate(&[1, 1, 1], 6).unwrap();
        let sum = Money::try_sum(parts).unwrap().unwrap();
        assert_eq!(sum, m);
    }

    #[test]
    fn extend_rate_unit_mismatch() {
        let per = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count).unwrap();
        let other = UnitRef::<CountDim>::checked(UnitId(2), DimensionKind::Count).unwrap();
        let cost = UnitCost::new(Decimal::new(10, 2), CurrencyId(840), per).unwrap();
        let qty = Quantity::new(Decimal::from(3), other).unwrap();
        assert!(matches!(
            cost.extend(qty, 6),
            Err(MoneyError::RateUnitMismatch { .. })
        ));
    }
}
