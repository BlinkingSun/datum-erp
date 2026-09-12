//! Locations module: warehouses, bins, and ledger virtual locations.

#![cfg_attr(test, allow(unused_crate_dependencies))]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use datum_audit as _;

mod api;
mod domain;
mod error;
mod events;
mod hooks;
mod states;
pub mod store;

pub use api::{
    ERROR_CODES, ListResponse, ROUTES, create_location, deactivate_location, error_code,
    get_location, list_locations, list_tree, openapi_document, patch_location,
};
pub use domain::{
    BOUNDARY_VARIANTS, CreateLocation, ListFilter, Location, LocationKind, LocationStatus,
    LocationTreeNode, Site, UpdateLocation, boundary_code, validate_code,
};
pub use error::{Error, Result};
pub use events::{LOCATION_DEACTIVATED, register_schemas};
pub use store::{
    boundary_location_id, create, deactivate, ensure_wip, get, list, list_flat, seed_install,
    update,
};

use datum_module::ModuleManifest;

/// Embedded migrator.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Parsed manifest from `module.toml`.
pub fn manifest() -> Result<ModuleManifest> {
    Ok(ModuleManifest::parse(include_str!("../module.toml"))?)
}

/// Run this crate's migrations on `pool` (after kernel migrators).
pub async fn migrate(pool: &datum_db::Pool) -> Result<()> {
    datum_db::migrate::run(pool, &[("datum-mod-locations", &MIGRATOR)])
        .await
        .map_err(Error::from)
}

/// Install: module migrations are already applied; seed boundaries and register module row.
pub async fn install(tx: &mut datum_db::Tx<'_>, enabled: bool) -> Result<()> {
    store::seed_install(tx).await?;
    let manifest = manifest()?;
    datum_module::install(tx, &manifest, enabled).await?;
    Ok(())
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
    fn migrator_has_real_migration() {
        assert!(MIGRATOR.migrations.len() >= 2);
    }

    #[test]
    fn manifest_parses() {
        let m = manifest().expect("manifest");
        assert_eq!(m.id, "mod-locations");
        assert!(m.permissions.contains_key("locations.view"));
        assert!(m.permissions.contains_key("locations.edit"));
        assert!(!m.regulated);
        assert!(
            m.routes
                .iter()
                .any(|r| r.path == "/api/v1/locations/{id}" && r.permission == "locations.view")
        );
        assert!(m.routes.iter().any(|r| {
            r.path == "/api/v1/locations/{id}/deactivate" && r.permission == "locations.edit"
        }));
    }

    #[test]
    fn http_manifest_binds_view_and_edit() {
        assert!(ROUTES.iter().any(|r| {
            r.method == "GET"
                && r.path == "/api/v1/locations/{id}"
                && r.permission == "locations.view"
        }));
        for (method, path) in [
            ("POST", "/api/v1/locations"),
            ("PATCH", "/api/v1/locations/{id}"),
            ("POST", "/api/v1/locations/{id}/deactivate"),
        ] {
            assert!(
                ROUTES.iter().any(|r| {
                    r.method == method && r.path == path && r.permission == "locations.edit"
                }),
                "{method} {path} must bind locations.edit"
            );
        }
        for (method, path) in [
            ("GET", "/api/v1/locations"),
            ("GET", "/api/v1/locations/{id}"),
            ("GET", "/api/v1/locations/tree"),
        ] {
            assert!(
                ROUTES.iter().any(|r| {
                    r.method == method && r.path == path && r.permission == "locations.view"
                }),
                "{method} {path} must bind locations.view"
            );
        }
        assert!(ERROR_CODES.contains(&"REFUSED"));
        assert_eq!(error_code(&Error::OnHand), "REFUSED");
        assert_eq!(error_code(&Error::Protected), "REFUSED");
        let doc = openapi_document();
        let paths = doc.get("paths").and_then(|v| v.as_object()).expect("paths");
        assert!(paths.contains_key("/api/v1/locations/{id}"));
        let codes = &doc["components"]["schemas"]["ErrorEnvelope"]["properties"]["error"]["properties"]
            ["code"]["enum"];
        assert!(
            codes
                .as_array()
                .is_some_and(|e| e.iter().any(|v| v == "REFUSED"))
        );
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
