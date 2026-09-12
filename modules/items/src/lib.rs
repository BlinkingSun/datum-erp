//! Part master module (`items`).
//!
//! Writes go through [`datum_db::Tx`]. SQL uses `sqlx::query` / `query_as` /
//! `query_scalar` (CONTRACT §5a as amended). Session-protocol helpers stay
//! confined to `datum-db` / `datum-audit` / `datum-test`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

#[cfg(test)]
use datum_audit as _;
#[cfg(test)]
use datum_identity as _;
#[cfg(test)]
use tokio as _;

pub mod api;
pub mod domain;
pub mod error;
pub mod events;
pub mod hooks;
pub mod states;
pub mod store;

pub use api::{ROUTES, openapi_document};
pub use domain::{DOC_TYPE, Item, Kind, ListFilter, NewItem, Page, Status, UpdateItem};
pub use error::{Error, Result};
pub use states::item_machine;
pub use store::{create, get, list, obsolete, release, update};

use datum_module::{KernelBuilder, ModuleManifest};

/// Embedded migrator (`placeholder` + `0001_items`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Parsed `module.toml`.
pub fn manifest() -> Result<ModuleManifest> {
    Ok(ModuleManifest::parse(include_str!("../module.toml"))?)
}

/// Register routes, events, and the item state machine on `builder`.
///
/// Machines are registered through [`KernelBuilder::register_machine`] with an
/// explicit [`datum_statemachine::SignatureDeclaration::NotRequired`] reason
/// (the composition-root TOML converter does not preserve `reason` on
/// non-required edges).
pub fn register(builder: &mut KernelBuilder) -> Result<()> {
    events::register_event_schemas()?;
    hooks::register_hooks(builder);
    let mut manifest = manifest()?;
    manifest.machines.clear();
    builder.apply_manifest(&manifest)?;
    builder.register_machine(item_machine()?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::number_is_valid;
    use proptest::prelude::*;

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
    fn manifest_id_is_items_and_not_regulated() {
        let m = manifest().expect("module.toml");
        assert_eq!(m.id, "items");
        assert!(!m.regulated);
        assert_eq!(
            m.dependencies.get("kernel").map(String::as_str),
            Some("^0.1")
        );
        assert!(m.permissions.contains_key("items.view"));
        assert!(m.permissions.contains_key("items.edit"));
        assert!(m.permissions.contains_key("items.release"));
        assert_eq!(m.dependencies.len(), 1);
    }

    #[test]
    fn openapi_contains_the_four_routes_and_error_envelope() {
        let doc = openapi_document();
        let paths = doc.get("paths").and_then(Value::as_object).expect("paths");
        assert!(paths.contains_key("/api/v1/items"));
        assert!(paths.contains_key("/api/v1/items/{id}"));
        assert!(paths.contains_key("/api/v1/items/{id}/release"));
        assert!(paths["/api/v1/items"].get("get").is_some());
        assert!(paths["/api/v1/items"].get("post").is_some());
        assert!(paths["/api/v1/items/{id}"].get("get").is_some());
        assert!(paths["/api/v1/items/{id}"].get("patch").is_some());
        assert!(paths["/api/v1/items/{id}/release"].get("post").is_some());
        let envelope = &doc["components"]["schemas"]["ErrorEnvelope"];
        assert_eq!(
            envelope["properties"]["error"]["properties"]["code"]["type"],
            "string"
        );
        assert!(
            envelope["properties"]["error"]["properties"]["code"]["enum"]
                .as_array()
                .is_some_and(|e| e.iter().any(|v| v == "VALIDATION"))
        );
    }

    #[test]
    fn canonical_example_number_is_valid() {
        assert!(number_is_valid("MDS-450-M4x12"));
        assert!(number_is_valid("RM-TI-BAR-12"));
        assert!(!number_is_valid("HAS SPACE"));
        assert!(!number_is_valid("bad_underscore"));
        assert!(!number_is_valid(""));
        assert!(!number_is_valid(&"A".repeat(41)));
    }

    #[test]
    fn machine_declares_not_required_on_every_edge() {
        let m = item_machine().expect("machine");
        assert_eq!(m.edges.len(), 2);
        for e in &m.edges {
            match &e.signature {
                datum_statemachine::SignatureDeclaration::NotRequired { reason } => {
                    assert_eq!(*reason, crate::domain::NOT_REQUIRED_REASON);
                }
                other => panic!("expected NotRequired, got {other:?}"),
            }
        }
    }

    use serde_json::Value;

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
