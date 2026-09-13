//! Canonical install order (D-2b-10) and module-graph topological sort.

use std::collections::{BTreeSet, HashMap};

use sqlx::migrate::Migrator;

use crate::{Error, MIGRATOR, Result};

/// Kernel crates' migrators in dependency order (CONTRACT §4; D-2b-10).
///
/// Prefix of [`CANONICAL_ORDER`]. First-party modules follow via
/// [`crate::wave_2s1_order`] then [`crate::slice_migrators`].
pub const KERNEL_ORDER: &[&str] = &[
    "datum-db",
    "datum-audit",
    "datum-identity",
    "datum-numbering",
    "datum-uom",
    "datum-events",
    "datum-jobs",
    "datum-ledger",
    "datum-statemachine",
    "datum-esign",
    "datum-customfields",
    "datum-documents",
    "datum-print",
    "datum-module",
];

/// The only install order (D-2b-10). Extends [`KERNEL_ORDER`] by
/// `wave_2s1_order()` (`items`, `locations`, `lots`) then
/// [`crate::slice_migrators`] (`inventory`, `production-min`, `genealogy`,
/// `server`). Nothing outside this module may name a migrator list.
pub const CANONICAL_ORDER: &[&str] = &[
    "datum-db",
    "datum-audit",
    "datum-identity",
    "datum-numbering",
    "datum-uom",
    "datum-events",
    "datum-jobs",
    "datum-ledger",
    "datum-statemachine",
    "datum-esign",
    "datum-customfields",
    "datum-documents",
    "datum-print",
    "datum-module",
    "datum-mod-items",
    "datum-mod-locations",
    "datum-mod-lots",
    "datum-mod-inventory",
    "datum-mod-production-min",
    "datum-mod-genealogy",
    "datum-server",
];

/// CONTRACT §4 kernel edges used to prove [`KERNEL_ORDER`] (and
/// [`CANONICAL_ORDER`]) is a topological sort.
///
/// `datum-core` is omitted: it has no migrator and is not in [`KERNEL_ORDER`].
pub const CONTRACT_KERNEL_EDGES: &[(&str, &str)] = &[
    ("datum-audit", "datum-db"),
    ("datum-identity", "datum-db"),
    ("datum-identity", "datum-audit"),
    ("datum-numbering", "datum-db"),
    ("datum-uom", "datum-db"),
    ("datum-uom", "datum-audit"),
    ("datum-events", "datum-db"),
    ("datum-jobs", "datum-db"),
    ("datum-jobs", "datum-events"),
    ("datum-ledger", "datum-db"),
    ("datum-ledger", "datum-audit"),
    ("datum-ledger", "datum-uom"),
    ("datum-statemachine", "datum-db"),
    ("datum-statemachine", "datum-audit"),
    ("datum-statemachine", "datum-identity"),
    ("datum-esign", "datum-db"),
    ("datum-esign", "datum-audit"),
    ("datum-esign", "datum-identity"),
    ("datum-customfields", "datum-db"),
    ("datum-customfields", "datum-audit"),
    ("datum-documents", "datum-db"),
    ("datum-documents", "datum-audit"),
    ("datum-documents", "datum-identity"),
    ("datum-documents", "datum-numbering"),
    ("datum-documents", "datum-statemachine"),
    ("datum-print", "datum-db"),
    ("datum-print", "datum-audit"),
    ("datum-print", "datum-documents"),
    ("datum-print", "datum-esign"),
    ("datum-module", "datum-db"),
    ("datum-module", "datum-audit"),
    ("datum-module", "datum-identity"),
    ("datum-module", "datum-numbering"),
    ("datum-module", "datum-uom"),
    ("datum-module", "datum-events"),
    ("datum-module", "datum-jobs"),
    ("datum-module", "datum-ledger"),
    ("datum-module", "datum-statemachine"),
    ("datum-module", "datum-esign"),
    ("datum-module", "datum-customfields"),
    ("datum-module", "datum-documents"),
    ("datum-module", "datum-print"),
];

