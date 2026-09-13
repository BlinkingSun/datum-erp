//! HTTP sessions: cookie `datum_session` / bearer token, CSRF double-submit.

use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use datum_core::{Actor, ActorKind, Identifier};
use datum_db::{Pool, Tx, WriteContext};
use datum_identity::UserId;
use uuid::Uuid;

use crate::error::{Error, Result};

/// Permission keys the HTTP surface consults. Snapshotted at login so GET
/// handlers can authorize without opening a write transaction.
pub const KNOWN_PERMISSIONS: &[&str] = &[
    "identity.session",
    "items.view",
    "items.edit",
    "items.release",
    "locations.view",
    "locations.edit",
    "lots.view",
    "lots.edit",
    "lots.release",
    "inventory.view",
    "inventory.receive",
    "inventory.issue",
    "inventory.move",
    "inventory.count",
    "inventory.adjust",
    "production.view",
    "production.create",
    "production.release",
    "production.issue",
    "production.complete",
    "genealogy.view",
    "validation.manifest.read",
    "audit.export",
    "calibration.approve",
    "wo.release",
    "esign.bundle.read",
];

/// Authenticated HTTP session copied from `datum-identity::login` into
/// `server_transient.http_session`.
#[derive(Debug, Clone)]
pub struct HttpSession {
    /// Session id (cookie / bearer).
    pub id: Uuid,
    /// Principal.
    pub principal: UserId,
    /// Display name snapshot.
    pub display_name: String,
    /// CSRF token (cookie `datum_csrf`).
    pub csrf: String,
    /// Expiry.
    #[allow(dead_code)]
    pub expires_at: DateTime<Utc>,
    /// Permission keys granted at login.
    pub permissions: Vec<String>,
}

impl HttpSession {
    /// Whether this session's snapshot includes `key`.
    pub fn allows(&self, key: &str) -> bool {
        self.permissions.iter().any(|p| p == key)
    }
}

/// Persist a session after `datum_identity::login`.
pub async fn store(
    tx: &mut Tx<'_>,
    session_id: Uuid,
    principal: UserId,
    display_name: &str,
    csrf: &str,
    expires_at: DateTime<Utc>,
    permissions: &[String],
) -> Result<()> {
    tx.execute(
        sqlx::query(
            r#"INSERT INTO server_transient.http_session
                   (id, principal_id, display_name, csrf, expires_at, permissions)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(session_id)
        .bind(principal.as_uuid())
        .bind(display_name)
        .bind(csrf)
        .bind(expires_at)
        .bind(permissions),
    )
    .await?;
    Ok(())
}

type SessionRow = (Uuid, Uuid, String, String, DateTime<Utc>, Vec<String>);

/// Load a live session from the app pool (SELECT only; no write transaction).
pub async fn load_from_pool(pool: &Pool, id: Uuid) -> Result<HttpSession> {
    let row: Option<SessionRow> = sqlx::query_as(
        r#"SELECT id, principal_id, display_name, csrf, expires_at, permissions
             FROM server_transient.http_session
            WHERE id = $1 AND expires_at > now()"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row_to_session(row)
}

fn row_to_session(row: Option<SessionRow>) -> Result<HttpSession> {
    let Some((id, principal, display_name, csrf, expires_at, permissions)) = row else {
        return Err(Error::unauthenticated("session missing or expired"));
    };
    Ok(HttpSession {
        id,
        principal: UserId::from_identifier(Identifier::from_uuid(principal)),
        display_name,
        csrf,
        expires_at,
        permissions,
    })
}

/// Snapshot of keys the principal holds (called inside the login transaction).
pub async fn granted_permissions(tx: &mut Tx<'_>, actor: Actor) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for key in KNOWN_PERMISSIONS {
        if datum_identity::rbac::has_permission(tx, actor, key).await? {
            out.push((*key).to_string());
        }
    }
    Ok(out)
}

/// Drop a session (logout).
pub async fn drop_session(tx: &mut Tx<'_>, id: Uuid) -> Result<()> {
    tx.execute(sqlx::query("DELETE FROM server_transient.http_session WHERE id = $1").bind(id))
        .await?;
    Ok(())
}

/// Parse `Cookie` / `Authorization` for a session id.
pub fn session_id_from_headers(headers: &HeaderMap) -> Option<Uuid> {
    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok())
        && let Some(token) = auth.strip_prefix("Bearer ")
        && let Ok(id) = Uuid::parse_str(token.trim())
    {
        return Some(id);
    }
    cookie(headers, "datum_session").and_then(|v| Uuid::parse_str(&v).ok())
}

/// Cookie value by name.
pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get("cookie")?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=')
            && k.trim() == name
        {
            return Some(v.trim().to_string());
        }
    }
    None
}

/// True when this request used a cookie (CSRF required on mutations).
pub fn used_cookie(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_none_or(|a| !a.starts_with("Bearer "))
        && cookie(headers, "datum_session").is_some()
}

/// CSRF double-submit (docs/10 §7).
pub fn check_csrf(headers: &HeaderMap, session: &HttpSession) -> Result<()> {
    if !used_cookie(headers) {
        return Ok(());
    }
    let header = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Error::forbidden("X-CSRF-Token is required for cookie authentication"))?;
    if header != session.csrf {
        return Err(Error::forbidden("CSRF token mismatch"));
    }
    Ok(())
}

/// `WriteContext` for an authenticated mutating request (source kind `api`).
pub fn write_context(
    session: &HttpSession,
    action: &str,
    request_id: &str,
    headers: &HeaderMap,
    config_version: &str,
) -> WriteContext {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());
    let app = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(80).collect::<String>());
    let mut ctx = WriteContext::new(
        Actor {
            id: session.principal.0,
            kind: ActorKind::User,
        },
        action,
        "api",
    );
    ctx.actor_display = Some(session.display_name.clone());
    ctx.session_id = Some(session.id.to_string());
    ctx.request_id = Some(request_id.to_string());
    ctx.source_ip = ip;
    ctx.client_app = app.or_else(|| Some("datum-http".into()));
    ctx.config_version = Some(config_version.to_string());
    ctx.reason = Some("api".into());
    ctx
}

/// System write context for boot / IQ (not a user request).
pub fn system_ctx(action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(datum_identity::SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        action,
        "maintenance",
    );
    ctx.actor_display = Some("system".into());
    ctx.reason = Some("server.boot".into());
    ctx
}
