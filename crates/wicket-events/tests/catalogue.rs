//! Schema class catalogue and audit-trigger attachment.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use sqlx::{postgres::PgPool, query};
use wicket_test::db_case;

use common::{has_audit_trigger, migrate_and_install, pg_code, table_owner, table_schema};

#[tokio::test]
async fn every_event_table_is_audited_except_transient() {
    let db = db_case!("evt_audited");
    migrate_and_install(&db).await;
    let pool: &PgPool = db.app_pool();

    assert_eq!(table_schema(pool, "event").await, "app");
    assert_eq!(table_schema(pool, "subscription").await, "app");
    assert_eq!(table_schema(pool, "delivery").await, "transient");

    assert!(
        has_audit_trigger(pool, "app.event").await,
        "app.event must carry zz_audit_* (event trigger on CREATE TABLE)"
    );
    assert!(
        has_audit_trigger(pool, "app.subscription").await,
        "app.subscription must carry zz_audit_*"
    );
    assert!(
        !has_audit_trigger(pool, "transient.delivery").await,
        "transient.delivery must not be audited"
    );

    assert_eq!(
        table_owner(db.migrate_pool(), "app.event").await,
        "wicket_owner"
    );
    assert_eq!(
        table_owner(db.migrate_pool(), "app.subscription").await,
        "wicket_owner"
    );
    assert_eq!(
        table_owner(db.migrate_pool(), "transient.delivery").await,
        "wicket_owner"
    );

    wicket_db::ddl::check(db.migrate_pool())
        .await
        .expect("catalogue lint");

    db.finish().await.expect("finish");
}

#[tokio::test]
async fn app_event_has_no_delete_path() {
    let db = db_case!("evt_nodelete");
    migrate_and_install(&db).await;

    let err = query("DELETE FROM app.event")
        .execute(db.app_pool())
        .await
        .expect_err("wicket_app must not DELETE app.event");
    assert_eq!(pg_code(&err), "42501");

    db.finish().await.expect("finish");
}
