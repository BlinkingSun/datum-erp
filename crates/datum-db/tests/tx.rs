//! Real-parts tests for `connect`, pool hooks, and `Tx::begin` (D3 §2.1–§2.3).
//!
//! Creates a scratch database `datum_skel_<random>` from `DATUM_BOOTSTRAP_URL`, applies
//! the five-role SQL, runs as `datum_migrate` and `datum_app`, and drops the database.

#![allow(clippy::disallowed_methods, clippy::disallowed_macros)] // scratch setup and GUC reads.

use datum_core::{Actor, ActorKind, Identifier};
use datum_db::{Tx, WriteContext, WritePool, connect};
use datum_test as _;
use proptest as _;
use serde as _;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, Executor};
use thiserror as _;

fn bootstrap_url() -> Option<String> {
    match std::env::var("DATUM_BOOTSTRAP_URL") {
        Ok(u) if !u.is_empty() => Some(u),
        _ if std::env::var("DATUM_REQUIRE_PG").as_deref() == Ok("1") => {
            panic!("DATUM_REQUIRE_PG=1 but DATUM_BOOTSTRAP_URL is unset");
        }
        _ => None,
    }
}

fn scratch_name() -> String {
    format!("datum_skel_{}", Identifier::generate().as_uuid().simple())
}

const FIVE_ROLES: &str = r#"
DO $roles$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'datum_owner') THEN
    CREATE ROLE datum_owner NOLOGIN;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'datum_audit_row') THEN
    CREATE ROLE datum_audit_row NOLOGIN;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'datum_audit_event') THEN
    CREATE ROLE datum_audit_event NOLOGIN;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'datum_migrate') THEN
    CREATE ROLE datum_migrate LOGIN;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'datum_app') THEN
    CREATE ROLE datum_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
  END IF;
END
$roles$;
GRANT datum_owner TO datum_migrate;
REVOKE SET ON PARAMETER session_replication_role FROM datum_app;
"#;

struct Scratch {
    bootstrap: String,
    name: String,
    migrate_url: String,
    app_url: String,
}

impl Scratch {
    async fn create() -> Option<Self> {
        let bootstrap = with_os_user(&bootstrap_url()?);
        let name = scratch_name();
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "scratch name must be a safe ident"
        );
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&bootstrap)
            .await
            .expect("bootstrap connect");
        sqlx::query(AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&admin)
            .await
            .expect("create scratch database");
        admin.execute(FIVE_ROLES).await.expect("five-role SQL");
        sqlx::query(AssertSqlSafe(format!(
            "GRANT CONNECT ON DATABASE {name} TO datum_migrate, datum_app"
        )))
        .execute(&admin)
        .await
        .expect("grant connect");
        admin.close().await;
        let migrate_url = rewrite_url(&bootstrap, "datum_migrate", &name);
        let app_url = rewrite_url(&bootstrap, "datum_app", &name);
        Some(Self {
            bootstrap,
            name,
            migrate_url,
            app_url,
        })
    }

    async fn drop(self) {
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.bootstrap)
            .await
            .expect("bootstrap reconnect");
        let _ = sqlx::query(AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS {} WITH (FORCE)",
            self.name
        )))
        .execute(&admin)
        .await;
        admin.close().await;
    }
}

/// sqlx 0.9 treats a user-less URL as role `anonymous`. libpq would use the OS user.
fn with_os_user(url: &str) -> String {
    if url.contains('@') {
        return url.to_string();
    }
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "postgres".into());
    if let Some(rest) = url.strip_prefix("postgres://") {
        format!("postgres://{user}@{rest}")
    } else if let Some(rest) = url.strip_prefix("postgresql://") {
        format!("postgresql://{user}@{rest}")
    } else {
        url.to_string()
    }
}

fn rewrite_url(bootstrap: &str, user: &str, db: &str) -> String {
    let (head, query) = match bootstrap.split_once('?') {
        Some((h, q)) => (h, format!("?{q}")),
        None => (bootstrap, String::new()),
    };
    let rest = head
        .strip_prefix("postgres://")
        .or_else(|| head.strip_prefix("postgresql://"))
        .expect("postgres URL");
    let hostport = rest.rsplit_once('@').map(|(_, hp)| hp).unwrap_or(rest);
    let hostport = hostport
        .split_once('/')
        .map(|(hp, _)| hp)
        .unwrap_or(hostport);
    format!("postgres://{user}@{hostport}/{db}{query}")
}

fn test_ctx() -> WriteContext {
    WriteContext::new(
        Actor {
            id: Identifier::generate(),
            kind: ActorKind::User,
        },
        "test.begin",
        "test",
    )
}

async fn assert_txid_bound(url: &str) {
    let pool = connect(url).await.expect("connect");
    let write = WritePool::new(pool.clone());
    let ctx = test_ctx();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let stored = tx.setting("datum.txid").await.expect("txid guc");
    let live = tx.pg_txid().await.expect("pg_current_xact_id");
    assert_eq!(stored, live, "datum.txid must equal pg_current_xact_id()");
    let actor = tx.setting("datum.actor_id").await.expect("actor_id");
    assert_eq!(actor, ctx.actor.id.as_uuid().to_string());
    let action = tx.setting("datum.action").await.expect("action");
    assert_eq!(action, "test.begin");
    let source = tx.setting("datum.source_kind").await.expect("source_kind");
    assert_eq!(source, "test");
    tx.commit().await.expect("commit");

    let mut probe = pool.begin().await.expect("probe begin");
    let leaked: (Option<String>,) =
        sqlx::query_as("SELECT pg_catalog.current_setting('datum.actor_id', true)")
            .fetch_one(&mut *probe)
            .await
            .expect("probe setting");
    assert!(
        leaked.0.as_deref().unwrap_or("").is_empty(),
        "actor_id leaked across transactions: {:?}",
        leaked.0
    );
    probe.rollback().await.expect("probe rollback");
    pool.close().await;
}

#[tokio::test]
async fn tx_actor_bound_to_transaction_id() {
    let Some(scratch) = Scratch::create().await else {
        eprintln!("skipping tx_actor_bound_to_transaction_id: DATUM_BOOTSTRAP_URL unset");
        return;
    };
    let migrate_url = scratch.migrate_url.clone();
    let app_url = scratch.app_url.clone();
    let handle = tokio::spawn(async move {
        assert_txid_bound(&migrate_url).await;
        assert_txid_bound(&app_url).await;
    });
    let outcome = handle.await;
    scratch.drop().await;
    outcome.expect("tx_actor_bound_to_transaction_id panicked");
}

#[tokio::test]
async fn connect_sets_utc_and_application_name() {
    let Some(scratch) = Scratch::create().await else {
        eprintln!("skipping connect_sets_utc_and_application_name: DATUM_BOOTSTRAP_URL unset");
        return;
    };
    let migrate_url = scratch.migrate_url.clone();
    let handle = tokio::spawn(async move {
        let pool = connect(&migrate_url).await.expect("connect");
        let row: (String, String) = sqlx::query_as(
            "SELECT current_setting('TimeZone'), current_setting('application_name')",
        )
        .fetch_one(&pool)
        .await
        .expect("settings");
        assert_eq!(row.0.to_uppercase(), "UTC");
        assert_eq!(row.1, "datum");
        pool.close().await;
    });
    let outcome = handle.await;
    scratch.drop().await;
    outcome.expect("connect_sets_utc_and_application_name panicked");
}
