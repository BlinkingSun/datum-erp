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
    ListResponse, create_location, deactivate_location, error_code, get_location, list_locations,
    list_tree, patch_location,
};
pub use domain::{
    BOUNDARY_VARIANTS, CreateLocation, Location, LocationKind, LocationStatus, LocationTreeNode,
    Site, UpdateLocation, boundary_code, validate_code,
};
pub use error::{Error, Result};
pub use events::{LOCATION_DEACTIVATED, register_schemas};
pub use store::{
    boundary_location_id, create, deactivate, default_site_id, ensure_wip, get, list_flat,
    seed_install, update,
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
