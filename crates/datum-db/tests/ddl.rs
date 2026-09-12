//! DDL catalogue lints.

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies
)]

mod common;

use datum_db::{Error, ddl};
use datum_test::db_case;

#[tokio::test]
async fn ddl_check_rejects_cascade() {
    let db = db_case!("ddl_cascade");
    sqlx::raw_sql(
        r#"
        CREATE TABLE app.ddl_parent (id int PRIMARY KEY);
        CREATE TABLE app.ddl_child (
            id int PRIMARY KEY,
            parent_id int REFERENCES app.ddl_parent(id) ON DELETE CASCADE
        );
        "#,
    )
    .execute(db.migrate_pool())
    .await
    .expect("offending fk");
    let err = ddl::check(db.migrate_pool())
        .await
        .expect_err("cascade must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("CASCADE") && msg.contains("ddl_child"),
        "err={msg}"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn ddl_check_rejects_app_delete_outside_transient() {
    let db = db_case!("ddl_delete");
    sqlx::raw_sql(
        r#"
        CREATE TABLE app.ddl_delete_probe (id int PRIMARY KEY);
        GRANT DELETE ON app.ddl_delete_probe TO datum_app;
        "#,
    )
    .execute(db.migrate_pool())
    .await
    .expect("offending grant");
    let err = ddl::check(db.migrate_pool())
        .await
        .expect_err("DELETE grant must fail");
    match err {
        Error::Ddl(v) => {
            let joined = v.join("\n");
            assert!(
                joined.contains("ddl_delete_probe") && joined.contains("DELETE"),
                "violations={joined}"
            );
        }
        other => panic!("expected Error::Ddl, got {other}"),
    }
    db.finish().await.expect("finish");
}
