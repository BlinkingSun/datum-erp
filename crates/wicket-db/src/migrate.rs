//! Multi-crate migration runner (D3 §11; composition order owned later by `wicket-module`).

use sqlx::migrate::Migrator;
use sqlx::{AssertSqlSafe, Connection, PgConnection};

use crate::error::{Error, Result, is_undefined_table};
use crate::{Pool, app_version};

/// D-2b-11 six-GUC migration preamble (D-2b-12). `is_local = true` so the
/// values last for the implicit (apply) or explicit (flush) transaction.
/// Well-known actor is `identity.principal` 'migration'.
const MIGRATION_GUC_PREAMBLE: &str = r#"
SELECT
  pg_catalog.set_config('wicket.actor_id',      '00000000-0000-4000-8000-000000000002', true),
  pg_catalog.set_config('wicket.actor_kind',    'migration', true),
  pg_catalog.set_config('wicket.actor_display', 'migration', true),
  pg_catalog.set_config('wicket.txid',          pg_catalog.pg_current_xact_id()::text, true),
  pg_catalog.set_config('wicket.action',        'migrate', true),
  pg_catalog.set_config('wicket.source_kind',   'migration', true);
"#;

/// Advisory lock key held for the whole of [`run`].
pub const ADVISORY_LOCK_KEY: i64 = 0x0044_4154_554D_0001;

/// Run each crate's migrator in the given order, inside one advisory lock.
///
/// Records `(crate, version, applied_at, app_version)` in `wicket.schema_history`.
/// Already-recorded `(crate, version)` pairs are skipped (idempotent).
///
/// Sets the six `wicket.*` migration GUCs on this connection for each apply
/// and for the history insert (D-2b-12), so a later `run` still records
/// history after `wicket.schema_history` is attached. Individual migrations
/// need no preamble of their own to survive `zz_audit_row`.
///
/// Call as `wicket_migrate`. Schema `wicket` / `wicket.schema_history` come from
/// this crate's `0001_wicket_schema` migration, which must run first.
pub async fn run(pool: &Pool, crates: &[(&str, &Migrator)]) -> Result<()> {
    let mut conn = pool.acquire().await?;
    sqlx::query("SELECT pg_catalog.pg_advisory_lock($1)")
        .bind(ADVISORY_LOCK_KEY)
        .execute(&mut *conn)
        .await?;
    let outcome = run_locked(&mut conn, crates).await;
    let unlock = sqlx::query("SELECT pg_catalog.pg_advisory_unlock($1)")
        .bind(ADVISORY_LOCK_KEY)
        .execute(&mut *conn)
        .await;
    match (outcome, unlock) {
        (Ok(()), Ok(_)) => Ok(()),
        (Err(e), _) => Err(e),
        (Ok(()), Err(e)) => Err(Error::from(e)),
    }
}

async fn run_locked(conn: &mut PgConnection, crates: &[(&str, &Migrator)]) -> Result<()> {
    grant_owner_create(conn).await?;
    let version = app_version();
    let mut pending: Vec<(String, i64)> = Vec::new();
    for (crate_name, migrator) in crates {
        for migration in migrator.iter() {
            if migration.migration_type.is_down_migration() {
                continue;
            }
            if recorded(conn, crate_name, migration.version).await? {
                continue;
            }
            apply_sql(conn, migration.sql.as_str()).await?;
            reassign_login_owned(conn).await?;
            pending.push(((*crate_name).to_string(), migration.version));
            flush_pending(conn, &mut pending, &version).await?;
        }
    }
    flush_pending(conn, &mut pending, &version).await?;
    Ok(())
}

