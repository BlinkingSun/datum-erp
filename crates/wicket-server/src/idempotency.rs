//! Idempotency keys in `server_transient` (docs/10 §4.2; D-W1-2).

use serde_json::Value;
use sha2_lite::sha256_hex;
use uuid::Uuid;

use wicket_db::Tx;

use crate::error::{Error, Result};

mod sha2_lite {
    /// SHA-256 hex via `wicket_audit` (already a kernel dep).
    pub fn sha256_hex(bytes: &[u8]) -> String {
        wicket_audit::sha256::hex(&wicket_audit::sha256::digest(bytes))
    }
}

/// Hash a JSON body for replay comparison.
pub fn body_hash(body: &[u8]) -> String {
    sha256_hex(body)
}

/// Look up a stored replay. `None` means first use (caller must insert after).
pub async fn replay(tx: &mut Tx<'_>, key: Uuid, hash: &str) -> Result<Option<(u16, Value)>> {
    let row: Option<(String, i32, Value)> = tx
        .fetch_optional(
            sqlx::query_as(
                r#"SELECT body_hash, status, response
                     FROM server_transient.idempotency
                    WHERE key = $1"#,
            )
            .bind(key),
        )
        .await?;
    match row {
        None => Ok(None),
        Some((stored, status, response)) if stored == hash => {
            Ok(Some((u16::try_from(status).unwrap_or(500), response)))
        }
        Some(_) => Err(Error::http(
            "IDEMPOTENCY_CONFLICT",
            "Idempotency-Key was reused with a different body",
            None,
            axum::http::StatusCode::CONFLICT,
        )),
    }
}

/// Store the first-response pair.
pub async fn remember(
    tx: &mut Tx<'_>,
    key: Uuid,
    hash: &str,
    status: u16,
    response: &Value,
) -> Result<()> {
    tx.execute(
        sqlx::query(
            r#"INSERT INTO server_transient.idempotency (key, body_hash, status, response)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (key) DO NOTHING"#,
        )
        .bind(key)
        .bind(hash)
        .bind(i32::from(status))
        .bind(response),
    )
    .await?;
    Ok(())
}

/// Parse `Idempotency-Key` (required on POST).
pub fn require_key(headers: &axum::http::HeaderMap) -> Result<Uuid> {
    let raw = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            Error::validation(
                "Idempotency-Key is required on POST",
                Some("Idempotency-Key"),
            )
        })?;
    Uuid::parse_str(raw)
        .map_err(|_| Error::validation("Idempotency-Key must be a UUID", Some("Idempotency-Key")))
}
