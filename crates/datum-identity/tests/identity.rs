//! Named identity tests (SPEC).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use chrono::{Duration as ChronoDuration, Utc};
use datum_core::{Actor, ActorKind, Identifier};
use datum_db::Tx;
use datum_identity::rbac::{assign_role, has_permission, load_bundles, seed_bundles};
use datum_identity::{
    ARGON2ID_M_KIB, ARGON2ID_P, ARGON2ID_T, CredentialKind, LOCKOUT_AFTER, PrincipalKind,
    SYSTEM_ID, complete_reset, create_principal, deactivate_principal, hash_password_with_params,
    load_principal, login, rename_principal, request_reset, set_login_credential,
    set_signing_credential, verify_login_secret, verify_password, verify_signing,
};
use datum_test::db_case;
use serde_json::Value;
use sqlx::{query as sql_query, query_as as sql_query_as, query_scalar as sql_query_scalar};
use tokio::time::{Duration, sleep};

use common::{
    count_audit, has_zz_audit, migrate_identity, pg_code, system_ctx, table_owner, user_ctx,
    write_pool,
};

#[tokio::test]
async fn principal_cannot_be_deleted() {
    let db = db_case!("id_no_del");
    migrate_identity(&db).await;
    let err = sql_query("DELETE FROM identity.principal WHERE username = 'system'")
        .execute(db.app_pool())
        .await
        .expect_err("delete must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn username_is_never_reused() {
    let db = db_case!("id_reuse");
    migrate_identity(&db).await;
    let write = write_pool(&db).await;
    let ctx = system_ctx("identity.create");
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let p = create_principal(&mut tx, PrincipalKind::User, "ada", "Ada")
        .await
        .expect("create");
    tx.commit().await.expect("commit");

    let mut tx = Tx::begin(&write, &system_ctx("identity.deactivate"))
        .await
        .expect("begin2");
    deactivate_principal(&mut tx, p.id).await.expect("deact");
    tx.commit().await.expect("commit2");

    let history: i64 = sql_query_scalar(
        "SELECT count(*) FROM identity.username_history WHERE lower(username) = 'ada'",
    )
    .fetch_one(db.app_pool())
    .await
    .expect("history");
    assert!(history >= 1, "history row present");

    let mut tx = Tx::begin(&write, &system_ctx("identity.create"))
        .await
        .expect("begin3");
    let err = create_principal(&mut tx, PrincipalKind::User, "ada", "Ada 2")
        .await
        .expect_err("reuse");
    assert!(
        matches!(err, datum_identity::Error::UsernameReused),
        "got {err:?}"
    );
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn display_name_at_returns_name_as_of() {
    let db = db_case!("id_name_at");
    migrate_identity(&db).await;
    let write = write_pool(&db).await;
    let mut tx = Tx::begin(&write, &system_ctx("identity.create"))
        .await
        .expect("begin");
    let p = create_principal(&mut tx, PrincipalKind::User, "grace", "Grace Hopper")
        .await
        .expect("create");
    tx.commit().await.expect("commit");
    let t0 = Utc::now();
    sleep(Duration::from_millis(20)).await;

    let mut tx = Tx::begin(&write, &system_ctx("identity.rename"))
        .await
        .expect("begin2");
    rename_principal(&mut tx, p.id, "Rear Admiral Hopper")
        .await
        .expect("rename");
    tx.commit().await.expect("commit2");
    sleep(Duration::from_millis(20)).await;
    let t1 = Utc::now();

    let loaded = load_principal(db.app_pool(), p.id).await.expect("load");
    let as_of_old = loaded
        .display_name_at(db.app_pool(), t0)
        .await
        .expect("old");
    let as_of_new = loaded
        .display_name_at(db.app_pool(), t1)
        .await
        .expect("new");
    assert_eq!(as_of_old, "Grace Hopper");
    assert_eq!(as_of_new, "Rear Admiral Hopper");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn signing_credential_is_separate_from_login() {
    let db = db_case!("id_two_cred");
    migrate_identity(&db).await;
    let write = write_pool(&db).await;
    let mut tx = Tx::begin(&write, &system_ctx("identity.create"))
        .await
        .expect("begin");
    let p = create_principal(&mut tx, PrincipalKind::User, "alice", "Alice")
        .await
        .expect("create");
    set_login_credential(&mut tx, p.id, "login-secret-ok")
        .await
        .expect("login cred");
    set_signing_credential(&mut tx, p.id, "signing-secret-ok")
        .await
        .expect("sign cred");
    tx.commit().await.expect("commit");

    let mut tx = Tx::begin(&write, &system_ctx("identity.verify"))
        .await
        .expect("begin2");
    assert!(
        verify_login_secret(&mut tx, p.id, "login-secret-ok")
            .await
            .expect("vl")
    );
    assert!(
        !verify_login_secret(&mut tx, p.id, "signing-secret-ok")
            .await
            .expect("vl2")
    );
    assert!(
        verify_signing(&mut tx, p.id, "signing-secret-ok")
            .await
            .expect("vs")
    );
    assert!(
        !verify_signing(&mut tx, p.id, "login-secret-ok")
            .await
            .expect("vs2")
    );
    tx.commit().await.expect("commit2");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn reset_requires_two_principals() {
    let db = db_case!("id_reset");
    migrate_identity(&db).await;
    let write = write_pool(&db).await;
    let mut tx = Tx::begin(&write, &system_ctx("identity.create"))
        .await
        .expect("begin");
    let admin = create_principal(&mut tx, PrincipalKind::User, "admin", "Admin")
        .await
        .expect("admin");
    let subject = create_principal(&mut tx, PrincipalKind::User, "bob", "Bob")
        .await
        .expect("bob");
    set_login_credential(&mut tx, subject.id, "old-password")
        .await
        .expect("cred");
    tx.commit().await.expect("commit");

    let mut tx = Tx::begin(&write, &user_ctx(subject.id.as_uuid(), "identity.reset"))
        .await
        .expect("self-request begin");
    let err = request_reset(&mut tx, subject.actor(), subject.id, CredentialKind::Login)
        .await
        .expect_err("actor == subject");
    assert!(
        matches!(err, datum_identity::Error::ResetRequiresTwoPrincipals),
        "got {err:?}"
    );
    tx.rollback().await.expect("self-request rollback");

    let admin_actor = admin.actor();
    let mut tx = Tx::begin(&write, &user_ctx(admin.id.as_uuid(), "identity.reset"))
        .await
        .expect("begin2");
    let token = request_reset(&mut tx, admin_actor, subject.id, CredentialKind::Login)
        .await
        .expect("request");
    let err = complete_reset(&mut tx, admin_actor, &token, "new-password")
        .await
        .expect_err("same actor");
    assert!(
        matches!(err, datum_identity::Error::ResetRequiresTwoPrincipals),
        "got {err:?}"
    );
    tx.commit().await.expect("commit2");

    let mut tx = Tx::begin(&write, &user_ctx(subject.id.as_uuid(), "identity.reset"))
        .await
        .expect("begin3");
    complete_reset(&mut tx, subject.actor(), &token, "new-password")
        .await
        .expect("subject completes");
    tx.commit().await.expect("commit3");

    let mut tx = Tx::begin(&write, &system_ctx("identity.verify"))
        .await
        .expect("begin4");
    assert!(
        verify_login_secret(&mut tx, subject.id, "new-password")
            .await
            .expect("new")
    );
    tx.commit().await.expect("commit4");
    db.finish().await.expect("finish");
}

#[test]
fn argon2id_parameters_are_pinned_and_tested() {
    assert_eq!(ARGON2ID_M_KIB, 19_456);
    assert_eq!(ARGON2ID_T, 2);
    assert_eq!(ARGON2ID_P, 1);
    let salt = b"datum-identity16";
    let phc = hash_password_with_params(b"password", salt, ARGON2ID_M_KIB, ARGON2ID_T, ARGON2ID_P)
        .expect("hash");
    assert!(phc.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
    assert!(verify_password(b"password", &phc).expect("ok"));
    assert!(!verify_password(b"wrong", &phc).expect("no"));
}

#[tokio::test]
async fn lockout_after_n_failures() {
    let db = db_case!("id_lock");
    migrate_identity(&db).await;
    let write = write_pool(&db).await;
    let mut tx = Tx::begin(&write, &system_ctx("identity.create"))
        .await
        .expect("begin");
    let p = create_principal(&mut tx, PrincipalKind::User, "lockme", "Lock")
        .await
        .expect("create");
    set_login_credential(&mut tx, p.id, "correct-horse")
        .await
        .expect("cred");
    tx.commit().await.expect("commit");

    for i in 0..LOCKOUT_AFTER {
        let mut tx = Tx::begin(&write, &system_ctx("identity.login"))
            .await
            .expect("begin fail");
        let err = login(&mut tx, "lockme", "wrong", None, None)
            .await
            .expect_err("fail");
        if i + 1 < LOCKOUT_AFTER {
            assert!(
                matches!(err, datum_identity::Error::InvalidCredentials),
                "i={i} {err:?}"
            );
        } else {
            assert!(
                matches!(err, datum_identity::Error::Lockout { .. }),
                "i={i} {err:?}"
            );
        }
        tx.commit().await.expect("commit fail");
    }

    let mut tx = Tx::begin(&write, &system_ctx("identity.login"))
        .await
        .expect("begin locked");
    let err = login(&mut tx, "lockme", "correct-horse", None, None)
        .await
        .expect_err("still locked");
    assert!(
        matches!(err, datum_identity::Error::Lockout { .. }),
        "got {err:?}"
    );
    tx.commit().await.expect("commit locked");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn has_permission_via_role_bundle() {
    let db = db_case!("id_rbac");
    migrate_identity(&db).await;
    let write = write_pool(&db).await;
    let fixture = include_str!("../fixtures/roles.toml");
    let bundles = load_bundles(fixture).expect("toml");
    let mut tx = Tx::begin(&write, &system_ctx("identity.rbac"))
        .await
        .expect("begin");
    let p = create_principal(&mut tx, PrincipalKind::User, "op", "Operator")
        .await
        .expect("create");
    let ids = seed_bundles(&mut tx, &bundles).await.expect("seed");
    let operator = ids[0];
    assign_role(&mut tx, p.id, operator).await.expect("assign");
    let actor = Actor {
        id: p.id.0,
        kind: p.kind,
    };
    assert!(
        has_permission(&mut tx, actor, "calibration.view")
            .await
            .expect("view")
    );
    assert!(
        !has_permission(&mut tx, actor, "calibration.approve")
            .await
            .expect("approve")
    );
    tx.commit().await.expect("commit");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn audit_fk_validates_existing_rows() {
    let db = db_case!("id_fk");
    migrate_identity(&db).await;
    let row: (bool, bool) = sql_query_as(
        r#"SELECT convalidated, conrelid = 'audit.event'::regclass
           FROM pg_constraint WHERE conname = 'event_actor_fk'"#,
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("event_actor_fk");
    assert!(row.0 && row.1, "event_actor_fk must exist and be VALID");
    let orphans: i64 = sql_query_scalar(
        r#"SELECT count(*) FROM audit.event e
           WHERE NOT EXISTS (
             SELECT 1 FROM identity.principal p WHERE p.id = e.actor_id
           )"#,
    )
    .fetch_one(db.app_pool())
    .await
    .expect("orphans");
    assert_eq!(orphans, 0, "every audit.event.actor_id is a principal");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn every_identity_table_is_audited() {
    let db = db_case!("id_audited");
    migrate_identity(&db).await;
    let tables = [
        "principal",
        "username_history",
        "display_name_history",
        "login_credential",
        "signing_credential",
        "credential_reset",
        "role",
        "role_permission",
        "principal_role",
    ];
    for table in tables {
        assert!(
            has_zz_audit(db.migrate_pool(), "identity", table).await,
            "{table} missing zz_audit_row"
        );
        assert_eq!(
            table_owner(db.migrate_pool(), "identity", table).await,
            "datum_owner",
            "{table} owner"
        );
    }
    assert!(
        !has_zz_audit(db.migrate_pool(), "transient", "session").await,
        "transient.session must not be audited"
    );

    let write = write_pool(&db).await;
    let mut tx = Tx::begin(&write, &system_ctx("identity.create"))
        .await
        .expect("begin");
    let p = create_principal(&mut tx, PrincipalKind::User, "audited", "Audited")
        .await
        .expect("create");
    set_login_credential(&mut tx, p.id, "secret-one")
        .await
        .expect("login");
    set_signing_credential(&mut tx, p.id, "secret-two")
        .await
        .expect("sign");
    let bundles = load_bundles(include_str!("../fixtures/roles.toml")).expect("toml");
    let ids = seed_bundles(&mut tx, &bundles).await.expect("seed");
    assign_role(&mut tx, p.id, ids[0]).await.expect("assign");
    let sys = Actor {
        id: Identifier::from_uuid(SYSTEM_ID),
        kind: ActorKind::ServicePrincipal,
    };
    let token = request_reset(&mut tx, sys, p.id, CredentialKind::Login)
        .await
        .expect("reset request");
    let _ = token;
    tx.commit().await.expect("commit");

    for table in [
        "principal",
        "username_history",
        "display_name_history",
        "login_credential",
        "signing_credential",
        "role",
        "role_permission",
        "principal_role",
        "credential_reset",
    ] {
        let n = count_audit(db.app_pool(), table).await;
        assert!(n >= 1, "{table} has no audit row (n={n})");
    }
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn display_name_at_before_first_history_row() {
    let db = db_case!("id_name_before");
    migrate_identity(&db).await;
    let write = write_pool(&db).await;
    let mut tx = Tx::begin(&write, &system_ctx("identity.create"))
        .await
        .expect("begin");
    let p = create_principal(&mut tx, PrincipalKind::User, "lovelace", "Ada Lovelace")
        .await
        .expect("create");
    tx.commit().await.expect("commit");

    let mut tx = Tx::begin(&write, &system_ctx("identity.rename"))
        .await
        .expect("begin2");
    rename_principal(&mut tx, p.id, "Countess Lovelace")
        .await
        .expect("rename");
    tx.commit().await.expect("commit2");

    let loaded = load_principal(db.app_pool(), p.id).await.expect("load");
    assert_eq!(loaded.display_name, "Countess Lovelace");
    let before = loaded.created_at - ChronoDuration::seconds(60);
    let as_of = loaded
        .display_name_at(db.app_pool(), before)
        .await
        .expect("t < t0");
    assert_eq!(
        as_of, "Ada Lovelace",
        "t < t0 must return the first recorded name, not the live name"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = db_case!("id_raw_tx");
    migrate_identity(&db).await;
    let before: i64 = sql_query_scalar("SELECT count(*) FROM identity.principal")
        .fetch_one(db.app_pool())
        .await
        .expect("before");
    let err = sql_query(
        r#"INSERT INTO identity.principal
               (id, kind, username, display_name, status)
           VALUES (gen_random_uuid(), 'user', 'raw-write', 'Raw', 'active')"#,
    )
    .execute(db.app_pool())
    .await
    .expect_err("raw write must fail");
    assert_eq!(pg_code(&err), "42501", "err={err}");
    let after: i64 = sql_query_scalar("SELECT count(*) FROM identity.principal")
        .fetch_one(db.app_pool())
        .await
        .expect("after");
    assert_eq!(before, after, "table must be unchanged");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn login_records_device_and_ip() {
    let db = db_case!("id_login_dev");
    migrate_identity(&db).await;
    let write = write_pool(&db).await;
    let mut tx = Tx::begin(&write, &system_ctx("identity.create"))
        .await
        .expect("begin");
    let p = create_principal(&mut tx, PrincipalKind::User, "logme", "Log Me")
        .await
        .expect("create");
    set_login_credential(&mut tx, p.id, "correct-horse")
        .await
        .expect("cred");
    tx.commit().await.expect("commit");

    let mut tx = Tx::begin(&write, &system_ctx("identity.login"))
        .await
        .expect("begin login");
    let session = login(
        &mut tx,
        "logme",
        "correct-horse",
        Some("workstation-1"),
        Some("127.0.0.1"),
    )
    .await
    .expect("login");
    tx.commit().await.expect("commit login");

    let row: (Option<String>, Option<String>) =
        sql_query_as("SELECT device, host(ip)::text FROM transient.session WHERE id = $1")
            .bind(session.id)
            .fetch_one(db.app_pool())
            .await
            .expect("session row");
    assert_eq!(row.0.as_deref(), Some("workstation-1"));
    assert_eq!(row.1.as_deref(), Some("127.0.0.1"));
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn hash_columns_are_redacted_in_audit() {
    let db = db_case!("id_redact");
    migrate_identity(&db).await;
    let write = write_pool(&db).await;
    let mut tx = Tx::begin(&write, &system_ctx("identity.create"))
        .await
        .expect("begin");
    let p = create_principal(&mut tx, PrincipalKind::User, "redactme", "Redact")
        .await
        .expect("create");
    set_login_credential(&mut tx, p.id, "super-secret")
        .await
        .expect("login");
    set_signing_credential(&mut tx, p.id, "other-secret")
        .await
        .expect("sign");
    tx.commit().await.expect("commit");

    let rows: Vec<(String, Value)> = sql_query_as(
        r#"SELECT table_name::text, new_row
             FROM audit.event
            WHERE table_name IN ('login_credential', 'signing_credential')
              AND op = 'INSERT'"#,
    )
    .fetch_all(db.app_pool())
    .await
    .expect("rows");
    assert!(!rows.is_empty());
    for (table, new_row) in rows {
        assert_eq!(
            new_row.get("hash").and_then(Value::as_str),
            Some("[redacted]"),
            "{table} hash not redacted: {new_row}"
        );
        assert!(
            !new_row
                .get("hash")
                .and_then(Value::as_str)
                .unwrap_or("")
                .contains("argon2"),
            "{table} leaked PHC"
        );
    }
    db.finish().await.expect("finish");
}
