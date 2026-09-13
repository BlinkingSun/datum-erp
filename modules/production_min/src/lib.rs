//! Minimal work-order module (Wave 2s `mod-production-min`).
//!
//! Create, release, issue material, complete, and receive the finished lot.
//! Not the Phase 3 `production` module; a surface that module later subsumes.
//! Writes go through [`datum_db::Tx`]. Lots and serials are kernel entities.

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
    CompletionBody, ErrorBody, ErrorFields, QuantityBody, ROUTES, WorkOrderBody, openapi_document,
};
pub use domain::{
    CompleteRequest, Completion, CreateWorkOrder, DEFAULT_FINISHED_LOT_TEMPLATE, DOC_TYPE,
    FinishedLotTemplate, IssueLine, IssueMaterialRequest, ListFilter, NOT_REQUIRED_REASON, Page,
    StartRequest, Status, WorkOrder,
};
pub use error::{Error, Result};
pub use events::{COMPLETED, WORK_ORDER_RELEASED, register_schemas};
pub use hooks::register as register_hooks;
pub use states::work_order_machine;
pub use store::{
    complete, create, issue_material, list, load, load_completion, load_issue_lines, release, start,
};

use datum_db::Tx;
use datum_module::{KernelBuilder, ModuleManifest, Profile};

/// Embedded migrator (`placeholder` + `0001_production_min`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Parsed `module.toml`.
pub fn manifest() -> Result<ModuleManifest> {
    Ok(ModuleManifest::parse(include_str!("../module.toml"))?)
}

/// Register event schemas, routes, and the work-order state machine on `builder`.
pub fn register(builder: &mut KernelBuilder, _profile: &Profile) -> Result<()> {
    register_schemas()?;
    register_hooks(builder)?;
    let mut manifest = manifest()?;
    manifest.machines.clear();
    builder.apply_manifest(&manifest)?;
    builder.register_machine(work_order_machine()?)?;
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
    fn manifest_id_is_mod_production_min_and_not_regulated() {
        let m = manifest().expect("module.toml");
        assert_eq!(m.id, "mod-production-min");
        assert!(!m.regulated);
        assert!(m.permissions.contains_key("production.view"));
        assert!(m.permissions.contains_key("production.create"));
        assert!(m.permissions.contains_key("production.release"));
        assert!(m.permissions.contains_key("production.complete"));
        assert!(m.routes.iter().any(|r| r.path == "/api/v1/work-orders"));
    }

    #[test]
    fn openapi_contains_named_routes_and_error_envelope() {
        let doc = openapi_document();
        let paths = doc.get("paths").and_then(Value::as_object).expect("paths");
        assert!(paths.contains_key("/api/v1/work-orders"));
        assert!(paths.contains_key("/api/v1/work-orders/{id}"));
        assert!(paths.contains_key("/api/v1/work-orders/{id}/release"));
        assert!(paths.contains_key("/api/v1/work-orders/{id}/issue"));
        assert!(paths.contains_key("/api/v1/work-orders/{id}/complete"));
        let envelope = &doc["components"]["schemas"]["ErrorEnvelope"];
        assert_eq!(
            envelope["properties"]["error"]["properties"]["code"]["type"],
            "string"
        );
    }

    #[test]
    fn transition_edges_all_declare_not_required_with_reason() {
        let m = work_order_machine().expect("machine");
        assert_eq!(m.edges.len(), 5);
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
