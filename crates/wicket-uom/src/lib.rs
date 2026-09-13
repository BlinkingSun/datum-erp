//! Kernel units of measure: catalog, conversion engine, and stock boundary helper.

#![cfg_attr(test, allow(unused_crate_dependencies))]

use serde as _;
use wicket_audit as _;

mod catalog;
mod error;
mod item_stock;
mod pin;
mod policy;
mod stock;

pub use catalog::{UomCatalog, load_catalog, split_with_policy};
pub use error::{Error, Result};
pub use item_stock::{ItemStockMeasure, pin_item_stock, update_item_stock};
pub use pin::pin_lot_factor;
pub use policy::{Operation, apply_rounding_policy};
pub use stock::{StockConversion, to_stock};

pub use wicket_core::{
    ConversionContext, Converted, DimensionKind, Rounding, UnitCatalog, UnitConverter, UnitId,
};

/// Embedded migrator (`placeholder` + `0001_uom`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Load a unit id from the catalog (read pool).
pub async fn load_unit(pool: &wicket_db::Pool, id: UnitId) -> Result<UnitId> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM uom.unit WHERE id = $1")
        .bind(id.0)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Db(e.into()))?;
    row.map(|(id,)| UnitId(id)).ok_or(Error::UnknownUnit(id))
}

/// Convenience convert using a loaded catalog (same as [`UnitConverter::convert`]).
pub fn convert<D: wicket_core::Dimension>(
    catalog: &UomCatalog,
    qty: wicket_core::Quantity<D>,
    to: wicket_core::UnitRef<D>,
    ctx: &ConversionContext,
) -> core::result::Result<Converted<D>, wicket_core::QuantityError> {
    catalog.convert(qty, to, ctx)
}

#[cfg(test)]
mod tests {
    use super::{Error, MIGRATOR};
    use proptest::prelude::*;
    use tokio as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(!MIGRATOR.migrations.is_empty());
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = wicket_test::postgres_available();
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, .. ProptestConfig::default() })]

        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
