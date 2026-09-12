//! Reversible `lots` migration.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_test::db_case;
use lots::MIGRATOR;
use sqlx::{migrate::Migrator, query_scalar};

fn lots_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_lots");
    migrator
}

#[tokio::test]
async fn reverse_migration_tested() {
    let db = db_case!("lots_rev");
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("kernel");
    let boot = db.bootstrap_pool().await.expect("bootstrap");
    datum_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;

    let migrator = lots_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    let present: bool = query_scalar("SELECT to_regclass('lots.lot') IS NOT NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("present");
    assert!(present, "up must create lots.lot");

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = query_scalar("SELECT to_regclass('lots.lot') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("dropped");
    assert!(gone, "0001 down must drop lots.lot");

    migrator.run(db.migrate_pool()).await.expect("up again");
    let again: bool = query_scalar("SELECT to_regclass('lots.lot') IS NOT NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("up again");
    assert!(again);
    db.finish().await.expect("finish");
}
