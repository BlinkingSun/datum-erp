//! Separate-connection `audit.log_event` path.

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies
)]

mod common;

use datum_db::security::{UnattributableWrite, log_unattributable_write};
use datum_db::{Error, WritePool};
use datum_test::db_case;

#[tokio::test]
async fn log_unattributable_write_unimplemented() {
    let db = db_case!("sec_unimpl");
    let write = WritePool::new(db.app_pool().clone());
    let err = log_unattributable_write(
        &write,
        &UnattributableWrite {
            action: Some("test.write".into()),
            reason: None,
            doc_type: None,
            doc_id: None,
            esign_id: None,
            detail: r#"{"sqlstate":"42501"}"#.into(),
        },
    )
    .await
    .expect_err("missing audit.log_event");
    assert!(matches!(err, Error::Unimplemented), "got {err}");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn log_unattributable_write_calls_audit_log_event() {
    let db = db_case!("sec_stub");
    sqlx::raw_sql(
        r#"
        CREATE TABLE transient.security_probe (
            kind text NOT NULL,
            action text,
            detail jsonb
        );
        CREATE FUNCTION audit.log_event(
            kind text,
            action text,
            reason text,
            doc_type text,
            doc_id text,
            esign_id text,
            detail jsonb
        ) RETURNS void
        LANGUAGE plpgsql
        AS $$
        BEGIN
            INSERT INTO transient.security_probe (kind, action, detail)
            VALUES (kind, action, detail);
        END $$;
        GRANT EXECUTE ON FUNCTION audit.log_event(text, text, text, text, text, text, jsonb)
            TO datum_app;
        "#,
    )
    .execute(db.migrate_pool())
    .await
    .expect("stub audit.log_event");

    let write = WritePool::new(db.app_pool().clone());
    log_unattributable_write(
        &write,
        &UnattributableWrite {
            action: Some("test.write".into()),
            reason: None,
            doc_type: None,
            doc_id: None,
            esign_id: None,
            detail: r#"{"sqlstate":"42501","request_id":"r1"}"#.into(),
        },
    )
    .await
    .expect("stub call");

    let (kind, action): (String, String) =
        sqlx::query_as("SELECT kind, action FROM transient.security_probe")
            .fetch_one(db.migrate_pool())
            .await
            .expect("probe row");
    assert_eq!(kind, "security.unattributable_write");
    assert_eq!(action, "test.write");
    db.finish().await.expect("finish");
}
