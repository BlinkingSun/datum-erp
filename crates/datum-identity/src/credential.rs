//! Login and signing credentials (invariant 14) and two-person reset (D3 §9).

use chrono::{DateTime, Utc};
use datum_core::Actor;
use serde::{Deserialize, Serialize};
use sqlx::{query as sql_query, query_as as sql_query_as};
use uuid::Uuid;

use crate::argon2id::{self, hash_password, verify_password};
use crate::blake2b;
use crate::{Error, Result, UserId, map_tx};

/// Which credential a reset targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CredentialKind {
    /// Login secret.
    Login,
    /// Signing secret (re-prompted at signature time).
    Signing,
}

impl CredentialKind {
    fn as_db(self) -> &'static str {
        match self {
            CredentialKind::Login => "login",
            CredentialKind::Signing => "signing",
        }
    }
}

/// Store (or rotate) the login Argon2id hash.
pub async fn set_login_credential(
    tx: &mut datum_db::Tx<'_>,
    principal: UserId,
    secret: &str,
) -> Result<()> {
    let phc = hash_password(secret.as_bytes())?;
    upsert_login_hash(tx, principal, &phc).await
}

/// Store (or rotate) the signing Argon2id hash.
pub async fn set_signing_credential(
    tx: &mut datum_db::Tx<'_>,
    principal: UserId,
    secret: &str,
) -> Result<()> {
    let phc = hash_password(secret.as_bytes())?;
    let n = tx
        .execute(
            sql_query(
                r#"INSERT INTO identity.signing_credential
                       (principal_id, hash, established_at)
                   VALUES ($1, $2, now())
                   ON CONFLICT (principal_id) DO UPDATE
                     SET hash = EXCLUDED.hash, established_at = now()"#,
            )
            .bind(principal.as_uuid())
            .bind(&phc),
        )
        .await
        .map_err(map_tx)?;
    let _ = n;
    Ok(())
}

/// Verify a signing secret. Never accepts the login secret.
pub async fn verify_signing(
    tx: &mut datum_db::Tx<'_>,
    principal: UserId,
    secret: &str,
) -> Result<bool> {
    let row: Option<(String,)> = tx
        .fetch_optional(
            sql_query_as("SELECT hash FROM identity.signing_credential WHERE principal_id = $1")
                .bind(principal.as_uuid()),
        )
        .await
        .map_err(map_tx)?;
    let Some((hash,)) = row else {
        return Ok(false);
    };
    Ok(verify_password(secret.as_bytes(), &hash)?)
}

/// Verify a login secret without touching lockout counters.
pub async fn verify_login_secret(
    tx: &mut datum_db::Tx<'_>,
    principal: UserId,
    secret: &str,
) -> Result<bool> {
    let row: Option<(String,)> = tx
        .fetch_optional(
            sql_query_as("SELECT hash FROM identity.login_credential WHERE principal_id = $1")
                .bind(principal.as_uuid()),
        )
        .await
        .map_err(map_tx)?;
    let Some((hash,)) = row else {
        return Ok(false);
    };
    Ok(verify_password(secret.as_bytes(), &hash)?)
}

pub(crate) async fn load_login_row(
    tx: &mut datum_db::Tx<'_>,
    principal: UserId,
) -> Result<Option<LoginRow>> {
    let row: Option<(String, i32, Option<DateTime<Utc>>)> = tx
        .fetch_optional(
            sql_query_as(
                r#"SELECT hash, failed_attempts, locked_until
                   FROM identity.login_credential WHERE principal_id = $1"#,
            )
            .bind(principal.as_uuid()),
        )
        .await
        .map_err(map_tx)?;
    Ok(row.map(|(hash, failed_attempts, locked_until)| LoginRow {
        hash,
        failed_attempts,
        locked_until,
    }))
}

pub(crate) struct LoginRow {
    pub hash: String,
    pub failed_attempts: i32,
    pub locked_until: Option<DateTime<Utc>>,
}

pub(crate) async fn record_login_failure(
    tx: &mut datum_db::Tx<'_>,
    principal: UserId,
) -> Result<LoginRow> {
    let row: (String, i32, Option<DateTime<Utc>>) = tx
        .fetch_one(
            sql_query_as(
                r#"UPDATE identity.login_credential
                      SET failed_attempts = failed_attempts + 1,
                          last_failed_at = now(),
                          locked_until = CASE
                            WHEN failed_attempts + 1 >= $2
                              THEN now() + make_interval(secs => $3)
                            ELSE locked_until
                          END
                    WHERE principal_id = $1
                RETURNING hash, failed_attempts, locked_until"#,
            )
            .bind(principal.as_uuid())
            .bind(crate::LOCKOUT_AFTER)
            .bind(crate::LOCKOUT_SECS as f64),
        )
        .await
        .map_err(map_tx)?;
    Ok(LoginRow {
        hash: row.0,
        failed_attempts: row.1,
        locked_until: row.2,
    })
}

