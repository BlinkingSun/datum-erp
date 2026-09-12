//! Commit-mode PostgreSQL test harness.
//!
//! Each test clones an ephemeral database from `DATUM_TEST_TEMPLATE` and drops it
//! afterwards. Deferred constraint triggers fire at commit, which is the point:
//! a rolled-back wrapper transaction would never arm them.

#![allow(clippy::disallowed_methods, clippy::disallowed_macros)] // test harness: creates and drops databases and probes sessions with raw SQL; never ships in the binary; CONTRACT section 5a exemption

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, Connection, PgConnection, PgPool, Postgres, Transaction};
use tokio::runtime::Builder as RuntimeBuilder;

/// Per-test ephemeral database: migrate pool, app pool, and the database name.
#[derive(Debug)]
pub struct TestDb {
    migrate_pool: PgPool,
    app_pool: PgPool,
    database: String,
    bootstrap_url: String,
    finished: bool,
}

/// Failures opening, migrating, or dropping a [`TestDb`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Database URL missing or the server did not answer in time.
    #[error("postgres unavailable: {0}")]
    Unavailable(String),
    /// A SQLx driver or protocol error.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    /// A SQLx migration failed.
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    /// A required environment variable was missing or malformed.
    #[error("{0}")]
    Env(String),
}

static CASE_SEQ: AtomicU64 = AtomicU64::new(1);

/// `Err(reason)` if `DATUM_MIGRATE_DATABASE_URL` is unset or the server does not
/// answer within 2 s.
pub fn postgres_available() -> Result<(), String> {
    let url = match std::env::var("DATUM_MIGRATE_DATABASE_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => return Err("DATUM_MIGRATE_DATABASE_URL is unset".to_string()),
    };
    let join = std::thread::Builder::new()
        .name("datum-test-pg-probe".into())
        .spawn(move || {
            let rt = RuntimeBuilder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("runtime: {e}"))?;
            rt.block_on(async move {
                match tokio::time::timeout(Duration::from_secs(2), PgConnection::connect(&url))
                    .await
                {
                    Ok(Ok(_conn)) => Ok(()),
                    Ok(Err(e)) => Err(format!("postgres unavailable: {e}")),
                    Err(_) => Err("server did not answer within 2 s".to_string()),
                }
            })
        })
        .map_err(|e| format!("postgres probe thread: {e}"))?;
    join.join()
        .unwrap_or_else(|_| Err("postgres probe thread panicked".to_string()))
}

/// Panics with the reason. Callers use it under `DATUM_REQUIRE_PG=1`.
pub fn require_postgres() {
    if let Err(reason) = postgres_available() {
        panic!("{reason}");
    }
}

impl TestDb {
    /// `CREATE DATABASE datum_t_<name>_<random> OWNER datum_owner TEMPLATE $DATUM_TEST_TEMPLATE`
    /// via `DATUM_BOOTSTRAP_URL`, then open migrate (`datum_migrate`) and app
    /// (`datum_app`) pools with the D3 §2.2 hooks.
    pub async fn case(name: &str) -> Result<Self, Error> {
        let migrate_url = required_url("DATUM_MIGRATE_DATABASE_URL")?;
        let app_url = required_url("DATUM_DATABASE_URL")?;
        let bootstrap_url = required_url("DATUM_BOOTSTRAP_URL")?;
        let template = match std::env::var("DATUM_TEST_TEMPLATE") {
            Ok(t) if !t.is_empty() => t,
            _ => return Err(Error::Env("DATUM_TEST_TEMPLATE is unset".to_string())),
        };
        assert_safe_ident(&template)?;

        let database = case_database_name(name);
        assert_safe_ident(&database)?;

        create_database(&bootstrap_url, &database, &template).await?;

        let migrate_pool = match open_pool(&rewrite_database(&migrate_url, &database)?).await {
            Ok(p) => p,
            Err(e) => {
                let _ = drop_database(&bootstrap_url, &database).await;
                return Err(e);
            }
        };
        let app_pool = match open_pool(&rewrite_database(&app_url, &database)?).await {
            Ok(p) => p,
            Err(e) => {
                migrate_pool.close().await;
                let _ = drop_database(&bootstrap_url, &database).await;
                return Err(e);
            }
        };

        tracing::debug!(%database, "opened test database");
        Ok(Self {
            migrate_pool,
            app_pool,
            database,
            bootstrap_url,
            finished: false,
        })
    }

