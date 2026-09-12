//! Reversible items migration (PLAN §6 invariant 8).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

use datum_mod_items::MIGRATOR;
use datum_module::{migrate_prefix, migrate_suffix};
use datum_test::db_case;
use sqlx::{migrate::Migrator, query_scalar};

fn items_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_items");
    migrator
}

#[tokio::test]
async fn reversible_migration_drops_items_schema() {
    let db = db_case!("items_rev");
    migrate_prefix(db.migrate_pool()).await.expect("prefix");
    migrate_suffix(db.migrate_pool()).await.expect("suffix");
    let boot = db.bootstrap_pool().await.expect("boot");
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged");
    boot.close().await;
    let migrator = items_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    let present: bool = query_scalar("SELECT to_regclass('items.item') IS NOT NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("present");
    assert!(present, "up must create items.item");

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = query_scalar("SELECT to_regclass('items.item') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("gone");
    assert!(gone, "0001 down must drop items.item");

    migrator.run(db.migrate_pool()).await.expect("up again");
    let again: bool = query_scalar("SELECT to_regclass('items.item') IS NOT NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("again");
    assert!(again, "second up must recreate items.item");

    db.finish().await.expect("finish");
}

async fn item_has_postings_exists(pool: &sqlx::PgPool) -> bool {
    query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pg_proc p
            JOIN pg_namespace n ON n.oid = p.pronamespace
            WHERE n.nspname = 'items' AND p.proname = 'item_has_postings'
         )",
    )
    .fetch_one(pool)
    .await
    .expect("fn exists")
}

#[tokio::test]
async fn migrate_down_then_up() {
    let db = db_case!("items_down_up");
    migrate_prefix(db.migrate_pool()).await.expect("prefix");
    migrate_suffix(db.migrate_pool()).await.expect("suffix");
    let boot = db.bootstrap_pool().await.expect("boot");
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged");
    boot.close().await;
    let migrator = items_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    let present: bool = query_scalar("SELECT to_regclass('items.item') IS NOT NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("present");
    assert!(present, "up must create items.item");
    assert!(
        !item_has_postings_exists(db.migrate_pool()).await,
        "0002 must drop items.item_has_postings"
    );

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = query_scalar("SELECT to_regclass('items.item') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("gone");
    assert!(gone, "down must drop items.item");
    assert!(
        !item_has_postings_exists(db.migrate_pool()).await,
        "down must not recreate items.item_has_postings"
    );

    migrator.run(db.migrate_pool()).await.expect("up again");
    let again: bool = query_scalar("SELECT to_regclass('items.item') IS NOT NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("again");
    assert!(again, "second up must recreate items.item");
    assert!(
        !item_has_postings_exists(db.migrate_pool()).await,
        "second up must leave items.item_has_postings dropped"
    );

    db.finish().await.expect("finish");
}
