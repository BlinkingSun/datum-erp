//! Shared JSON wire types (`AnyQuantity`, money).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use wicket_core::{AnyQuantity, CurrencyId, DimensionKind, Money, UnitId};

use crate::error::{Error, Result};

/// Wire quantity (amount is a decimal string).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantityBody {
    /// Amount as a decimal string.
    pub amount: String,
    /// Catalog unit id.
    pub unit: i64,
    /// Dimension kind.
    pub dimension: String,
}

impl QuantityBody {
    /// Parse to [`AnyQuantity`]. A JSON number for `amount` is rejected by serde
    /// because the field is a string.
    pub fn to_qty(&self) -> Result<AnyQuantity> {
        let amount: Decimal = self.amount.parse().map_err(|_| {
            Error::validation("amount must be a decimal string", Some("quantity.amount"))
        })?;
        let dimension = parse_dim(&self.dimension)?;
        Ok(AnyQuantity {
            amount,
            unit: UnitId(self.unit),
            dimension,
        })
    }

    /// From domain.
    pub fn from_qty(q: &AnyQuantity) -> Self {
        Self {
            amount: q.amount.to_string(),
            unit: q.unit.0,
            dimension: format!("{:?}", q.dimension),
        }
    }
}

/// Wire money.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoneyBody {
    /// Amount as a decimal string.
    pub amount: String,
    /// ISO 4217 numeric.
    pub currency: i32,
}

impl MoneyBody {
    /// Parse.
    pub fn to_money(&self) -> Result<Money> {
        let amount: Decimal = self
            .amount
            .parse()
            .map_err(|_| Error::validation("amount must be a decimal string", Some("amount")))?;
        Money::new(amount, CurrencyId(self.currency))
            .map_err(|e| Error::validation(e.to_string(), Some("amount")))
    }
}

fn parse_dim(s: &str) -> Result<DimensionKind> {
    match s {
        "Count" => Ok(DimensionKind::Count),
        "Length" => Ok(DimensionKind::Length),
        "Mass" => Ok(DimensionKind::Mass),
        "Time" => Ok(DimensionKind::Time),
        "Volume" => Ok(DimensionKind::Volume),
        "Area" => Ok(DimensionKind::Area),
        other => Err(Error::validation(
            format!("unknown dimension {other}"),
            Some("quantity.dimension"),
        )),
    }
}

/// Parse a UUID path / body field into `T`.
pub fn parse_uuid<T>(s: &str, field: &str, wrap: fn(uuid::Uuid) -> T) -> Result<T> {
    let u = uuid::Uuid::parse_str(s)
        .map_err(|_| Error::validation(format!("invalid uuid for {field}"), Some(field)))?;
    Ok(wrap(u))
}
