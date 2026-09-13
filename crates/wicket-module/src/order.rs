//! Canonical install order (D-2b-10) and module-graph topological sort.

use std::collections::{BTreeSet, HashMap};

use sqlx::migrate::Migrator;

use crate::{Error, MIGRATOR, Result};

/// Kernel crates' migrators in dependency order (CONTRACT §4; D-2b-10).
///
/// Prefix of [`CANONICAL_ORDER`]. First-party modules follow via
/// [`crate::wave_2s1_order`] then [`crate::slice_migrators`].
pub const KERNEL_ORDER: &[&str] = &[
    "wicket-db",
    "wicket-audit",
    "wicket-identity",
    "wicket-numbering",
    "wicket-uom",
    "wicket-events",
    "wicket-jobs",
    "wicket-ledger",
    "wicket-statemachine",
    "wicket-esign",
    "wicket-customfields",
    "wicket-documents",
    "wicket-print",
    "wicket-module",
];

/// The only install order (D-2b-10). Extends [`KERNEL_ORDER`] by
/// `wave_2s1_order()` (`items`, `locations`, `lots`) then
/// [`crate::slice_migrators`] (`inventory`, `production-min`, `genealogy`,
/// `server`). Nothing outside this module may name a migrator list.
pub const CANONICAL_ORDER: &[&str] = &[
    "wicket-db",
    "wicket-audit",
    "wicket-identity",
    "wicket-numbering",
    "wicket-uom",
    "wicket-events",
    "wicket-jobs",
    "wicket-ledger",
    "wicket-statemachine",
    "wicket-esign",
    "wicket-customfields",
    "wicket-documents",
    "wicket-print",
    "wicket-module",
    "wicket-mod-items",
    "wicket-mod-locations",
    "wicket-mod-lots",
    "wicket-mod-inventory",
    "wicket-mod-production-min",
    "wicket-mod-genealogy",
    "wicket-server",
];

/// CONTRACT §4 kernel edges used to prove [`KERNEL_ORDER`] (and
/// [`CANONICAL_ORDER`]) is a topological sort.
///
/// `wicket-core` is omitted: it has no migrator and is not in [`KERNEL_ORDER`].
pub const CONTRACT_KERNEL_EDGES: &[(&str, &str)] = &[
    ("wicket-audit", "wicket-db"),
    ("wicket-identity", "wicket-db"),
    ("wicket-identity", "wicket-audit"),
    ("wicket-numbering", "wicket-db"),
    ("wicket-uom", "wicket-db"),
    ("wicket-uom", "wicket-audit"),
    ("wicket-events", "wicket-db"),
    ("wicket-jobs", "wicket-db"),
    ("wicket-jobs", "wicket-events"),
    ("wicket-ledger", "wicket-db"),
    ("wicket-ledger", "wicket-audit"),
    ("wicket-ledger", "wicket-uom"),
    ("wicket-statemachine", "wicket-db"),
    ("wicket-statemachine", "wicket-audit"),
    ("wicket-statemachine", "wicket-identity"),
    ("wicket-esign", "wicket-db"),
    ("wicket-esign", "wicket-audit"),
    ("wicket-esign", "wicket-identity"),
    ("wicket-customfields", "wicket-db"),
    ("wicket-customfields", "wicket-audit"),
    ("wicket-documents", "wicket-db"),
    ("wicket-documents", "wicket-audit"),
    ("wicket-documents", "wicket-identity"),
    ("wicket-documents", "wicket-numbering"),
    ("wicket-documents", "wicket-statemachine"),
    ("wicket-print", "wicket-db"),
    ("wicket-print", "wicket-audit"),
    ("wicket-print", "wicket-documents"),
    ("wicket-print", "wicket-esign"),
    ("wicket-module", "wicket-db"),
    ("wicket-module", "wicket-audit"),
    ("wicket-module", "wicket-identity"),
    ("wicket-module", "wicket-numbering"),
    ("wicket-module", "wicket-uom"),
    ("wicket-module", "wicket-events"),
    ("wicket-module", "wicket-jobs"),
    ("wicket-module", "wicket-ledger"),
    ("wicket-module", "wicket-statemachine"),
    ("wicket-module", "wicket-esign"),
    ("wicket-module", "wicket-customfields"),
    ("wicket-module", "wicket-documents"),
    ("wicket-module", "wicket-print"),
];