/// First-party module edges used to prove [`CANONICAL_ORDER`] is a
/// topological sort of the Wave 2s graph (`module.toml` `depends_on`).
pub const CONTRACT_SLICE_EDGES: &[(&str, &str)] = &[
    ("datum-mod-lots", "datum-mod-items"),
    ("datum-mod-lots", "datum-mod-locations"),
    ("datum-mod-inventory", "datum-mod-items"),
    ("datum-mod-inventory", "datum-mod-locations"),
    ("datum-mod-inventory", "datum-mod-lots"),
    ("datum-mod-production-min", "datum-mod-items"),
    ("datum-mod-production-min", "datum-mod-locations"),
    ("datum-mod-production-min", "datum-mod-lots"),
    ("datum-mod-production-min", "datum-mod-inventory"),
    ("datum-mod-genealogy", "datum-mod-items"),
    ("datum-mod-genealogy", "datum-mod-locations"),
    ("datum-mod-genealogy", "datum-mod-lots"),
    ("datum-mod-genealogy", "datum-mod-inventory"),
    ("datum-server", "datum-module"),
    ("datum-server", "datum-mod-items"),
    ("datum-server", "datum-mod-locations"),
    ("datum-server", "datum-mod-lots"),
    ("datum-server", "datum-mod-inventory"),
    ("datum-server", "datum-mod-production-min"),
    ("datum-server", "datum-mod-genealogy"),
];

/// [`KERNEL_ORDER`] paired with each crate's embedded migrator.
pub fn kernel_crates() -> Vec<(&'static str, &'static Migrator)> {
    vec![
        ("datum-db", &datum_db::MIGRATOR),
        ("datum-audit", &datum_audit::MIGRATOR),
        ("datum-identity", &datum_identity::MIGRATOR),
        ("datum-numbering", &datum_numbering::MIGRATOR),
        ("datum-uom", &datum_uom::MIGRATOR),
        ("datum-events", &datum_events::MIGRATOR),
        ("datum-jobs", &datum_jobs::MIGRATOR),
        ("datum-ledger", &datum_ledger::MIGRATOR),
        ("datum-statemachine", &datum_statemachine::MIGRATOR),
        ("datum-esign", &datum_esign::MIGRATOR),
        ("datum-customfields", &datum_customfields::MIGRATOR),
        ("datum-documents", &datum_documents::MIGRATOR),
        ("datum-print", &datum_print::MIGRATOR),
        ("datum-module", &MIGRATOR),
    ]
}

/// [`KERNEL_ORDER`] migrators. Alias of [`kernel_crates`] (D-2b-10 includes
/// `datum-print` and `datum-module` in the kernel prefix).
pub fn kernel_migrators() -> Vec<(&'static str, &'static Migrator)> {
    kernel_crates()
}

/// [`CANONICAL_ORDER`] paired with each crate's embedded migrator.
pub fn canonical_migrators() -> Result<Vec<(&'static str, &'static Migrator)>> {
    let mut by_name: HashMap<&str, &'static Migrator> = HashMap::new();
    for (name, migrator) in kernel_crates() {
        by_name.insert(name, migrator);
    }
    for (name, migrator) in crate::install_graph::wave_2s1_migrators()? {
        by_name.insert(name, migrator);
    }
    for (name, migrator) in crate::install_graph::slice_migrators() {
        by_name.insert(name, migrator);
    }
    CANONICAL_ORDER
        .iter()
        .map(|name| {
            by_name
                .get(name)
                .copied()
                .map(|migrator| (*name, migrator))
                .ok_or_else(|| Error::Manifest(format!("canonical migrator missing for {name}")))
        })
        .collect()
}

/// Names of the crates [`migrate_prefix`] applies (db + audit).
pub const MIGRATE_PREFIX: &[&str] = &["datum-db", "datum-audit"];

