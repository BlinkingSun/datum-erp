//! Named commit-mode tests (SPEC). Both `plain-shop` and `regulated-device`.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use chrono::{Duration, Utc};
use datum_core::{Identifier, NoSignatures};
use datum_db::Tx;
use datum_documents::{
    BlobStore, Error, Manifest, Status, attach, create, document_machine, effective_at, history,
    link, load, new_revision, set_legal_hold, transition, transition_context, verify_blob,
};
use serde_json::json;
use sqlx::{query, query_scalar};

use common::{
    AcceptingGate, PROFILES, actor_with_docs, blob_store, db_sqlstate, dummy_token, frozen_engine,
    migrate, open_db, persist_engine, write_ctx, write_pool,
};

async fn for_each_profile<F, Fut>(f: F)
where
    F: Fn(&'static str) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    for profile in PROFILES {
        f(profile).await;
    }
}

#[tokio::test]
async fn number_is_gap_free_after_rollback() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_gap", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let first = create(&mut tx, &eng, "SOP", "Work instruction", "quality")
            .await
            .unwrap();
        let n1 = load(&mut tx, first).await.unwrap().number;
        tx.rollback().await.unwrap();

        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let second = create(&mut tx, &eng, "SOP", "Work instruction", "quality")
            .await
            .unwrap();
        let n2 = load(&mut tx, second).await.unwrap().number;
        tx.commit().await.unwrap();
        assert_eq!(n1, n2, "rolled-back create must not consume the number");
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn void_keeps_number() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_void", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let (_, base) = actor_with_docs(&write, profile).await;
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let id = create(&mut tx, &eng, "SOP", "Void me", "quality")
            .await
            .unwrap();
        let number = load(&mut tx, id).await.unwrap().number.clone();
        tx.commit().await.unwrap();

        let ctx = transition_context(base.clone(), id, "void");
        let mut tx = Tx::begin(&write, &ctx).await.unwrap();
        let doc = transition(&mut tx, &eng, &NoSignatures, id, "void", &ctx, None)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(doc.status, Status::Void);
        assert_eq!(doc.number, number);

        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let next = create(&mut tx, &eng, "SOP", "Next", "quality")
            .await
            .unwrap();
        let next_n = load(&mut tx, next).await.unwrap().number;
        tx.commit().await.unwrap();
        assert_ne!(next_n, number, "voided number is not reused");
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn revision_chain_rebuilds_from_rows() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_chain", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let id = create(&mut tx, &eng, "SOP", "Chain", "quality")
            .await
            .unwrap();
        let a = new_revision(&mut tx, id, "A", Manifest::content(json!({"n": 1})))
            .await
            .unwrap();
        let b = new_revision(&mut tx, id, "B", Manifest::content(json!({"n": 2})))
            .await
            .unwrap();
        let c = new_revision(&mut tx, id, "C", Manifest::content(json!({"n": 3})))
            .await
            .unwrap();
        let rebuilt = history(&mut tx, id).await.unwrap();
        let live = history(&mut tx, id).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(rebuilt.len(), 3);
        assert_eq!(rebuilt[0].id, a);
        assert_eq!(rebuilt[1].id, b);
        assert_eq!(rebuilt[2].id, c);
        assert_eq!(rebuilt[1].supersedes, Some(a));
        assert_eq!(rebuilt[2].supersedes, Some(b));
        assert_eq!(rebuilt, live);
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn no_delete_privilege_on_documents() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_nodel", profile).await else {
            return;
        };
        migrate(&db).await;
        for table in [
            "documents.document",
            "documents.revision",
            "documents.blob",
            "documents.attachment",
            "documents.link",
        ] {
            let allowed: bool =
                query_scalar("SELECT has_table_privilege('datum_app', $1, 'DELETE')")
                    .bind(table)
                    .fetch_one(db.app_pool())
                    .await
                    .unwrap();
            assert!(!allowed, "{table} must not grant DELETE to datum_app");
        }
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let id = create(&mut tx, &eng, "SOP", "Keep", "quality")
            .await
            .unwrap();
        let err = tx
            .execute(
                query("DELETE FROM documents.document WHERE document_id = $1").bind(id.as_uuid()),
            )
            .await
            .unwrap_err();
        assert_eq!(db_sqlstate(&err), "42501");
        tx.rollback().await.ok();
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn no_cascade_in_schema() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_casc", profile).await else {
            return;
        };
        migrate(&db).await;
        let n: i64 = query_scalar(
            "SELECT count(*) FROM pg_constraint c
             JOIN pg_namespace n ON n.oid = c.connamespace
             WHERE n.nspname = 'documents' AND c.contype = 'f' AND c.confdeltype = 'c'",
        )
        .fetch_one(db.app_pool())
        .await
        .unwrap();
        assert_eq!(n, 0, "ON DELETE CASCADE is absent from schema documents");
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn blob_is_content_addressed_and_deduplicated() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_blob", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let store = blob_store("dedup");
        let bytes = b"same-bytes-twice";
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let id = create(&mut tx, &eng, "SOP", "Blobs", "quality")
            .await
            .unwrap();
        let rev = new_revision(&mut tx, id, "A", Manifest::content(json!({})))
            .await
            .unwrap();
        let a1 = attach(&mut tx, &store, rev, bytes, "a.pdf", "application/pdf")
            .await
            .unwrap();
        let a2 = attach(&mut tx, &store, rev, bytes, "b.pdf", "application/pdf")
            .await
            .unwrap();
        assert_ne!(a1, a2);
        tx.commit().await.unwrap();
        let blobs: i64 = query_scalar("SELECT count(*) FROM documents.blob")
            .fetch_one(db.app_pool())
            .await
            .unwrap();
        let atts: i64 = query_scalar("SELECT count(*) FROM documents.attachment")
            .fetch_one(db.app_pool())
            .await
            .unwrap();
        assert_eq!(blobs, 1, "same bytes yield one blob");
        assert_eq!(atts, 2);
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn verify_blob_detects_corruption() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_corr", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let store = blob_store("corr");
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let id = create(&mut tx, &eng, "SOP", "Corr", "quality")
            .await
            .unwrap();
        let rev = new_revision(&mut tx, id, "A", Manifest::content(json!({})))
            .await
            .unwrap();
        attach(
            &mut tx,
            &store,
            rev,
            b"clean-bytes",
            "a.bin",
            "application/octet-stream",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let hash = store.put(b"clean-bytes").unwrap();
        verify_blob(&store, hash).unwrap();
        let path = store
            .root()
            .join(&hash.to_hex()[0..2])
            .join(&hash.to_hex()[2..4])
            .join(hash.to_hex());
        std::fs::write(&path, b"tampered").unwrap();
        let err = verify_blob(&store, hash).unwrap_err();
        assert!(matches!(err, Error::BlobCorrupt { .. }), "got {err:?}");
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn attachment_immutable() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_immut", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let store = blob_store("immut");
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let id = create(&mut tx, &eng, "SOP", "Imm", "quality")
            .await
            .unwrap();
        let rev = new_revision(&mut tx, id, "A", Manifest::content(json!({})))
            .await
            .unwrap();
        attach(&mut tx, &store, rev, b"bytes", "a.pdf", "application/pdf")
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let mut tx = Tx::begin(&write, &write_ctx("documents.edit", profile))
            .await
            .unwrap();
        let err = tx
            .execute(query("UPDATE documents.attachment SET filename = 'x'"))
            .await
            .unwrap_err();
        let code = db_sqlstate(&err);
        assert!(
            code == "42501" || code == "P0001",
            "attachment update must fail, got {code}"
        );
        tx.rollback().await.ok();
        db.finish().await.unwrap();
    })
    .await;
}

