//! Engine write seam: wicket_app cannot DML sm.instance; transitions still
//! succeed for both LOGIN roles through sm.spawn_instance / sm.transition_instance.
#![allow(missing_docs)]
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use sqlx::{query as sql_query, query_scalar as sql_query_scalar};
use wicket_core::{Identifier, NoPostings, NoSignatures, PostingSink};
use wicket_db::{Error as DbError, Tx, WritePool};
use wicket_statemachine::{DocRef, Engine, current_state};
use wicket_test::db_case;

use common::{
    actor_with_perm, draft_release_machine, instance_state, migrate_and_install, persist_spawn,
    system_ctx, write_pool, write_pool_migrate,
};

fn frozen_engine() -> (Engine, DocRef) {
    let mut eng = Engine::new();
    eng.register_machine(draft_release_machine(false))
        .expect("machine");
    eng.freeze().expect("freeze");
    let doc = DocRef {
        doc_type: "wo".into(),
        doc_id: Identifier::generate(),
    };
    (eng, doc)
}

async fn table_priv(pool: &sqlx::PgPool, role: &str, privs: &str) -> bool {
    sql_query_scalar("SELECT has_table_privilege($1, 'sm.instance', $2)")
        .bind(role)
        .bind(privs)
        .fetch_one(pool)
        .await
        .expect("has_table_privilege")
}

async fn release(eng: &Engine, write: &WritePool, doc: &DocRef) {
    let (_, ctx) = actor_with_perm(write, "wo.release", doc, "release").await;
    let mut tx = Tx::begin(write, &ctx).await.expect("begin release");
    let sink: Box<dyn PostingSink> = Box::new(NoPostings);
    eng.transition(&mut tx, sink, doc, "release", None, &NoSignatures, &ctx)
        .await
        .expect("transition");
    tx.commit().await.expect("commit release");
}

#[tokio::test]
async fn direct_update_instance_as_app_is_42501() {
    let db = db_case!("sm_wr_42501");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (eng, doc) = frozen_engine();
    let ctx = system_ctx("sm.write_seam");
    persist_spawn(&eng, &write, &ctx, &doc).await;

    assert!(
        table_priv(db.app_pool(), "wicket_app", "SELECT").await,
        "SELECT on sm.instance stays"
    );
    assert!(
        !table_priv(db.app_pool(), "wicket_app", "UPDATE").await,
        "UPDATE revoked from wicket_app"
    );
    assert!(
        !table_priv(db.app_pool(), "wicket_app", "INSERT").await,
        "INSERT revoked from wicket_app"
    );
    assert!(
        !table_priv(db.app_pool(), "wicket_app", "DELETE").await,
        "DELETE revoked from wicket_app"
    );

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let err = tx
        .execute(
            sql_query(
                r#"UPDATE sm.instance
                      SET state = 'Released',
                          version = version + 1,
                          entered_at = now()
                    WHERE doc_type = $1 AND doc_id = $2"#,
            )
            .bind(&doc.doc_type)
            .bind(doc.doc_id.as_uuid()),
        )
        .await
        .expect_err("direct UPDATE must fail");
    assert!(
        matches!(err, DbError::Refused(ref s) if s.as_str() == "42501"),
        "direct UPDATE sm.instance as wicket_app must be 42501, got {err:?}"
    );
    tx.rollback().await.ok();
    assert_eq!(
        instance_state(db.app_pool(), &doc).await.as_deref(),
        Some("Draft"),
        "forged UPDATE must not land"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn engine_transition_succeeds_for_app_and_migrate() {
    let db = db_case!("sm_wr_roles");
    migrate_and_install(&db).await;
    let (eng, _) = frozen_engine();
    for (label, write) in [
        ("wicket_app", write_pool(&db)),
        ("wicket_migrate", write_pool_migrate(&db)),
    ] {
        let doc = DocRef {
            doc_type: "wo".into(),
            doc_id: Identifier::generate(),
        };
        let ctx = system_ctx("sm.write_seam");
        persist_spawn(&eng, &write, &ctx, &doc).await;
        release(&eng, &write, &doc).await;
        assert_eq!(
            instance_state(db.app_pool(), &doc).await.as_deref(),
            Some("Released"),
            "{label} engine transition"
        );
        let mut tx = Tx::begin(&write, &ctx).await.expect("begin read");
        assert_eq!(
            current_state(&mut tx, &doc).await.unwrap().map(|s| s.0),
            Some("Released".into()),
            "{label} query seam follows the executor"
        );
        tx.commit().await.expect("commit read");
    }
    db.finish().await.expect("finish");
}
