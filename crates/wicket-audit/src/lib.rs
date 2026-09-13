//! Kernel audit trail: the database writes the row; this crate is the SQL and
//! a thin Rust API over it.
#![allow(clippy::disallowed_methods, clippy::disallowed_macros)] // D3 §11: audit owns trail SQL.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;
use wicket_core::{Actor, Identifier};

pub mod export;
pub mod install;
pub mod sha256;

/// Crate error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented (PDF hook until `wicket-print`).
    #[error("unimplemented")]
    Unimplemented,
    /// Core error.
    #[error(transparent)]
    Core(#[from] wicket_core::Error),
    /// Database error.
    #[error(transparent)]
    Db(#[from] wicket_db::Error),
    /// Filesystem error while writing an export bundle.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// JSON error while writing an export bundle.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}

/// Embedded migrator (`placeholder` + `0001_audit`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Context the persistence layer needs to write an audit row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditCtx {
    /// Actor.
    pub actor: Actor,
    /// Reason.
    pub reason: Option<String>,
    /// Source identifier.
    pub source: Option<Identifier>,
}

impl AuditCtx {
    /// Test constructor.
    pub fn test(actor: Actor) -> Self {
        Self {
            actor,
            reason: None,
            source: None,
        }
    }
}

/// Typed view of one `audit.event` row (plus the stub `entity` field).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Entry id (`event_id`).
    pub id: Identifier,
    /// Entity name (`table_name`, or the kernel event `action`).
    pub entity: String,
}

/// Write path other crates call. Records a kernel `app_event` via `audit.log_event`.
pub async fn record(
    tx: &mut wicket_db::Tx<'_>,
    ctx: &AuditCtx,
    entry: AuditEntry,
) -> Result<Identifier> {
    let reason = ctx.reason.clone().unwrap_or_default();
    let row: (Uuid,) = tx
        .fetch_one(
            sqlx::query_as(
                r#"SELECT audit.log_event(
                       $1, $2, $3, $4, $5, $6, CAST($7 AS jsonb)
                   )"#,
            )
            .bind("kernel.record")
            .bind(&entry.entity)
            .bind(&reason)
            .bind("")
            .bind("")
            .bind("")
            .bind("{}"),
        )
        .await?;
    let _ = row;
    Ok(entry.id)
}

/// Attach `zz_audit_row` and `zz_audit_truncate` to `rel` (`schema.table`).
pub async fn attach(pool: &PgPool, rel: &str) -> Result<()> {
    sqlx::query("SELECT audit.attach($1::regclass)")
        .bind(rel)
        .execute(pool)
        .await?;
    Ok(())
}

/// Recompute seals in `[from_seq, to_seq]`. Returns the first divergent `seq`,
/// or `None` if every seal matches.
pub async fn verify(pool: &PgPool, from_seq: i64, to_seq: i64) -> Result<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT seq
          FROM audit.verify($1, $2)
         WHERE NOT ok
         ORDER BY seq
         LIMIT 1
        "#,
    )
    .bind(from_seq)
    .bind(to_seq)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

/// Current chain head, if any seal has been written.
#[derive(Debug, Clone)]
pub struct Head {
    /// Gap-free sequence number.
    pub seq: i64,
    /// Seal hash (`sha256`, 32 bytes).
    pub hash: Vec<u8>,
    /// Transaction id (`xid8` as text).
    pub xid: String,
    /// `clock_timestamp()` at commit.
    pub sealed_at: chrono::DateTime<chrono::Utc>,
    /// Audit rows in the sealed transaction.
    pub row_count: i32,
    /// Algorithm id (`wicket-audit-1`).
    pub chain_algo: String,
}

#[derive(sqlx::FromRow)]
struct HeadRow {
    seq: i64,
    hash: Vec<u8>,
    xid: String,
    sealed_at: chrono::DateTime<chrono::Utc>,
    row_count: i32,
    chain_algo: String,
}

/// Read [`audit.head()`](Head).
pub async fn head(pool: &PgPool) -> Result<Option<Head>> {
    let row: Option<HeadRow> = sqlx::query_as(
        r#"
            SELECT seq, hash, xid::text AS xid, sealed_at, row_count, chain_algo
              FROM audit.head()
            "#,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| Head {
        seq: r.seq,
        hash: r.hash,
        xid: r.xid,
        sealed_at: r.sealed_at,
        row_count: r.row_count,
        chain_algo: r.chain_algo,
    }))
}

/// Off-box anchor records.
pub mod anchor {
    use sqlx::PgPool;

    use crate::Result;

    /// Persist an off-box anchor of chain head `(seq, hash)` at `sink`.
    pub async fn record(
        pool: &PgPool,
        seq: i64,
        hash: &[u8],
        sink: &str,
        receipt: Option<&str>,
    ) -> Result<()> {
        sqlx::query("SELECT audit.record_anchor($1, $2, $3, $4)")
            .bind(seq)
            .bind(hash)
            .bind(sink)
            .bind(receipt)
            .execute(pool)
            .await?;
        Ok(())
    }
}

pub use export::bundle;
pub use install::{install_privileged, uninstall_privileged};

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tokio as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder_and_audit() {
        assert!(MIGRATOR.migrations.len() >= 2);
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = wicket_test::postgres_available();
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
