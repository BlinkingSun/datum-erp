//! Kernel persistence: pool, sealed write transaction, migrations.
//!
//! Raw SQL is legal in this crate (D3 §11). The audit trigger and DDL attach stay
//! unimplemented until `wicket-audit`.

#![allow(clippy::disallowed_methods, clippy::disallowed_macros)] // D3 §11: this crate owns set_config / pool SQL.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgArguments, PgConnectOptions, PgPoolOptions, PgRow};
use sqlx::{FromRow, Postgres};
use wicket_core::Actor;

mod error;
mod tx;

pub mod ddl;
pub mod migrate;
pub mod security;

pub use error::{Error, Result, SqlState};
pub use tx::{Tx, retry_serializable};

/// Connection pool. Thin alias so other crates can name it.
pub type Pool = sqlx::PgPool;

/// Embedded migrator (`placeholder` + `0001_wicket_schema`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Every `wicket.*` setting [`Tx::begin`] binds (D3 §2.1 plus invariant 17).
pub const WICKET_SETTINGS: &[&str] = &[
    "wicket.actor_id",
    "wicket.actor_kind",
    "wicket.actor_display",
    "wicket.acting_for",
    "wicket.session_id",
    "wicket.request_id",
    "wicket.source_kind",
    "wicket.source_device",
    "wicket.source_ip",
    "wicket.client_app",
    "wicket.action",
    "wicket.reason",
    "wicket.doc_type",
    "wicket.doc_id",
    "wicket.esign_id",
    "wicket.txid",
    "wicket.app_version",
    "wicket.config_version",
];

/// Write pool. No `Deref`, no `as_pool`, no `into_inner` (D3 §2.1).
#[derive(Debug, Clone)]
pub struct WritePool(Pool);

impl WritePool {
    /// Wrap a pool created by [`connect`].
    pub fn new(pool: Pool) -> Self {
        Self(pool)
    }

    /// Open a write pool with D3 §2.2 hooks.
    pub async fn connect(url: &str) -> Result<Self> {
        Ok(Self(connect(url).await?))
    }

    /// Open a write pool with caller [`PgPoolOptions`] plus D3 §2.2 hooks.
    pub async fn connect_with(url: &str, options: PgPoolOptions) -> Result<Self> {
        Ok(Self(connect_with(url, options).await?))
    }
}

/// Read pool. Never calls `set_config`. Fetch-only: no `execute`.
///
/// Writes go through the sealed [`Tx`]. Callers construct a sqlx query and
/// pass it to [`ReadPool::fetch_one`] / [`ReadPool::fetch_optional`] / [`ReadPool::fetch_all`].
#[derive(Debug, Clone)]
pub struct ReadPool(Pool);

impl ReadPool {
    /// Wrap a pool created by [`connect`].
    pub fn new(pool: Pool) -> Self {
        Self(pool)
    }

    /// Open a read pool with D3 §2.2 hooks.
    pub async fn connect(url: &str) -> Result<Self> {
        Ok(Self(connect(url).await?))
    }

    /// Idle connections currently in the pool.
    pub fn idle(&self) -> usize {
        self.0.num_idle()
    }

    /// Borrow the inner pool for kernel read APIs that still take [`Pool`].
    ///
    /// [`ReadPool`] itself has no `execute`. A write issued on the returned
    /// `&Pool` is the same fence smell this type exists to close (CONTRACT §5a).
    pub fn as_pool(&self) -> &Pool {
        &self.0
    }

