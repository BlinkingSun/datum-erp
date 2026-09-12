//! Reversible migration tests for `datum-mod-locations`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_docs
)]

mod common;

use datum_mod_locations::MIGRATOR;
use datum_test::db_case;
use sqlx::migrate::Migrator;

fn locations_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_locations");
    migrator
}

#[tokio::test]
async fn reversible_migration_drops_locations_schema() {
    let db = db_case!("loc_migrate_rev");
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
            ("datum-ledger", &datum_ledger::MIGRATOR),
        ],
    )
    .await
    .expect("kernel prefix");
    let migrator = locations_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    migrator.undo(db.migrate_pool(), 0).await.expect("down");
    let gone: bool = sqlx::query_scalar("SELECT to_regclass('locations.location') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("probe");
    assert!(gone);
    migrator.run(db.migrate_pool()).await.expect("up again");
    db.finish().await.expect("finish");
}