async fn create_in_review(
    write: &datum_db::WritePool,
    eng: &datum_statemachine::Engine,
    profile: &str,
    gate: &dyn datum_core::SignatureGate,
) -> datum_documents::DocumentId {
    persist_engine(write, eng, profile).await;
    let (_, base) = actor_with_docs(write, profile).await;
    let mut tx = Tx::begin(write, &write_ctx("documents.create", profile))
        .await
        .unwrap();
    let id = create(&mut tx, eng, "SOP", "Review", "quality")
        .await
        .unwrap();
    new_revision(&mut tx, id, "A", Manifest::content(json!({})))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let ctx = transition_context(base, id, "submit");
    let mut tx = Tx::begin(write, &ctx).await.unwrap();
    transition(&mut tx, eng, gate, id, "submit", &ctx, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    id
}

#[tokio::test]
async fn approval_machine_edges_through_kernel() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_mach", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let gate = AcceptingGate::new();
        let (_, base) = actor_with_docs(&write, profile).await;
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let id = create(&mut tx, &eng, "SOP", "Walk", "quality")
            .await
            .unwrap();
        new_revision(&mut tx, id, "A", Manifest::content(json!({})))
            .await
            .unwrap();
        tx.commit().await.unwrap();

        for edge in ["submit", "approve", "make_effective"] {
            let ctx = transition_context(base.clone(), id, edge);
            let ver: i64 =
                query_scalar("SELECT version FROM sm.instance WHERE doc_type = $1 AND doc_id = $2")
                    .bind(datum_documents::DOC_TYPE)
                    .bind(id.as_uuid())
                    .fetch_one(db.app_pool())
                    .await
                    .unwrap();
            let token = dummy_token(
                id,
                if edge == "make_effective" {
                    "Responsible"
                } else {
                    "Approved"
                },
                ver,
            );
            let mut tx = Tx::begin(&write, &ctx).await.unwrap();
            transition(&mut tx, &eng, &gate, id, edge, &ctx, Some(&token))
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
        let mut tx = Tx::begin(&write, &write_ctx("documents.view", profile))
            .await
            .unwrap();
        let doc = load(&mut tx, id).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(doc.status, Status::Effective);
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn regulated_approve_refused_under_no_signatures_nothing_written() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_nosig", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine("regulated-device");
        persist_engine(&write, &eng, profile).await;
        let id = create_in_review(&write, &eng, profile, &NoSignatures).await;
        let before: String =
            query_scalar("SELECT status FROM documents.document WHERE document_id = $1")
                .bind(id.as_uuid())
                .fetch_one(db.app_pool())
                .await
                .unwrap();
        let audit_before: i64 =
            query_scalar("SELECT count(*) FROM audit.event WHERE table_name = 'document'")
                .fetch_one(db.app_pool())
                .await
                .unwrap();
        let (_, base) = actor_with_docs(&write, profile).await;
        let ctx = transition_context(base, id, "approve");
        let token = dummy_token(id, "Approved", 2);
        let mut tx = Tx::begin(&write, &ctx).await.unwrap();
        let err = transition(
            &mut tx,
            &eng,
            &NoSignatures,
            id,
            "approve",
            &ctx,
            Some(&token),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(
                err,
                Error::Signature(datum_core::SignatureError::NoProvider)
            ),
            "got {err:?}"
        );
        tx.rollback().await.unwrap();
        let after: String =
            query_scalar("SELECT status FROM documents.document WHERE document_id = $1")
                .bind(id.as_uuid())
                .fetch_one(db.app_pool())
                .await
                .unwrap();
        assert_eq!(before, after);
        assert_eq!(before, "InReview");
        let audit_after: i64 =
            query_scalar("SELECT count(*) FROM audit.event WHERE table_name = 'document'")
                .fetch_one(db.app_pool())
                .await
                .unwrap();
        assert_eq!(audit_before, audit_after, "refused edge writes nothing");
        let _ = profile;
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn plain_shop_approve_not_required() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_plain", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine("plain-shop");
        persist_engine(&write, &eng, profile).await;
        let approve = document_machine("plain-shop")
            .unwrap()
            .edges
            .into_iter()
            .find(|e| e.name == "approve")
            .unwrap();
        assert!(matches!(
            approve.signature,
            datum_statemachine::SignatureDeclaration::NotRequired { .. }
        ));
        let id = create_in_review(&write, &eng, profile, &NoSignatures).await;
        let (_, base) = actor_with_docs(&write, profile).await;
        let ctx = transition_context(base, id, "approve");
        let mut tx = Tx::begin(&write, &ctx).await.unwrap();
        let doc = transition(&mut tx, &eng, &NoSignatures, id, "approve", &ctx, None)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(doc.status, Status::Approved);
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn effective_at_picks_the_right_revision() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_eff", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let t0 = Utc::now();
        let t1 = t0 + Duration::days(30);
        let t2 = t0 + Duration::days(60);
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let id = create(&mut tx, &eng, "SOP", "Eff", "quality")
            .await
            .unwrap();
        let a = new_revision(
            &mut tx,
            id,
            "A",
            Manifest {
                content: json!({"rev": "A"}),
                effective_from: Some(t0),
                effective_until: Some(t1),
                from_precision: Some(datum_documents::DatePrecision::Day),
                until_precision: Some(datum_documents::DatePrecision::Day),
            },
        )
        .await
        .unwrap();
        let b = new_revision(
            &mut tx,
            id,
            "B",
            Manifest {
                content: json!({"rev": "B"}),
                effective_from: Some(t1),
                effective_until: Some(t2),
                from_precision: Some(datum_documents::DatePrecision::Day),
                until_precision: Some(datum_documents::DatePrecision::Day),
            },
        )
        .await
        .unwrap();
        let at_a = effective_at(&mut tx, id, t0 + Duration::days(1))
            .await
            .unwrap()
            .unwrap();
        let at_b = effective_at(&mut tx, id, t1 + Duration::days(1))
            .await
            .unwrap()
            .unwrap();
        let at_none = effective_at(&mut tx, id, t2 + Duration::days(1))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(at_a.id, a);
        assert_eq!(at_b.id, b);
        assert!(at_none.is_none());
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn overlapping_effectivity_refused() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_ovl", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let t0 = Utc::now();
        let t1 = t0 + Duration::days(40);
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let id = create(&mut tx, &eng, "SOP", "Ovl", "quality")
            .await
            .unwrap();
        new_revision(
            &mut tx,
            id,
            "A",
            Manifest {
                content: json!({}),
                effective_from: Some(t0),
                effective_until: Some(t1),
                from_precision: Some(datum_documents::DatePrecision::Day),
                until_precision: Some(datum_documents::DatePrecision::Day),
            },
        )
        .await
        .unwrap();
        let err = new_revision(
            &mut tx,
            id,
            "B",
            Manifest {
                content: json!({}),
                effective_from: Some(t0 + Duration::days(10)),
                effective_until: Some(t1 + Duration::days(10)),
                from_precision: Some(datum_documents::DatePrecision::Day),
                until_precision: Some(datum_documents::DatePrecision::Day),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::OverlappingEffectivity), "got {err:?}");
        tx.rollback().await.unwrap();
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn legal_hold_blocks_obsolete() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_hold", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let gate = AcceptingGate::new();
        let (_, base) = actor_with_docs(&write, profile).await;
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let id = create(&mut tx, &eng, "SOP", "Hold", "quality")
            .await
            .unwrap();
        new_revision(&mut tx, id, "A", Manifest::content(json!({})))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        for edge in ["submit", "approve", "make_effective"] {
            let ctx = transition_context(base.clone(), id, edge);
            let ver: (i64,) = sqlx::query_as(
                "SELECT version FROM sm.instance WHERE doc_type = $1 AND doc_id = $2",
            )
            .bind(datum_documents::DOC_TYPE)
            .bind(id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .unwrap();
            let token = dummy_token(
                id,
                if edge == "make_effective" {
                    "Responsible"
                } else {
                    "Approved"
                },
                ver.0,
            );
            let mut tx = Tx::begin(&write, &ctx).await.unwrap();
            transition(&mut tx, &eng, &gate, id, edge, &ctx, Some(&token))
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
        let mut tx = Tx::begin(&write, &write_ctx("documents.edit", profile))
            .await
            .unwrap();
        set_legal_hold(&mut tx, id, true).await.unwrap();
        tx.commit().await.unwrap();
        let ctx = transition_context(base, id, "obsolete");
        let mut tx = Tx::begin(&write, &ctx).await.unwrap();
        let err = transition(&mut tx, &eng, &gate, id, "obsolete", &ctx, None)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::LegalHold), "got {err:?}");
        tx.rollback().await.unwrap();
        let status: String =
            query_scalar("SELECT status FROM documents.document WHERE document_id = $1")
                .bind(id.as_uuid())
                .fetch_one(db.app_pool())
                .await
                .unwrap();
        assert_eq!(status, "Effective");
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn every_write_audited_with_stamps() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_aud", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let id = create(&mut tx, &eng, "SOP", "Audit me", "quality")
            .await
            .unwrap();
        new_revision(&mut tx, id, "A", Manifest::content(json!({})))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let n: i64 = query_scalar(
            "SELECT count(*) FROM audit.event
             WHERE table_name IN ('document', 'revision')
               AND action = 'documents.create'
               AND actor_id IS NOT NULL
               AND at IS NOT NULL",
        )
        .fetch_one(db.app_pool())
        .await
        .unwrap();
        assert!(n >= 2, "create+revision must be audited, got {n}");
        let stamped: i64 = query_scalar(
            "SELECT count(*) FROM audit.event
             WHERE table_name = 'document'
               AND app_version IS NOT NULL
               AND config_version IS NOT NULL",
        )
        .fetch_one(db.app_pool())
        .await
        .unwrap();
        assert!(stamped >= 1);
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn writes_go_through_tx() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_fence", profile).await else {
            return;
        };
        migrate(&db).await;
        let before: i64 = query_scalar("SELECT count(*) FROM documents.document")
            .fetch_one(db.app_pool())
            .await
            .unwrap();
        let err = query(
            "INSERT INTO documents.document
             (document_id, kind, number, title, status, retention_class)
             VALUES ($1, 'SOP', 'SOP-9999', 'raw', 'Draft', 'quality')",
        )
        .bind(Identifier::generate().as_uuid())
        .execute(db.app_pool())
        .await
        .unwrap_err();
        assert_eq!(
            err.as_database_error()
                .and_then(|d| d.code().map(|c| c.to_string()))
                .unwrap_or_default(),
            "42501"
        );
        let after: i64 = query_scalar("SELECT count(*) FROM documents.document")
            .fetch_one(db.app_pool())
            .await
            .unwrap();
        assert_eq!(
            before, after,
            "raw pool write must leave the table unchanged"
        );
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn link_binds_revision_to_record() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_link", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let id = create(&mut tx, &eng, "SOP", "Link", "quality")
            .await
            .unwrap();
        let rev = new_revision(&mut tx, id, "A", Manifest::content(json!({})))
            .await
            .unwrap();
        link(
            &mut tx,
            rev,
            "items.item",
            Identifier::generate(),
            "governs",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let n: i64 = query_scalar("SELECT count(*) FROM documents.link")
            .fetch_one(db.app_pool())
            .await
            .unwrap();
        assert_eq!(n, 1);
        db.finish().await.unwrap();
    })
    .await;
}