/// First-party module edges used to prove [`CANONICAL_ORDER`] is a
/// topological sort of the Wave 2s graph (`module.toml` `depends_on`).
pub const CONTRACT_SLICE_EDGES: &[(&str, &str)] = &[
    ("wicket-mod-lots", "wicket-mod-items"),
    ("wicket-mod-lots", "wicket-mod-locations"),
    ("wicket-mod-inventory", "wicket-mod-items"),
    ("wicket-mod-inventory", "wicket-mod-locations"),
    ("wicket-mod-inventory", "wicket-mod-lots"),
    ("wicket-mod-production-min", "wicket-mod-items"),
    ("wicket-mod-production-min", "wicket-mod-locations"),
    ("wicket-mod-production-min", "wicket-mod-lots"),
    ("wicket-mod-production-min", "wicket-mod-inventory"),
    ("wicket-mod-genealogy", "wicket-mod-items"),
    ("wicket-mod-genealogy", "wicket-mod-locations"),
    ("wicket-mod-genealogy", "wicket-mod-lots"),
    ("wicket-mod-genealogy", "wicket-mod-inventory"),
    ("wicket-server", "wicket-module"),
    ("wicket-server", "wicket-mod-items"),
    ("wicket-server", "wicket-mod-locations"),
    ("wicket-server", "wicket-mod-lots"),
    ("wicket-server", "wicket-mod-inventory"),
    ("wicket-server", "wicket-mod-production-min"),
    ("wicket-server", "wicket-mod-genealogy"),
];

/// [`KERNEL_ORDER`] paired with each crate's embedded migrator.
pub fn kernel_crates() -> Vec<(&'static str, &'static Migrator)> {
    vec![
        ("wicket-db", &wicket_db::MIGRATOR),
        ("wicket-audit", &wicket_audit::MIGRATOR),
        ("wicket-identity", &wicket_identity::MIGRATOR),
        ("wicket-numbering", &wicket_numbering::MIGRATOR),
        ("wicket-uom", &wicket_uom::MIGRATOR),
        ("wicket-events", &wicket_events::MIGRATOR),
        ("wicket-jobs", &wicket_jobs::MIGRATOR),
        ("wicket-ledger", &wicket_ledger::MIGRATOR),
        ("wicket-statemachine", &wicket_statemachine::MIGRATOR),
        ("wicket-esign", &wicket_esign::MIGRATOR),
        ("wicket-customfields", &wicket_customfields::MIGRATOR),
        ("wicket-documents", &wicket_documents::MIGRATOR),
        ("wicket-print", &wicket_print::MIGRATOR),
        ("wicket-module", &MIGRATOR),
    ]
}

/// [`KERNEL_ORDER`] migrators. Alias of [`kernel_crates`] (D-2b-10 includes
/// `wicket-print` and `wicket-module` in the kernel prefix).
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
pub const MIGRATE_PREFIX: &[&str] = &["wicket-db", "wicket-audit"];

/// Apply `wicket-db` then `wicket-audit`.
///
/// Harnesses that need the event trigger up should call [`install_upto`]
/// rather than this plus a hand-rolled suffix (D-2b-13).
pub async fn migrate_prefix(pool: &wicket_db::Pool) -> Result<()> {
    wicket_db::migrate::run(
        pool,
        &[
            ("wicket-db", &wicket_db::MIGRATOR),
            ("wicket-audit", &wicket_audit::MIGRATOR),
        ],
    )
    .await?;
    Ok(())
}

