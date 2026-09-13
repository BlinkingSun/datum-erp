//! Reversible migration tests.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_customfields::MIGRATOR;
use sqlx::{migrate::Migrator, query_scalar};

use common::open_db;

fn cf_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_customfields");
    migrator
}

#[tokio::test]
async fn migrate_down_then_up() {
    for profile in common::PROFILES {
        let Some(db) = open_db("cf_mig", profile).await else {
            continue;
        };
        datum_db::migrate::run(
            db.migrate_pool(),
            &[
                ("datum-db", &datum_db::MIGRATOR),
                ("datum-audit", &datum_audit::MIGRATOR),
            ],
        )
        .await
        .expect("kernel");
        let migrator = cf_migrator();
        migrator.run(db.migrate_pool()).await.expect("up");
        migrator.undo(db.migrate_pool(), 0).await.expect("down");
        let gone: bool = query_scalar("SELECT to_regclass('customfields.definition') IS NULL")
            .fetch_one(db.migrate_pool())
            .await
            .expect("probe");
        assert!(gone);
        migrator.run(db.migrate_pool()).await.expect("up again");
        db.finish().await.expect("finish");
    }
}
