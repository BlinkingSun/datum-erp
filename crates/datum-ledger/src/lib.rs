//! Inventory and cost ledger (stub API; Wave 2 fills the engine).

/// Crate error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented.
    #[error("unimplemented")]
    Unimplemented,
    /// Core error.
    #[error(transparent)]
    Core(#[from] datum_core::Error),
    /// Database error.
    #[error(transparent)]
    Db(#[from] datum_db::Error),
}

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Embedded placeholder migrator.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Posting id (ledger-local newtype over Identifier).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PostingId(pub datum_core::Identifier);

/// Group id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GroupId(pub datum_core::Identifier);

/// Ledger kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum LedgerKind {
    /// Inventory.
    Inventory,
    /// Cost.
    Cost,
    /// Labor.
    Labor,
}

/// Virtual location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum VirtualLocation {
    /// Supplier.
    Supplier,
    /// Customer.
    Customer,
    /// Scrap.
    Scrap,
    /// Adjustment.
    Adjustment,
    /// WIP.
    Wip,
}

/// Dimension keys of a posting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Dimensions {
    /// Item.
    pub item: Option<datum_core::ItemId>,
    /// Location.
    pub location: Option<datum_core::LocationId>,
    /// Lot.
    pub lot: Option<datum_core::LotId>,
    /// Serial.
    pub serial: Option<datum_core::SerialId>,
    /// Work order.
    pub work_order: Option<datum_core::Identifier>,
    /// Cost element label.
    pub cost_element: Option<datum_core::Identifier>,
}

/// A ledger posting. Constructable; persistence is Wave 2.
#[derive(Debug, Clone)]
pub struct Posting {
    /// Id.
    pub id: PostingId,
    /// Group.
    pub group_id: GroupId,
    /// Ledger.
    pub ledger: LedgerKind,
    /// Posted at (server-stamped on write).
    pub posted_at: chrono::DateTime<chrono::Utc>,
    /// Posted by.
    pub posted_by: datum_core::Actor,
    /// Dimensions.
    pub dimensions: Dimensions,
    /// Quantity.
    pub quantity: datum_core::AnyQuantity,
    /// Money amount when present.
    pub amount: Option<datum_core::Money>,
    /// Source.
    pub source: Option<datum_core::Identifier>,
    /// Reason.
    pub reason: Option<String>,
}

/// Post a row. Unimplemented.
pub async fn post(_tx: &mut datum_db::Tx<'_>, _posting: Posting) -> Result<PostingId> {
    let _ = rust_decimal::Decimal::ZERO;
    let _ = core::any::type_name::<datum_audit::Error>();
    let _ = core::any::type_name::<datum_uom::Error>();
    let _ = core::any::type_name::<dyn datum_core::PostingSink>();
    Err(Error::Unimplemented)
}

/// Rebuild projections from the ledger. Unimplemented.
pub async fn rebuild_projections(_pool: &datum_db::Pool) -> Result<()> {
    Err(Error::Unimplemented)
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
        assert!(!MIGRATOR.migrations.is_empty());
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
