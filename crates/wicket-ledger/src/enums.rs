//! Rust ↔ SQL enum bijection (CONTRACT §6.2 rule 7, D2 §5.1).

use wicket_core::{Boundary, CostElement, GroupKind, ValueAccount};

use crate::{Error, Result};

/// Discriminator stored on `ledger.posting.measure`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Measure {
    /// Quantity (matter) row.
    Quantity,
    /// Value (money) row.
    Value,
}

/// Per-item costing method stored on `ledger.stock_item`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CostMethod {
    /// Oldest open layer first.
    Fifo,
    /// Pro-rata across the open pool.
    MovingAvg,
    /// Layer quantity at standard amount; difference to `PPV`.
    Standard,
}

/// Map a [`GroupKind`] to the SQL enum label.
pub fn group_kind_sql(kind: GroupKind) -> Result<&'static str> {
    match kind {
        GroupKind::Movement => Ok("MOVEMENT"),
        GroupKind::Adjustment => Ok("ADJUSTMENT"),
        GroupKind::Transformation => Ok("TRANSFORMATION"),
        GroupKind::Valuation => Ok("VALUATION"),
        GroupKind::Reversal => Ok("REVERSAL"),
        other => Err(Error::Core(wicket_core::Error::Invariant(format!(
            "unmapped group kind {other:?}"
        )))),
    }
}

/// Parse a SQL `ledger.group_kind` label.
pub fn group_kind_from_sql(label: &str) -> Result<GroupKind> {
    match label {
        "MOVEMENT" => Ok(GroupKind::Movement),
        "ADJUSTMENT" => Ok(GroupKind::Adjustment),
        "TRANSFORMATION" => Ok(GroupKind::Transformation),
        "VALUATION" => Ok(GroupKind::Valuation),
        "REVERSAL" => Ok(GroupKind::Reversal),
        other => Err(Error::Core(wicket_core::Error::Invariant(format!(
            "unknown group kind {other}"
        )))),
    }
}

/// Map a [`Boundary`] to the SQL enum label.
pub fn boundary_sql(boundary: Boundary) -> Result<&'static str> {
    match boundary {
        Boundary::Supplier => Ok("SUPPLIER"),
        Boundary::Customer => Ok("CUSTOMER"),
        Boundary::Scrap => Ok("SCRAP"),
        Boundary::Adjustment => Ok("ADJUSTMENT"),
        Boundary::Rounding => Ok("ROUNDING"),
        Boundary::Consumed => Ok("CONSUMED"),
        Boundary::Produced => Ok("PRODUCED"),
        other => Err(Error::Core(wicket_core::Error::Invariant(format!(
            "unmapped boundary {other:?}"
        )))),
    }
}

/// Parse a SQL `ledger.boundary` label.
pub fn boundary_from_sql(label: &str) -> Result<Boundary> {
    match label {
        "SUPPLIER" => Ok(Boundary::Supplier),
        "CUSTOMER" => Ok(Boundary::Customer),
        "SCRAP" => Ok(Boundary::Scrap),
        "ADJUSTMENT" => Ok(Boundary::Adjustment),
        "ROUNDING" => Ok(Boundary::Rounding),
        "CONSUMED" => Ok(Boundary::Consumed),
        "PRODUCED" => Ok(Boundary::Produced),
        other => Err(Error::Core(wicket_core::Error::Invariant(format!(
            "unknown boundary {other}"
        )))),
    }
}

/// Map a [`CostElement`] to the SQL enum label.
pub fn cost_element_sql(el: CostElement) -> Result<&'static str> {
    match el {
        CostElement::Material => Ok("MATERIAL"),
        CostElement::Labor => Ok("LABOR"),
        CostElement::Burden => Ok("BURDEN"),
        CostElement::Outside => Ok("OUTSIDE"),
        other => Err(Error::Core(wicket_core::Error::Invariant(format!(
            "unmapped cost element {other:?}"
        )))),
    }
}

/// Parse a SQL `ledger.cost_element` label.
pub fn cost_element_from_sql(label: &str) -> Result<CostElement> {
    match label {
        "MATERIAL" => Ok(CostElement::Material),
        "LABOR" => Ok(CostElement::Labor),
        "BURDEN" => Ok(CostElement::Burden),
        "OUTSIDE" => Ok(CostElement::Outside),
        other => Err(Error::Core(wicket_core::Error::Invariant(format!(
            "unknown cost element {other}"
        )))),
    }
}

/// Map a [`ValueAccount`] to the SQL enum label.
pub fn value_account_sql(account: ValueAccount) -> Result<&'static str> {
    match account {
        ValueAccount::Inventory => Ok("INVENTORY"),
        ValueAccount::Wip => Ok("WIP"),
        ValueAccount::Cogs => Ok("COGS"),
        ValueAccount::ScrapExpense => Ok("SCRAP_EXPENSE"),
        ValueAccount::AdjustmentExpense => Ok("ADJUSTMENT_EXPENSE"),
        ValueAccount::ApAccrual => Ok("AP_ACCRUAL"),
        ValueAccount::LaborAbsorbed => Ok("LABOR_ABSORBED"),
        ValueAccount::BurdenAbsorbed => Ok("BURDEN_ABSORBED"),
        ValueAccount::Ppv => Ok("PPV"),
        ValueAccount::MfgVariance => Ok("MFG_VARIANCE"),
        ValueAccount::Rounding => Ok("ROUNDING"),
        other => Err(Error::Core(wicket_core::Error::Invariant(format!(
            "unmapped value account {other:?}"
        )))),
    }
}