/// Apply `datum-db` then `datum-audit`.
///
/// Harnesses that need the event trigger up should call [`install_upto`]
/// rather than this plus a hand-rolled suffix (D-2b-13).
pub async fn migrate_prefix(pool: &datum_db::Pool) -> Result<()> {
    datum_db::migrate::run(
        pool,
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await?;
    Ok(())
}

/// Apply identity through this crate (the [`KERNEL_ORDER`] suffix after
/// [`MIGRATE_PREFIX`]). Prefer [`install_upto`]: this path does not install
/// the event trigger (D-2b-11 / D-2b-13).
pub async fn migrate_suffix(pool: &datum_db::Pool) -> Result<()> {
    let crates = kernel_migrators();
    let rest: Vec<_> = crates
        .iter()
        .copied()
        .filter(|(name, _)| !MIGRATE_PREFIX.contains(name))
        .collect();
    for (name, migrator) in &rest {
        datum_db::migrate::run(pool, &[(*name, *migrator)])
            .await
            .map_err(|e| Error::Manifest(format!("migrate {name}: {e}")))?;
    }
    Ok(())
}

/// App-class tables the event trigger may miss (`datum` schema is skipped;
/// numbering.counter is `audit.exempt`; `datum.schema_class` stays unattached
/// so later migrators can INSERT class rows without actor GUCs).
///
/// Belt-and-braces (D-2b-13): under D-2b-11 every crate attaches its own.
/// [`attach_kernel_audit`] is idempotent.
pub const KERNEL_AUDIT_RELS: &[&str] = &[
    "datum.schema_history",
    "module.installed",
    "module.configuration",
    "module.install_log",
    "identity.principal",
    "identity.username_history",
    "identity.display_name_history",
    "identity.login_credential",
    "identity.signing_credential",
    "identity.credential_reset",
    "identity.role",
    "identity.role_permission",
    "identity.principal_role",
    "esign.signature",
    "esign.meaning_policy",
    "esign.supersession",
    "customfields.definition",
    "customfields.value_string",
    "customfields.value_text",
    "customfields.value_integer",
    "customfields.value_decimal",
    "customfields.value_bool",
    "customfields.value_date",
    "customfields.value_enum",
    "customfields.value_reference",
    "uom.unit",
    "uom.item_stock",
    "uom.factor",
    "uom.rounding_policy",
    "app.event",
    "app.subscription",
    "app.run_log",
    "ledger.stock_item",
    "ledger.location",
    "ledger.posting_group",
    "ledger.posting",
    "ledger.consumption",
    "sm.machine",
    "sm.state",
    "sm.edge",
    "sm.instance",
    "documents.document",
    "documents.revision",
    "documents.blob",
    "documents.attachment",
    "documents.link",
    "print.install",
    "print.template",
    "print.render_log",
    "items.item",
    "items.item_revision_history",
    "locations.site",
    "locations.location",
    "lots.lot",
    "lots.serial",
    "lots.package",
    "lots.status_history",
];

/// App-class tables owned by the Wave 2s slice (inventory / production_min /
/// server). Belt-and-braces attach set for [`install_upto`] / [`install_slice`].
pub const SLICE_AUDIT_RELS: &[&str] = &[
    "inventory.document",
    "inventory.document_line",
    "production_min.work_order",
    "production_min.issue_line",
    "production_min.completion",
    "server.boot_record",
];

/// Attach `datum.schema_history` and every other app-class table that missed
/// the event trigger (`datum` schema, or tables created before privileged).
pub async fn attach_kernel_audit(pool: &datum_db::Pool) -> Result<()> {
    attach_listed_audit(pool, KERNEL_AUDIT_RELS).await
}

/// Attach every Wave 2s slice app-class table in [`SLICE_AUDIT_RELS`].
pub async fn attach_slice_audit(pool: &datum_db::Pool) -> Result<()> {
    attach_listed_audit(pool, SLICE_AUDIT_RELS).await
}

async fn attach_listed_audit(pool: &datum_db::Pool, rels: &[&str]) -> Result<()> {
    for rel in rels {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1::text) IS NOT NULL")
            .bind(rel)
            .fetch_one(pool)
            .await?;
        if exists {
            datum_audit::attach(pool, rel).await?;
        }
    }
    Ok(())
}

