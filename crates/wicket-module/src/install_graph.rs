//! Wave 2s.1 module install graph: manifest + migrator per integrated crate.

use sqlx::migrate::Migrator;

use crate::manifest::ModuleManifest;
use crate::order::topological_order;
use crate::profile::{Profile, ProfileId};
use crate::{Error, Result};

/// `wicket-mod-items` embedded migrator (path relative to this crate's manifest).
pub static ITEMS_MIGRATOR: Migrator = sqlx::migrate!("../../modules/items/migrations");
/// `wicket-mod-locations` embedded migrator.
pub static LOCATIONS_MIGRATOR: Migrator = sqlx::migrate!("../../modules/locations/migrations");
/// `wicket-mod-lots` embedded migrator.
pub static LOTS_MIGRATOR: Migrator = sqlx::migrate!("../../modules/lots/migrations");
/// `wicket-mod-inventory` embedded migrator.
pub static INVENTORY_MIGRATOR: Migrator = sqlx::migrate!("../../modules/inventory/migrations");
/// `wicket-mod-production-min` embedded migrator.
pub static PRODUCTION_MIN_MIGRATOR: Migrator =
    sqlx::migrate!("../../modules/production_min/migrations");
/// `wicket-mod-genealogy` embedded migrator.
pub static GENEALOGY_MIGRATOR: Migrator = sqlx::migrate!("../../modules/genealogy/migrations");
/// `wicket-server` embedded migrator (`server.boot_record`).
pub static SERVER_MIGRATOR: Migrator = sqlx::migrate!("../wicket-server/migrations");

pub const ITEMS_MANIFEST: &str = include_str!("../../../modules/items/module.toml");
pub const LOCATIONS_MANIFEST: &str = include_str!("../../../modules/locations/module.toml");
pub const LOTS_MANIFEST: &str = include_str!("../../../modules/lots/module.toml");

/// One integrated first-party module the composition root owns.
pub struct ModuleInstallSpec {
    /// Manifest id (`mod-items`, …).
    pub id: &'static str,
    /// `wicket.schema_history` crate label.
    pub migrate_crate: &'static str,
    /// Parsed `module.toml` from the owning crate.
    pub manifest: fn() -> Result<ModuleManifest>,
    /// SQL migrations run at install time.
    pub migrator: &'static Migrator,
}

/// Wave 2s.1 modules integrated on main (R-2s-2 name triple).
pub fn wave_2s1_specs() -> &'static [ModuleInstallSpec] {
    static SPECS: [ModuleInstallSpec; 3] = [
        ModuleInstallSpec {
            id: "mod-items",
            migrate_crate: "wicket-mod-items",
            manifest: || ModuleManifest::parse(ITEMS_MANIFEST),
            migrator: &ITEMS_MIGRATOR,
        },
        ModuleInstallSpec {
            id: "mod-locations",
            migrate_crate: "wicket-mod-locations",
            manifest: || ModuleManifest::parse(LOCATIONS_MANIFEST),
            migrator: &LOCATIONS_MIGRATOR,
        },
        ModuleInstallSpec {
            id: "mod-lots",
            migrate_crate: "wicket-mod-lots",
            manifest: || ModuleManifest::parse(LOTS_MANIFEST),
            migrator: &LOTS_MIGRATOR,
        },
    ];
    &SPECS
}

/// Lookup by manifest id.
pub fn spec_by_id(id: &str) -> Option<&'static ModuleInstallSpec> {
    wave_2s1_specs().iter().find(|s| s.id == id)
}

/// Topological install order for Wave 2s.1 (`lots` after `items` and `locations`).
pub fn wave_2s1_order() -> Result<Vec<String>> {
    let mut nodes = Vec::with_capacity(wave_2s1_specs().len());
    for spec in wave_2s1_specs() {
        nodes.push((spec.manifest)()?.node());
    }
    topological_order(&nodes)
}

/// `(crate, migrator)` pairs in dependency order.
pub fn wave_2s1_migrators() -> Result<Vec<(&'static str, &'static Migrator)>> {
    let order = wave_2s1_order()?;
    let by_id: std::collections::BTreeMap<&str, &ModuleInstallSpec> =
        wave_2s1_specs().iter().map(|s| (s.id, s)).collect();
    order
        .iter()
        .map(|id| {
            let spec = by_id
                .get(id.as_str())
                .ok_or_else(|| Error::UnknownModule(id.clone()))?;
            Ok((spec.migrate_crate, spec.migrator as &'static Migrator))
        })
        .collect()
}

/// Run Wave 2s.1 SQL migrations on the migrate pool (after kernel migrators).
pub async fn migrate_wave_2s1_modules(pool: &wicket_db::Pool) -> Result<()> {
    let crates = wave_2s1_migrators()?;
    if crates.is_empty() {
        return Ok(());
    }
    wicket_db::migrate::run(pool, &crates).await?;
    Ok(())
}

/// `(crate, migrator)` pairs for the Wave 2s slice (inventory, production,
/// genealogy, server). Order matches `wicket-server` boot.
pub fn slice_migrators() -> Vec<(&'static str, &'static Migrator)> {
    vec![
        ("wicket-mod-inventory", &INVENTORY_MIGRATOR),
        ("wicket-mod-production-min", &PRODUCTION_MIN_MIGRATOR),
        ("wicket-mod-genealogy", &GENEALOGY_MIGRATOR),
        ("wicket-server", &SERVER_MIGRATOR),
    ]
}

/// Run Wave 2s slice SQL migrations (after Wave 2s.1, before audit attach).
pub async fn migrate_slice_modules(pool: &wicket_db::Pool) -> Result<()> {
    let crates = slice_migrators();
    wicket_db::migrate::run(pool, &crates).await?;
    Ok(())
}

/// App-class tables owned by Wave 2s.1 modules (audit matrix / attach set).
pub const WAVE_2S1_AUDIT_RELS: &[&str] = &[
    "items.item",
    "items.item_revision_history",
    "locations.site",
    "locations.location",
    "lots.lot",
    "lots.serial",
    "lots.package",
    "lots.status_history",
];

/// `mod-items` / `mod-lots` register machines from code; skip manifest machines.
pub fn manifest_machines_owned_by_register(id: &str) -> bool {
    matches!(id, "mod-items" | "mod-lots")
}

/// Regulated-device profile: lot `release` is a Required signature point.
pub fn lots_release_is_required(profile: &Profile) -> bool {
    profile.id == ProfileId::RegulatedDevice
}