/// Parse a SQL `ledger.value_account` label.
pub fn value_account_from_sql(label: &str) -> Result<ValueAccount> {
    match label {
        "INVENTORY" => Ok(ValueAccount::Inventory),
        "WIP" => Ok(ValueAccount::Wip),
        "COGS" => Ok(ValueAccount::Cogs),
        "SCRAP_EXPENSE" => Ok(ValueAccount::ScrapExpense),
        "ADJUSTMENT_EXPENSE" => Ok(ValueAccount::AdjustmentExpense),
        "AP_ACCRUAL" => Ok(ValueAccount::ApAccrual),
        "LABOR_ABSORBED" => Ok(ValueAccount::LaborAbsorbed),
        "BURDEN_ABSORBED" => Ok(ValueAccount::BurdenAbsorbed),
        "PPV" => Ok(ValueAccount::Ppv),
        "MFG_VARIANCE" => Ok(ValueAccount::MfgVariance),
        "ROUNDING" => Ok(ValueAccount::Rounding),
        other => Err(Error::Core(wicket_core::Error::Invariant(format!(
            "unknown value account {other}"
        )))),
    }
}

/// Map a [`Measure`] to the SQL enum label.
pub fn measure_sql(measure: Measure) -> Result<&'static str> {
    match measure {
        Measure::Quantity => Ok("QUANTITY"),
        Measure::Value => Ok("VALUE"),
    }
}

/// Parse a SQL `ledger.measure` label.
pub fn measure_from_sql(label: &str) -> Result<Measure> {
    match label {
        "QUANTITY" => Ok(Measure::Quantity),
        "VALUE" => Ok(Measure::Value),
        other => Err(Error::Core(wicket_core::Error::Invariant(format!(
            "unknown measure {other}"
        )))),
    }
}

/// Map a [`CostMethod`] to the check-constrained text column.
pub fn cost_method_sql(method: CostMethod) -> &'static str {
    match method {
        CostMethod::Fifo => "FIFO",
        CostMethod::MovingAvg => "MOVING_AVG",
        CostMethod::Standard => "STANDARD",
    }
}

/// Parse `ledger.stock_item.cost_method`.
pub fn cost_method_from_sql(label: &str) -> Option<CostMethod> {
    match label {
        "FIFO" => Some(CostMethod::Fifo),
        "MOVING_AVG" => Some(CostMethod::MovingAvg),
        "STANDARD" => Some(CostMethod::Standard),
        _ => None,
    }
}

/// Every [`GroupKind`] variant paired with its SQL label.
pub fn group_kind_variants() -> &'static [(GroupKind, &'static str)] {
    &[
        (GroupKind::Movement, "MOVEMENT"),
        (GroupKind::Adjustment, "ADJUSTMENT"),
        (GroupKind::Transformation, "TRANSFORMATION"),
        (GroupKind::Valuation, "VALUATION"),
        (GroupKind::Reversal, "REVERSAL"),
    ]
}

/// Every [`Boundary`] variant paired with its SQL label.
pub fn boundary_variants() -> &'static [(Boundary, &'static str)] {
    &[
        (Boundary::Supplier, "SUPPLIER"),
        (Boundary::Customer, "CUSTOMER"),
        (Boundary::Scrap, "SCRAP"),
        (Boundary::Adjustment, "ADJUSTMENT"),
        (Boundary::Rounding, "ROUNDING"),
        (Boundary::Consumed, "CONSUMED"),
        (Boundary::Produced, "PRODUCED"),
    ]
}

/// Every [`CostElement`] variant paired with its SQL label.
pub fn cost_element_variants() -> &'static [(CostElement, &'static str)] {
    &[
        (CostElement::Material, "MATERIAL"),
        (CostElement::Labor, "LABOR"),
        (CostElement::Burden, "BURDEN"),
        (CostElement::Outside, "OUTSIDE"),
    ]
}

/// Every [`ValueAccount`] variant paired with its SQL label.
pub fn value_account_variants() -> &'static [(ValueAccount, &'static str)] {
    &[
        (ValueAccount::Inventory, "INVENTORY"),
        (ValueAccount::Wip, "WIP"),
        (ValueAccount::Cogs, "COGS"),
        (ValueAccount::ScrapExpense, "SCRAP_EXPENSE"),
        (ValueAccount::AdjustmentExpense, "ADJUSTMENT_EXPENSE"),
        (ValueAccount::ApAccrual, "AP_ACCRUAL"),
        (ValueAccount::LaborAbsorbed, "LABOR_ABSORBED"),
        (ValueAccount::BurdenAbsorbed, "BURDEN_ABSORBED"),
        (ValueAccount::Ppv, "PPV"),
        (ValueAccount::MfgVariance, "MFG_VARIANCE"),
        (ValueAccount::Rounding, "ROUNDING"),
    ]
}

/// Every [`Measure`] variant paired with its SQL label.
pub fn measure_variants() -> &'static [(Measure, &'static str)] {
    &[(Measure::Quantity, "QUANTITY"), (Measure::Value, "VALUE")]
}

/// Boundary labels a group kind may name (D2 §4.2). `None` (real location) is always allowed.
pub fn boundary_permitted(kind: GroupKind, boundary: Boundary) -> bool {
    match kind {
        GroupKind::Movement => matches!(boundary, Boundary::Supplier | Boundary::Customer),
        GroupKind::Adjustment => {
            matches!(
                boundary,
                Boundary::Scrap | Boundary::Adjustment | Boundary::Rounding
            )
        }
        GroupKind::Transformation => matches!(boundary, Boundary::Consumed | Boundary::Produced),
        GroupKind::Valuation => false,
        GroupKind::Reversal => true,
        _ => false,
    }
}
