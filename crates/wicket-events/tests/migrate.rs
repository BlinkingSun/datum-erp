//! Reversible events migration.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use sqlx::{migrate::Migrator, query_scalar};
use wicket_events::MIGRATOR;
use wicket_test::db_case;

use common::table_schema;

fn events_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_events");
    migrator
}

#[tokio::test]
async fn migration_is_reversible() {
    let db = db_case!("evt_rev");
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-db", &wicket_db::MIGRATOR),
            ("wicket-audit", &wicket_audit::MIGRATOR),
        ],
    )
    .await
    .expect("kernel");
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    wicket_audit::install_privileged(&boot)
        .await
        .expect("privileged");

    let migrator = events_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    assert_eq!(table_schema(db.migrate_pool(), "event").await, "app");
    assert_eq!(
        table_schema(db.migrate_pool(), "delivery").await,
        "transient"
    );

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = query_scalar("SELECT to_regclass('app.event') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("dropped");
    assert!(gone, "0001 down must drop app.event");
    let gone_d: bool = query_scalar("SELECT to_regclass('transient.delivery') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("dropped delivery");
    assert!(gone_d);

    migrator.run(db.migrate_pool()).await.expect("up again");
    assert_eq!(table_schema(db.migrate_pool(), "event").await, "app");
    assert_eq!(
        table_schema(db.migrate_pool(), "delivery").await,
        "transient"
    );

    boot.close().await;
    db.finish().await.expect("finish");
}