pub(crate) async fn record_login_success(
    tx: &mut datum_db::Tx<'_>,
    principal: UserId,
) -> Result<()> {
    tx.execute(
        sql_query(
            r#"UPDATE identity.login_credential
                  SET failed_attempts = 0, locked_until = NULL, last_failed_at = NULL
                WHERE principal_id = $1"#,
        )
        .bind(principal.as_uuid()),
    )
    .await
    .map_err(map_tx)?;
    Ok(())
}

/// Request a reset. Returns the one-time token (shown once).
pub async fn request_reset(
    tx: &mut datum_db::Tx<'_>,
    actor: Actor,
    subject: UserId,
    kind: CredentialKind,
) -> Result<String> {
    if actor.id.as_uuid() == subject.as_uuid() {
        return Err(Error::ResetRequiresTwoPrincipals);
    }
    let token = Uuid::now_v7().to_string();
    let token_hash = token_digest(&token);
    tx.execute(
        sql_query(
            r#"INSERT INTO identity.credential_reset
                   (id, principal_id, requested_by, kind, token_hash, created_at, expires_at)
               VALUES ($1, $2, $3, $4, $5, now(), now() + interval '1 hour')"#,
        )
        .bind(Uuid::now_v7())
        .bind(subject.as_uuid())
        .bind(actor.id.as_uuid())
        .bind(kind.as_db())
        .bind(&token_hash),
    )
    .await
    .map_err(map_tx)?;
    Ok(token)
}

/// Complete a reset. The completing actor must be the subject and must not be the requester.
pub async fn complete_reset(
    tx: &mut datum_db::Tx<'_>,
    actor: Actor,
    token: &str,
    new_secret: &str,
) -> Result<()> {
    let token_hash = token_digest(token);
    let row: Option<(Uuid, Uuid, Uuid, String)> = tx
        .fetch_optional(
            sql_query_as(
                r#"SELECT id, principal_id, requested_by, kind
                     FROM identity.credential_reset
                    WHERE token_hash = $1
                      AND completed_at IS NULL
                      AND expires_at > now()"#,
            )
            .bind(&token_hash),
        )
        .await
        .map_err(map_tx)?;
    let Some((reset_id, subject, requested_by, kind)) = row else {
        return Err(Error::ResetInvalid);
    };
    if actor.id.as_uuid() == requested_by {
        return Err(Error::ResetRequiresTwoPrincipals);
    }
    if actor.id.as_uuid() != subject {
        return Err(Error::ResetRequiresTwoPrincipals);
    }
    let user = UserId::from_identifier(datum_core::Identifier::from_uuid(subject));
    match kind.as_str() {
        "login" => set_login_credential(tx, user, new_secret).await?,
        "signing" => set_signing_credential(tx, user, new_secret).await?,
        _ => return Err(Error::ResetInvalid),
    }
    tx.execute(
        sql_query(
            r#"UPDATE identity.credential_reset
                  SET completed_at = now(), completed_by = $2
                WHERE id = $1"#,
        )
        .bind(reset_id)
        .bind(actor.id.as_uuid()),
    )
    .await
    .map_err(map_tx)?;
    Ok(())
}

fn token_digest(token: &str) -> String {
    hex(&blake2b::hash(token.as_bytes(), 32))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

async fn upsert_login_hash(tx: &mut datum_db::Tx<'_>, principal: UserId, phc: &str) -> Result<()> {
    tx.execute(
        sql_query(
            r#"INSERT INTO identity.login_credential
                   (principal_id, hash, m, t, p, rotated_at, failed_attempts)
               VALUES ($1, $2, $3, $4, $5, now(), 0)
               ON CONFLICT (principal_id) DO UPDATE
                 SET hash = EXCLUDED.hash,
                     m = EXCLUDED.m,
                     t = EXCLUDED.t,
                     p = EXCLUDED.p,
                     rotated_at = now(),
                     failed_attempts = 0,
                     locked_until = NULL,
                     last_failed_at = NULL"#,
        )
        .bind(principal.as_uuid())
        .bind(phc)
        .bind(argon2id::M_KIB as i32)
        .bind(argon2id::T_COST as i32)
        .bind(argon2id::P_COST as i32),
    )
    .await
    .map_err(map_tx)?;
    Ok(())
}
