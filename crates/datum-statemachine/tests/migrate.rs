//! Reversible migration.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_statemachine::MIGRATOR;
use datum_test::db_case;
use sqlx::migrate::Migrator;
use sqlx::{query_as as sql_query_as, query_scalar as sql_query_scalar};

use common::{bootstrap_pool, migrate_and_install};

fn sm_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_statemachine");
    migrator
}

async fn query_seam_fn_exists(pool: &sqlx::PgPool, name: &str) -> bool {
    sql_query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pg_proc p
            JOIN pg_namespace n ON n.oid = p.pronamespace
            WHERE n.nspname = 'sm' AND p.proname = $1
         )",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("fn exists")
}

async fn is_security_definer(pool: &sqlx::PgPool, name: &str) -> bool {
    sql_query_scalar(
        "SELECT p.prosecdef FROM pg_proc p
          JOIN pg_namespace n ON n.oid = p.pronamespace
         WHERE n.nspname = 'sm' AND p.proname = $1",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("prosecdef")
}

#[tokio::test]
async fn migration_is_reversible() {
    let db = db_case!("sm_rev");
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("kernel");
    let boot = bootstrap_pool(db.database()).await;
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged");

    let migrator = sm_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    assert!(
        query_seam_fn_exists(db.migrate_pool(), "current_state").await,
        "0002 must create sm.current_state"
    );
    assert!(query_seam_fn_exists(db.migrate_pool(), "instance_exists").await);
    assert!(query_seam_fn_exists(db.migrate_pool(), "machine_id_for").await);
    assert!(
        query_seam_fn_exists(db.migrate_pool(), "spawn_instance").await,
        "0003 must create sm.spawn_instance"
    );
    assert!(query_seam_fn_exists(db.migrate_pool(), "transition_instance").await);
    assert!(query_seam_fn_exists(db.migrate_pool(), "instance_engine_guard").await);
    assert!(
        !is_security_definer(db.migrate_pool(), "current_state").await,
        "current_state must be invoker-rights"
    );
    assert!(!is_security_definer(db.migrate_pool(), "instance_exists").await);
    assert!(!is_security_definer(db.migrate_pool(), "machine_id_for").await);
    assert!(
        !is_security_definer(db.migrate_pool(), "spawn_instance").await,
        "spawn_instance must be invoker-rights"
    );
    assert!(!is_security_definer(db.migrate_pool(), "transition_instance").await);
    assert!(!is_security_definer(db.migrate_pool(), "instance_engine_guard").await);
    let app_exec: bool = sql_query_scalar(
        "SELECT has_function_privilege('datum_app', 'sm.current_state(text, uuid)', 'EXECUTE')",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("app execute");
    assert!(app_exec, "EXECUTE granted to datum_app");
    let public_exec: bool = sql_query_scalar(
        "SELECT has_function_privilege('public', 'sm.current_state(text, uuid)', 'EXECUTE')",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("public execute");
    assert!(!public_exec, "EXECUTE revoked from PUBLIC");
    let spawn_exec: bool = sql_query_scalar(
        "SELECT has_function_privilege('datum_app', 'sm.spawn_instance(text, uuid, uuid, text)', 'EXECUTE')",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("spawn execute");
    assert!(spawn_exec, "EXECUTE granted to datum_app on spawn_instance");
    let app_update: bool =
        sql_query_scalar("SELECT has_table_privilege('datum_app', 'sm.instance', 'UPDATE')")
            .fetch_one(db.migrate_pool())
            .await
            .expect("app update");
    assert!(!app_update, "UPDATE on sm.instance revoked from datum_app");
    let app_insert: bool =
        sql_query_scalar("SELECT has_table_privilege('datum_app', 'sm.instance', 'INSERT')")
            .fetch_one(db.migrate_pool())
            .await
            .expect("app insert");
    assert!(!app_insert, "INSERT on sm.instance revoked from datum_app");
    let app_select: bool =
        sql_query_scalar("SELECT has_table_privilege('datum_app', 'sm.instance', 'SELECT')")
            .fetch_one(db.migrate_pool())
            .await
            .expect("app select");
    assert!(app_select, "SELECT on sm.instance stays for the query seam");
    let owner_before: String = sql_query_scalar(
        r#"SELECT r.rolname::text
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
           JOIN pg_roles r ON r.oid = c.relowner
          WHERE n.nspname = 'sm' AND c.relname = 'instance'"#,
    )
    .fetch_one(db.app_pool())
    .await
    .expect("owner");

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = sql_query_scalar("SELECT to_regclass('sm.instance') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("gone");
    assert!(gone, "sm.instance must be dropped");
    assert!(
        !query_seam_fn_exists(db.migrate_pool(), "current_state").await,
        "0002 down must drop sm.current_state"
    );
    assert!(!query_seam_fn_exists(db.migrate_pool(), "instance_exists").await);
    assert!(!query_seam_fn_exists(db.migrate_pool(), "machine_id_for").await);
    assert!(
        !query_seam_fn_exists(db.migrate_pool(), "spawn_instance").await,
        "0003 down must drop sm.spawn_instance"
    );
    assert!(!query_seam_fn_exists(db.migrate_pool(), "transition_instance").await);

    migrator.run(db.migrate_pool()).await.expect("up again");
    assert!(query_seam_fn_exists(db.migrate_pool(), "current_state").await);
    assert!(query_seam_fn_exists(db.migrate_pool(), "spawn_instance").await);
    assert!(
        !is_security_definer(db.migrate_pool(), "machine_id_for").await,
        "machine_id_for must stay invoker-rights after re-up"
    );
    assert!(
        !is_security_definer(db.migrate_pool(), "transition_instance").await,
        "transition_instance must stay invoker-rights after re-up"
    );
    let app_update_after: bool =
        sql_query_scalar("SELECT has_table_privilege('datum_app', 'sm.instance', 'UPDATE')")
            .fetch_one(db.migrate_pool())
            .await
            .expect("app update after");
    assert!(
        !app_update_after,
        "UPDATE on sm.instance stays revoked after re-up"
    );
    let owner_after: String = sql_query_scalar(
        r#"SELECT r.rolname::text
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
           JOIN pg_roles r ON r.oid = c.relowner
          WHERE n.nspname = 'sm' AND c.relname = 'instance'"#,
    )
    .fetch_one(db.app_pool())
    .await
    .expect("owner after");
    assert_eq!(owner_before, owner_after);
    assert_eq!(owner_after, "datum_owner");

    boot.close().await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn catalogue_accepts_sm_schema() {
    let db = db_case!("sm_ddl");
    migrate_and_install(&db).await;
    datum_db::ddl::check(db.migrate_pool())
        .await
        .expect("ddl check");
    let class: String =
        sql_query_scalar("SELECT class FROM datum.schema_class WHERE nspname = 'sm'")
            .fetch_one(db.app_pool())
            .await
            .expect("class");
    assert_eq!(class, "app");
    let cols: Vec<(String, String)> = sql_query_as(
        r#"SELECT c.relname::text, a.attname::text
           FROM pg_attribute a
           JOIN pg_class c ON c.oid = a.attrelid
           JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE n.nspname = 'sm'
            AND a.attnum > 0 AND NOT a.attisdropped
          ORDER BY 1, 2"#,
    )
    .fetch_all(db.app_pool())
    .await
    .expect("cols");
    assert!(cols.iter().any(|(t, c)| t == "instance" && c == "state"));
    db.finish().await.expect("finish");
}
