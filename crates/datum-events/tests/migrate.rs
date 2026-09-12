//! Reversible events migration.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_events::MIGRATOR;
use datum_test::db_case;
use sqlx::{migrate::Migrator, query_scalar};

use common::{bootstrap_pool, grant_create_on_database, table_schema};

fn events_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_events");
    migrator
}

#[tokio::test]
async fn migration_is_reversible() {
    let db = db_case!("evt_rev");
    grant_create_on_database(db.database()).await;
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
