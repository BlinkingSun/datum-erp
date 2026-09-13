//! Install / enable / disable / upgrade (`docs/03` §6).

use std::collections::{BTreeMap, BTreeSet};

use wicket_db::Tx;
use wicket_identity::rbac::{RoleBundle, seed_bundles};

use crate::manifest::ModuleManifest;
use crate::profile::{Profile, ProfileId};
use crate::{Error, Result};

/// Record a module inside `tx`. Module SQL runs in this same transaction
/// (`docs/03` §6: install runs the module's migrations inside a transaction).
///
/// Also writes `module.install_log` so the install transaction has two rows
/// that commit or roll back together. A failing migration leaves nothing
/// persisted once the caller rolls the `Tx` back.
pub async fn install(tx: &mut Tx<'_>, manifest: &ModuleManifest, enabled: bool) -> Result<()> {
    apply_module_migrations(tx, manifest).await?;
    let hash = manifest.manifest_hash(enabled)?;
    tx.execute(
        sqlx::query(
            r#"INSERT INTO module.installed
                   (id, version, regulated, installed_at, enabled, enabled_changed_at, manifest_hash)
               VALUES ($1, $2, $3, now(), $4, now(), $5)
               ON CONFLICT (id) DO UPDATE
                 SET version = EXCLUDED.version,
                     regulated = EXCLUDED.regulated,
                     enabled = EXCLUDED.enabled,
                     enabled_changed_at = now(),
                     manifest_hash = EXCLUDED.manifest_hash"#,
        )
        .bind(&manifest.id)
        .bind(&manifest.version)
        .bind(manifest.regulated)
        .bind(enabled)
        .bind(&hash),
    )
    .await?;
    tx.execute(
        sqlx::query(
            r#"INSERT INTO module.install_log (module_id, applied_at)
               VALUES ($1, now())
               ON CONFLICT (module_id) DO UPDATE SET applied_at = now()"#,
        )
        .bind(&manifest.id),
    )
    .await?;
    if !manifest.permissions.is_empty() {
        let bundle = RoleBundle {
            name: format!("module:{}", manifest.id),
            permissions: manifest.permissions.keys().cloned().collect(),
        };
        seed_bundles(tx, &[bundle]).await?;
    }
    if !manifest.custom_fields.fields.is_empty() {
        wicket_customfields::register_from_manifest(tx, &manifest.custom_fields).await?;
    }
    Ok(())
}

/// Enable `id` and every dependency (closure rule).
///
/// Refuses with [`Error::EnableRefused`] when the profile disallows the module
/// (a `regulated = true` module on `plain-shop`, or a module not listed on
/// the profile).
pub async fn enable(
    tx: &mut Tx<'_>,
    id: &str,
    catalog: &[ModuleManifest],
    profile: &Profile,
) -> Result<Vec<String>> {
    let by_id: BTreeMap<&str, &ModuleManifest> =
        catalog.iter().map(|m| (m.id.as_str(), m)).collect();
    let mut needed = BTreeSet::new();
    collect_deps(id, &by_id, &mut needed)?;
    for mid in &needed {
        profile_permits_enable(profile, mid, catalog)?;
    }
    let mut enabled = Vec::new();
    for mid in needed {
        set_enabled(tx, &mid, true, catalog).await?;
        enabled.push(mid);
    }
    Ok(enabled)
}

/// Disable `id` unless another enabled module depends on it.
pub async fn disable(tx: &mut Tx<'_>, id: &str, catalog: &[ModuleManifest]) -> Result<()> {
    let dependents = enabled_dependents(tx, id, catalog).await?;
    if !dependents.is_empty() {
        return Err(Error::DisableRefused {
            id: id.to_string(),
            dependents: dependents.join(", "),
        });
    }
    set_enabled(tx, id, false, catalog).await?;
    Ok(())
}

/// Upgrade records a new version and re-hashes. Forward SQL is `wicket_migrate`.
pub async fn upgrade(tx: &mut Tx<'_>, manifest: &ModuleManifest) -> Result<()> {
    let enabled = is_enabled(tx, &manifest.id).await?.unwrap_or(false);
    install(tx, manifest, enabled).await
}

/// Uninstall is not offered when any regulated module is installed, or when
/// the named module itself is regulated (`docs/03` §6).
pub async fn uninstall(tx: &mut Tx<'_>, id: &str) -> Result<()> {
    let regulated: Option<(bool,)> = tx
        .fetch_optional(
            sqlx::query_as("SELECT regulated FROM module.installed WHERE id = $1").bind(id),
        )
        .await?;
    let Some((is_reg,)) = regulated else {
        return Err(Error::UnknownModule(id.to_string()));
    };
    let any_reg: (bool,) = tx
        .fetch_one(sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM module.installed WHERE regulated)",
        ))
        .await?;
    if is_reg || any_reg.0 {
        return Err(Error::UninstallForbidden);
    }
    Err(Error::UninstallForbidden)
}

/// Rows currently in `module.installed`.
pub async fn list_installed(tx: &mut Tx<'_>) -> Result<Vec<InstalledRow>> {
    let rows: Vec<(String, String, bool, bool, String)> = tx
        .fetch_all(sqlx::query_as(
            r#"SELECT id, version, regulated, enabled, manifest_hash
               FROM module.installed
               ORDER BY id"#,
        ))
        .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, version, regulated, enabled, manifest_hash)| InstalledRow {
                id,
                version,
                regulated,
                enabled,
                manifest_hash,
            },
        )
        .collect())
}

