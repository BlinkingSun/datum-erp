//! Roles and permission keys declared by module manifests (`docs/03` §2).
//!
//! Seeded bundles per profile are D-W1-5 key 10. This crate ships the loader
//! and a test fixture; the composition root supplies profile values.

use serde::{Deserialize, Serialize};
use sqlx::{query as sql_query, query_as as sql_query_as};
use uuid::Uuid;
use wicket_core::Actor;

use crate::{Error, Result, RoleId, UserId, map_tx};

/// A named role and the permission keys it grants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleBundle {
    /// Role name (`operator`, `admin`, ...).
    pub name: String,
    /// Permission keys (`calibration.approve`, ...).
    pub permissions: Vec<String>,
}

/// Parse the subset of TOML this crate understands:
///
/// ```toml
/// [[bundle]]
/// name = "operator"
/// permissions = ["identity.session", "calibration.view"]
/// ```
pub fn load_bundles(toml: &str) -> Result<Vec<RoleBundle>> {
    let mut bundles = Vec::new();
    let mut current: Option<RoleBundle> = None;
    for raw in toml.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[bundle]]" {
            if let Some(b) = current.take() {
                bundles.push(b);
            }
            current = Some(RoleBundle {
                name: String::new(),
                permissions: Vec::new(),
            });
            continue;
        }
        let Some(bundle) = current.as_mut() else {
            return Err(Error::Bundle("key outside [[bundle]]".into()));
        };
        if let Some(rest) = line.strip_prefix("name") {
            let rest = rest.trim_start();
            let Some(rest) = rest.strip_prefix('=') else {
                return Err(Error::Bundle("name missing =".into()));
            };
            bundle.name = unquote(rest.trim())?;
        } else if let Some(rest) = line.strip_prefix("permissions") {
            let rest = rest.trim_start();
            let Some(rest) = rest.strip_prefix('=') else {
                return Err(Error::Bundle("permissions missing =".into()));
            };
            bundle.permissions = parse_string_array(rest.trim())?;
        } else {
            return Err(Error::Bundle(format!("unknown key in bundle: {line}")));
        }
    }
    if let Some(b) = current {
        bundles.push(b);
    }
    for b in &bundles {
        if b.name.is_empty() {
            return Err(Error::Bundle("bundle missing name".into()));
        }
    }
    Ok(bundles)
}

/// Insert roles and their permission keys. Idempotent on role name.
pub async fn seed_bundles(
    tx: &mut wicket_db::Tx<'_>,
    bundles: &[RoleBundle],
) -> Result<Vec<RoleId>> {
    let mut ids = Vec::new();
    for bundle in bundles {
        let id = RoleId::generate();
        let row: (Uuid,) = tx
            .fetch_one(
                sql_query_as(
                    r#"INSERT INTO identity.role (id, name)
                       VALUES ($1, $2)
                       ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
                       RETURNING id"#,
                )
                .bind(id.as_uuid())
                .bind(&bundle.name),
            )
            .await
            .map_err(map_tx)?;
        let role = RoleId(wicket_core::Identifier::from_uuid(row.0));
        for key in &bundle.permissions {
            tx.execute(
                sql_query(
                    r#"INSERT INTO identity.role_permission (role_id, permission_key)
                       VALUES ($1, $2)
                       ON CONFLICT DO NOTHING"#,
                )
                .bind(role.as_uuid())
                .bind(key),
            )
            .await
            .map_err(map_tx)?;
        }
        ids.push(role);
    }
    Ok(ids)
}

/// Grant `role` to `principal`.
pub async fn assign_role(
    tx: &mut wicket_db::Tx<'_>,
    principal: UserId,
    role: RoleId,
) -> Result<()> {
    tx.execute(
        sql_query(
            r#"INSERT INTO identity.principal_role (principal_id, role_id)
               VALUES ($1, $2)
               ON CONFLICT DO NOTHING"#,
        )
        .bind(principal.as_uuid())
        .bind(role.as_uuid()),
    )
    .await
    .map_err(map_tx)?;
    Ok(())
}

/// Whether `actor` holds `key` through any assigned role.
pub async fn has_permission(tx: &mut wicket_db::Tx<'_>, actor: Actor, key: &str) -> Result<bool> {
    let row: (bool,) = tx
        .fetch_one(
            sql_query_as(
                r#"SELECT EXISTS (
                     SELECT 1
                       FROM identity.principal_role pr
                       JOIN identity.role_permission rp ON rp.role_id = pr.role_id
                      WHERE pr.principal_id = $1
                        AND rp.permission_key = $2
                   )"#,
            )
            .bind(actor.id.as_uuid())
            .bind(key),
        )
        .await
        .map_err(map_tx)?;
    Ok(row.0)
}

fn unquote(s: &str) -> Result<String> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Ok(inner.replace("\\\"", "\""));
    }
    Ok(s.to_string())
}

fn parse_string_array(s: &str) -> Result<Vec<String>> {
    let s = s.trim();
    let Some(inner) = s.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        return Err(Error::Bundle("permissions is not an array".into()));
    };
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for part in inner.split(',') {
        out.push(unquote(part)?);
    }
    Ok(out)
}
