//! Sessions (`transient.session`) and the local password [`Provider`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{query as sql_query, query_as as sql_query_as};
use uuid::Uuid;

use crate::argon2id::verify_password;
use crate::credential::{
    load_login_row, record_login_failure, record_login_success, verify_signing,
};
use crate::principal::PrincipalStatus;
use crate::{Error, Result, UserId, map_tx};

/// Opaque session issued by [`login`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Opaque session id; source of `WriteContext.session_id`.
    pub id: Uuid,
    /// Principal.
    pub principal: UserId,
    /// Created.
    pub created_at: DateTime<Utc>,
    /// Last seen.
    pub last_seen_at: DateTime<Utc>,
    /// Expiry.
    pub expires_at: DateTime<Utc>,
}

/// Authentication provider. Optional OIDC is a later module that implements this trait.
pub trait Provider: Send + Sync {
    /// Authenticate `username` / `secret` and issue a session.
    /// `device` and `ip` are stored on `transient.session` for the audit context.
    fn login(
        &self,
        tx: &mut datum_db::Tx<'_>,
        username: &str,
        secret: &str,
        device: Option<&str>,
        ip: Option<&str>,
    ) -> impl Future<Output = Result<Session>> + Send;
}

/// Local Argon2id password provider. The only implementation in this crate.
#[derive(Debug, Default, Clone, Copy)]
pub struct PasswordProvider;

impl Provider for PasswordProvider {
    async fn login(
        &self,
        tx: &mut datum_db::Tx<'_>,
        username: &str,
        secret: &str,
        device: Option<&str>,
        ip: Option<&str>,
    ) -> Result<Session> {
        login(tx, username, secret, device, ip).await
    }
}

/// Authenticate and issue a session. Typed [`Error::Lockout`] after [`crate::LOCKOUT_AFTER`] failures.
/// `device` and `ip` are recorded on the session row (caller supplies them from the request).
pub async fn login(
    tx: &mut datum_db::Tx<'_>,
    username: &str,
    secret: &str,
    device: Option<&str>,
    ip: Option<&str>,
) -> Result<Session> {
    let principal = match load_by_username_tx(tx, username).await {
        Ok(p) => p,
        Err(Error::NotFound) => {
            log_login(tx, "login.failure", username).await?;
            return Err(Error::InvalidCredentials);
        }
        Err(e) => return Err(e),
    };
    if principal.status != PrincipalStatus::Active {
        log_login(tx, "login.failure", username).await?;
        return Err(Error::Inactive);
    }
    let Some(row) = load_login_row(tx, principal.id).await? else {
        log_login(tx, "login.failure", username).await?;
        return Err(Error::InvalidCredentials);
    };
    if let Some(until) = row.locked_until
        && until > Utc::now()
    {
        log_login(tx, "login.lockout", username).await?;
        return Err(Error::Lockout {
            until,
            failures: row.failed_attempts,
        });
    }
    if !verify_password(secret.as_bytes(), &row.hash)? {
        let after = record_login_failure(tx, principal.id).await?;
        log_login(tx, "login.failure", username).await?;
        if let Some(until) = after.locked_until
            && until > Utc::now()
        {
            return Err(Error::Lockout {
                until,
                failures: after.failed_attempts,
            });
        }
        return Err(Error::InvalidCredentials);
    }
    record_login_success(tx, principal.id).await?;
    let row: (Uuid, DateTime<Utc>, DateTime<Utc>, DateTime<Utc>) = tx
        .fetch_one(
            sql_query_as(
                r#"INSERT INTO transient.session
                       (id, principal_id, created_at, last_seen_at, expires_at, device, ip)
                   VALUES (
                     gen_random_uuid(), $1, now(), now(), now() + interval '12 hours',
                     $2, CAST($3 AS inet)
                   )
                   RETURNING id, created_at, last_seen_at, expires_at"#,
            )
            .bind(principal.id.as_uuid())
            .bind(device)
            .bind(ip),
        )
        .await
        .map_err(map_tx)?;
    log_login(tx, "login.success", username).await?;
    Ok(Session {
        id: row.0,
        principal: principal.id,
        created_at: row.1,
        last_seen_at: row.2,
        expires_at: row.3,
    })
}

/// Signing re-auth: the login session is not enough (invariant 14).
pub async fn reauth_signing(
    tx: &mut datum_db::Tx<'_>,
    principal: UserId,
    secret: &str,
) -> Result<()> {
    if verify_signing(tx, principal, secret).await? {
        Ok(())
    } else {
        Err(Error::InvalidCredentials)
    }
}

async fn load_by_username_tx(
    tx: &mut datum_db::Tx<'_>,
    username: &str,
) -> Result<crate::Principal> {
    use crate::principal::{PrincipalRow, row_to_principal};

    let row: Option<PrincipalRow> = tx
        .fetch_optional(
            sql_query_as(
                r#"SELECT id, kind, username, display_name, status, created_at, deactivated_at
                   FROM identity.principal WHERE lower(username) = lower($1)"#,
            )
            .bind(username),
        )
        .await
        .map_err(map_tx)?;
    let Some(row) = row else {
        return Err(Error::NotFound);
    };
    row_to_principal(row)
}

async fn log_login(tx: &mut datum_db::Tx<'_>, kind: &str, username: &str) -> Result<()> {
    let detail = format!(r#"{{"username":{}}}"#, serde_json_string(username));
    tx.execute(
        sql_query(r#"SELECT audit.log_event($1, $2, '', '', '', '', CAST($3 AS jsonb))"#)
            .bind(kind)
            .bind("identity.login")
            .bind(detail),
    )
    .await
    .map_err(map_tx)?;
    Ok(())
}

fn serde_json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