/// `ALTER TABLE ... OWNER TO wicket_owner` requires CREATE on the schema.
async fn grant_owner_create(conn: &mut PgConnection) -> Result<()> {
    sqlx::raw_sql(
        r#"
        DO $g$
        DECLARE n text;
        BEGIN
          FOREACH n IN ARRAY ARRAY['app', 'transient', 'audit'] LOOP
            IF EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = n) THEN
              EXECUTE format('GRANT USAGE, CREATE ON SCHEMA %I TO wicket_owner', n);
            END IF;
          END LOOP;
        END
        $g$;
        "#,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Apply one up-migration as `wicket_migrate`. Ownership is reassigned
/// afterwards so later crates inherit `wicket_owner` table ownership.
///
/// The six-GUC preamble is prepended so the migration's implicit
/// transaction carries a matching `wicket.txid` (D-2b-12).
async fn apply_sql(conn: &mut PgConnection, sql: &str) -> Result<()> {
    if sql_is_noop(sql) {
        return Ok(());
    }
    let wrapped = format!("{MIGRATION_GUC_PREAMBLE}\n{sql}");
    sqlx::raw_sql(AssertSqlSafe(wrapped))
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Tables in app-class schemas must not stay owned by a login role (D3 §1.1).
async fn reassign_login_owned(conn: &mut PgConnection) -> Result<()> {
    sqlx::raw_sql(
        r#"
        DO $owner$
        DECLARE r record;
        BEGIN
          IF EXISTS (
            SELECT 1
            FROM pg_namespace n
            JOIN pg_roles o ON o.oid = n.nspowner
            WHERE n.nspname = 'wicket' AND o.rolcanlogin
          ) THEN
            ALTER SCHEMA wicket OWNER TO wicket_owner;
          END IF;
          FOR r IN
            SELECT n.nspname, c.relname
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            JOIN pg_roles o ON o.oid = c.relowner
            WHERE n.nspname IN ('wicket', 'app', 'transient', 'audit')
              AND c.relkind IN ('r', 'p', 'v', 'm')
              AND o.rolcanlogin
              AND c.relname IS DISTINCT FROM '_sqlx_migrations'
          LOOP
            -- Table OWNER TO also reassigns the owned serial/identity sequence.
            EXECUTE format('ALTER TABLE %I.%I OWNER TO wicket_owner', r.nspname, r.relname);
          END LOOP;
        END
        $owner$;
        "#,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

fn sql_is_noop(sql: &str) -> bool {
    sql.lines().all(|line| {
        let t = line.trim();
        t.is_empty() || t.starts_with("--")
    })
}

async fn recorded(conn: &mut PgConnection, crate_name: &str, version: i64) -> Result<bool> {
    let row = sqlx::query_as::<_, (bool,)>(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM wicket.schema_history
            WHERE crate = $1 AND version = $2
        )
        "#,
    )
    .bind(crate_name)
    .bind(version)
    .fetch_one(&mut *conn)
    .await;
    match row {
        Ok((exists,)) => Ok(exists),
        Err(e) if is_undefined_table(&e) => Ok(false),
        Err(e) => Err(Error::from(e)),
    }
}

async fn flush_pending(
    conn: &mut PgConnection,
    pending: &mut Vec<(String, i64)>,
    app_version: &str,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    if !schema_history_exists(conn).await? {
        return Ok(());
    }
    let mut tx = conn.begin().await?;
    sqlx::raw_sql(MIGRATION_GUC_PREAMBLE)
        .execute(&mut *tx)
        .await?;
    for (crate_name, version) in pending.drain(..) {
        sqlx::query(
            r#"
            INSERT INTO wicket.schema_history (crate, version, applied_at, app_version)
            VALUES ($1, $2, now(), $3)
            ON CONFLICT (crate, version) DO NOTHING
            "#,
        )
        .bind(&crate_name)
        .bind(version)
        .bind(app_version)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn schema_history_exists(conn: &mut PgConnection) -> Result<bool> {
    let (exists,): (bool,) =
        sqlx::query_as("SELECT to_regclass('wicket.schema_history') IS NOT NULL")
            .fetch_one(&mut *conn)
            .await?;
    Ok(exists)
}
