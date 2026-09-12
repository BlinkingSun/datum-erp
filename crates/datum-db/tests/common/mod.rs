#![allow(dead_code)]

use std::time::Duration;

use datum_core::{Actor, ActorKind, Identifier};
use datum_db::{WriteContext, WritePool};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, Connection, PgConnection};

pub fn test_ctx() -> WriteContext {
    ctx_named("test.begin")
}

pub fn ctx_named(action: &str) -> WriteContext {
    WriteContext::new(
        Actor {
            id: Identifier::generate(),
            kind: ActorKind::User,
        },
        action,
        "test",
    )
}

pub fn rewrite_database(url: &str, database: &str) -> String {
    let Some((prefix, rest)) = url.rsplit_once('/') else {
        panic!("url has no database path: {url}");
    };
    let qs = rest
        .split_once('?')
        .map(|(_, q)| format!("?{q}"))
        .unwrap_or_default();
    format!("{prefix}/{database}{qs}")
}

pub fn app_url(database: &str) -> String {
    rewrite_database(
        &std::env::var("DATUM_DATABASE_URL").expect("DATUM_DATABASE_URL"),
        database,
    )
}

pub fn migrate_url(database: &str) -> String {
    rewrite_database(
        &std::env::var("DATUM_MIGRATE_DATABASE_URL").expect("DATUM_MIGRATE_DATABASE_URL"),
        database,
    )
}

pub async fn write_pool(database: &str, max_connections: u32) -> WritePool {
    WritePool::connect_with(
        &app_url(database),
        PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5)),
    )
    .await
    .expect("WritePool::connect_with")
}

pub fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
}

pub const REQUIRE_CONTEXT_STUB: &str = r#"
CREATE TABLE app.probe (id int PRIMARY KEY, n int NOT NULL DEFAULT 1);
CREATE FUNCTION app.require_context_stub() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF nullif(current_setting('datum.actor_id', true), '') IS NULL THEN
    RAISE EXCEPTION 'datum: refused write with no attributable actor'
      USING ERRCODE = '42501',
            HINT = 'begin the transaction through datum_db::Tx::begin';
  END IF;
  IF nullif(current_setting('datum.txid', true), '')::xid8
       IS DISTINCT FROM pg_current_xact_id() THEN
    RAISE EXCEPTION 'datum: refused write with context from another transaction'
      USING ERRCODE = '42501';
  END IF;
  IF nullif(current_setting('datum.action', true), '') IS NULL
     OR nullif(current_setting('datum.source_kind', true), '') IS NULL THEN
    RAISE EXCEPTION 'datum: refused write with no declared action'
      USING ERRCODE = '42501';
  END IF;
  IF TG_OP = 'DELETE' THEN
    RETURN OLD;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER zz_audit_row
  AFTER INSERT OR UPDATE OR DELETE ON app.probe
  FOR EACH ROW
  EXECUTE FUNCTION app.require_context_stub();
"#;

pub async fn install_require_context_stub(pool: &sqlx::PgPool) {
    sqlx::raw_sql(REQUIRE_CONTEXT_STUB)
        .execute(pool)
        .await
        .expect("require_context stub");
}

pub async fn connect_app_raw(database: &str) -> PgConnection {
    PgConnection::connect(&app_url(database))
        .await
        .expect("raw app connection")
}

/// Cloned TestDb databases are owned by the bootstrap user, so `datum_migrate`
/// cannot `CREATE SCHEMA` until CREATE is granted on the database.
pub async fn grant_create_on_database(database: &str) {
    assert!(
        database
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    );
    let url = std::env::var("DATUM_BOOTSTRAP_URL").expect("DATUM_BOOTSTRAP_URL");
    let mut opts: PgConnectOptions = url.parse().expect("bootstrap url");
    // Match datum-test: sqlx does not default a missing user the way libpq does.
    // Percent-encoded socket URLs (`postgres://user@%2Ftmp/postgres`) have
    // userinfo; `postgres://%2Ftmp/postgres` does not — the `@` check must look
    // only at the authority, not the whole string.
    if !url_has_userinfo(&url)
        && let Ok(user) = std::env::var("USER").or_else(|_| std::env::var("LOGNAME"))
    {
        opts = opts.username(&user);
    }
    let mut conn = PgConnection::connect_with(&opts)
        .await
        .expect("bootstrap connect");
    let sql = format!("GRANT CREATE ON DATABASE {database} TO datum_migrate, datum_owner");
    sqlx::raw_sql(AssertSqlSafe(sql))
        .execute(&mut conn)
        .await
        .expect("GRANT CREATE");
}

fn url_has_userinfo(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    rest.split(['/', '?']).next().unwrap_or("").contains('@')
}
