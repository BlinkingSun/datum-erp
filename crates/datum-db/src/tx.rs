//! Sealed write transaction (D3 §2.1–§2.3).

use std::future::Future;

use datum_core::ActorKind;
use sqlx::postgres::{PgArguments, PgQueryResult, PgRow};
use sqlx::{Execute, Executor, FromRow, Postgres, Transaction};

use crate::error::{Error, Result};
use crate::{DATUM_SETTINGS, WriteContext, WritePool, app_version, guc};

/// Sealed write transaction. Actor is transaction-local (D3 §2.1).
///
/// No `Deref` / `DerefMut` to the inner `sqlx::Transaction`. The only public
/// constructors are [`Tx::begin`] and [`Tx::begin_serializable`].
pub struct Tx<'c> {
    inner: Transaction<'c, Postgres>,
}

impl<'c> Tx<'c> {
    /// Begin a `READ COMMITTED` write transaction and bind every `datum.*` setting
    /// in one `set_config(..., true)` statement.
    pub async fn begin(pool: &'c WritePool, ctx: &WriteContext) -> Result<Tx<'c>> {
        let inner = pool.0.begin().await?;
        bind_context(inner, ctx).await
    }

    /// Begin a `SERIALIZABLE` write transaction (ledger) and bind context.
    pub async fn begin_serializable(pool: &'c WritePool, ctx: &WriteContext) -> Result<Tx<'c>> {
        let inner = pool
            .0
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await?;
        bind_context(inner, ctx).await
    }

    /// Run `sqlx::query` / a static SQL string on this transaction.
    pub async fn execute<'q, E>(&mut self, query: E) -> Result<PgQueryResult>
    where
        E: Execute<'q, Postgres> + 'q,
    {
        Executor::execute(&mut *self.inner, query)
            .await
            .map_err(Error::from)
    }

    /// Fetch exactly one row.
    pub async fn fetch_one<'q, T>(
        &mut self,
        query: sqlx::query::QueryAs<'q, Postgres, T, PgArguments>,
    ) -> Result<T>
    where
        T: Send + Unpin + for<'r> FromRow<'r, PgRow>,
    {
        query.fetch_one(&mut *self.inner).await.map_err(Error::from)
    }

    /// Fetch at most one row.
    pub async fn fetch_optional<'q, T>(
        &mut self,
        query: sqlx::query::QueryAs<'q, Postgres, T, PgArguments>,
    ) -> Result<Option<T>>
    where
        T: Send + Unpin + for<'r> FromRow<'r, PgRow>,
    {
        query
            .fetch_optional(&mut *self.inner)
            .await
            .map_err(Error::from)
    }

    /// Fetch every row.
    pub async fn fetch_all<'q, T>(
        &mut self,
        query: sqlx::query::QueryAs<'q, Postgres, T, PgArguments>,
    ) -> Result<Vec<T>>
    where
        T: Send + Unpin + for<'r> FromRow<'r, PgRow>,
    {
        query.fetch_all(&mut *self.inner).await.map_err(Error::from)
    }

    /// Read a transaction-local GUC (`current_setting(name, true)`).
    pub async fn setting(&mut self, name: &str) -> Result<String> {
        let row: (Option<String>,) = sqlx::query_as("SELECT pg_catalog.current_setting($1, true)")
            .bind(name)
            .fetch_one(&mut *self.inner)
            .await?;
        Ok(row.0.unwrap_or_default())
    }

    /// Current PostgreSQL transaction id (`pg_current_xact_id()::text`).
    pub async fn pg_txid(&mut self) -> Result<String> {
        let row: (String,) = sqlx::query_as("SELECT pg_catalog.pg_current_xact_id()::text")
            .fetch_one(&mut *self.inner)
            .await?;
        Ok(row.0)
    }

    /// The seventeen-plus `datum.*` names bound by [`Tx::begin`].
    pub fn datum_settings() -> &'static [&'static str] {
        DATUM_SETTINGS
    }

    /// Commit. Consumes the transaction.
    pub async fn commit(self) -> Result<()> {
        self.inner.commit().await?;
        Ok(())
    }

    /// Roll back. Consumes the transaction.
    pub async fn rollback(self) -> Result<()> {
        self.inner.rollback().await?;
        Ok(())
    }
}

/// Re-run `f` on [`Error::Serialization`] up to `n` times after the first attempt.
pub async fn retry_serializable<T, F, Fut>(mut f: F, n: u32) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut retries = 0u32;
    loop {
        match f().await {
            Err(Error::Serialization) if retries < n => {
                retries += 1;
            }
            other => return other,
        }
    }
}

async fn bind_context<'c>(
    mut inner: Transaction<'c, Postgres>,
    ctx: &WriteContext,
) -> Result<Tx<'c>> {
    let actor_id = ctx.actor.id.as_uuid().to_string();
    let actor_kind = match ctx.actor.kind {
        ActorKind::User => "User",
        ActorKind::ServicePrincipal => "ServicePrincipal",
        _ => "User",
    };
    let app_version = app_version();
    let config_version = guc(&ctx.config_version);
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
                      pg_catalog.pg_current_xact_id()::text,      true),
                  pg_catalog.set_config('datum.app_version',  $16, true),
                  pg_catalog.set_config('datum.config_version',$17, true)"#,
    )
    .bind(&actor_id)
    .bind(actor_kind)
    .bind(guc(&ctx.actor_display))
    .bind(guc(&ctx.acting_for))
    .bind(guc(&ctx.session_id))
    .bind(guc(&ctx.request_id))
    .bind(&ctx.source_kind)
    .bind(guc(&ctx.source_device))
    .bind(guc(&ctx.source_ip))
    .bind(guc(&ctx.client_app))
    .bind(&ctx.action)
    .bind(guc(&ctx.reason))
    .bind(guc(&ctx.doc_type))
    .bind(guc(&ctx.doc_id))
    .bind(guc(&ctx.esign_id))
    .bind(&app_version)
    .bind(config_version)
    .execute(&mut *inner)
    .await?;
    Ok(Tx { inner })
}
