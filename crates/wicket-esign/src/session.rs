//! Continuous-session relaxation (D-2b-3). Shipped `off` in both profiles.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{query as sql_query, query_as as sql_query_as};
use uuid::Uuid;
use wicket_core::Identifier;
use wicket_identity::UserId;

use crate::error::map_tx;
use crate::{Error, Result};

/// Sub-keys of SPEC-profiles key 4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPolicy {
    /// `"off"` or `"on"`. v1 ships `"off"` in both profiles.
    pub continuous_session: String,
    /// Idle timeout in seconds (default 300).
    pub idle_timeout_secs: i64,
    /// Max window in seconds from `opened_at` (default 900). Also the token TTL.
    pub max_window_secs: i64,
}

impl Default for SessionPolicy {
    fn default() -> Self {
        Self {
            continuous_session: "off".into(),
            idle_timeout_secs: 300,
            max_window_secs: 900,
        }
    }
}

impl SessionPolicy {
    /// Relaxation enabled.
    pub fn is_on(&self) -> bool {
        self.continuous_session == "on"
    }
}

/// A `transient.signing_session` row.
#[derive(Debug, Clone)]
pub struct SigningSession {
    /// Session id.
    pub id: Uuid,
    /// Principal.
    pub principal_id: Uuid,
    /// Login session this signing session is bound to.
    pub login_session_id: Option<Uuid>,
    /// Device fingerprint.
    pub device_fingerprint: Option<String>,
    /// Boot epoch.
    pub boot_epoch: String,
    /// Source IP.
    pub source_ip: Option<String>,
    /// Opened at.
    pub opened_at: DateTime<Utc>,
    /// Last successful signing.
    pub last_signed_at: DateTime<Utc>,
    /// Closed at, if closed.
    pub closed_at: Option<DateTime<Utc>>,
    /// Close reason.
    pub close_reason: Option<String>,
}

type SessionRow = (
    Uuid,
    Uuid,
    Option<Uuid>,
    Option<String>,
    String,
    Option<String>,
    DateTime<Utc>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<String>,
);

fn row_to_session(row: SessionRow) -> SigningSession {
    SigningSession {
        id: row.0,
        principal_id: row.1,
        login_session_id: row.2,
        device_fingerprint: row.3,
        boot_epoch: row.4,
        source_ip: row.5,
        opened_at: row.6,
        last_signed_at: row.7,
        closed_at: row.8,
        close_reason: row.9,
    }
}

/// Close every open signing session for the actor bound on `tx` with `reason`.
///
/// The principal is [`wicket_db::Tx::setting`] `"wicket.actor_id"` (the
/// [`wicket_db::WriteContext`] actor), never a parameter (D-2b-8).
pub async fn close_session(tx: &mut wicket_db::Tx<'_>, reason: &str) -> Result<u64> {
    let raw = tx.setting("wicket.actor_id").await.map_err(map_tx)?;
    let uuid = Uuid::parse_str(&raw).map_err(|e| Error::Invariant(e.to_string()))?;
    close_sessions_for(
        tx,
        UserId::from_identifier(Identifier::from_uuid(uuid)),
        reason,
    )
    .await
}

/// Close every open signing session for `principal` with `reason`.
pub(crate) async fn close_sessions_for(
    tx: &mut wicket_db::Tx<'_>,
    principal: UserId,
    reason: &str,
) -> Result<u64> {
    let n: (i64,) = tx
        .fetch_one(
            sql_query_as("SELECT esign.close_signing_sessions($1, $2)")
                .bind(principal.as_uuid())
                .bind(reason),
        )
        .await
        .map_err(map_tx)?;
    Ok(n.0 as u64)
}

pub(crate) async fn load_open(
    tx: &mut wicket_db::Tx<'_>,
    principal: UserId,
) -> Result<Option<SigningSession>> {
    let row: Option<SessionRow> = tx
        .fetch_optional(
            sql_query_as(
                r#"SELECT id, principal_id, login_session_id, device_fingerprint, boot_epoch,
                          source_ip, opened_at, last_signed_at, closed_at, close_reason
                     FROM esign.load_open_signing_session($1)"#,
            )
            .bind(principal.as_uuid()),
        )
        .await
        .map_err(map_tx)?;
    Ok(row.map(row_to_session))
}

pub(crate) fn is_continuous(
    session: &SigningSession,
    now: DateTime<Utc>,
    policy: &SessionPolicy,
) -> bool {
    if session.closed_at.is_some() {
        return false;
    }
    let idle = chrono::Duration::seconds(policy.idle_timeout_secs.max(0));
    let window = chrono::Duration::seconds(policy.max_window_secs.max(0));
    now < session.last_signed_at + idle && now < session.opened_at + window
}

pub(crate) async fn open_session(
    tx: &mut wicket_db::Tx<'_>,
    principal: UserId,
    login_session_id: Option<Uuid>,
    device_fingerprint: Option<&str>,
    boot_epoch: &str,
    source_ip: Option<&str>,
) -> Result<SigningSession> {
    let id = Uuid::now_v7();
    let row: SessionRow = tx
        .fetch_one(
            sql_query_as(
                r#"SELECT id, principal_id, login_session_id, device_fingerprint, boot_epoch,
                          source_ip, opened_at, last_signed_at, closed_at, close_reason
                     FROM esign.open_signing_session($1, $2, $3, $4, $5, $6)"#,
            )
            .bind(id)
            .bind(principal.as_uuid())
            .bind(login_session_id)
            .bind(device_fingerprint)
            .bind(boot_epoch)
            .bind(source_ip),
        )
        .await
        .map_err(map_tx)?;
    Ok(row_to_session(row))
}

pub(crate) async fn touch_session(tx: &mut wicket_db::Tx<'_>, id: Uuid) -> Result<()> {
    tx.execute(sql_query("SELECT esign.touch_signing_session($1)").bind(id))
        .await
        .map_err(map_tx)?;
    Ok(())
}

pub(crate) fn device_changed(
    session: &SigningSession,
    device: Option<&str>,
    ip: Option<&str>,
    boot: &str,
) -> bool {
    if session.boot_epoch != boot {
        return true;
    }
    if session.device_fingerprint.as_deref() != device {
        return true;
    }
    if session.source_ip.as_deref() != ip {
        return true;
    }
    false
}

/// Wire shape for `POST /esign/challenges`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    /// Components the client must present.
    pub components_required: Vec<String>,
    /// When the current signing session expires, if any.
    pub signing_session_expires_at: Option<DateTime<Utc>>,
    /// Credential kind (`signing_password` / `idp_step_up`).
    pub credential_kind: String,
}

/// Issue a challenge: two components unless a live continuous session exists and relaxation is on.
pub async fn challenge(
    tx: &mut wicket_db::Tx<'_>,
    principal: UserId,
    policy: &SessionPolicy,
) -> Result<Challenge> {
    let open = load_open(tx, principal).await?;
    let now = Utc::now();
    let (components, expires) = match open {
        Some(s) if policy.is_on() && is_continuous(&s, now, policy) => {
            let idle = s.last_signed_at + chrono::Duration::seconds(policy.idle_timeout_secs);
            let window = s.opened_at + chrono::Duration::seconds(policy.max_window_secs);
            let exp = idle.min(window);
            (vec!["secret".into()], Some(exp))
        }
        _ => (vec!["code".into(), "secret".into()], None),
    };
    Ok(Challenge {
        components_required: components,
        signing_session_expires_at: expires,
        credential_kind: "signing_password".into(),
    })
}
