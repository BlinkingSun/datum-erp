//! Record-keyed manifestation read seam on the sealed Tx and on ReadPool
//! under both LOGIN roles (D-2b-2, including supersession).
#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_core::{Identifier, RecordRef};
use datum_db::Tx;
use datum_esign::{
    manifestation, manifestation_for_record, manifestation_for_record_on, mint, supersede,
};
use datum_test::db_case;

use common::{
    both_profiles, instance, migrate_esign, mint_req, read_pool, read_pool_migrate, record,
    signer_with_perm, system_ctx, two_components, write_pool,
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
        for (label, pool) in [("datum_app", &app), ("datum_migrate", &mig)] {
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
        let p = signer_with_perm(&write, &format!("rr-sup-{}", profile.slug), common::PERM).await;
        let doc_id = Identifier::generate();
        let rec = record(doc_id, 3);
        let inst = instance(doc_id, 3, "Draft");
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
        supersede(&mut tx, old.id, new.id).await.expect("supersede");
        let listed = manifestation_for_record(&mut tx, &rec).await.expect("list");
        tx.commit().await.expect("commit");
        assert_eq!(listed.len(), 2, "oldest first");
        assert_eq!(listed[0].signature.id, old.id.to_string());
        assert!(listed[0].signature.superseded, "old is superseded");
        assert_eq!(listed[1].signature.id, new.id.to_string());
        assert!(!listed[1].signature.superseded, "new is live");
        db.finish().await.expect("finish");
    }
}