/// Apply [`CANONICAL_ORDER`] through `crate_name` (inclusive).
///
/// The one published harness entry point (D-2b-13). `install_privileged`
/// runs once, immediately after `datum-audit`, and is never dropped
/// (D-2b-11). [`attach_kernel_audit`] / [`attach_slice_audit`] run afterwards
/// as belt-and-braces: under D-2b-11 every crate attaches its own tables.
pub async fn install_upto(
    migrate: &datum_db::Pool,
    bootstrap: &datum_db::Pool,
    crate_name: &str,
) -> Result<()> {
    let crates = canonical_migrators()?;
    let idx = crates
        .iter()
        .position(|(name, _)| *name == crate_name)
        .ok_or_else(|| {
            Error::Manifest(format!(
                "unknown crate {crate_name}; not in CANONICAL_ORDER"
            ))
        })?;
    let mut saw_audit = false;
    for (name, migrator) in crates.iter().take(idx + 1) {
        datum_db::migrate::run(migrate, &[(*name, *migrator)])
            .await
            .map_err(|e| Error::Manifest(format!("migrate {name}: {e}")))?;
        if *name == "datum-audit" {
            datum_audit::install_privileged(bootstrap).await?;
            saw_audit = true;
        }
    }
    if saw_audit {
        attach_kernel_audit(migrate).await?;
        attach_slice_audit(migrate).await?;
    }
    Ok(())
}

/// [`install_upto`] through [`KERNEL_ORDER`]'s last crate (`datum-module`).
pub async fn install_kernel(migrate: &datum_db::Pool, bootstrap: &datum_db::Pool) -> Result<()> {
    install_upto(migrate, bootstrap, "datum-module").await
}

/// [`install_upto`] through [`CANONICAL_ORDER`]'s last crate (`datum-server`).
pub async fn install_slice(migrate: &datum_db::Pool, bootstrap: &datum_db::Pool) -> Result<()> {
    install_upto(migrate, bootstrap, "datum-server").await
}

/// Run the full kernel (and this crate) without the privileged event-trigger step.
pub async fn run_migrations(pool: &datum_db::Pool) -> Result<()> {
    migrate_prefix(pool).await?;
    migrate_suffix(pool).await?;
    Ok(())
}

/// One node of the module dependency graph (CONTRACT §6.2 rule 8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleNode {
    /// Module id. Ties in the topological order are broken by this string.
    pub id: String,
    /// Modules this one depends on (they run first).
    pub depends_on: Vec<String>,
}

impl From<ModuleNode> for datum_statemachine::ModuleNode {
    fn from(n: ModuleNode) -> Self {
        datum_statemachine::ModuleNode {
            id: n.id,
            depends_on: n.depends_on,
        }
    }
}

/// Dependency-topological order; ties broken by module id (CONTRACT §6.2 rule 8).
pub fn topological_order(nodes: &[ModuleNode]) -> Result<Vec<String>> {
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut indeg: HashMap<String, usize> = HashMap::new();
    for n in nodes {
        indeg.entry(n.id.clone()).or_insert(0);
        children.entry(n.id.clone()).or_default();
        for dep in &n.depends_on {
            indeg.entry(dep.clone()).or_insert(0);
            children.entry(dep.clone()).or_default();
            *indeg.entry(n.id.clone()).or_insert(0) += 1;
            children.entry(dep.clone()).or_default().push(n.id.clone());
        }
    }
    let mut ready: BTreeSet<String> = indeg
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(k, _)| k.clone())
        .collect();
    let mut out = Vec::new();
    while let Some(id) = ready.iter().next().cloned() {
        ready.remove(&id);
        out.push(id.clone());
        if let Some(chs) = children.get(&id) {
            for ch in chs {
                if let Some(d) = indeg.get_mut(ch) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        ready.insert(ch.clone());
                    }
                }
            }
        }
    }
    if out.len() != indeg.len() {
        return Err(Error::DependencyCycle);
    }
    Ok(out)
}

/// True when `order` is a topological sort of `edges` (`(crate, depends_on)`).
pub fn is_topological_sort(order: &[&str], edges: &[(&str, &str)]) -> bool {
    let pos: HashMap<&str, usize> = order.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    for (crate_name, dep) in edges {
        let Some(&c) = pos.get(crate_name) else {
            continue;
        };
        let Some(&d) = pos.get(dep) else {
            continue;
        };
        if d >= c {
            return false;
        }
    }
    true
}