    /// Pool connected as `datum_migrate`.
    pub fn migrate_pool(&self) -> &PgPool {
        &self.migrate_pool
    }

    /// Pool connected as `datum_app`.
    pub fn app_pool(&self) -> &PgPool {
        &self.app_pool
    }

    /// Ephemeral database name (`datum_t_<name>_<random>`).
    pub fn database(&self) -> &str {
        &self.database
    }

    /// Run the given migrators in the order given (graph order is the caller's
    /// job), as `datum_migrate`.
    pub async fn migrate(&self, migrators: &[&sqlx::migrate::Migrator]) -> Result<(), Error> {
        for migrator in migrators {
            migrator.run(self.migrate_pool()).await?;
        }
        Ok(())
    }

    /// A transaction on the given pool that the test MUST commit or roll back
    /// explicitly; never auto-rollback.
    pub async fn begin(&self, pool: &PgPool) -> Result<Transaction<'static, Postgres>, Error> {
        Ok(pool.begin().await?)
    }

    /// `DROP DATABASE ... WITH (FORCE)`. Idempotent.
    pub async fn finish(mut self) -> Result<(), Error> {
        self.finished = true;
        let _ = tokio::time::timeout(Duration::from_secs(2), async {
            self.migrate_pool.close().await;
            self.app_pool.close().await;
        })
        .await;
        drop_database(&self.bootstrap_url, &self.database).await
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        let database = self.database.clone();
        let bootstrap_url = self.bootstrap_url.clone();
        // Best-effort: FORCE drop from a detached thread. Do not close the
        // pools here and do not join — those sockets belong to the test
        // runtime, and joining a closer thread deadlocks it.
        let _ = std::thread::Builder::new()
            .name("datum-test-drop".into())
            .spawn(move || {
                let Ok(rt) = RuntimeBuilder::new_current_thread().enable_all().build() else {
                    return;
                };
                rt.block_on(async {
                    let _ = drop_database(&bootstrap_url, &database).await;
                });
            });
    }
}

/// Open a [`TestDb`] for `name`, skipping or panicking according to `DATUM_REQUIRE_PG`.
///
/// Expands to [`TestDb::case`] after [`require_postgres`] when `DATUM_REQUIRE_PG=1`,
/// or an early `return` when the server is absent and that variable is unset.
#[macro_export]
macro_rules! db_case {
    ($name:expr) => {{
        if ::std::env::var("DATUM_REQUIRE_PG").ok().as_deref() == Some("1") {
            $crate::require_postgres();
        } else if let Err(_reason) = $crate::postgres_available() {
            return;
        }
        $crate::TestDb::case($name)
            .await
            .unwrap_or_else(|e| panic!("test database: {e}"))
    }};
}

fn required_url(name: &str) -> Result<String, Error> {
    match std::env::var(name) {
        Ok(u) if !u.is_empty() => Ok(u),
        _ => Err(Error::Unavailable(format!("{name} is unset"))),
    }
}

fn assert_safe_ident(name: &str) -> Result<(), Error> {
    let ok = !name.is_empty()
        && name.len() <= 63
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(Error::Env(format!("unsafe database identifier {name:?}")))
    }
}

fn case_database_name(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let seq = CASE_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let suffix = format!("{seq:x}_{nanos:x}");
    let prefix = "datum_t_";
    let budget = 63usize.saturating_sub(prefix.len() + 1 + suffix.len());
    let slug: String = slug.chars().take(budget.max(1)).collect();
    format!("{prefix}{slug}_{suffix}")
}

fn rewrite_database(url: &str, database: &str) -> Result<String, Error> {
    let Some((prefix, rest)) = url.rsplit_once('/') else {
        return Err(Error::Env(format!("url has no database path: {url}")));
    };
    let qs = rest
        .split_once('?')
        .map(|(_, q)| format!("?{q}"))
        .unwrap_or_default();
    Ok(format!("{prefix}/{database}{qs}"))
}

