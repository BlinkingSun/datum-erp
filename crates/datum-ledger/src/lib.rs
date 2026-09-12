//! Append-only inventory and cost ledger.
//!
//! Groups, postings, the consumption edge (cost layers and genealogy), the
//! deferred constraint trigger, [`GroupBuilder`] as [`datum_core::PostingSink`],
//! rebuildable projections, reversal, and the rounding-residual home.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![cfg_attr(test, allow(unused_crate_dependencies))]

use datum_audit as _;
use datum_core::{DimensionKind, Identifier};
use serde as _;

mod allocate;
mod builder;
mod enums;
mod error;
mod genealogy;
mod poison;
mod post;
mod projections;
mod registry;
mod residual;
mod reverse;

pub use allocate::{
    AllocationEdge, Layer, SQL_OPEN_LAYERS, allocate_withdrawal, load_open_layers,
    sql_reads_consuming_value_rows, take_query_log,
};
#[cfg(feature = "test-utils")]
pub use builder::test_inject_quantity_without_contributed_mark;
pub use builder::{GroupBuilder, UOM_CONVERSION_RESIDUAL};
pub use enums::{
    CostMethod, Measure, boundary_from_sql, boundary_permitted, boundary_sql, boundary_variants,
    cost_element_from_sql, cost_element_sql, cost_element_variants, cost_method_from_sql,
    cost_method_sql, group_kind_from_sql, group_kind_sql, group_kind_variants, measure_from_sql,
    measure_sql, measure_variants, value_account_from_sql, value_account_sql,
    value_account_variants,
};
pub use error::{Error, Result, map_sqlstate};
pub use genealogy::{Node, TraceStart, trace_backward, trace_forward};
#[cfg(feature = "test-utils")]
pub use poison::test_is_marked as test_poison_is_marked;
pub use post::{bind_tx, commit, post};
pub use projections::{BalanceSlice, apply_group, balance_at, rebuild, verify_projection};
pub use registry::{StockItem, load_stock_item, upsert_location, upsert_stock_item};
pub use residual::{post_uom_conversion_residual, post_uom_residual_flush};
pub use reverse::reverse;

/// Embedded migrator (`placeholder` + `0001_ledger`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Group identifier: the `ledger.posting_group.group_id` uuid.
pub type GroupId = Identifier;

/// Seeded `uom.unit` ids (datum-uom 0001) → dimension. Used when reconstructing
/// [`datum_core::AnyQuantity`] from a posting row.
pub(crate) fn infer_dimension(unit: i64) -> DimensionKind {
    match unit {
        1 => DimensionKind::Count,
        2..=4 => DimensionKind::Length,
        5 | 6 => DimensionKind::Mass,
        7 | 8 => DimensionKind::Time,
        _ => DimensionKind::Count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tokio as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(MIGRATOR.migrations.len() >= 2);
        assert!(MIGRATOR.iter().any(|m| m.version == 1));
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    #[test]
    fn zl_mapping_is_exhaustive() {
        for (code, pred) in [
            ("ZL000", Error::GroupHasNoHeader),
            ("ZL001", Error::GroupExtended),
            ("ZL002", Error::QuantityNotConserved),
            ("ZL003", Error::ValueNotConserved),
            ("ZL004", Error::CostElementReclassified),
            ("ZL005", Error::AllocationIncomplete),
            ("ZL006", Error::ReversalNotExact),
            ("ZL007", Error::LayersNotRestored),
        ] {
            let mapped = map_sqlstate(code, "").expect(code);
            assert_eq!(mapped.to_string(), pred.to_string());
        }
        let already = map_sqlstate("23505", "duplicate key posting_group_reversed_once").unwrap();
        assert!(matches!(already, Error::AlreadyReversed));
    }

    #[test]
    fn allocator_sql_does_not_read_consuming_value_rows() {
        assert!(!sql_reads_consuming_value_rows(SQL_OPEN_LAYERS));
        assert!(sql_reads_consuming_value_rows(
            "SELECT v.amount FROM ledger.posting v
              WHERE v.values_posting_id = p.posting_id AND p.quantity < 0"
        ));
    }

    #[test]
    fn rust_sql_labels_cover_every_named_variant() {
        for (k, s) in group_kind_variants() {
            assert_eq!(group_kind_sql(*k).unwrap(), *s);
            assert_eq!(group_kind_from_sql(s).unwrap(), *k);
        }
        for (b, s) in boundary_variants() {
            assert_eq!(boundary_sql(*b).unwrap(), *s);
            assert_eq!(boundary_from_sql(s).unwrap(), *b);
        }
        for (e, s) in cost_element_variants() {
            assert_eq!(cost_element_sql(*e).unwrap(), *s);
            assert_eq!(cost_element_from_sql(s).unwrap(), *e);
        }
        for (a, s) in value_account_variants() {
            assert_eq!(value_account_sql(*a).unwrap(), *s);
            assert_eq!(value_account_from_sql(s).unwrap(), *a);
        }
        for (m, s) in measure_variants() {
            assert_eq!(measure_sql(*m).unwrap(), *s);
            assert_eq!(measure_from_sql(s).unwrap(), *m);
        }
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
