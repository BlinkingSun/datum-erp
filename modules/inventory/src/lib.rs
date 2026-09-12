//! Inventory module (Wave 2s `mod-inventory`).
//!
//! Receipts, issues, moves, adjustments, and cycle counts over the ledger.
//! On-hand / allocated / available are ledger projections (PLAN invariant 1).
//! Lots and serials are kernel entities, never text columns.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod api;
mod domain;
mod error;
mod events;
mod hooks;
mod states;
mod store;

pub use api::{
    DocumentBody, ErrorBody, ErrorFields, LineBody, OnHandBody, QuantityBody, ROUTES,
    openapi_document,
};
pub use domain::{
    AdjustRequest, BalanceQuery, CountLine, CountRequest, DOC_TYPE, Document, DocumentKind,
    DocumentLine, DocumentStatus, IssueRequest, LineInput, MoveRequest, NOT_REQUIRED_REASON,
    ReceiveRequest, ReleaseRequest, ReturnRequest, ShipRequest,
};
pub use error::{Error, Result};
pub use events::{ADJUSTED, ISSUED, LOT_RECEIVED, RECEIPT_POSTED, register_schemas};
pub use hooks::register as register_hooks;
pub use states::document_machine;
pub use store::{
    adjust, allocated, available, customer_return, cycle_count, document_history, issue_to_wip,
    load_document, move_stock, on_hand, receive, release_from_quarantine, ship_to_customer,
};

use datum_db::Tx;
use datum_module::{KernelBuilder, ModuleManifest, Profile};

/// Embedded migrator (`placeholder` + `0001_inventory`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Parsed `module.toml`.
pub fn manifest() -> Result<ModuleManifest> {
    Ok(ModuleManifest::parse(include_str!("../module.toml"))?)
}

/// Register event schemas, routes, and the document state machine on `builder`.
pub fn register(builder: &mut KernelBuilder, _profile: &Profile) -> Result<()> {
    register_schemas()?;
    register_hooks(builder)?;
    let mut manifest = manifest()?;
    manifest.machines.clear();
    builder.apply_manifest(&manifest)?;
    builder.register_machine(document_machine()?)?;
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
    use proptest::prelude::*;
    use serde_json::Value;

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
    fn manifest_id_is_mod_inventory_and_not_regulated() {
        let m = manifest().expect("module.toml");
        assert_eq!(m.id, "mod-inventory");
        assert!(!m.regulated);
        assert!(m.permissions.contains_key("inventory.view"));
        assert!(m.permissions.contains_key("inventory.receive"));
        assert!(m.permissions.contains_key("inventory.issue"));
        assert!(m.permissions.contains_key("inventory.move"));
        assert!(m.permissions.contains_key("inventory.adjust"));
        assert!(m.permissions.contains_key("inventory.count"));
        assert!(
            m.routes
                .iter()
                .any(|r| r.path == "/api/v1/inventory/receipts")
        );
    }

    #[test]
    fn openapi_contains_named_routes_and_error_envelope() {
        let doc = openapi_document();
        let paths = doc.get("paths").and_then(Value::as_object).expect("paths");
        assert!(paths.contains_key("/api/v1/inventory/receipts"));
        assert!(paths.contains_key("/api/v1/inventory/issues"));
        assert!(paths.contains_key("/api/v1/inventory/moves"));
        assert!(paths.contains_key("/api/v1/inventory/adjustments"));
        assert!(paths.contains_key("/api/v1/inventory/counts"));
        assert!(paths.contains_key("/api/v1/inventory/on-hand"));
        assert!(paths.contains_key("/api/v1/inventory/documents/{id}"));
        let envelope = &doc["components"]["schemas"]["ErrorEnvelope"];
        assert_eq!(
            envelope["properties"]["error"]["properties"]["code"]["type"],
            "string"
        );
    }

    #[test]
    fn machine_declares_not_required_on_every_edge() {
        let m = document_machine().expect("machine");
        assert_eq!(m.edges.len(), 6);
        for e in &m.edges {
            match &e.signature {
                datum_statemachine::SignatureDeclaration::NotRequired { reason } => {
                    assert_eq!(*reason, NOT_REQUIRED_REASON);
                }
                other => panic!("expected NotRequired, got {other:?}"),
            }
        }
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
