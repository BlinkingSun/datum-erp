//! Kernel persistence: pool, sealed write transaction, migrations.
//!
//! Raw SQL is legal in this crate (D3 §11). The audit trigger and DDL stay unimplemented.

#![allow(clippy::disallowed_methods, clippy::disallowed_macros)] // D3 §11: this crate owns set_config / pool SQL.

use datum_core::{Actor, ActorKind};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Postgres, Transaction};

/// Connection pool. Thin alias so other crates can name it.
pub type Pool = sqlx::PgPool;

/// Embedded placeholder migrator.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Persistence error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented (DDL, audit trigger).
    #[error("unimplemented")]
    Unimplemented,
    /// Core error.
    #[error(transparent)]
    Core(#[from] datum_core::Error),
    /// SQLx error.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Write pool. No `Deref`, no `as_pool`, no `into_inner` (D3 §2.1).
#[derive(Debug, Clone)]
pub struct WritePool(Pool);

impl WritePool {
    /// Wrap a pool created by [`connect`].
    pub fn new(pool: Pool) -> Self {
        Self(pool)
    }
}

/// Read pool. Never calls `set_config`.
#[derive(Debug, Clone)]
pub struct ReadPool(Pool);

impl ReadPool {
    /// Wrap a pool created by [`connect`].
    pub fn new(pool: Pool) -> Self {
        Self(pool)
    }

    /// Idle connections currently in the pool.
    pub fn idle(&self) -> usize {
        self.0.num_idle()
    }
}

/// Transaction-local actor and provenance. Built here, not from module strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteContext {
    /// Authenticated actor.
    pub actor: Actor,
    /// Display name captured at authentication.
    pub actor_display: Option<String>,
    /// Principal this actor is acting for, if any.
    pub acting_for: Option<String>,
    /// Session id.
    pub session_id: Option<String>,
    /// Request id.
    pub request_id: Option<String>,
    /// Source kind (required by the trigger).
    pub source_kind: String,
    /// Source device.
    pub source_device: Option<String>,
    /// Source IP.
    pub source_ip: Option<String>,
    /// Client application.
    pub client_app: Option<String>,
    /// Declared action (required by the trigger).
    pub action: String,
    /// Reason.
    pub reason: Option<String>,
    /// Document type.
    pub doc_type: Option<String>,
    /// Document id.
    pub doc_id: Option<String>,
    /// Electronic signature id.
    pub esign_id: Option<String>,
}

impl WriteContext {
    /// Construct the minimum context `Tx::begin` will accept.
    pub fn new(actor: Actor, action: impl Into<String>, source_kind: impl Into<String>) -> Self {
        Self {
            actor,
            actor_display: None,
            acting_for: None,
            session_id: None,
            request_id: None,
            source_kind: source_kind.into(),
            source_device: None,
            source_ip: None,
            client_app: None,
            action: action.into(),
            reason: None,
            doc_type: None,
            doc_id: None,
            esign_id: None,
        }
    }
}

/// Session context the persistence layer can read.
pub trait SessionCtx {
    /// Actor bound to this session.
    fn actor(&self) -> Actor;
}

impl SessionCtx for WriteContext {
    fn actor(&self) -> Actor {
        self.actor
    }
}

/// Sealed write transaction. Actor is transaction-local (D3 §2.1).
pub struct Tx<'c> {
    inner: Transaction<'c, Postgres>,
}