async fn open_pool(url: &str) -> Result<PgPool, Error> {
    Ok(PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .after_connect(|c, _meta| {
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
        .after_release(|c, _meta| {
            Box::pin(async move {
                match sqlx::raw_sql("RESET ALL").execute(&mut *c).await {
                    Ok(_) => Ok(true),
                    Err(_) => Ok(false),
                }
            })
        })
        .connect(url)
        .await?)
}

fn is_sqlstate_55006(err: &sqlx::Error) -> bool {
    err.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "55006")
}

async fn create_database(bootstrap_url: &str, database: &str, template: &str) -> Result<(), Error> {
    let mut conn = connect_bootstrap(bootstrap_url).await?;
    let sql = format!("CREATE DATABASE {database} OWNER datum_owner TEMPLATE {template}");
    const RETRY_CEILING: Duration = Duration::from_secs(5);
    let mut slept = Duration::ZERO;
    let mut backoff = Duration::from_millis(100);
    let mut original_55006: Option<sqlx::Error> = None;

    loop {
        match sqlx::raw_sql(AssertSqlSafe(sql.clone()))
            .execute(&mut conn)
            .await
        {
            Ok(_) => return Ok(()),
            Err(e) if is_sqlstate_55006(&e) => {
                if original_55006.is_none() {
                    original_55006 = Some(e);
                }
                if slept >= RETRY_CEILING {
                    let e = original_55006.expect("55006 retry without stored error");
                    return Err(Error::Unavailable(format!(
                        "CREATE DATABASE {database}: {e}"
                    )));
                }
                let wait = backoff.min(RETRY_CEILING.saturating_sub(slept));
                if wait.is_zero() {
                    let e = original_55006.expect("55006 retry without stored error");
                    return Err(Error::Unavailable(format!(
                        "CREATE DATABASE {database}: {e}"
                    )));
                }
                tokio::time::sleep(wait).await;
                slept += wait;
                backoff = backoff.saturating_mul(2);
            }
            Err(e) => {
                return Err(Error::Unavailable(format!(
                    "CREATE DATABASE {database}: {e}"
                )));
            }
        }
    }
}

async fn drop_database(bootstrap_url: &str, database: &str) -> Result<(), Error> {
    assert_safe_ident(database)?;
    let mut conn = connect_bootstrap(bootstrap_url).await?;
    let sql = format!("DROP DATABASE IF EXISTS {database} WITH (FORCE)");
    sqlx::raw_sql(AssertSqlSafe(sql)).execute(&mut conn).await?;
    Ok(())
}

async fn connect_bootstrap(bootstrap_url: &str) -> Result<PgConnection, Error> {
    let opts = bootstrap_options(bootstrap_url)?;
    match tokio::time::timeout(Duration::from_secs(2), PgConnection::connect_with(&opts)).await {
        Ok(Ok(conn)) => Ok(conn),
        Ok(Err(e)) => Err(Error::Unavailable(format!("bootstrap connect: {e}"))),
        Err(_) => Err(Error::Unavailable(
            "bootstrap server did not answer within 2 s".to_string(),
        )),
    }
}

/// sqlx's URL parser does not default a missing user to the OS account the way
/// libpq does; on this MacBook `DATUM_BOOTSTRAP_URL` has no user (loopback trust).
fn bootstrap_options(url: &str) -> Result<PgConnectOptions, Error> {
    let mut opts: PgConnectOptions = url
        .parse()
        .map_err(|e| Error::Env(format!("DATUM_BOOTSTRAP_URL: {e}")))?;
    if !url_has_userinfo(url)
        && let Some(user) = os_username()
    {
        opts = opts.username(&user);
    }
    Ok(opts)
}

fn url_has_userinfo(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    rest.split(['/', '?']).next().unwrap_or("").contains('@')
}

fn os_username() -> Option<String> {
    std::env::var("PGUSER")
        .ok()
        .or_else(|| std::env::var("USER").ok())
        .or_else(|| std::env::var("LOGNAME").ok())
        .filter(|u| !u.is_empty() && u != "anonymous")
        .or_else(|| {
            std::process::Command::new("id")
                .arg("-un")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|u| !u.is_empty() && u != "anonymous")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::Instant;

    fn pg_code(err: &sqlx::Error) -> String {
        err.as_database_error()
            .and_then(|d| d.code().map(|c| c.into_owned()))
            .unwrap_or_else(|| format!("{err}"))
    }

    async fn assert_sqlstate_42501(pool: &PgPool, sql: &'static str) {
        let err = sqlx::query(sql)
            .execute(pool)
            .await
            .expect_err("expected privilege denial");
        assert_eq!(pg_code(&err), "42501", "sql={sql} err={err}");
    }

    /// Re-invoke this test binary with Datum URLs removed. Edition 2024 makes
    /// `env::set_var` unsafe, and workspace lints forbid `unsafe`.
    fn rerun_unset(test_name: &str, require_pg: bool) -> std::process::Output {
        let exe = std::env::current_exe().expect("current_exe");
        let mut cmd = Command::new(exe);
        cmd.args([test_name, "--exact", "--nocapture"]);
        cmd.env("DATUM_TEST_CHILD", "1");
        cmd.env_remove("DATUM_MIGRATE_DATABASE_URL");
        cmd.env_remove("DATUM_DATABASE_URL");
        cmd.env_remove("DATUM_BOOTSTRAP_URL");
        cmd.env_remove("DATUM_TEST_TEMPLATE");
        cmd.env_remove("DATABASE_URL");
        if require_pg {
            cmd.env("DATUM_REQUIRE_PG", "1");
        } else {
            cmd.env_remove("DATUM_REQUIRE_PG");
        }
        cmd.output().expect("spawn child test")
    }

    fn assert_child_ok(out: &std::process::Output) {
        assert!(
            out.status.success(),
            "child failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[tokio::test]
    async fn connect_reports_unavailable_without_url() {
        if std::env::var("DATUM_TEST_CHILD").ok().as_deref() != Some("1") {
            let out = rerun_unset("connect_reports_unavailable_without_url", false);
            assert_child_ok(&out);
            return;
        }
        let start = Instant::now();
        let err = TestDb::case("no_url").await.expect_err("unset urls");
        let elapsed = start.elapsed();
        assert!(
            elapsed <= Duration::from_secs(2),
            "Unavailable took {elapsed:?}"
        );
        assert!(
            matches!(err, Error::Unavailable(_)),
            "expected Unavailable, got {err}"
        );
        assert!(postgres_available().is_err());
    }

    #[tokio::test]
    async fn clone_retries_while_template_is_in_use() {
        if std::env::var("DATUM_REQUIRE_PG").ok().as_deref() == Some("1") {
            require_postgres();
        } else if let Err(_reason) = postgres_available() {
            return;
        }

        let template = std::env::var("DATUM_TEST_TEMPLATE").expect("DATUM_TEST_TEMPLATE");
        assert_safe_ident(&template).expect("template ident");
        let migrate_url = required_url("DATUM_MIGRATE_DATABASE_URL").expect("migrate url");
        let template_url = rewrite_database(&migrate_url, &template).expect("template url");

        let hold = tokio::spawn(async move {
            let conn = PgConnection::connect(&template_url)
                .await
                .expect("connect to template");
            tokio::time::sleep(Duration::from_millis(300)).await;
            drop(conn);
        });

        let db = TestDb::case("clone_retry_hold")
            .await
            .expect("clone while template session is held");
        hold.await.expect("hold task");

        db.finish().await.expect("finish");
    }

    #[tokio::test]
    async fn case_database_is_owned_by_datum_owner() {
        let db = db_case!("owned_by_datum_owner");
        let owner: String = sqlx::query_scalar(
            "SELECT pg_catalog.pg_get_userbyid(datdba)
             FROM pg_catalog.pg_database
             WHERE datname = current_database()",
        )
        .fetch_one(db.migrate_pool())
        .await
        .expect("pg_database.datdba");
        assert_eq!(owner, "datum_owner");
        db.finish().await.expect("finish");
    }

    #[tokio::test]
    async fn migrate_role_can_create_schema_in_case_database() {
        let db = db_case!("migrate_create_schema");
        sqlx::query("CREATE SCHEMA migrate_probe")
            .execute(db.migrate_pool())
            .await
            .expect("CREATE SCHEMA as datum_migrate with no prior GRANT");
        db.finish().await.expect("finish");
    }

    #[tokio::test]
    async fn case_databases_are_isolated() {
        let a = db_case!("isolated");
        let b = db_case!("isolated");
        sqlx::query("CREATE TABLE app.probe_iso (id int PRIMARY KEY)")
            .execute(a.migrate_pool())
            .await
            .expect("create a");
        sqlx::query("CREATE TABLE app.probe_iso (id int PRIMARY KEY)")
            .execute(b.migrate_pool())
            .await
            .expect("create b");
        let da = a.database().to_string();
        let db = b.database().to_string();
        a.finish().await.expect("finish a");
        b.finish().await.expect("finish b");

        let bootstrap = required_url("DATUM_BOOTSTRAP_URL").expect("bootstrap url");
        let mut conn = connect_bootstrap(&bootstrap).await.expect("bootstrap");
        let leftover: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_database WHERE datname = $1 OR datname = $2",
        )
        .bind(&da)
        .bind(&db)
        .fetch_one(&mut conn)
        .await
        .expect("pg_database");
        assert_eq!(leftover, 0, "case databases still listed");
    }

    #[tokio::test]
    async fn begin_does_not_auto_rollback() {
        let db = db_case!("begin_commit");
        sqlx::query("CREATE TABLE app.seen (id int PRIMARY KEY)")
            .execute(db.migrate_pool())
            .await
            .expect("create seen");
        let mut tx = db.begin(db.migrate_pool()).await.expect("begin");
        sqlx::query("INSERT INTO app.seen (id) VALUES (1)")
            .execute(&mut *tx)
            .await
            .expect("insert");
        tx.commit().await.expect("commit");
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM app.seen")
            .fetch_one(db.app_pool())
            .await
            .expect("count");
        assert_eq!(n, 1);
        db.finish().await.expect("finish");
    }

    #[tokio::test]
    async fn canary_deferred_constraint_fires_at_commit() {
        let db = db_case!("canary");
        sqlx::raw_sql(
            "CREATE TABLE transient.canary (id int PRIMARY KEY, n int NOT NULL);
             CREATE FUNCTION transient.canary_guard() RETURNS trigger
             LANGUAGE plpgsql AS $$
             BEGIN
               IF NEW.n = 0 THEN
                 RAISE EXCEPTION 'canary: deferred constraint fired'
                   USING ERRCODE = '23514';
               END IF;
               RETURN NEW;
             END $$;
             CREATE CONSTRAINT TRIGGER canary_deferred
               AFTER INSERT ON transient.canary
               DEFERRABLE INITIALLY DEFERRED
               FOR EACH ROW
               EXECUTE FUNCTION transient.canary_guard();",
        )
        .execute(db.migrate_pool())
        .await
        .expect("canary ddl");

        let mut tx = db.begin(db.migrate_pool()).await.expect("begin");
        sqlx::query("INSERT INTO transient.canary (id, n) VALUES (1, 0)")
            .execute(&mut *tx)
            .await
            .expect("insert must succeed; the trigger is deferred");
        let commit_err = tx.commit().await.expect_err("commit must fail");
        assert_eq!(pg_code(&commit_err), "23514", "commit err={commit_err}");

        let mut tx = db.begin(db.migrate_pool()).await.expect("begin rollback");
        sqlx::query("INSERT INTO transient.canary (id, n) VALUES (2, 0)")
            .execute(&mut *tx)
            .await
            .expect("insert");
        tx.rollback().await.expect("rollback");
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM transient.canary")
            .fetch_one(db.app_pool())
            .await
            .expect("count");
        assert_eq!(n, 0, "rolled-back attempt must report nothing");
        db.finish().await.expect("finish");
    }

    #[tokio::test]
    async fn grants_pattern_holds() {
        let db = db_case!("grants");
        sqlx::raw_sql(
            "CREATE TABLE audit.probe (id int PRIMARY KEY, n int NOT NULL);
             INSERT INTO audit.probe (id, n) VALUES (1, 1);
             CREATE TABLE app.probe (id int PRIMARY KEY, n int NOT NULL);
             INSERT INTO app.probe (id, n) VALUES (1, 1);
             CREATE TABLE transient.probe (id int PRIMARY KEY, n int NOT NULL);
             INSERT INTO transient.probe (id, n) VALUES (1, 1);",
        )
        .execute(db.migrate_pool())
        .await
        .expect("probe tables after 02-grants.sql");

        let app = db.app_pool();

        let audit_n: i32 = sqlx::query_scalar("SELECT n FROM audit.probe WHERE id = 1")
            .fetch_one(app)
            .await
            .expect("audit SELECT");
        assert_eq!(audit_n, 1);
        assert_sqlstate_42501(app, "INSERT INTO audit.probe (id, n) VALUES (2, 2)").await;

        let app_n: i32 = sqlx::query_scalar("SELECT n FROM app.probe WHERE id = 1")
            .fetch_one(app)
            .await
            .expect("app SELECT");
        assert_eq!(app_n, 1);
        sqlx::query("INSERT INTO app.probe (id, n) VALUES (2, 2)")
            .execute(app)
            .await
            .expect("app INSERT");
        sqlx::query("UPDATE app.probe SET n = 3 WHERE id = 1")
            .execute(app)
            .await
            .expect("app UPDATE");
        assert_sqlstate_42501(app, "DELETE FROM app.probe WHERE id = 1").await;

        let t_n: i32 = sqlx::query_scalar("SELECT n FROM transient.probe WHERE id = 1")
            .fetch_one(app)
            .await
            .expect("transient SELECT");
        assert_eq!(t_n, 1);
        sqlx::query("INSERT INTO transient.probe (id, n) VALUES (2, 2)")
            .execute(app)
            .await
            .expect("transient INSERT");
        sqlx::query("UPDATE transient.probe SET n = 3 WHERE id = 2")
            .execute(app)
            .await
            .expect("transient UPDATE");
        sqlx::query("DELETE FROM transient.probe WHERE id = 1")
            .execute(app)
            .await
            .expect("transient DELETE");

        assert_sqlstate_42501(app, "TRUNCATE audit.probe").await;
        assert_sqlstate_42501(app, "TRUNCATE app.probe").await;
        assert_sqlstate_42501(app, "TRUNCATE transient.probe").await;

        db.finish().await.expect("finish");
    }

    #[tokio::test]
    async fn pool_hooks_reset_state() {
        let db = db_case!("pool_hooks");
        let pool = db.migrate_pool();
        let mut conn = pool.acquire().await.expect("acquire");
        sqlx::query("SELECT set_config('datum.pool_probe', 'leaked', false)")
            .execute(&mut *conn)
            .await
            .expect("set guc");
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *conn)
            .await
            .expect("pid");
        drop(conn);

        let mut conn = pool.acquire().await.expect("reacquire");
        let pid2: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *conn)
            .await
            .expect("pid2");
        let leaked: Option<String> =
            sqlx::query_scalar("SELECT current_setting('datum.pool_probe', true)")
                .fetch_one(&mut *conn)
                .await
                .expect("guc");
        drop(conn);
        assert!(
            leaked.is_none() || leaked.as_deref() == Some(""),
            "GUC leaked on pid {pid}->{pid2}: {leaked:?}"
        );
        db.finish().await.expect("finish");
    }

    #[test]
    fn require_pg_hard_fails() {
        if std::env::var("DATUM_TEST_CHILD").ok().as_deref() != Some("1") {
            let out = rerun_unset("require_pg_hard_fails", true);
            assert_child_ok(&out);
            return;
        }
        let panicked = std::panic::catch_unwind(require_postgres).is_err();
        assert!(panicked, "require_postgres must panic with no database");
    }

    #[test]
    fn error_unavailable_formats() {
        let err = Error::Unavailable("no url".into());
        assert!(err.to_string().contains("unavailable"));
    }
}
