//! Rounding policy lookup and the single exit that applies a [`Rounding`] rule.

use datum_core::{ItemId, Rounding, UnitId};

/// Named operation for [`uom.rounding_policy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation {
    /// Boundary conversion to stock units (D2 R2).
    Stock,
    /// Generic unit conversion split.
    Convert,
}

impl Operation {
    /// Database token for this operation.
    pub fn as_str(self) -> &'static str {
        match self {
            Operation::Stock => "stock",
            Operation::Convert => "convert",
        }
    }
}

/// Parse a policy row's rule text.
pub fn rule_from_db(s: &str) -> Option<Rounding> {
    match s {
        "HalfUp" => Some(Rounding::HalfUp),
        "HalfEven" => Some(Rounding::HalfEven),
        "TowardZero" => Some(Rounding::TowardZero),
        "AwayFromZero" => Some(Rounding::AwayFromZero),
        _ => None,
    }
}

/// Serialize a rule for inserts.
#[allow(dead_code)]
pub fn rule_to_db(rule: Rounding) -> &'static str {
    match rule {
        Rounding::HalfUp => "HalfUp",
        Rounding::HalfEven => "HalfEven",
        Rounding::TowardZero => "TowardZero",
        Rounding::AwayFromZero => "AwayFromZero",
        _ => "HalfEven",
    }
}

/// Resolve rounding policy for an item and unit, else default half-even at stock boundary.
pub fn rounding_for(
    policies: &std::collections::HashMap<(ItemId, UnitId, Operation), Rounding>,
    item: ItemId,
    unit: UnitId,
    op: Operation,
    default: Rounding,
) -> Rounding {
    policies.get(&(item, unit, op)).copied().unwrap_or(default)
}

/// Apply a rounding rule to a [`Converted`] result. This is the only place in this crate
/// that names a [`Rounding`] rule for splitting a conversion residual.
pub fn apply_rounding_policy<D: datum_core::Dimension>(
    converted: datum_core::Converted<D>,
    rule: Rounding,
) -> (datum_core::Quantity<D>, datum_core::Quantity<D>) {
    converted.split(rule)
}
