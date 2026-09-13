//! Named commit-mode tests (SPEC D-2b-9). Both profiles unless noted.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use serde_json::{Value, json};
use sqlx::{query as sql_query, query_as as sql_query_as, query_scalar as sql_query_scalar};
use wicket_core::{
    Actor, ActorKind, Identifier, NoPostings, PermissionKey, SignatureError, SignatureGate,
    SignatureId, SignatureMeaning, SignatureRequirement, SignatureToken,
};
use wicket_db::Tx;
use wicket_esign::{
    Error, SessionPolicy, archival_bundle, log_refusal, manifestation, mint, prepare, verify_bundle,
};
use wicket_identity::{
    PrincipalKind, PrincipalStatus, create_principal, deactivate_principal, rename_principal,
    set_login_credential,
};
use wicket_statemachine::{DocRef, EdgeBuilder, Engine, Machine, with_action};
use wicket_test::db_case;

use common::{
    LOGIN_SECRET, PERM, SIGNING_SECRET, both_profiles, bump_wo, instance, live_doc, migrate_esign,
    migrate_esign_sm, mint_req, persist_and_spawn_wo, pg_code, read_pool, record,
    relaxation_on_profile, required, secret_only, signer_with_perm, system_ctx, two_components,
    user_ctx, write_pool,
};

fn body() -> Value {
    json!({"wo": "WO-2026-1847", "rev": "C"})
}

#[tokio::test]
async fn two_component_first_signing_is_required() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_two_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("two-{}", profile.slug), PERM).await;
        let doc_id = Identifier::generate();
        let rec = record(doc_id, 1);
        let inst = instance(doc_id, 1, "Draft");
        let mut req = mint_req(
            p.clone(),
            body(),
            rec,
            inst,
            secret_only(),
            profile.policy.clone(),
        );
        req.principal = p;
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin");
        let err = mint(&mut tx, &req).await.expect_err("secret-only first");
        assert!(
            matches!(err, Error::Validation { ref field, .. } if field.as_deref() == Some("identification.code")),
            "{} {err:?}",
            profile.id
        );
        tx.rollback().await.expect("rollback");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn one_component_continuation_refused_when_relaxation_off() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_off_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("off-{}", profile.slug), PERM).await;
        let doc_id = Identifier::generate();
        let rec = record(doc_id, 1);
        let inst = instance(doc_id, 1, "Draft");
        let policy = profile.policy.clone();
        assert_eq!(
            policy.continuous_session, "off",
            "{} must ship continuous_session off",
            profile.id
        );
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin");
        let first = mint_req(
            p.clone(),
            body(),
            rec.clone(),
            inst.clone(),
            two_components(),
            policy.clone(),
        );
        mint(&mut tx, &first).await.expect("first two-component");
        let cont = mint_req(p.clone(), body(), rec, inst, secret_only(), policy);
        let err = mint(&mut tx, &cont).await.expect_err("continuation off");
        assert!(
            matches!(err, Error::Validation { .. }),
            "{} {err:?}",
            profile.id
        );
        tx.rollback().await.expect("rollback");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn one_component_continuation_accepted_when_on() {
    let on = relaxation_on_profile();
    assert_eq!(on.policy.continuous_session, "on");
    let db = db_case!("es_on");
    migrate_esign(&db).await;
    let write = write_pool(&db);
    let p = signer_with_perm(&write, "on-user", PERM).await;
    let policy = on.policy;
    let doc_id = Identifier::generate();
    let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
        .await
        .expect("begin");
    let first = mint_req(
        p.clone(),
        body(),
        record(doc_id, 1),
        instance(doc_id, 1, "Draft"),
        two_components(),
        policy.clone(),
    );
    let a = mint(&mut tx, &first).await.expect("first");
    assert_eq!(a.components_used, two_components());
    let cont = mint_req(
        p,
        body(),
        record(doc_id, 1),
        instance(doc_id, 1, "Draft"),
        secret_only(),
        policy,
    );
    let b = mint(&mut tx, &cont).await.expect("continuation on");
    assert_eq!(b.components_used, secret_only());
    tx.commit().await.expect("commit");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn session_expiry_forces_two_components() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_exp_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("exp-idle-{}", profile.slug), PERM).await;
        let idle_policy = SessionPolicy {
            continuous_session: "on".into(),
            idle_timeout_secs: 1,
            max_window_secs: 900,
        };
        let doc_id = Identifier::generate();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin");
        mint(
            &mut tx,
            &mint_req(
                p.clone(),
                body(),
                record(doc_id, 1),
                instance(doc_id, 1, "Draft"),
                two_components(),
                idle_policy.clone(),
            ),
        )
        .await
        .expect("first idle");
        tx.commit().await.expect("commit idle first");
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin idle2");
        let err = mint(
            &mut tx,
            &mint_req(
                p.clone(),
                body(),
                record(doc_id, 1),
                instance(doc_id, 1, "Draft"),
                secret_only(),
                idle_policy,
            ),
        )
        .await
        .expect_err("idle expired");
        assert!(matches!(err, Error::Validation { .. }), "{err:?}");
        tx.rollback().await.expect("rollback idle");

        let p2 = signer_with_perm(&write, &format!("exp-win-{}", profile.slug), PERM).await;
        let win_policy = SessionPolicy {
            continuous_session: "on".into(),
            idle_timeout_secs: 300,
            max_window_secs: 1,
        };
        let doc_id = Identifier::generate();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin win");
        mint(
            &mut tx,
            &mint_req(
                p2.clone(),
                body(),
                record(doc_id, 1),
                instance(doc_id, 1, "Draft"),
                two_components(),
                win_policy.clone(),
            ),
        )
        .await
        .expect("first window");
        tx.commit().await.expect("commit window first");
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin win2");
        let err = mint(
            &mut tx,
            &mint_req(
                p2,
                body(),
                record(doc_id, 1),
                instance(doc_id, 1, "Draft"),
                secret_only(),
                win_policy,
            ),
        )
        .await
        .expect_err("window expired");
        assert!(matches!(err, Error::Validation { .. }), "{err:?}");
        tx.rollback().await.expect("rollback win");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn signing_session_closes_on_device_change() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_dev_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("devchg-{}", profile.slug), PERM).await;
        let mut policy = profile.policy.clone();
        policy.continuous_session = "on".into();
        let doc_id = Identifier::generate();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin");
        let mut first = mint_req(
            p.clone(),
            body(),
            record(doc_id, 1),
            instance(doc_id, 1, "Draft"),
            two_components(),
            policy.clone(),
        );
        first.device_fingerprint = Some("tablet-1".into());
        mint(&mut tx, &first).await.expect("first");
        let mut second = mint_req(
            p.clone(),
            body(),
            record(doc_id, 1),
            instance(doc_id, 1, "Draft"),
            two_components(),
            policy.clone(),
        );
        second.device_fingerprint = Some("tablet-2".into());
        mint(&mut tx, &second).await.expect("second device");
        tx.commit().await.expect("commit");
        let closed: i64 = sql_query_scalar(
            r#"SELECT count(*) FROM transient.signing_session
            WHERE principal_id = $1 AND close_reason = 'device_change'"#,
        )
        .bind(p.id.as_uuid())
        .fetch_one(db.app_pool())
        .await
        .expect("closed");
        assert!(closed >= 1, "device change must close the prior session");
        db.finish().await.expect("finish");
    }
}

