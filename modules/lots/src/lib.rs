//! Lot and serial identity (Wave 2s `mod-lots`).
//!
//! Invariants 9–12 live here: constrained identifiers, serial-within-lot,
//! package hierarchy, expiry precision, and the nullable UDI attachment point.
//! Writes go through [`datum_db::Tx`]. This module never posts to the ledger.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod api;
mod domain;
mod error;
mod events;
mod hooks;
mod states;
mod store;

pub use api::{
    CreateLotBody, CreateSerialsBody, ExpiryWire, ListBody, LotBody, PackageBody, SerialBody,
    SetStatusBody, create_lot as create_lot_http, create_serials as create_serials_http, get_lot,
    list_lots, list_packages, list_serials, set_status as set_status_http,
};
pub use domain::{
    CreateLot, DEFAULT_LOT_TEMPLATE, DEFAULT_SERIAL_TEMPLATE, Expiry, ExpiryPrecision, Lot,
    LotStatus, Package, PackageId, PackageLevel, Serial, StatusHistory, StatusTarget, UdiTarget,
    validate_identifier,
};
pub use error::{Error, Result};
pub use events::{LOT_CREATED, SERIALS_CREATED, STATUS_CHANGED, register_schemas};
pub use hooks::register as register_hooks;
pub use states::{DOC_TYPE, STATES};
pub use store::{
    attach_udi, create_lot, create_package, create_serials, load_lot, load_serial,
    package_hierarchy, resolve, set_status, trace_keys,
};

use datum_db::Tx;
use datum_module::{KernelBuilder, ModuleManifest};

/// Embedded migrator (`placeholder` + `0001_lots`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Parsed `module.toml`.
pub fn manifest() -> Result<ModuleManifest> {
    Ok(ModuleManifest::parse(include_str!("../module.toml"))?)
}

/// Register event schemas, then fold this module's extension points into `builder`.
pub fn apply(builder: &mut KernelBuilder) -> Result<()> {
    register_schemas()?;
    builder.apply_manifest(&manifest()?)?;
    register_hooks(builder)?;
    Ok(())
}

pub(crate) async fn stamps(tx: &mut Tx<'_>) -> Result<(String, String)> {
    let app = datum_db::app_version();
    let cfg = tx.setting("datum.config_version").await?;
    Ok((app, cfg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_audit as _;
    use datum_identity as _;
    use proptest::prelude::*;
    use tokio as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(!MIGRATOR.migrations.is_empty());
        assert!(MIGRATOR.iter().any(|m| m.version == 1));
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    #[test]
    fn manifest_parses_lots_id_and_permissions() {
        let m = manifest().expect("module.toml");
        assert_eq!(m.id, "lots");
        assert!(!m.regulated);
        assert!(m.permissions.contains_key("lots.view"));
        assert!(m.permissions.contains_key("lots.create"));
        assert!(m.permissions.contains_key("lots.status"));
        assert!(m.routes.iter().any(|r| r.path == "/api/v1/lots"));
        assert_eq!(m.machines.len(), 1);
        assert_eq!(m.machines[0].doc_type, "lot");
    }

    #[test]
    fn identifier_refuses_lowercase_space_and_21() {
        assert!(validate_identifier("LOT-BAR-24-4412").is_ok());
        assert!(validate_identifier("HT-ATI-24-8831").is_ok());
        assert!(validate_identifier("SN-450-000134").is_ok());
        assert!(matches!(
            validate_identifier("lot-bar-24-4412"),
            Err(Error::InvalidIdentifier(_))
        ));
        assert!(matches!(
            validate_identifier("LOT BAR"),
            Err(Error::InvalidIdentifier(_))
        ));
        assert!(matches!(
            validate_identifier("ABCDEFGHIJKLMNOPQRSTU"),
            Err(Error::InvalidIdentifier(_))
        ));
    }

    #[test]
    fn expiry_month_stores_first_of_month() {
        let e = Expiry::from_year_month(2026, 9).expect("sep");
        assert_eq!(e.date.to_string(), "2026-09-01");
        assert_eq!(e.precision, ExpiryPrecision::Month);
        let wire = crate::ExpiryWire::from(e);
        assert_eq!(wire.date, "2026-09");
        assert_eq!(wire.precision, ExpiryPrecision::Month);
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