/// Installed-module row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledRow {
    /// Module id.
    pub id: String,
    /// Version.
    pub version: String,
    /// Regulated flag.
    pub regulated: bool,
    /// Enabled flag.
    pub enabled: bool,
    /// Manifest hash (changes with the enabled set).
    pub manifest_hash: String,
}

async fn set_enabled(
    tx: &mut Tx<'_>,
    id: &str,
    enabled: bool,
    catalog: &[ModuleManifest],
) -> Result<()> {
    let Some(manifest) = catalog.iter().find(|m| m.id == id) else {
        return Err(Error::UnknownModule(id.to_string()));
    };
    let hash = manifest.manifest_hash(enabled)?;
    let n = tx
        .execute(
            sqlx::query(
                r#"UPDATE module.installed
                   SET enabled = $2,
                       enabled_changed_at = now(),
                       manifest_hash = $3
                 WHERE id = $1"#,
            )
            .bind(id)
            .bind(enabled)
            .bind(&hash),
        )
        .await?;
    if n.rows_affected() == 0 {
        return Err(Error::UnknownModule(id.to_string()));
    }
    Ok(())
}

async fn is_enabled(tx: &mut Tx<'_>, id: &str) -> Result<Option<bool>> {
    let row: Option<(bool,)> = tx
        .fetch_optional(
            sqlx::query_as("SELECT enabled FROM module.installed WHERE id = $1").bind(id),
        )
        .await?;
    Ok(row.map(|r| r.0))
}

async fn enabled_dependents(
    tx: &mut Tx<'_>,
    id: &str,
    catalog: &[ModuleManifest],
) -> Result<Vec<String>> {
    let enabled: Vec<(String,)> = tx
        .fetch_all(
            sqlx::query_as("SELECT id FROM module.installed WHERE enabled AND id <> $1").bind(id),
        )
        .await?;
    let enabled: BTreeSet<String> = enabled.into_iter().map(|r| r.0).collect();
    let mut names = Vec::new();
    for m in catalog {
        if enabled.contains(&m.id) && m.dependencies.keys().any(|d| d == id) {
            names.push(m.id.clone());
        }
    }
    names.sort();
    Ok(names)
}

fn collect_deps(
    id: &str,
    by_id: &BTreeMap<&str, &ModuleManifest>,
    out: &mut BTreeSet<String>,
) -> Result<()> {
    if !out.insert(id.to_string()) {
        return Ok(());
    }
    let Some(m) = by_id.get(id) else {
        return Err(Error::UnknownModule(id.to_string()));
    };
    for dep in m.dependencies.keys() {
        if dep == "kernel" {
            continue;
        }
        collect_deps(dep, by_id, out)?;
    }
    Ok(())
}

/// Record the effective profile as an audited configuration row (key 1).
pub async fn record_profile(
    tx: &mut Tx<'_>,
    profile_id: &str,
    spec_version: &str,
    body: &serde_json::Value,
    content_hash: &str,
) -> Result<bool> {
    let existing: Option<(String,)> = tx
        .fetch_optional(sqlx::query_as(
            "SELECT content_hash FROM module.configuration WHERE id = 'effective-profile'",
        ))
        .await?;
    match existing {
        Some((hash,)) if hash == content_hash => Ok(false),
        Some(_) | None => {
            tx.execute(
                sqlx::query(
                    r#"INSERT INTO module.configuration
                           (id, profile_id, spec_version, body, content_hash, recorded_at)
                       VALUES ('effective-profile', $1, $2, $3, $4, now())
                       ON CONFLICT (id) DO UPDATE
                         SET profile_id = EXCLUDED.profile_id,
                             spec_version = EXCLUDED.spec_version,
                             body = EXCLUDED.body,
                             content_hash = EXCLUDED.content_hash,
                             recorded_at = now()"#,
                )
                .bind(profile_id)
                .bind(spec_version)
                .bind(body)
                .bind(content_hash),
            )
            .await?;
            Ok(true)
        }
    }
}

/// Apply each of `manifest.migrations` on `tx` (single statements; DML only).
async fn apply_module_migrations(tx: &mut Tx<'_>, manifest: &ModuleManifest) -> Result<()> {
    for sql in &manifest.migrations {
        if sql_is_noop(sql) {
            continue;
        }
        tx.execute(sqlx::query(*sql)).await?;
    }
    Ok(())
}

fn sql_is_noop(sql: &str) -> bool {
    sql.lines().all(|line| {
        let t = line.trim();
        t.is_empty() || t.starts_with("--")
    })
}

/// Profile enablement lever: `plain-shop` cannot turn on a regulated module,
/// and a module absent from the profile's compiled-in list is refused.
pub fn profile_permits_enable(
    profile: &Profile,
    id: &str,
    catalog: &[ModuleManifest],
) -> Result<()> {
    let Some(manifest) = catalog.iter().find(|m| m.id == id) else {
        return Err(Error::UnknownModule(id.to_string()));
    };
    if !profile.modules.iter().any(|m| m.id == id) {
        return Err(Error::EnableRefused {
            id: id.to_string(),
            reason: format!("{id} is not in profile {}", profile.id.as_str()),
        });
    }
    if manifest.regulated && profile.id == ProfileId::PlainShop {
        return Err(Error::EnableRefused {
            id: id.to_string(),
            reason: "plain-shop cannot enable a regulated = true module".into(),
        });
    }
    Ok(())
}
