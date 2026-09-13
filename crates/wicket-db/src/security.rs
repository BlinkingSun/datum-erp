//! Separate-connection security-event path (D3 §2.4).

use crate::WritePool;
use crate::error::{Error, Result, is_undefined_function};

/// Payload for [`log_unattributable_write`].
#[derive(Debug, Clone)]
pub struct UnattributableWrite {
    /// Declared action of the refused request, if known.
    pub action: Option<String>,
    /// Reason, if known.
    pub reason: Option<String>,
    /// Document type, if known.
    pub doc_type: Option<String>,
    /// Document id, if known.
    pub doc_id: Option<String>,
    /// Electronic signature id, if known.
    pub esign_id: Option<String>,
    /// JSON object (text) with request id, source, SQLSTATE, and any extra detail.
    pub detail: String,
}

/// Record a `security.unattributable_write` event on a **separate** connection.
///
/// Calls `audit.log_event`. Returns [`Error::Unimplemented`] when that function
/// does not exist yet (this batch; `wicket-audit` ships the real one).
///
/// The caller must not hold the failing transaction's connection if the write
/// pool has `max_connections(1)`.
pub async fn log_unattributable_write(
    pool: &WritePool,
    details: &UnattributableWrite,
) -> Result<()> {
    let mut conn = pool.0.acquire().await?;
    let result = sqlx::query(
        r#"SELECT audit.log_event(
                $1, $2, $3, $4, $5, $6, CAST($7 AS jsonb)
           )"#,
    )
    .bind("security.unattributable_write")
    .bind(details.action.as_deref().unwrap_or(""))
    .bind(details.reason.as_deref().unwrap_or(""))
    .bind(details.doc_type.as_deref().unwrap_or(""))
    .bind(details.doc_id.as_deref().unwrap_or(""))
    .bind(details.esign_id.as_deref().unwrap_or(""))
    .bind(details.detail.as_str())
    .execute(&mut *conn)
    .await;
    match result {
        Ok(_) => Ok(()),
        Err(e) if is_undefined_function(&e) => Err(Error::Unimplemented),
        Err(e) => Err(Error::from(e)),
    }
}
