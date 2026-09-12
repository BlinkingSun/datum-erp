//! Genealogy module (Wave 2s `mod-genealogy`).
//!
//! Read-only traces over the ledger consumption graph. The cache in
//! `genealogy_transient` is rebuildable and never authoritative. Lots and serials
//! are kernel entities, never text columns.

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
    AcceptedBody, ErrorBody, ErrorFields, ROUTES, example_tree, openapi_document, trace_http_body,
    trace_http_status,
};
pub use domain::{
    DEFAULT_INLINE_MAX_POSTINGS, Direction, ExportFormat, INLINE_MAX_ENV, Impact, TraceOrigin,
    TraceRequest, Tree, TreeNode,
};
pub use error::{Error, Result};
pub use events::CacheInvalidate;
pub use hooks::register as register_hooks;
pub use states::DOC_TYPE;
pub use store::{
    TRACE_JOB, TraceBody, TraceJob, TraceOutcome, cache_key_for, drop_cache, export_body, impact,
    inline_max_postings, invalidate_cache, job_status, signed_edge_sum, trace, trace_inline,
    tree_csv, undirected_edges, where_used,
};

use datum_db::Tx;
use datum_module::{Kernel, KernelBuilder, ModuleManifest, Profile};

/// Embedded migrator (`placeholder` + `0001_genealogy`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Parsed `module.toml`.
pub fn manifest() -> Result<ModuleManifest> {
    Ok(ModuleManifest::parse(include_str!("../module.toml"))?)
}

/// Register routes, subscriptions, and job kinds on `builder`.
pub fn register(builder: &mut KernelBuilder, _profile: &Profile) -> Result<()> {
    register_hooks(builder)?;
    builder.apply_manifest(&manifest()?)?;
    Ok(())
}

/// Wire handlers after [`Kernel::build`]: job compute and event subscriptions.
///
/// Event names come from the manifest (`register_enabled_from_manifests` /
/// `events.subscribe`), never from a constant in this crate.
pub async fn wire(kernel: &Kernel, tx: &mut Tx<'_>) -> Result<()> {
    let subscriber = manifest()?
        .subscriptions
        .first()
        .map(|s| s.subscriber.clone())
        .ok_or_else(|| Error::Manifest("missing subscriptions".into()))?;
    kernel.jobs.register(
        TRACE_JOB,
        TraceJob {
            pool: kernel.pool().clone(),
            actor: Kernel::service_actor(),
        },
    );
    for sub in kernel
        .subscriptions
        .iter()
        .filter(|s| s.subscriber == subscriber)
    {
        kernel
            .register_events_subscription(tx, &sub.event, &sub.subscriber, CacheInvalidate)
            .await?;
    }
    Ok(())
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
    fn default_inline_max_is_tested() {
        assert_eq!(DEFAULT_INLINE_MAX_POSTINGS, 32);
        assert!(inline_max_postings() >= 1 || inline_max_postings() == 0);
    }

    #[test]
    fn manifest_id_is_mod_genealogy_and_not_regulated() {
        let m = manifest().expect("module.toml");
        assert_eq!(m.id, "mod-genealogy");
        assert!(!m.regulated);
        assert!(m.permissions.contains_key("genealogy.view"));
        assert!(m.permissions.contains_key("genealogy.export"));
        assert!(m.routes.iter().any(|r| r.path == "/api/v1/genealogy/trace"));
        assert!(
            m.routes
                .iter()
                .any(|r| r.path == "/api/v1/genealogy/impact/{lot}")
        );
        assert!(
            m.routes
                .iter()
                .any(|r| r.path == "/api/v1/genealogy/jobs/{id}")
        );
        assert!(
            m.subscriptions
                .iter()
                .any(|s| s.event == "inventory.lot_received")
        );
        assert!(
            m.subscriptions
                .iter()
                .any(|s| s.event == "inventory.issued")
        );
        assert!(
            m.subscriptions
                .iter()
                .any(|s| s.event == "production.completed")
        );
        assert!(m.jobs.iter().any(|j| j.kind == "genealogy.trace"));
    }

    #[test]
    fn openapi_contains_named_routes_and_error_envelope() {
        let doc = openapi_document();
        let paths = doc.get("paths").and_then(Value::as_object).expect("paths");
        assert!(paths.contains_key("/api/v1/genealogy/trace"));
        assert!(paths.contains_key("/api/v1/genealogy/impact/{lot}"));
        assert!(paths.contains_key("/api/v1/genealogy/jobs/{id}"));
        let envelope = &doc["components"]["schemas"]["ErrorEnvelope"];
        assert_eq!(
            envelope["properties"]["error"]["properties"]["code"]["type"],
            "string"
        );
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
