//! Schema class catalogue for jobs tables.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use sqlx::postgres::PgPool;
use sqlx::query;
use wicket_test::db_case;

use common::{has_audit_trigger, migrate_and_install, pg_code, table_class, table_schema};

#[tokio::test]
async fn jobs_tables_match_schema_classes() {
    let db = db_case!("jobs_catalogue");
    migrate_and_install(&db).await;
    let pool: &PgPool = db.app_pool();

    assert_eq!(table_schema(pool, "job").await, "transient");
    assert_eq!(table_schema(pool, "schedule").await, "transient");
    assert_eq!(table_schema(pool, "run_log").await, "app");

    assert_eq!(table_class(pool, "job").await, "transient");
    assert_eq!(table_class(pool, "schedule").await, "transient");
    assert_eq!(table_class(pool, "run_log").await, "app");

    assert!(
        !has_audit_trigger(pool, "transient.job").await,
        "transient.job must not be audited"
    );
    assert!(
        !has_audit_trigger(pool, "transient.schedule").await,
        "transient.schedule must not be audited"
    );
    assert!(
        has_audit_trigger(pool, "app.run_log").await,
        "app.run_log must be audited"
    );

    query("DELETE FROM transient.job WHERE false")
        .execute(pool)
        .await
        .expect("wicket_app DELETE on transient.job");
    query("DELETE FROM transient.schedule WHERE false")
        .execute(pool)
        .await
        .expect("wicket_app DELETE on transient.schedule");

    let err = query("DELETE FROM app.run_log")
        .execute(pool)
        .await
        .expect_err("wicket_app must not DELETE app.run_log");
    assert_eq!(pg_code(&err), "42501");

    wicket_db::ddl::check(db.migrate_pool())
        .await
        .expect("catalogue lint");

    db.finish().await.expect("finish");
}
