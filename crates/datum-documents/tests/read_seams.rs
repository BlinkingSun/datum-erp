//! Render read seam: revision_for_render / attachments_for_render
//! on the sealed Tx and on ReadPool under both LOGIN roles.
#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use chrono::{DateTime, Duration, Utc};
use datum_db::Tx;
use datum_documents::{
    DatePrecision, Error, Manifest, RevisionId, Status, attach, attachments_for_render,
    attachments_for_render_on, create, hash_bytes, new_revision, revision_for_render,
    revision_for_render_on,
};
use serde_json::json;
use sqlx::query_as;

use common::{
    PROFILES, blob_store, frozen_engine, migrate, open_db, persist_engine, read_pool_app,
    read_pool_migrate, write_ctx, write_pool,
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
async fn revision_and_attachments_for_render_on_tx() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_rr_tx", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let store = blob_store(&format!("rr_tx_{}", common::profile_suffix(profile)));
        let from = Utc::now();
        let until = from + Duration::days(30);
        let content = json!({"n": 1, "note": "render"});
        let bytes_a = b"alpha-bytes";
        let bytes_b = b"bravo-bytes";

        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let doc = create(&mut tx, &eng, "SOP", "Work instruction", "quality")
            .await
            .unwrap();
        let rev = new_revision(
            &mut tx,
            doc,
            "A",
            Manifest {
                content: content.clone(),
                effective_from: Some(from),
                effective_until: Some(until),
                from_precision: Some(DatePrecision::Day),
                until_precision: Some(DatePrecision::Day),
            },
        )
        .await
        .unwrap();
        let a_b = attach(&mut tx, &store, rev, bytes_b, "b.pdf", "application/pdf")
            .await
            .unwrap();
        let a_a = attach(&mut tx, &store, rev, bytes_a, "a.pdf", "application/pdf")
            .await
            .unwrap();

        let view = revision_for_render(&mut tx, rev).await.unwrap();
        assert_eq!(view.revision_id, rev);
        assert_eq!(view.document_id, doc);
        assert!(view.number.starts_with("SOP-"), "number {}", view.number);
        assert_eq!(view.title, "Work instruction");
        assert_eq!(view.label, "A");
        assert_eq!(view.status, Status::Draft);
        // timestamptz is microsecond; chrono DateTime can carry nanoseconds.
        // Expected values are the stored row, not the Instant that went in.
        let stored: (Option<DateTime<Utc>>, Option<DateTime<Utc>>) = tx
            .fetch_one(
                query_as(
                    "SELECT effective_from, effective_until
                     FROM documents.revision
                     WHERE revision_id = $1",
                )
                .bind(rev.as_uuid()),
            )
            .await
            .unwrap();
        assert!(stored.0.is_some(), "effective_from was written");
        assert!(stored.1.is_some(), "effective_until was written");
        assert_eq!(view.effective_from, stored.0);
        assert_eq!(view.effective_until, stored.1);
        assert_eq!(
            stored.1.unwrap() - stored.0.unwrap(),
            Duration::days(30),
            "stored window is 30 days"
        );
        assert_eq!(view.content_manifest, content);
        let expected_hash = datum_audit::sha256::digest(&serde_json::to_vec(&content).unwrap());
        assert_eq!(view.content_hash, expected_hash);

        let atts = attachments_for_render(&mut tx, rev).await.unwrap();
        assert_eq!(atts.len(), 2, "two attachments");
        assert_eq!(atts[0].id, a_a);
        assert_eq!(atts[0].filename, "a.pdf");
        assert_eq!(atts[0].media_type, "application/pdf");
        assert_eq!(atts[0].blob_hash, hash_bytes(bytes_a));
        assert_eq!(atts[0].byte_size, bytes_a.len() as i64);
        assert_eq!(atts[1].id, a_b);
        assert_eq!(atts[1].filename, "b.pdf");
        assert_eq!(atts[1].blob_hash, hash_bytes(bytes_b));

        tx.commit().await.unwrap();
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn render_reads_on_read_pool_under_app_and_migrate() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_rr_roles", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let store = blob_store(&format!("rr_roles_{}", common::profile_suffix(profile)));
        let content = json!({"body": "pool"});
        let bytes = b"pool-bytes";

        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let doc = create(&mut tx, &eng, "WI", "Pool read", "quality")
            .await
            .unwrap();
        let rev = new_revision(&mut tx, doc, "B", Manifest::content(content.clone()))
            .await
            .unwrap();
        attach(
            &mut tx,
            &store,
            rev,
            bytes,
            "wi.bin",
            "application/octet-stream",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let missing = RevisionId::generate();
        let app = read_pool_app(&db);
        let mig = read_pool_migrate(&db);
        for (label, pool) in [("datum_app", &app), ("datum_migrate", &mig)] {
            let view = revision_for_render_on(pool, rev)
                .await
                .unwrap_or_else(|e| panic!("{label} revision_for_render: {e}"));
            assert_eq!(view.label, "B", "{label}");
            assert_eq!(view.title, "Pool read", "{label}");
            assert_eq!(view.status, Status::Draft, "{label}");
            assert_eq!(view.content_manifest, content, "{label}");
            assert!(view.effective_from.is_none(), "{label}");
            assert!(view.effective_until.is_none(), "{label}");

            let atts = attachments_for_render_on(pool, rev)
                .await
                .unwrap_or_else(|e| panic!("{label} attachments_for_render: {e}"));
            assert_eq!(atts.len(), 1, "{label}");
            assert_eq!(atts[0].filename, "wi.bin", "{label}");
            assert_eq!(atts[0].media_type, "application/octet-stream", "{label}");
            assert_eq!(atts[0].blob_hash, hash_bytes(bytes), "{label}");

            let err = revision_for_render_on(pool, missing)
                .await
                .expect_err("{label} missing revision");
            assert!(
                matches!(err, Error::NotFound),
                "{label} missing revision: {err:?}"
            );
            let err = attachments_for_render_on(pool, missing)
                .await
                .expect_err("{label} missing attachments");
            assert!(
                matches!(err, Error::NotFound),
                "{label} missing attachments: {err:?}"
            );
        }
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn attachments_for_render_empty_when_none() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("doc_rr_empty", profile).await else {
            return;
        };
        migrate(&db).await;
        let write = write_pool(&db);
        let eng = frozen_engine(profile);
        persist_engine(&write, &eng, profile).await;
        let mut tx = Tx::begin(&write, &write_ctx("documents.create", profile))
            .await
            .unwrap();
        let doc = create(&mut tx, &eng, "SOP", "No files", "quality")
            .await
            .unwrap();
        let rev = new_revision(&mut tx, doc, "A", Manifest::content(json!({})))
            .await
            .unwrap();
        let atts = attachments_for_render(&mut tx, rev).await.unwrap();
        assert!(atts.is_empty(), "no attachments");
        tx.commit().await.unwrap();
        db.finish().await.unwrap();
    })
    .await;
}
