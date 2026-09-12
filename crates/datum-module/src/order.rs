//! Kernel crate order (CONTRACT §4) and module-graph topological sort.

use std::collections::{BTreeSet, HashMap};

use sqlx::migrate::Migrator;

use crate::{Error, MIGRATOR, Result};

/// Kernel crates' migrators in dependency order (SPEC deliverable 3).
///
/// After this list, first-party modules run in topological order. This crate's
/// own migrator (`datum-module`) is appended by [`kernel_migrators`].
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
];

/// CONTRACT §4 kernel edges used to prove [`KERNEL_ORDER`] is a topological sort.
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
    ]
}

/// [`kernel_crates`] plus this crate's registry schema.
pub fn kernel_migrators() -> Vec<(&'static str, &'static Migrator)> {
    let mut crates = kernel_crates();
    crates.push(("datum-module", &MIGRATOR));
    crates
}

/// Names of the crates [`migrate_prefix`] applies (db + audit).
pub const MIGRATE_PREFIX: &[&str] = &["datum-db", "datum-audit"];

/// Apply `datum-db` then `datum-audit`.
///
/// `datum.schema_history` is attached after the remaining crates in
/// [`migrate_suffix`]: attaching earlier would 42501 the runner's own
/// history inserts (no actor GUC on that path).
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

/// Apply identity through this crate. Call [`datum_audit::install_privileged`]
/// **after** this (so uom seed inserts are not judged by `zz_audit_row`), then
/// [`attach_kernel_audit`].
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

/// App-class tables created before [`datum_audit::install_privileged`].
///
/// `numbering.counter` is `audit.exempt`; `datum.schema_class` stays
/// unattached so later migrators can INSERT class rows without actor GUCs.
const KERNEL_AUDIT_RELS: &[&str] = &[
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
    "uom.unit",
    "uom.item_stock",
    "uom.posting_stub",
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
];

/// Attach `datum.schema_history` and every other app-class table that missed
/// the event trigger (tables created before `install_privileged`).
pub async fn attach_kernel_audit(pool: &datum_db::Pool) -> Result<()> {
    for rel in KERNEL_AUDIT_RELS {
        datum_audit::attach(pool, rel).await?;
    }
    Ok(())
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
