//! Reversible inventory migration (PLAN §6 invariant 8).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

use datum_module::{migrate_prefix, migrate_suffix};
use datum_test::db_case;
use sqlx::{migrate::Migrator, query_scalar};

fn inventory_migrator() -> Migrator {
    let mut migrator =
        Migrator::with_migrations(datum_mod_inventory::MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_inventory");
    migrator
}

#[tokio::test]
async fn reversible_migration_drops_inventory_schema() {
    let db = db_case!("inv_rev");
    migrate_prefix(db.migrate_pool()).await.expect("prefix");
    migrate_suffix(db.migrate_pool()).await.expect("suffix");
    let boot = db.bootstrap_pool().await.expect("boot");
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged");
    boot.close().await;
    let migrator = inventory_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    let present: bool = query_scalar("SELECT to_regclass('inventory.document') IS NOT NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("present");
    assert!(present, "up must create inventory.document");

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = query_scalar("SELECT to_regclass('inventory.document') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("gone");
    assert!(gone, "0001 down must drop inventory.document");

    migrator.run(db.migrate_pool()).await.expect("up again");
    let again: bool = query_scalar("SELECT to_regclass('inventory.document') IS NOT NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("again");
    assert!(again, "second up must recreate inventory.document");

    db.finish().await.expect("finish");
}
