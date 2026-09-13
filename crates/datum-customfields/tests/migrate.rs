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

async fn has_zz_audit_row(pool: &sqlx::PgPool, rel: &str) -> bool {
    query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM pg_trigger t
             WHERE t.tgrelid = $1::regclass
               AND t.tgname = 'zz_audit_row'
               AND NOT t.tgisinternal
        )
        "#,
    )
    .bind(rel)
    .fetch_one(pool)
    .await
    .expect("zz_audit_row")
}

#[tokio::test]
async fn migrate_down_then_up() {
    for profile in common::PROFILES {
        let Some(db) = open_db("cf_mig", profile).await else {
            continue;
        };
        common::migrate_predecessors(&db).await;
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

#[tokio::test]
async fn migrates_at_canonical_position() {
    let Some(db) = open_db("cf_canon", "plain-shop").await else {
        return;
    };
    common::migrate(&db).await;
    assert!(
        has_zz_audit_row(db.migrate_pool(), "customfields.definition").await,
        "customfields.definition must carry zz_audit_row at canonical position"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn migrates_as_last_crate() {
    let Some(db) = open_db("cf_last", "plain-shop").await else {
        return;
    };
    common::migrate_as_last_crate(&db).await;
    assert!(
        has_zz_audit_row(db.migrate_pool(), "customfields.definition").await,
        "customfields.definition must carry zz_audit_row as last crate with audit_attach up"
    );
    db.finish().await.expect("finish");
}