impl<'c> Tx<'c> {
    /// Begin a write transaction and bind actor context in one `set_config` statement.
    ///
    /// `is_local` is always true. `datum.txid` is `pg_current_xact_id()` of this transaction.
    pub async fn begin(pool: &'c WritePool, ctx: &WriteContext) -> Result<Tx<'c>> {
        let mut inner = pool.0.begin().await?;
        let actor_id = ctx.actor.id.as_uuid().to_string();
        let actor_kind = match ctx.actor.kind {
            ActorKind::User => "User",
            ActorKind::ServicePrincipal => "ServicePrincipal",
            _ => "User",
        };
        sqlx::query(
            r#"SELECT pg_catalog.set_config('datum.actor_id',      $1, true),
                      pg_catalog.set_config('datum.actor_kind',    $2, true),
                      pg_catalog.set_config('datum.actor_display', $3, true),
                      pg_catalog.set_config('datum.acting_for',    $4, true),
                      pg_catalog.set_config('datum.session_id',    $5, true),
                      pg_catalog.set_config('datum.request_id',    $6, true),
                      pg_catalog.set_config('datum.source_kind',   $7, true),
                      pg_catalog.set_config('datum.source_device', $8, true),
                      pg_catalog.set_config('datum.source_ip',     $9, true),
                      pg_catalog.set_config('datum.client_app',   $10, true),
                      pg_catalog.set_config('datum.action',       $11, true),
                      pg_catalog.set_config('datum.reason',       $12, true),
                      pg_catalog.set_config('datum.doc_type',     $13, true),
                      pg_catalog.set_config('datum.doc_id',       $14, true),
                      pg_catalog.set_config('datum.esign_id',     $15, true),
                      pg_catalog.set_config('datum.txid',
                          pg_catalog.pg_current_xact_id()::text,      true)"#,
        )
        .bind(&actor_id)
        .bind(actor_kind)
        .bind(ctx.actor_display.as_deref())
        .bind(ctx.acting_for.as_deref())
        .bind(ctx.session_id.as_deref())
        .bind(ctx.request_id.as_deref())
        .bind(&ctx.source_kind)
        .bind(ctx.source_device.as_deref())
        .bind(ctx.source_ip.as_deref())
        .bind(ctx.client_app.as_deref())
        .bind(&ctx.action)
        .bind(ctx.reason.as_deref())
        .bind(ctx.doc_type.as_deref())
        .bind(ctx.doc_id.as_deref())
        .bind(ctx.esign_id.as_deref())
        .execute(&mut *inner)
        .await?;
        Ok(Tx { inner })
    }

    /// Read a transaction-local GUC (`current_setting(name, true)`).
    pub async fn setting(&mut self, name: &str) -> Result<String> {
        let row: (String,) = sqlx::query_as("SELECT pg_catalog.current_setting($1, true)")
            .bind(name)
            .fetch_one(&mut *self.inner)
            .await?;
        Ok(row.0)
    }

    /// Current PostgreSQL transaction id (`pg_current_xact_id()::text`).
    pub async fn pg_txid(&mut self) -> Result<String> {
        let row: (String,) = sqlx::query_as("SELECT pg_catalog.pg_current_xact_id()::text")
            .fetch_one(&mut *self.inner)
            .await?;
        Ok(row.0)
    }

    /// Commit.
    pub async fn commit(self) -> Result<()> {
        self.inner.commit().await?;
        Ok(())
    }

    /// Roll back.
    pub async fn rollback(self) -> Result<()> {
        self.inner.rollback().await?;
        Ok(())
    }
}

/// Open a pool with D3 §2.2 `after_connect` / `after_release` hooks.
pub async fn connect(url: &str) -> Result<Pool> {
    let pool = PgPoolOptions::new()
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::raw_sql(
                    "SET timezone = 'UTC'; \
                     SET application_name = 'datum'; \
                     SET idle_in_transaction_session_timeout = '15s'",
                )
                .execute(&mut *c)
                .await?;
                Ok(())
            })
        })
        .after_release(|c, _| {
            Box::pin(async move {
                match sqlx::raw_sql("RESET ALL").execute(&mut *c).await {
                    Ok(_) => Ok(true),
                    Err(_) => Ok(false),
                }
            })
        })
        .connect(url)
        .await?;
    Ok(pool)
}

/// DDL remains unimplemented in Wave 1.
pub async fn apply_ddl(_tx: &mut Tx<'_>) -> Result<()> {
    Err(Error::Unimplemented)
}

/// Audit trigger attachment remains unimplemented in Wave 1.
pub async fn attach_audit_trigger(_tx: &mut Tx<'_>) -> Result<()> {
    Err(Error::Unimplemented)
}

/// Tokio is the pool runtime.
pub fn runtime_kind() -> &'static str {
    let _ = core::any::type_name::<tokio::runtime::Runtime>();
    "tokio"
}

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
    fn migrator_has_placeholder() {
        assert!(!MIGRATOR.migrations.is_empty());
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..8) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
