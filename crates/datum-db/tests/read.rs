//! ReadPool fetch surface: SELECT under both LOGIN roles; INSERT refused at GRANT.

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies
)]

mod common;

use datum_db::{Error, ReadPool};
use datum_test::db_case;

use common::app_url;

/// Probe table `datum_app` may SELECT and must not INSERT/UPDATE/DELETE.
async fn install_select_only_probe(db: &datum_test::TestDb) {
    sqlx::raw_sql(
        r#"
        CREATE TABLE app.read_probe (id int PRIMARY KEY, n int NOT NULL DEFAULT 0);
        REVOKE INSERT, UPDATE, DELETE ON app.read_probe FROM datum_app;
        INSERT INTO app.read_probe (id, n) VALUES (1, 7), (2, 9);
        "#,
    )
    .execute(db.migrate_pool())
    .await
    .expect("select-only probe");
}

#[tokio::test]
async fn read_pool_selects_under_app_and_migrate() {
    let db = db_case!("read_both_roles");
    install_select_only_probe(&db).await;

    let read_app = ReadPool::new(db.app_pool().clone());
    let read_mig = ReadPool::new(db.migrate_pool().clone());
    let connected = ReadPool::connect(&app_url(db.database()))
        .await
        .expect("ReadPool::connect");

    for (label, read) in [
        ("datum_app wrap", &read_app),
        ("datum_migrate wrap", &read_mig),
        ("datum_app connect", &connected),
    ] {
        let (n,): (i32,) = read
            .fetch_one(sqlx::query_as("SELECT n FROM app.read_probe WHERE id = 1"))
            .await
            .unwrap_or_else(|e| panic!("{label} fetch_one: {e}"));
        assert_eq!(n, 7, "{label}");

        let missing: Option<(i32,)> = read
            .fetch_optional(sqlx::query_as("SELECT n FROM app.read_probe WHERE id = 99"))
            .await
            .unwrap_or_else(|e| panic!("{label} fetch_optional: {e}"));
        assert!(missing.is_none(), "{label}");

        let rows: Vec<(i32,)> = read
            .fetch_all(sqlx::query_as("SELECT n FROM app.read_probe ORDER BY id"))
            .await
            .unwrap_or_else(|e| panic!("{label} fetch_all: {e}"));
        assert_eq!(rows, vec![(7,), (9,)], "{label}");

        let actor: (Option<String>,) = read
            .fetch_one(sqlx::query_as(
                "SELECT pg_catalog.current_setting('datum.actor_id', true)",
            ))
            .await
            .unwrap_or_else(|e| panic!("{label} actor_id: {e}"));
        assert!(
            actor.0.as_deref().unwrap_or("").is_empty(),
            "{label} must not bind actor: {:?}",
            actor.0
        );
        let _ = read.idle();
    }

    connected.as_pool().close().await;
    db.finish().await.expect("finish");
}

/// An INSERT smuggled through the fetch surface (`INSERT … RETURNING`) fails
/// at GRANT (`42501`) as `datum_app`. The probe row count is unchanged.
#[tokio::test]
async fn read_pool_insert_fails_at_grant() {
    let db = db_case!("read_no_insert");
    install_select_only_probe(&db).await;
    let read = ReadPool::new(db.app_pool().clone());

    let err = read
        .fetch_one::<(i32,)>(sqlx::query_as(
            "INSERT INTO app.read_probe (id, n) VALUES (3, 1) RETURNING id",
        ))
        .await
        .expect_err("INSERT via fetch must fail");
    assert!(
        matches!(err, Error::Refused(ref s) if s.as_str() == "42501"),
        "grant-level refuse, got {err}"
    );

    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM app.read_probe")
        .fetch_one(db.migrate_pool())
        .await
        .expect("count");
    assert_eq!(n, 2, "INSERT via ReadPool must persist nothing");
    db.finish().await.expect("finish");
}
