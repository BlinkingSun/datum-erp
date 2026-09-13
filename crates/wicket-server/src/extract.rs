//! Request helpers: request id, session, CSRF, idempotency, write context.

use axum::extract::Request;
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

use crate::boot::AppState;
use crate::envelope::{RequestId, error_response};
use crate::error::{Error, Result};
use crate::session::{self, HttpSession};

/// Generate / echo `X-Request-Id` (UUID v7).
pub async fn request_id_mw(mut req: Request, next: Next) -> Response {
    let id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::now_v7);
    let rid = RequestId(id.to_string());
    let _ = rid.as_str();
    req.extensions_mut().insert(rid);
    if let Ok(v) = axum::http::HeaderValue::from_str(&id.to_string()) {
        req.headers_mut().insert("x-request-id", v);
    }
    let mut resp = next.run(req).await;
    if let Ok(v) = axum::http::HeaderValue::from_str(&id.to_string()) {
        resp.headers_mut().insert("x-request-id", v);
    }
    resp
}

/// 1 MiB JSON body; 30 s handler timeout (docs/10 §8).
pub async fn limits_mw(req: Request, next: Next) -> Response {
    match tokio::time::timeout(std::time::Duration::from_secs(30), next.run(req)).await {
        Ok(r) => r,
        Err(_) => error_response(
            Error::http(
                "TIMEOUT",
                "handler exceeded 30 s",
                None,
                axum::http::StatusCode::GATEWAY_TIMEOUT,
            ),
            "timeout",
        ),
    }
}

/// Session loaded for this request, if any.
#[derive(Clone)]
pub struct Auth {
    /// Session.
    pub session: Option<HttpSession>,
}

impl Auth {
    /// Require a session. Does not open a write transaction.
    pub fn require(&self) -> Result<&HttpSession> {
        self.session
            .as_ref()
            .ok_or_else(|| Error::unauthenticated("authentication required"))
    }
}

/// Load session (optional). SELECT on the app pool; never opens a write `Tx`.
pub async fn load_auth(state: &AppState, headers: &HeaderMap, _request_id: &str) -> Result<Auth> {
    let Some(id) = session::session_id_from_headers(headers) else {
        return Ok(Auth { session: None });
    };
    let session = session::load_from_pool(state.pool(), id).await?;
    Ok(Auth {
        session: Some(session),
    })
}

/// Authenticated session that holds `permission`. Does not open a write `Tx`.
pub async fn require_permission(
    state: &AppState,
    headers: &HeaderMap,
    request_id: &str,
    permission: &str,
) -> Result<HttpSession> {
    let auth = load_auth(state, headers, request_id).await?;
    let session = auth.require()?.clone();
    if !permission.is_empty() && !session.allows(permission) {
        return Err(Error::forbidden(format!("missing permission {permission}")));
    }
    Ok(session)
}

/// Mutating request: session + CSRF + permission. No write `Tx`.
pub async fn require_mutation(
    state: &AppState,
    headers: &HeaderMap,
    request_id: &str,
    permission: &str,
) -> Result<HttpSession> {
    let session = require_permission(state, headers, request_id, permission).await?;
    session::check_csrf(headers, &session)?;
    Ok(session)
}

/// `If-Match: "<version>"` required on state-transition POSTs (docs/10 §4.3).
pub fn require_if_match(headers: &HeaderMap) -> Result<i64> {
    let raw = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            Error::validation(
                "If-Match is required on state-transition POST",
                Some("If-Match"),
            )
        })?;
    let trimmed = raw.trim().trim_matches('"');
    if trimmed == "*" {
        return Err(Error::validation(
            "If-Match: * is not accepted",
            Some("If-Match"),
        ));
    }
    trimmed.parse::<i64>().map_err(|_| {
        Error::validation(
            "If-Match must be a quoted integer version",
            Some("If-Match"),
        )
    })
}

/// Stale `If-Match` is 409 CONFLICT field=version.
pub fn check_version(actual: i64, expected: i64) -> Result<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(Error::conflict("stale version", Some("version")))
    }
}
