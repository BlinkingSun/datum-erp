//! Reversible jobs migration.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_jobs::MIGRATOR;
use datum_test::db_case;
use sqlx::{migrate::Migrator, query_scalar};

use common::table_schema;

fn jobs_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_jobs");
    migrator
}

#[tokio::test]
async fn migration_is_reversible() {
    let db = db_case!("jobs_rev");
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("kernel");
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged");

    let migrator = jobs_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    assert_eq!(table_schema(db.migrate_pool(), "job").await, "transient");
    assert_eq!(
        table_schema(db.migrate_pool(), "schedule").await,
        "transient"
    );
    assert_eq!(table_schema(db.migrate_pool(), "run_log").await, "app");

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = query_scalar("SELECT to_regclass('transient.job') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("dropped");
    assert!(gone, "0001 down must drop transient.job");
    let gone_log: bool = query_scalar("SELECT to_regclass('app.run_log') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("dropped run_log");
    assert!(gone_log, "0001 down must drop app.run_log");

    migrator.run(db.migrate_pool()).await.expect("up again");
    assert_eq!(table_schema(db.migrate_pool(), "job").await, "transient");
    assert_eq!(table_schema(db.migrate_pool(), "run_log").await, "app");

    boot.close().await;
    db.finish().await.expect("finish");
}
