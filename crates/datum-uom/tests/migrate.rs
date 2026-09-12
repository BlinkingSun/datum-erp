//! Reversible `datum-uom` migration (ADDENDUM 3 §4).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_test::db_case;
use datum_uom::MIGRATOR;
use sqlx::{migrate::Migrator, query_scalar};

fn uom_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_uom");
    migrator
}

async fn btree_gist_installed(pool: &sqlx::PgPool) -> bool {
    query_scalar("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'btree_gist')")
        .fetch_one(pool)
        .await
        .expect("btree_gist probe")
}

#[tokio::test]
async fn reversible_migration_drops_btree_gist() {
    let db = db_case!("uom_btree_gist_rev");
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("kernel");

    let migrator = uom_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    assert!(
        btree_gist_installed(db.migrate_pool()).await,
        "up must install btree_gist"
    );

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = query_scalar("SELECT to_regclass('uom.unit') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("uom dropped");
    assert!(gone, "0001 down must drop uom.unit");
    assert!(
        !btree_gist_installed(db.migrate_pool()).await,
        "0001 down must drop btree_gist on an isolated case database"
    );

    migrator.run(db.migrate_pool()).await.expect("up again");
    assert!(
        btree_gist_installed(db.migrate_pool()).await,
        "second up must recreate btree_gist"
    );

    db.finish().await.expect("finish");
}

#[tokio::test]
async fn migrate_down_then_up() {
    let db = db_case!("uom_down_up");
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("db+audit");

    let migrator = uom_migrator();
    migrator.run(db.migrate_pool()).await.expect("uom up");
    let shim_gone: bool =
        query_scalar("SELECT to_regprocedure('uom.item_has_postings(uuid)') IS NULL")
            .fetch_one(db.migrate_pool())
            .await
            .expect("shim probe");
    assert!(shim_gone, "0002 must drop uom.item_has_postings");

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = query_scalar("SELECT to_regclass('uom.unit') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("uom dropped");
    assert!(gone, "0001 down must drop uom.unit");

    migrator.run(db.migrate_pool()).await.expect("up again");
    let shim_still_gone: bool =
        query_scalar("SELECT to_regprocedure('uom.item_has_postings(uuid)') IS NULL")
            .fetch_one(db.migrate_pool())
            .await
            .expect("shim probe again");
    assert!(
        shim_still_gone,
        "0002 must still drop the shim after down-then-up"
    );

    db.finish().await.expect("finish");
}