/// Apply identity through this crate (the [`KERNEL_ORDER`] suffix after
/// [`MIGRATE_PREFIX`]). Prefer [`install_upto`]: this path does not install
/// the event trigger (D-2b-11 / D-2b-13).
pub async fn migrate_suffix(pool: &wicket_db::Pool) -> Result<()> {
    let crates = kernel_migrators();
    let rest: Vec<_> = crates
        .iter()
        .copied()
        .filter(|(name, _)| !MIGRATE_PREFIX.contains(name))
        .collect();
    for (name, migrator) in &rest {
        wicket_db::migrate::run(pool, &[(*name, *migrator)])
            .await
            .map_err(|e| Error::Manifest(format!("migrate {name}: {e}")))?;
    }
    Ok(())
}

/// App-class tables the event trigger may miss (`wicket` schema is skipped;
/// numbering.counter is `audit.exempt`; `wicket.schema_class` stays unattached
/// so later migrators can INSERT class rows without actor GUCs).
///
/// Belt-and-braces (D-2b-13): under D-2b-11 every crate attaches its own.
/// [`attach_kernel_audit`] is idempotent.
pub const KERNEL_AUDIT_RELS: &[&str] = &[
    "wicket.schema_history",
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

/// Attach `wicket.schema_history` and every other app-class table that missed
/// the event trigger (`wicket` schema, or tables created before privileged).
pub async fn attach_kernel_audit(pool: &wicket_db::Pool) -> Result<()> {
    attach_listed_audit(pool, KERNEL_AUDIT_RELS).await
}

/// Attach every Wave 2s slice app-class table in [`SLICE_AUDIT_RELS`].
pub async fn attach_slice_audit(pool: &wicket_db::Pool) -> Result<()> {
    attach_listed_audit(pool, SLICE_AUDIT_RELS).await
}

async fn attach_listed_audit(pool: &wicket_db::Pool, rels: &[&str]) -> Result<()> {
    for rel in rels {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1::text) IS NOT NULL")
            .bind(rel)
            .fetch_one(pool)
            .await?;
        if exists {
            wicket_audit::attach(pool, rel).await?;
        }
    }
    Ok(())
}

/// Apply [`CANONICAL_ORDER`] through `crate_name` (inclusive).
///
/// The one published harness entry point (D-2b-13). `install_privileged`
/// runs once, immediately after `wicket-audit`, and is never dropped
/// (D-2b-11). [`attach_kernel_audit`] / [`attach_slice_audit`] run afterwards
/// as belt-and-braces: under D-2b-11 every crate attaches its own tables.
pub async fn install_upto(
    migrate: &wicket_db::Pool,
    bootstrap: &wicket_db::Pool,
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
        wicket_db::migrate::run(migrate, &[(*name, *migrator)])
            .await
            .map_err(|e| Error::Manifest(format!("migrate {name}: {e}")))?;
        if *name == "wicket-audit" {
            wicket_audit::install_privileged(bootstrap).await?;
            saw_audit = true;
        }
    }
    if saw_audit {
        attach_kernel_audit(migrate).await?;
        attach_slice_audit(migrate).await?;
    }
    Ok(())
}

/// [`install_upto`] through Wave 2s.1 (`wicket-mod-lots`). Kernel crates plus
/// items/locations/lots; the slice (inventory / production / genealogy / server)
/// is [`install_slice`].
pub async fn install_kernel(migrate: &wicket_db::Pool, bootstrap: &wicket_db::Pool) -> Result<()> {
    install_upto(migrate, bootstrap, "wicket-mod-lots").await
}

/// [`install_upto`] through [`CANONICAL_ORDER`]'s last crate (`wicket-server`).
pub async fn install_slice(migrate: &wicket_db::Pool, bootstrap: &wicket_db::Pool) -> Result<()> {
    install_upto(migrate, bootstrap, "wicket-server").await
}

/// Run the full kernel (and this crate) without the privileged event-trigger step.
pub async fn run_migrations(pool: &wicket_db::Pool) -> Result<()> {
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

impl From<ModuleNode> for wicket_statemachine::ModuleNode {
    fn from(n: ModuleNode) -> Self {
        wicket_statemachine::ModuleNode {
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
