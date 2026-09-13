//! Record-keyed manifestation read seam on the sealed Tx and on ReadPool
//! under both LOGIN roles (D-2b-2, including live-version supersession).
#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use uuid::Uuid;
use wicket_core::{Identifier, RecordRef, SignatureId};
use wicket_db::Tx;
use wicket_esign::{
    manifestation, manifestation_for_record, manifestation_for_record_on, manifestation_in_tx,
    mint, prepare, signature_consumed_at, signature_consumed_at_on,
};
use wicket_test::db_case;

use common::{
    PERM, both_profiles, bump_wo, instance, live_doc, migrate_esign, mint_req,
    persist_and_spawn_wo, read_pool, read_pool_migrate, record, signer_with_perm, system_ctx,
    two_components, user_ctx, write_pool,
};

fn body() -> serde_json::Value {
    serde_json::json!({"wo": "WO-1", "op": 10})
}

#[tokio::test]
async fn manifestation_for_record_matches_d2b2_on_tx() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_rr_tx_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("rr-tx-{}", profile.slug), common::PERM).await;
        let doc_id = Identifier::generate();
        let rec = record(doc_id, 3);
        let inst = instance(doc_id, 3, "Draft");
        let b = body();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin");
        let sig = mint(
            &mut tx,
            &mint_req(
                p.clone(),
                b.clone(),
                rec.clone(),
                inst.clone(),
                two_components(),
                profile.policy.clone(),
            ),
        )
        .await
        .expect("mint");
        let listed = manifestation_for_record(&mut tx, &rec)
            .await
            .expect("list on tx");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].signature.id, sig.id.to_string());
        assert_eq!(listed[0].signature.printed_name, "M. Reyes");
        assert_eq!(listed[0].signature.meaning, "Released");
        assert!(!listed[0].signature.superseded);
        assert_eq!(listed[0].signature.superseded_by_version, None);
        assert_eq!(listed[0].signature.record.table, rec.table);
        assert_eq!(listed[0].signature.record.version, rec.version);
        let empty = manifestation_for_record(
            &mut tx,
            &RecordRef {
                table: rec.table.clone(),
                id: Identifier::generate(),
                version: rec.version,
            },
        )
        .await
        .expect("empty");
        assert!(empty.is_empty(), "unknown record is empty, not NotFound");
        tx.commit().await.expect("commit");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn manifestation_for_record_on_read_pool_under_app_and_migrate() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_rr_roles_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("rr-roles-{}", profile.slug), common::PERM).await;
        let doc_id = Identifier::generate();
        let rec = record(doc_id, 3);
        let inst = instance(doc_id, 3, "Draft");
        let b = body();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin");
        let sig = mint(
            &mut tx,
            &mint_req(
                p.clone(),
                b,
                rec.clone(),
                inst,
                two_components(),
                profile.policy.clone(),
            ),
        )
        .await
        .expect("mint");
        tx.commit().await.expect("commit");

        let by_id = manifestation(&read_pool(&db), sig.id).await.expect("by id");
        let app = read_pool(&db);
        let mig = read_pool_migrate(&db);
        for (label, pool) in [("wicket_app", &app), ("wicket_migrate", &mig)] {
            let listed = manifestation_for_record_on(pool, &rec)
                .await
                .unwrap_or_else(|e| panic!("{label} manifestation_for_record: {e}"));
            assert_eq!(listed.len(), 1, "{label}");
            assert_eq!(
                listed[0], by_id,
                "{label} wire shape matches manifestation(id)"
            );
            assert_eq!(
                listed[0].signature.credential_kind, "signing_password",
                "{label}"
            );
            assert!(!listed[0].signature.superseded, "{label}");
            assert_eq!(listed[0].signature.superseded_by_version, None, "{label}");
            let missing = manifestation_for_record_on(
                pool,
                &RecordRef {
                    table: rec.table.clone(),
                    id: Identifier::generate(),
                    version: 1,
                },
            )
            .await
            .unwrap_or_else(|e| panic!("{label} missing: {e}"));
            assert!(missing.is_empty(), "{label} missing record");
        }
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn manifestation_for_record_includes_supersession_state() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_rr_sup_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("rr-sup-{}", profile.slug), PERM).await;
        let doc_id = Identifier::generate();
        let (eng, doc, version) = persist_and_spawn_wo(&write, doc_id).await;
        let rec = record(doc_id, version);
        let inst = instance(doc_id, version, "Draft");
        let b = body();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin");
        let old = mint(
            &mut tx,
            &mint_req(
                p.clone(),
                b.clone(),
                rec.clone(),
                inst.clone(),
                two_components(),
                profile.policy.clone(),
            ),
        )
        .await
        .expect("mint old");
        let new = mint(
            &mut tx,
            &mint_req(
                p.clone(),
                b,
                rec.clone(),
                inst,
                two_components(),
                profile.policy.clone(),
            ),
        )
        .await
        .expect("mint new");
        tx.commit().await.expect("mint commit");
        let live = bump_wo(&write, &eng, &p, &doc).await;
        let listed = manifestation_for_record_on(&read_pool(&db), &rec)
            .await
            .expect("list");
        assert_eq!(listed.len(), 2, "oldest first");
        assert_eq!(listed[0].signature.id, old.id.to_string());
        assert!(listed[0].signature.superseded, "old is superseded");
        assert_eq!(listed[0].signature.superseded_by_version, Some(live));
        assert_eq!(listed[1].signature.id, new.id.to_string());
        assert!(listed[1].signature.superseded, "same record version");
        assert_eq!(listed[1].signature.superseded_by_version, Some(live));
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn manifestation_in_tx_matches_pool_wire() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_tx_wire_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("tx-wire-{}", profile.slug), PERM).await;
        let doc_id = Identifier::generate();
        let rec = record(doc_id, 3);
        let inst = instance(doc_id, 3, "Draft");
        let b = body();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin");
        let sig = mint(
            &mut tx,
            &mint_req(
                p.clone(),
                b,
                rec,
                inst,
                two_components(),
                profile.policy.clone(),
            ),
        )
        .await
        .expect("mint");
        let in_tx = manifestation_in_tx(&mut tx, sig.id)
            .await
            .expect("in-tx before commit");
        assert_eq!(in_tx.signature.id, sig.id.to_string());
        assert!(!in_tx.signature.superseded);
        assert_eq!(in_tx.signature.superseded_by_version, None);
        tx.commit().await.expect("commit");
        let on_pool = manifestation(&read_pool(&db), sig.id)
            .await
            .expect("pool after commit");
        assert_eq!(in_tx, on_pool, "one D-2b-2 wire");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn signature_consumed_at_none_until_claimed() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_cons_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("cons-{}", profile.slug), PERM).await;
        let doc_id = Identifier::generate();
        let rec = record(doc_id, 3);
        let inst = instance(doc_id, 3, "Draft");
        let b = body();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin");
        let sig = mint(
            &mut tx,
            &mint_req(
                p.clone(),
                b.clone(),
                rec,
                inst.clone(),
                two_components(),
                profile.policy.clone(),
            ),
        )
        .await
        .expect("mint");
        let open = signature_consumed_at(&mut tx, sig.id)
            .await
            .expect("open in-tx");
        assert_eq!(open, None, "mint leaves consumed_at NULL");
        let missing = signature_consumed_at(&mut tx, SignatureId::from_uuid(Uuid::now_v7()))
            .await
            .expect("missing");
        assert_eq!(missing, None, "missing row is None, not NotFound");
        tx.commit().await.expect("mint commit");

        let app = read_pool(&db);
        let mig = read_pool_migrate(&db);
        for (label, pool) in [("wicket_app", &app), ("wicket_migrate", &mig)] {
            let at = signature_consumed_at_on(pool, sig.id)
                .await
                .unwrap_or_else(|e| panic!("{label} open: {e}"));
            assert_eq!(at, None, "{label} unconsumed");
        }

        let token = sig.token(p.actor());
        let doc = live_doc(&sig, b, inst, &p);
        let mut tx = Tx::begin(&write, &user_ctx(&p, "wo.release"))
            .await
            .expect("claim begin");
        prepare(&mut tx, &token, &doc).await.expect("claim");
        let claimed = signature_consumed_at(&mut tx, sig.id)
            .await
            .expect("claimed in-tx")
            .expect("consumed_at set");
        tx.commit().await.expect("claim commit");
        let on_pool = signature_consumed_at_on(&read_pool(&db), sig.id)
            .await
            .expect("pool after claim")
            .expect("consumed_at visible");
        assert_eq!(claimed, on_pool);
        db.finish().await.expect("finish");
    }
}
