//! Reversible production_min migration (PLAN §6 invariant 8).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

use sqlx::{migrate::Migrator, query_scalar};
use wicket_module::{migrate_prefix, migrate_suffix};
use wicket_test::db_case;

fn production_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(
        wicket_mod_production_min::MIGRATOR
            .iter()
            .cloned()
            .collect(),
    );
    migrator.dangerous_set_table_name("transient._sqlx_migrations_production_min");
    migrator
}

#[tokio::test]
async fn reversible_migration_drops_production_min_schema() {
    let db = db_case!("prod_rev");
    migrate_prefix(db.migrate_pool()).await.expect("prefix");
    migrate_suffix(db.migrate_pool()).await.expect("suffix");
    let boot = db.bootstrap_pool().await.expect("boot");
    wicket_audit::install_privileged(&boot)
        .await
        .expect("privileged");
    boot.close().await;
    let migrator = production_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    let present: bool = query_scalar("SELECT to_regclass('production_min.work_order') IS NOT NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("present");
    assert!(present, "up must create production_min.work_order");

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = query_scalar("SELECT to_regclass('production_min.work_order') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("gone");
    assert!(gone, "0001 down must drop production_min.work_order");

    migrator.run(db.migrate_pool()).await.expect("up again");
    let again: bool = query_scalar("SELECT to_regclass('production_min.work_order') IS NOT NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("again");
    assert!(again, "second up must recreate production_min.work_order");

    db.finish().await.expect("finish");
}
