//! Reversible migration tests for `datum-mod-locations`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_docs
)]

use datum_mod_locations::MIGRATOR;
use datum_module::{migrate_prefix, migrate_suffix};
use datum_test::db_case;
use sqlx::migrate::Migrator;
use sqlx::query_scalar;

fn locations_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_locations");
    migrator
}

async fn install_kernel_trigger_up(db: &datum_test::TestDb) {
    migrate_prefix(db.migrate_pool()).await.expect("prefix");
    migrate_suffix(db.migrate_pool()).await.expect("suffix");
    let boot = db.bootstrap_pool().await.expect("bootstrap");
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged");
    boot.close().await;
}

#[tokio::test]
async fn reversible_migration_drops_locations_schema() {
    let db = db_case!("loc_migrate_rev");
    install_kernel_trigger_up(&db).await;
    let migrator = locations_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    migrator.undo(db.migrate_pool(), 0).await.expect("down");
    let gone: bool = query_scalar("SELECT to_regclass('locations.location') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("probe");
    assert!(gone);
    migrator.run(db.migrate_pool()).await.expect("up again");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn migrate_down_then_up() {
    let db = db_case!("loc_down_up");
    install_kernel_trigger_up(&db).await;
    let migrator = locations_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    let present: bool = query_scalar("SELECT to_regclass('locations.site') IS NOT NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("present");
    assert!(present, "up must create locations.site");

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = query_scalar("SELECT to_regclass('locations.site') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("gone");
    assert!(gone, "0001 down must drop locations.site");

    migrator.run(db.migrate_pool()).await.expect("up again");
    let again: bool = query_scalar("SELECT to_regclass('locations.site') IS NOT NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("again");
    assert!(again, "second up must recreate locations.site");

    db.finish().await.expect("finish");
}
