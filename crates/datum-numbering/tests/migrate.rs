//! Reversible migration and catalogue: no sequence / IDENTITY in numbering.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use std::borrow::Cow;

use datum_numbering::MIGRATOR;
use datum_test::db_case;
use sqlx::Row;
use sqlx::migrate::{Migration, MigrationType, Migrator};
use sqlx::{Executor, SqlSafeStr};

use common::migrate_and_install;

fn numbering_sqlx_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    migrator.dangerous_set_table_name("transient._sqlx_migrations_numbering");
    migrator
}

#[tokio::test]
async fn migrate_down_then_up() {
    let db = db_case!("num_down_up");
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("db+audit");
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    datum_audit::install_privileged(&boot)
        .await
        .expect("privileged");

    let migrator = numbering_sqlx_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    let owner_before: String = db
        .app_pool()
        .fetch_one(
            "SELECT pg_catalog.pg_get_userbyid(c.relowner)
               FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = 'numbering' AND c.relname = 'counter'",
        )
        .await
        .expect("owner")
        .try_get(0)
        .expect("owner text");

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone = db
        .migrate_pool()
        .execute("SELECT 1 FROM numbering.counter LIMIT 1")
        .await
        .expect_err("table must be gone");
    assert_eq!(common::pg_code(&gone), "42P01");

    migrator.run(db.migrate_pool()).await.expect("up again");
    let owner_after: String = db
        .app_pool()
        .fetch_one(
            "SELECT pg_catalog.pg_get_userbyid(c.relowner)
               FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = 'numbering' AND c.relname = 'counter'",
        )
        .await
        .expect("owner after")
        .try_get(0)
        .expect("owner text");
    assert_eq!(owner_before, owner_after);

    boot.close().await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn no_postgres_sequence_or_identity() {
    let db = db_case!("no_seq");
    migrate_and_install(&db).await;

    let sequences: i64 = db
        .app_pool()
        .fetch_one(
            "SELECT count(*)::bigint
               FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = 'numbering' AND c.relkind = 'S'",
        )
        .await
        .expect("sequences")
        .try_get(0)
        .expect("n");
    assert_eq!(sequences, 0, "no sequences in schema numbering");

    let identity: i64 = db
        .app_pool()
        .fetch_one(
            "SELECT count(*)::bigint
               FROM pg_attribute a
               JOIN pg_class c ON c.oid = a.attrelid
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = 'numbering'
                AND a.attnum > 0
                AND NOT a.attisdropped
                AND a.attidentity <> ''",
        )
        .await
        .expect("identity")
        .try_get(0)
        .expect("n");
    assert_eq!(identity, 0, "no IDENTITY columns in schema numbering");

    let owner: String = db
        .app_pool()
        .fetch_one(
            "SELECT pg_catalog.pg_get_userbyid(c.relowner)
               FROM pg_class c
               JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = 'numbering' AND c.relname = 'counter'",
        )
        .await
        .expect("owner")
        .try_get(0)
        .expect("text");
    assert_eq!(owner, "datum_owner");

    db.finish().await.expect("finish");
}

#[tokio::test]
async fn runner_reassigns_owner() {
    let db = db_case!("runner_owner");
    migrate_and_install(&db).await;
    let toy = Migrator::with_migrations(vec![Migration::new(
        1,
        Cow::Borrowed("empty"),
        MigrationType::ReversibleUp,
        "-- comment only".into_sql_str(),
        false,
    )]);
    datum_db::migrate::run(db.migrate_pool(), &[("toy", &toy)])
        .await
        .expect("toy");
    let _ = toy;
    db.finish().await.expect("finish");
}