    /// Fetch exactly one row.
    pub async fn fetch_one<'q, T>(
        &self,
        query: sqlx::query::QueryAs<'q, Postgres, T, PgArguments>,
    ) -> Result<T>
    where
        T: Send + Unpin + for<'r> FromRow<'r, PgRow>,
    {
        query.fetch_one(&self.0).await.map_err(Error::from)
    }

    /// Fetch at most one row.
    pub async fn fetch_optional<'q, T>(
        &self,
        query: sqlx::query::QueryAs<'q, Postgres, T, PgArguments>,
    ) -> Result<Option<T>>
    where
        T: Send + Unpin + for<'r> FromRow<'r, PgRow>,
    {
        query.fetch_optional(&self.0).await.map_err(Error::from)
    }

    /// Fetch every row.
    pub async fn fetch_all<'q, T>(
        &self,
        query: sqlx::query::QueryAs<'q, Postgres, T, PgArguments>,
    ) -> Result<Vec<T>>
    where
        T: Send + Unpin + for<'r> FromRow<'r, PgRow>,
    {
        query.fetch_all(&self.0).await.map_err(Error::from)
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
    /// Configuration version (invariant 17). Empty until `wicket-module` exists.
    pub config_version: Option<String>,
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
            config_version: None,
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

/// Crate version compiled in, plus `git describe` when the build script provides one.
pub fn app_version() -> String {
    match option_env!("WICKET_GIT_DESCRIBE") {
        Some(d) if !d.is_empty() => format!("{} ({d})", env!("CARGO_PKG_VERSION")),
        _ => env!("CARGO_PKG_VERSION").to_string(),
    }
}

pub(crate) fn guc(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
}

/// Open a pool with D3 §2.2 `after_connect` / `after_release` hooks.
pub async fn connect(url: &str) -> Result<Pool> {
    connect_with(url, PgPoolOptions::new()).await
}

/// Startup parameters applied to every pooled connection (D3 §2.2 / addendum 3).
///
/// PostgreSQL treats these as session defaults, so `RESET ALL` in `after_release`
/// restores them instead of reverting to cluster defaults.
fn connection_startup_options(url: &str) -> Result<PgConnectOptions> {
    let opts: PgConnectOptions = url.parse().map_err(Error::from)?;
    Ok(opts.options([
        ("TimeZone", "UTC"),
        ("application_name", "wicket"),
        ("idle_in_transaction_session_timeout", "15s"),
    ]))
}

/// Open a pool with caller options plus D3 §2.2 hooks (hooks always win).
pub async fn connect_with(url: &str, options: PgPoolOptions) -> Result<Pool> {
    let connect_opts = connection_startup_options(url)?;
    let pool = options
        .after_release(|c, _| {
            Box::pin(async move {
                match sqlx::raw_sql("RESET ALL").execute(&mut *c).await {
                    Ok(_) => Ok(true),
                    Err(_) => Ok(false),
                }
            })
        })
        .connect_with(connect_opts)
        .await?;
    Ok(pool)
}

/// DDL remains unimplemented in this crate (event-trigger attach is `wicket-audit`).
pub async fn apply_ddl(_tx: &mut Tx<'_>) -> Result<()> {
    Err(Error::Unimplemented)
}

/// Audit trigger attachment remains unimplemented (`wicket-audit`).
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
    use trybuild as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn poisoned_formats() {
        assert!(!Error::Poisoned.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(!MIGRATOR.migrations.is_empty());
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = wicket_test::postgres_available();
    }

    #[test]
    fn wicket_settings_are_the_eighteen() {
        assert_eq!(WICKET_SETTINGS.len(), 18);
        assert_eq!(WICKET_SETTINGS[0], "wicket.actor_id");
        assert_eq!(WICKET_SETTINGS[15], "wicket.txid");
        assert_eq!(WICKET_SETTINGS[16], "wicket.app_version");
        assert_eq!(WICKET_SETTINGS[17], "wicket.config_version");
    }

    /// Documented Linux CI form: percent-encoded socket directory as the host.
    #[test]
    fn percent_encoded_unix_socket_bootstrap_url_parses() {
        use sqlx::postgres::PgConnectOptions;
        let url =
            "postgres://wicket_bootstrap:s3cret@%2Fvar%2Frun%2Fpostgresql/postgres?sslmode=disable";
        let opts: PgConnectOptions = url.parse().expect("parse");
        assert_eq!(opts.get_username(), "wicket_bootstrap");
        assert_eq!(opts.get_database(), Some("postgres"));
        let socket = opts
            .get_socket()
            .expect("percent-encoded host must be a unix socket directory");
        let path = socket.to_string_lossy();
        assert_eq!(path.trim_end_matches('/'), "/var/run/postgresql");
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..8) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