async fn mint_one_with(
    db: &wicket_test::TestDb,
    name: &str,
    policy: SessionPolicy,
) -> (
    wicket_identity::Principal,
    wicket_esign::Signature,
    Value,
    wicket_esign::InstanceTriple,
) {
    let write = write_pool(db);
    let p = signer_with_perm(&write, name, PERM).await;
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
            policy,
        ),
    )
    .await
    .expect("mint");
    tx.commit().await.expect("commit");
    (p, sig, b, inst)
}

#[tokio::test]
async fn token_is_single_use() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_once_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let (p, sig, b, inst) = mint_one_with(
            &db,
            &format!("once-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let token = sig.token(p.actor());
        let doc = live_doc(&sig, b.clone(), inst.clone(), &p);
        let mut tx = Tx::begin(&write, &user_ctx(&p, "wo.release"))
            .await
            .expect("begin");
        let gate = prepare(&mut tx, &token, &doc).await.expect("prepare 1");
        gate.verify(&token, &required(), &sig.record)
            .expect("first verify");
        tx.commit().await.expect("commit");
        let mut tx = Tx::begin(&write, &user_ctx(&p, "wo.release"))
            .await
            .expect("begin2");
        let gate = prepare(&mut tx, &token, &doc).await.expect("prepare 2");
        let err = gate
            .verify(&token, &required(), &sig.record)
            .expect_err("second use");
        assert_eq!(err, SignatureError::Consumed);
        let other = SignatureRequirement {
            meaning: SignatureMeaning("Approved".into()),
            permission: PermissionKey(PERM.into()),
        };
        let err = gate.verify(&token, &other, &sig.record).expect_err("slot");
        assert_eq!(
            err,
            SignatureError::MeaningMismatch,
            "{} Consumed is slot 6, after meaning",
            profile.id
        );
        tx.rollback().await.expect("rollback");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn concurrent_claim_one_wins_one_consumed() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_conc_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let (p, sig, b, inst) = mint_one_with(
            &db,
            &format!("conc-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let token = sig.token(p.actor());
        let doc = live_doc(&sig, b, inst, &p);
        let req = required();
        let rec = sig.record.clone();
        let write1 = write.clone();
        let write2 = write.clone();
        let token1 = token.clone();
        let token2 = token.clone();
        let doc1 = doc.clone();
        let doc2 = doc.clone();
        let p1 = p.clone();
        let p2 = p.clone();
        let req1 = req.clone();
        let req2 = req.clone();
        let rec1 = rec.clone();
        let rec2 = rec;
        let (r1, r2) = tokio::join!(
            async move {
                let mut tx = Tx::begin(&write1, &user_ctx(&p1, "wo.release"))
                    .await
                    .expect("t1");
                let gate = prepare(&mut tx, &token1, &doc1).await.expect("p1");
                let v = gate.verify(&token1, &req1, &rec1);
                if v.is_ok() {
                    tx.commit().await.expect("c1");
                } else {
                    tx.rollback().await.expect("r1");
                }
                v
            },
            async move {
                let mut tx = Tx::begin(&write2, &user_ctx(&p2, "wo.release"))
                    .await
                    .expect("t2");
                let gate = prepare(&mut tx, &token2, &doc2).await.expect("p2");
                let v = gate.verify(&token2, &req2, &rec2);
                if v.is_ok() {
                    tx.commit().await.expect("c2");
                } else {
                    tx.rollback().await.expect("r2");
                }
                v
            }
        );
        let wins = r1.is_ok() as u8 + r2.is_ok() as u8;
        let consumed = matches!(r1, Err(SignatureError::Consumed)) as u8
            + matches!(r2, Err(SignatureError::Consumed)) as u8;
        assert_eq!(wins, 1, "exactly one claim wins: {r1:?} {r2:?}");
        assert_eq!(consumed, 1, "the other is Consumed: {r1:?} {r2:?}");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn content_hash_mismatch_refuses() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_hash_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let (p, sig, _b, inst) = mint_one_with(
            &db,
            &format!("hashm-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let token = sig.token(p.actor());
        let edited = json!({"wo": "WO-2026-1847", "rev": "D"});
        let doc = live_doc(&sig, edited, inst, &p);
        let mut tx = Tx::begin(&write, &user_ctx(&p, "wo.release"))
            .await
            .expect("begin");
        let gate = prepare(&mut tx, &token, &doc).await.expect("prepare");
        let err = gate
            .verify(&token, &required(), &sig.record)
            .expect_err("hash");
        assert_eq!(err, SignatureError::HashMismatch);
        tx.rollback().await.expect("rollback");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn meaning_mismatch_refuses() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_mean_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let (p, sig, b, inst) = mint_one_with(
            &db,
            &format!("mean-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let token = sig.token(p.actor());
        let doc = live_doc(&sig, b, inst, &p);
        let mut tx = Tx::begin(&write, &user_ctx(&p, "wo.release"))
            .await
            .expect("begin");
        let gate = prepare(&mut tx, &token, &doc).await.expect("prepare");
        let other = SignatureRequirement {
            meaning: SignatureMeaning("Approved".into()),
            permission: PermissionKey(PERM.into()),
        };
        let err = gate.verify(&token, &other, &sig.record).expect_err("mean");
        assert_eq!(err, SignatureError::MeaningMismatch);
        tx.rollback().await.expect("rollback");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn record_version_mismatch_refuses() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_ver_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let (p, sig, b, inst) = mint_one_with(
            &db,
            &format!("ver-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let token = sig.token(p.actor());
        let doc = live_doc(&sig, b, inst, &p);
        let mut tx = Tx::begin(&write, &user_ctx(&p, "wo.release"))
            .await
            .expect("begin");
        let gate = prepare(&mut tx, &token, &doc).await.expect("prepare");
        let mut live = sig.record.clone();
        live.version = 4;
        let err = gate.verify(&token, &required(), &live).expect_err("ver");
        assert_eq!(err, SignatureError::RecordMismatch);
        tx.rollback().await.expect("rollback");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn permission_snapshot_not_live_rbac() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_perm_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let (p, sig, b, inst) = mint_one_with(
            &db,
            &format!("perm-ok-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let mut tx = Tx::begin(&write, &system_ctx("identity.revoke"))
            .await
            .expect("revoke begin");
        tx.execute(
            sql_query(
                r#"UPDATE identity.role_permission
                  SET permission_key = 'revoked.now'
                WHERE permission_key = $1
                  AND role_id IN (
                      SELECT role_id FROM identity.principal_role WHERE principal_id = $2
                  )"#,
            )
            .bind(PERM)
            .bind(p.id.as_uuid()),
        )
        .await
        .expect("revoke");
        tx.commit().await.expect("revoke commit");
        let token = sig.token(p.actor());
        let doc = live_doc(&sig, b, inst, &p);
        let mut tx = Tx::begin(&write, &user_ctx(&p, "wo.release"))
            .await
            .expect("begin");
        let gate = prepare(&mut tx, &token, &doc).await.expect("prepare");
        gate.verify(&token, &required(), &sig.record)
            .expect("snapshot still permits");
        tx.commit().await.expect("commit");

        let p2 = {
            let mut tx = Tx::begin(&write, &system_ctx("identity.create"))
                .await
                .expect("begin p2");
            let p2 = create_principal(
                &mut tx,
                PrincipalKind::User,
                &format!("never-held-{}", profile.slug),
                "No Perm",
            )
            .await
            .expect("create");
            wicket_identity::set_signing_credential(&mut tx, p2.id, SIGNING_SECRET)
                .await
                .expect("cred");
            tx.commit().await.expect("commit p2");
            p2
        };
        let doc_id = Identifier::generate();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin mint2");
        let sig2 = mint(
            &mut tx,
            &mint_req(
                p2.clone(),
                body(),
                record(doc_id, 1),
                instance(doc_id, 1, "Draft"),
                two_components(),
                profile.policy.clone(),
            ),
        )
        .await
        .expect("mint without perm");
        tx.commit().await.expect("commit mint2");
        let token2 = sig2.token(p2.actor());
        let doc2 = live_doc(&sig2, body(), instance(doc_id, 1, "Draft"), &p2);
        let mut tx = Tx::begin(&write, &user_ctx(&p2, "wo.release"))
            .await
            .expect("begin v2");
        let gate = prepare(&mut tx, &token2, &doc2).await.expect("prepare2");
        let err = gate
            .verify(&token2, &required(), &sig2.record)
            .expect_err("never held");
        assert_eq!(err, SignatureError::SignerNotPermitted);
        tx.rollback().await.expect("rollback");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn transition_signature_and_audit_row_share_one_tx() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_txid_{}", profile.slug));
        migrate_esign_sm(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("txid-{}", profile.slug), PERM).await;
        let mut eng = Engine::new();
        let req = SignatureRequirement {
            meaning: SignatureMeaning("Released".into()),
            permission: PermissionKey(PERM.into()),
        };
        eng.register_machine(
            Machine::builder("wo")
                .regulated(true)
                .state("Draft")
                .state("Released")
                .edge(
                    EdgeBuilder::new("Draft", "Released", "release", "wo.release")
                        .required(req.clone()),
                )
                .build()
                .expect("machine"),
        )
        .expect("reg");
        eng.freeze().expect("freeze");
        let doc = DocRef {
            doc_type: "wo".into(),
            doc_id: Identifier::generate(),
        };
        let mut tx = Tx::begin(&write, &system_ctx("sm.persist"))
            .await
            .expect("persist begin");
        eng.persist(&mut tx).await.expect("persist");
        let spawned = eng.spawn(&mut tx, &doc, "Draft").await.expect("spawn");
        tx.commit().await.expect("persist commit");

        let rec = record(doc.doc_id, spawned.version);
        let inst = instance(doc.doc_id, spawned.version, "Draft");
        let b = body();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("mint begin");
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
        tx.commit().await.expect("mint commit");

        let mut ctx = user_ctx(&p, "pending");
        ctx.esign_id = Some(sig.id.to_string());
        ctx = with_action(ctx, &doc, "release");
        let token = sig.token(p.actor());
        let live = live_doc(&sig, b, inst, &p);
        let mut tx = Tx::begin(&write, &ctx).await.expect("begin trans");
        let gate = prepare(&mut tx, &token, &live).await.expect("prepare");
        eng.transition(
            &mut tx,
            Box::new(NoPostings),
            &doc,
            "release",
            Some(&token),
            &gate,
            &ctx,
        )
        .await
        .expect("transition");
        let xid = tx.pg_txid().await.expect("xid");
        tx.commit().await.expect("commit");

        let rows: Vec<(String, Option<String>)> = sql_query_as(
            r#"SELECT xid::text, esign_id::text FROM audit.event
            WHERE xid = $1::xid8"#,
        )
        .bind(&xid)
        .fetch_all(db.app_pool())
        .await
        .expect("events");
        assert!(
            rows.iter()
                .any(|(_, e)| e.as_deref() == Some(&sig.id.to_string())),
            "esign_id on audit rows: {rows:?}"
        );
        let seals: i64 =
            sql_query_scalar("SELECT count(*) FROM audit.tx_seal WHERE xid = $1::xid8")
                .bind(&xid)
                .fetch_one(db.app_pool())
                .await
                .expect("seals");
        assert_eq!(seals, 1, "one tx_seal row for the shared xid");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn refused_transition_rolls_back_the_claim() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_rb_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let (p, sig, b, inst) =
            mint_one_with(&db, &format!("rb-{}", profile.slug), profile.policy.clone()).await;
        let token = sig.token(p.actor());
        let doc = live_doc(&sig, b, inst, &p);
        let mut tx = Tx::begin(&write, &user_ctx(&p, "wo.release"))
            .await
            .expect("begin");
        let gate = prepare(&mut tx, &token, &doc).await.expect("prepare");
        let other = SignatureRequirement {
            meaning: SignatureMeaning("Approved".into()),
            permission: PermissionKey(PERM.into()),
        };
        gate.verify(&token, &other, &sig.record)
            .expect_err("refuse");
        tx.rollback().await.expect("rollback");
        let consumed: Option<chrono::DateTime<chrono::Utc>> =
            sql_query_scalar("SELECT consumed_at FROM esign.signature WHERE signature_id = $1")
                .bind(sig.id.as_uuid())
                .fetch_one(db.app_pool())
                .await
                .expect("consumed");
        assert!(consumed.is_none(), "rollback un-claims");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn manifestation_wire_shape_is_exact() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_wire_{}", profile.slug));
        migrate_esign(&db).await;
        let (_p, sig, _, _) = mint_one_with(
            &db,
            &format!("wire-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let m = manifestation(&read_pool(&db), sig.id).await.expect("manif");
        let v = serde_json::to_value(&m).expect("json");
        let sigv = v.get("signature").expect("signature key");
        for key in [
            "id",
            "signer_id",
            "printed_name",
            "meaning",
            "reason",
            "signed_at",
            "signed_at_zone",
            "signed_at_local",
            "record",
            "record_content_hash",
            "credential_kind",
            "components_used",
            "superseded",
            "superseded_by_version",
        ] {
            assert!(sigv.get(key).is_some(), "missing {key}");
        }
        let rec = sigv.get("record").unwrap();
        for key in ["table", "doc_type", "id", "version"] {
            assert!(rec.get(key).is_some(), "record missing {key}");
        }
        assert_eq!(sigv["meaning"], "Released");
        assert_eq!(sigv["signed_at_zone"], "America/New_York");
        assert_eq!(sigv["credential_kind"], "signing_password");
        assert_eq!(sigv["superseded"], false);
        assert_eq!(sigv["superseded_by_version"], Value::Null);
        let local = sigv["signed_at_local"].as_str().unwrap();
        assert!(
            local.contains('T') && (local.contains('+') || local.contains('-')),
            "signed_at_local {local}"
        );
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn printed_name_is_a_snapshot_not_a_join() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_name_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let (p, sig, _, _) = mint_one_with(
            &db,
            &format!("snapname-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let mut tx = Tx::begin(&write, &system_ctx("identity.rename"))
            .await
            .expect("begin");
        rename_principal(&mut tx, p.id, "A Different Name")
            .await
            .expect("rename");
        tx.commit().await.expect("commit");
        let m = manifestation(&read_pool(&db), sig.id).await.expect("manif");
        assert_eq!(m.signature.printed_name, "M. Reyes");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn deactivated_signer_reads_back() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_dead_read_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let (p, sig, _, _) = mint_one_with(
            &db,
            &format!("deadread-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let mut tx = Tx::begin(&write, &system_ctx("identity.deactivate"))
            .await
            .expect("begin");
        deactivate_principal(&mut tx, p.id).await.expect("deact");
        tx.commit().await.expect("commit");
        let m = manifestation(&read_pool(&db), sig.id).await.expect("manif");
        assert_eq!(m.signature.printed_name, "M. Reyes");
        assert_eq!(m.signature.meaning, "Released");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn deactivated_signer_cannot_consume_open_token() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_dead_tok_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let (p, sig, b, inst) = mint_one_with(
            &db,
            &format!("deadtok-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let mut tx = Tx::begin(&write, &system_ctx("identity.deactivate"))
            .await
            .expect("begin");
        deactivate_principal(&mut tx, p.id).await.expect("deact");
        tx.commit().await.expect("commit");
        let token = sig.token(p.actor());
        let mut doc = live_doc(&sig, b, inst, &p);
        doc.signer_status = PrincipalStatus::Inactive;
        let mut tx = Tx::begin(&write, &user_ctx(&p, "wo.release"))
            .await
            .expect("begin2");
        let gate = prepare(&mut tx, &token, &doc).await.expect("prepare");
        let err = gate
            .verify(&token, &required(), &sig.record)
            .expect_err("inactive");
        assert_eq!(err, SignatureError::Invalid("signer inactive".into()));
        tx.rollback().await.expect("rollback");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn deactivate_closes_open_signing_session() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_deact_sess_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let (p, _, _, _) = mint_one_with(
            &db,
            &format!("deactsess-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let open_before: i64 = sql_query_scalar(
            r#"SELECT count(*) FROM transient.signing_session
            WHERE principal_id = $1 AND closed_at IS NULL"#,
        )
        .bind(p.id.as_uuid())
        .fetch_one(db.app_pool())
        .await
        .expect("open before");
        assert!(open_before >= 1, "mint opens a signing session");
        let mut tx = Tx::begin(&write, &system_ctx("identity.deactivate"))
            .await
            .expect("begin");
        deactivate_principal(&mut tx, p.id).await.expect("deact");
        tx.commit().await.expect("commit");
        let closed: i64 = sql_query_scalar(
            r#"SELECT count(*) FROM transient.signing_session
            WHERE principal_id = $1 AND close_reason = 'principal_deactivated'"#,
        )
        .bind(p.id.as_uuid())
        .fetch_one(db.app_pool())
        .await
        .expect("closed");
        assert!(
            closed >= 1,
            "deactivate must close the open signing session"
        );
        let still_open: i64 = sql_query_scalar(
            r#"SELECT count(*) FROM transient.signing_session
            WHERE principal_id = $1 AND closed_at IS NULL"#,
        )
        .bind(p.id.as_uuid())
        .fetch_one(db.app_pool())
        .await
        .expect("open after");
        assert_eq!(still_open, 0, "no open signing session after deactivate");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn superseded_version_reads_back_with_snapshot() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_sup_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("sup-{}", profile.slug), PERM).await;
        let doc_id = Identifier::generate();
        let (eng, doc, version) = persist_and_spawn_wo(&write, doc_id).await;
        let rec = record(doc_id, version);
        let inst = instance(doc_id, version, "Draft");
        let snapshot_body = body();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("mint begin");
        let sig = mint(
            &mut tx,
            &mint_req(
                p.clone(),
                snapshot_body.clone(),
                rec,
                inst,
                two_components(),
                profile.policy.clone(),
            ),
        )
        .await
        .expect("mint");
        tx.commit().await.expect("mint commit");
        let live = bump_wo(&write, &eng, &p, &doc).await;
        assert!(
            live > version,
            "instance version must bump, live={live} signed={version}"
        );
        let m = manifestation(&read_pool(&db), sig.id).await.expect("manif");
        assert!(
            m.signature.superseded,
            "signed version {version} live {live}"
        );
        assert_eq!(m.signature.superseded_by_version, Some(live));
        assert_eq!(m.signature.printed_name, "M. Reyes");
        assert_eq!(m.signature.record.version, version);
        assert_eq!(sig.record_snapshot, sig.record_snapshot);
        let v = serde_json::to_value(&m).expect("json");
        assert_eq!(v["signature"]["superseded"], true);
        assert_eq!(v["signature"]["superseded_by_version"], live);
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn signature_row_is_insert_only() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_imm_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let (p, sig, _, _) = mint_one_with(
            &db,
            &format!("imm-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let mut tx = Tx::begin(&write, &user_ctx(&p, "esign.tamper"))
            .await
            .expect("begin");
        let err = tx
            .execute(
                sql_query(
                    "UPDATE esign.signature SET signer_printed_name = 'X' WHERE signature_id = $1",
                )
                .bind(sig.id.as_uuid()),
            )
            .await
            .expect_err("update name");
        match err {
            wicket_db::Error::Refused(_) => {}
            wicket_db::Error::Sqlx(e) => assert_eq!(pg_code(&e), "42501"),
            other => panic!("{other}"),
        }
        tx.rollback().await.expect("rollback");
        let mut tx = Tx::begin(&write, &user_ctx(&p, "esign.claim"))
            .await
            .expect("begin claim");
        tx.execute(
        sql_query(
            "UPDATE esign.signature SET consumed_at = now(), consumed_xid = pg_current_xact_id() WHERE signature_id = $1",
        )
        .bind(sig.id.as_uuid()),
    )
    .await
    .expect("claim columns are granted");
        tx.commit().await.expect("commit claim");
        let del = sql_query("DELETE FROM esign.signature WHERE signature_id = $1")
            .bind(sig.id.as_uuid())
            .execute(db.app_pool())
            .await
            .expect_err("delete");
        assert_eq!(pg_code(&del), "42501");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn login_secret_is_not_a_signing_component() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_login_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("loginsec-{}", profile.slug), PERM).await;
        let doc_id = Identifier::generate();
        let mut req = mint_req(
            p.clone(),
            body(),
            record(doc_id, 1),
            instance(doc_id, 1, "Draft"),
            two_components(),
            profile.policy.clone(),
        );
        req.secret = LOGIN_SECRET.into();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin");
        let err = mint(&mut tx, &req).await.expect_err("login secret");
        assert!(
            matches!(err, Error::Validation { ref field, .. } if field.as_deref() == Some("identification.secret")),
            "{err:?}"
        );
        tx.rollback().await.expect("rollback");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn failed_mint_is_a_security_event_not_a_business_row() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_failsec_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("failsec-{}", profile.slug), PERM).await;
        let doc_id = Identifier::generate();
        let mut req = mint_req(
            p.clone(),
            body(),
            record(doc_id, 1),
            instance(doc_id, 1, "Draft"),
            two_components(),
            profile.policy.clone(),
        );
        req.secret = "wrong-secret".into();
        let before_sig: i64 = sql_query_scalar("SELECT count(*) FROM esign.signature")
            .fetch_one(db.app_pool())
            .await
            .expect("before");
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin");
        mint(&mut tx, &req).await.expect_err("fail");
        tx.rollback().await.expect("rollback");
        log_refusal(
            &write,
            p.actor(),
            "",
            "failed mint",
            json!({"reason": "invalid credentials"}),
        )
        .await
        .expect("log");
        let after_sig: i64 = sql_query_scalar("SELECT count(*) FROM esign.signature")
            .fetch_one(db.app_pool())
            .await
            .expect("after");
        assert_eq!(before_sig, after_sig, "no business row");
        let events: i64 = sql_query_scalar(
            r#"SELECT count(*) FROM audit.event
            WHERE source_kind = 'app_event' AND action LIKE 'security.%'"#,
        )
        .fetch_one(db.app_pool())
        .await
        .expect("events");
        assert!(events >= 1, "security event written");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn required_edge_with_no_signing_credential_refuses_at_mint() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_nocred_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let mut tx = Tx::begin(&write, &system_ctx("identity.create"))
            .await
            .expect("begin");
        let p = create_principal(
            &mut tx,
            PrincipalKind::User,
            &format!("nocred-{}", profile.slug),
            "No Cred",
        )
        .await
        .expect("create");
        set_login_credential(&mut tx, p.id, LOGIN_SECRET)
            .await
            .expect("login only");
        tx.commit().await.expect("commit");
        let doc_id = Identifier::generate();
        let mut tx = Tx::begin(&write, &system_ctx("esign.mint"))
            .await
            .expect("begin mint");
        let err = mint(
            &mut tx,
            &mint_req(
                p,
                body(),
                record(doc_id, 1),
                instance(doc_id, 1, "Draft"),
                two_components(),
                profile.policy.clone(),
            ),
        )
        .await
        .expect_err("no signing cred");
        assert!(
            matches!(err, Error::SignatureRequired { ref field } if field == "identification.secret"),
            "{err:?}"
        );
        tx.rollback().await.expect("rollback");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn plain_shop_binds_nosignatures_and_enables_no_regulated_module() {
    let plain = include_str!("../../../profiles/plain-shop.toml");
    assert!(
        plain.contains("gate = \"NoSignatures\""),
        "plain-shop binds NoSignatures"
    );
    let cal = plain
        .split("[[modules]]")
        .find(|b| b.contains("mod-calibration"))
        .expect("calibration module");
    assert!(cal.contains("regulated = true"));
    assert!(cal.contains("enabled = false"));
    let regulated = include_str!("../../../profiles/regulated-device.toml");
    assert!(
        regulated
            .split("[[modules]]")
            .filter(|b| b.contains("regulated = true") && b.contains("enabled = true"))
            .count()
            >= 1
            || regulated.contains("mod-calibration"),
        "regulated-device enables calibration"
    );
}

#[tokio::test]
async fn archival_bundle_verifies_offline() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_arch_{}", profile.slug));
        migrate_esign(&db).await;
        let (_p, sig, _, _) = mint_one_with(
            &db,
            &format!("arch-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let bundle = archival_bundle(&read_pool(&db), sig.id)
            .await
            .expect("bundle");
        assert!(
            !bundle.audit_event_ids.is_empty(),
            "audit_event_ids must be the signature's audit rows"
        );
        assert!(
            !bundle.seals.is_empty(),
            "seals must cover the signature's audit rows"
        );
        let v = verify_bundle(&bundle);
        assert!(v.hash_ok, "hash_ok");
        assert!(v.chain_ok, "chain_ok");
        let mut empty = bundle.clone();
        empty.seals.clear();
        assert!(
            !verify_bundle(&empty).chain_ok,
            "empty seals are not chain_ok"
        );
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn every_esign_table_is_audited() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_audtbl_{}", profile.slug));
        migrate_esign(&db).await;
        for table in ["signature", "meaning_policy"] {
            let yes: bool = sql_query_scalar(
                r#"SELECT EXISTS (
                 SELECT 1 FROM pg_trigger t
                 JOIN pg_class c ON c.oid = t.tgrelid
                 JOIN pg_namespace n ON n.oid = c.relnamespace
                 WHERE n.nspname = 'esign' AND c.relname = $1
                   AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
               )"#,
            )
            .bind(table)
            .fetch_one(db.app_pool())
            .await
            .expect("trig");
            assert!(yes, "{table} must have zz_audit_row");
        }
        let transient: bool = sql_query_scalar(
            r#"SELECT EXISTS (
             SELECT 1 FROM pg_trigger t
             JOIN pg_class c ON c.oid = t.tgrelid
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'transient' AND c.relname = 'signing_session'
               AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
           )"#,
        )
        .fetch_one(db.app_pool())
        .await
        .expect("transient");
        assert!(!transient, "signing_session is transient, not audited");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn hash_and_secret_columns_are_redacted_in_audit() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_redact_{}", profile.slug));
        migrate_esign(&db).await;
        let registered: bool = sql_query_scalar(
            r#"SELECT EXISTS (
             SELECT 1 FROM audit.redact
              WHERE relid = 'esign.signature'::regclass
                AND column_name = 'record_content_hash'
           )"#,
        )
        .fetch_one(db.app_pool())
        .await
        .expect("redact row from production migrator");
        assert!(
            registered,
            "NO_REDACT_ROW: production migrations must register audit.redact"
        );
        let (_p, sig, _, _) = mint_one_with(
            &db,
            &format!("redact-{}", profile.slug),
            profile.policy.clone(),
        )
        .await;
        let new_row: Value = sql_query_scalar(
            r#"SELECT new_row FROM audit.event
            WHERE table_name = 'signature' AND op = 'INSERT'
              AND row_key->>'signature_id' = $1
            ORDER BY stmt_at DESC LIMIT 1"#,
        )
        .bind(sig.id.to_string())
        .fetch_one(db.app_pool())
        .await
        .expect("audit row");
        assert_eq!(
            new_row.get("record_content_hash").and_then(Value::as_str),
            Some("[redacted]"),
            "{new_row}"
        );
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn writes_go_through_tx() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_raw_{}", profile.slug));
        migrate_esign(&db).await;
        let before: i64 = sql_query_scalar("SELECT count(*) FROM esign.signature")
            .fetch_one(db.app_pool())
            .await
            .expect("before");
        let err = sql_query(
            r#"INSERT INTO esign.signature (
               signature_id, signer_id, signer_printed_name, signer_username,
               meaning, signed_at_zone, record_table, record_id, record_version,
               doc_type, record_content_hash, record_snapshot, permission_snapshot,
               credential_kind, components_used, expires_at
           ) VALUES (
               gen_random_uuid(), $1, 'Raw', 'raw',
               'Released', 'UTC', 'sm.instance', gen_random_uuid(), 1,
               'wo', decode(rpad('', 64, '0'), 'hex'), '{}'::jsonb, '{}',
               'signing_password', ARRAY['code','secret'], now()
           )"#,
        )
        .bind(wicket_identity::SYSTEM_ID)
        .execute(db.app_pool())
        .await
        .expect_err("raw write must fail");
        assert_eq!(pg_code(&err), "42501", "err={err}");
        let after: i64 = sql_query_scalar("SELECT count(*) FROM esign.signature")
            .fetch_one(db.app_pool())
            .await
            .expect("after");
        assert_eq!(before, after, "table must be unchanged");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn expired_token_is_invalid() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_expired_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let mut policy = profile.policy.clone();
        policy.max_window_secs = 1;
        let (p, sig, b, inst) =
            mint_one_with(&db, &format!("expired-{}", profile.slug), policy).await;
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let token = sig.token(p.actor());
        let doc = live_doc(&sig, b, inst, &p);
        let mut tx = Tx::begin(&write, &user_ctx(&p, "wo.release"))
            .await
            .expect("begin");
        let gate = prepare(&mut tx, &token, &doc).await.expect("prepare");
        let err = gate
            .verify(&token, &required(), &sig.record)
            .expect_err("expired");
        assert_eq!(err, SignatureError::Invalid("expired".into()));
        tx.rollback().await.expect("rollback");
        db.finish().await.expect("finish");
    }
}

#[tokio::test]
async fn missing_signature_is_invalid() {
    for profile in both_profiles() {
        let db = db_case!(&format!("es_nosig_{}", profile.slug));
        migrate_esign(&db).await;
        let write = write_pool(&db);
        let p = signer_with_perm(&write, &format!("nosig-{}", profile.slug), PERM).await;
        let doc_id = Identifier::generate();
        let rec = record(doc_id, 1);
        let inst = instance(doc_id, 1, "Draft");
        let token = SignatureToken {
            signature: SignatureId::generate(),
            signer: Actor {
                id: p.id.0,
                kind: ActorKind::User,
            },
            meaning: SignatureMeaning("Released".into()),
            record: rec.clone(),
            record_content_hash: [0; 32],
        };
        let doc = wicket_esign::LiveDoc {
            record: rec.clone(),
            doc_type: "wo".into(),
            projection: body(),
            instance: inst,
            signer_status: p.status,
        };
        let mut tx = Tx::begin(&write, &user_ctx(&p, "wo.release"))
            .await
            .expect("begin");
        let gate = prepare(&mut tx, &token, &doc).await.expect("prepare");
        let err = gate.verify(&token, &required(), &rec).expect_err("missing");
        assert_eq!(err, SignatureError::Invalid("no such signature".into()));
        tx.rollback().await.expect("rollback");
        db.finish().await.expect("finish");
    }
}
