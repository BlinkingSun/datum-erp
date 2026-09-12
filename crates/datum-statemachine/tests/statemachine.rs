//! Named commit-mode tests (SPEC).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use datum_core::{NoPostings, NoSignatures, PostingError, PostingSink};
use datum_db::Tx;
use datum_statemachine::{
    DocRef, EdgeBuilder, Engine, Error, HookPhase, Machine, ModuleNode, Veto, action_for,
};
use datum_test::db_case;

use common::{
    CollectingSink, CountingGate, actor_with_perm, catalog_audit_count, count_audit,
    draft_release_machine, dummy_intent, dummy_token, instance_state, machine_id_for,
    migrate_and_install, persist_spawn, pg_code, raw_insert_machine, system_ctx, table_count,
    write_pool,
};

/// Four-node diamond: A ← B, A ← C, B ← D, C ← D (D depends on B and C).
fn diamond_graph() -> Vec<ModuleNode> {
    vec![
        ModuleNode {
            id: "mod-a".into(),
            depends_on: vec![],
        },
        ModuleNode {
            id: "mod-b".into(),
            depends_on: vec!["mod-a".into()],
        },
        ModuleNode {
            id: "mod-c".into(),
            depends_on: vec!["mod-a".into()],
        },
        ModuleNode {
            id: "mod-d".into(),
            depends_on: vec!["mod-b".into(), "mod-c".into()],
        },
    ]
}

async fn engine_ready(required: bool) -> (Engine, DocRef) {
    let mut eng = Engine::new();
    eng.set_module_graph(diamond_graph()).expect("graph");
    eng.register_machine(draft_release_machine(required))
        .expect("machine");
    let doc = DocRef {
        doc_type: "wo".into(),
        doc_id: datum_core::Identifier::generate(),
    };
    (eng, doc)
}

#[tokio::test]
async fn hooks_run_in_topological_order_ties_by_module_id() {
    let db = db_case!("sm_topo");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(false).await;
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    for id in ["mod-d", "mod-c", "mod-a", "mod-b"] {
        let log = Arc::clone(&log);
        let name = id.to_string();
        eng.register_hook(
            id,
            "wo",
            "release",
            HookPhase::Before,
            200,
            move |_v, _s| {
                log.lock().unwrap().push(name.clone());
                Ok(())
            },
        )
        .expect("hook");
    }
    eng.freeze().expect("freeze");
    assert_eq!(
        eng.hook_order("wo", "release"),
        vec![
            "mod-a".to_string(),
            "mod-b".to_string(),
            "mod-c".to_string(),
            "mod-d".to_string()
        ]
    );
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let sink: Box<dyn PostingSink> = Box::new(NoPostings);
    eng.transition(&mut tx, sink, &doc, "release", None, &NoSignatures, &ctx)
        .await
        .expect("transition");
    tx.commit().await.expect("commit");
    assert_eq!(
        *log.lock().unwrap(),
        vec![
            "mod-a".to_string(),
            "mod-b".to_string(),
            "mod-c".to_string(),
            "mod-d".to_string()
        ]
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn veto_aborts_transaction_and_names_module() {
    let db = db_case!("sm_veto");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(false).await;
    eng.register_hook(
        "mod-a",
        "wo",
        "release",
        HookPhase::Before,
        200,
        |_v, _s| {
            Err(Veto {
                module: "mod-a".into(),
                reason: "operator not qualified".into(),
            })
        },
    )
    .expect("hook");
    eng.freeze().expect("freeze");
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let err = eng
        .transition(
            &mut tx,
            Box::new(NoPostings),
            &doc,
            "release",
            None,
            &NoSignatures,
            &ctx,
        )
        .await
        .expect_err("veto");
    assert!(
        matches!(err, Error::Veto { ref module, ref reason } if module == "mod-a" && reason.contains("qualified")),
        "got {err:?}"
    );
    tx.rollback().await.expect("rollback");
    assert_eq!(
        instance_state(db.app_pool(), &doc).await.as_deref(),
        Some("Draft")
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn hook_budget_overrun_fails_loudly() {
    let db = db_case!("sm_budget");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(false).await;
    eng.register_hook("mod-a", "wo", "release", HookPhase::Before, 5, |_v, _s| {
        std::thread::sleep(Duration::from_millis(50));
        Ok(())
    })
    .expect("hook");
    eng.freeze().expect("freeze");
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let err = eng
        .transition(
            &mut tx,
            Box::new(NoPostings),
            &doc,
            "release",
            None,
            &NoSignatures,
            &ctx,
        )
        .await
        .expect_err("budget");
    assert!(
        matches!(err, Error::HookBudgetExceeded { ref module, budget_ms: 5 } if module == "mod-a"),
        "got {err:?}"
    );
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn one_sink_per_transaction_and_finalize_after_all_hooks() {
    let db = db_case!("sm_sink");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(false).await;
    eng.register_hook(
        "mod-a",
        "wo",
        "release",
        HookPhase::Before,
        200,
        |_v, sink| {
            sink.contribute(dummy_intent()).expect("contrib before");
            Ok(())
        },
    )
    .expect("before");
    eng.register_hook(
        "mod-b",
        "wo",
        "release",
        HookPhase::After,
        200,
        |_v, sink| {
            sink.contribute(dummy_intent()).expect("contrib after");
            Ok(())
        },
    )
    .expect("after");
    eng.freeze().expect("freeze");
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let probe = CollectingSink::new();
    let contribs = Arc::clone(&probe.contribs);
    let finalized = Arc::clone(&probe.finalized);
    let sink = probe.into_box();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    eng.transition(&mut tx, sink, &doc, "release", None, &NoSignatures, &ctx)
        .await
        .expect("transition");
    tx.commit().await.expect("commit");
    assert!(finalized.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(contribs.lock().unwrap().len(), 2, "one group, both hooks");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn required_edge_refuses_without_token() {
    let db = db_case!("sm_notok");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(true).await;
    eng.freeze().expect("freeze");
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let err = eng
        .transition(
            &mut tx,
            Box::new(NoPostings),
            &doc,
            "release",
            None,
            &CountingGate::new(),
            &ctx,
        )
        .await
        .expect_err("no token");
    assert!(
        matches!(
            err,
            Error::Signature(datum_core::SignatureError::Invalid(_))
        ),
        "got {err:?}"
    );
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn required_edge_refuses_under_no_signatures() {
    let db = db_case!("sm_noprov");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(true).await;
    eng.freeze().expect("freeze");
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let token = dummy_token(&doc, 1);
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let err = eng
        .transition(
            &mut tx,
            Box::new(NoPostings),
            &doc,
            "release",
            Some(&token),
            &NoSignatures,
            &ctx,
        )
        .await
        .expect_err("NoProvider");
    assert!(
        matches!(
            err,
            Error::Signature(datum_core::SignatureError::NoProvider)
        ),
        "got {err:?}"
    );
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn not_required_edge_runs_without_gate_call() {
    let db = db_case!("sm_nogate");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(false).await;
    eng.freeze().expect("freeze");
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let gate = CountingGate::new();
    let token = dummy_token(&doc, 1);
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
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
    tx.commit().await.expect("commit");
    assert_eq!(gate.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn transition_writes_one_audit_row_with_action() {
    let db = db_case!("sm_audit");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(false).await;
    eng.freeze().expect("freeze");
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let action = action_for("wo", "release");
    let before = count_audit(db.app_pool(), "instance", &action).await;
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    eng.transition(
        &mut tx,
        Box::new(NoPostings),
        &doc,
        "release",
        None,
        &NoSignatures,
        &ctx,
    )
    .await
    .expect("transition");
    tx.commit().await.expect("commit");
    let after = count_audit(db.app_pool(), "instance", &action).await;
    assert_eq!(
        after - before,
        1,
        "exactly one audit row for the instance change"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn permission_denied_is_typed_and_leaves_no_row() {
    let db = db_case!("sm_perm");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(false).await;
    eng.freeze().expect("freeze");
    // Principal with a different permission.
    let (_, ctx) = actor_with_perm(&write, "wo.other", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let err = eng
        .transition(
            &mut tx,
            Box::new(NoPostings),
            &doc,
            "release",
            None,
            &NoSignatures,
            &ctx,
        )
        .await
        .expect_err("denied");
    assert!(
        matches!(err, Error::PermissionDenied { ref permission } if permission == "wo.release"),
        "got {err:?}"
    );
    tx.rollback().await.expect("rollback");
    assert_eq!(
        instance_state(db.app_pool(), &doc).await.as_deref(),
        Some("Draft")
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn after_hook_cannot_veto() {
    let db = db_case!("sm_after");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(false).await;
    eng.register_hook("mod-a", "wo", "release", HookPhase::After, 200, |_v, _s| {
        Err(Veto {
            module: "mod-a".into(),
            reason: "too late".into(),
        })
    })
    .expect("hook");
    eng.freeze().expect("freeze");
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let err = eng
        .transition(
            &mut tx,
            Box::new(NoPostings),
            &doc,
            "release",
            None,
            &NoSignatures,
            &ctx,
        )
        .await
        .expect_err("after veto");
    assert!(
        matches!(err, Error::AfterHookCannotVeto { ref module } if module == "mod-a"),
        "got {err:?}"
    );
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = db_case!("sm_raw_tx");
    migrate_and_install(&db).await;
    let before = table_count(db.app_pool(), "SELECT count(*) FROM sm.machine").await;
    let err = raw_insert_machine(db.app_pool()).await;
    assert_eq!(pg_code(&err), "42501", "err={err}");
    let after = table_count(db.app_pool(), "SELECT count(*) FROM sm.machine").await;
    assert_eq!(before, after, "table must be unchanged");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn startup_fails_when_required_edge_meets_no_signatures_in_release() {
    let m = draft_release_machine(true);
    let mut eng = Engine::new();
    eng.register_machine(m).expect("reg");
    let err = eng
        .check_gate_binding(true, true)
        .expect_err("release+noop");
    assert!(matches!(err, Error::StartupGate { .. }), "got {err:?}");
}

#[tokio::test]
async fn before_hook_error_still_finalizes_sink() {
    let db = db_case!("sm_before_fin");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(false).await;
    eng.register_hook(
        "mod-a",
        "wo",
        "release",
        HookPhase::Before,
        200,
        |_v, sink| {
            sink.contribute(dummy_intent())
                .expect("contrib before veto");
            Err(Veto {
                module: "mod-a".into(),
                reason: "before must finalize".into(),
            })
        },
    )
    .expect("before");
    eng.freeze().expect("freeze");
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let probe = CollectingSink::new();
    let finalized = Arc::clone(&probe.finalized);
    let sink = probe.into_box();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let err = eng
        .transition(&mut tx, sink, &doc, "release", None, &NoSignatures, &ctx)
        .await
        .expect_err("before-hook Err");
    assert!(
        matches!(err, Error::Veto { ref module, ref reason } if module == "mod-a" && reason.contains("finalize")),
        "got {err:?}"
    );
    assert!(
        finalized.load(std::sync::atomic::Ordering::SeqCst),
        "CONTRACT §6.2 rule 1: finalize after every hook including before-hook Err"
    );
    tx.rollback().await.expect("rollback");
    assert_eq!(
        instance_state(db.app_pool(), &doc).await.as_deref(),
        Some("Draft"),
        "before-hook Err must not mutate sm.instance"
    );
    db.finish().await.expect("finish");
}

#[test]
fn no_postings_finalize_reports_no_sink() {
    // CONTRACT §6.2: `pub struct NoPostings;   // contribute -> Err(NoSink); finalize -> Err(NoSink)`
    let mut np = NoPostings;
    assert!(
        matches!(np.contribute(dummy_intent()), Err(PostingError::NoSink)),
        "CONTRACT §6.2: contribute -> Err(NoSink)"
    );
    let sink: Box<dyn PostingSink> = Box::new(np);
    assert!(
        matches!(sink.finalize(), Err(PostingError::NoSink)),
        "CONTRACT §6.2: NoPostings finalize -> Err(NoSink) is the defined outcome, not silent success"
    );
}

#[tokio::test]
async fn after_hook_error_still_finalizes_sink() {
    let db = db_case!("sm_after_fin");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(false).await;
    eng.register_hook(
        "mod-a",
        "wo",
        "release",
        HookPhase::Before,
        200,
        |_v, sink| {
            sink.contribute(dummy_intent()).expect("contrib before");
            Ok(())
        },
    )
    .expect("before");
    eng.register_hook("mod-b", "wo", "release", HookPhase::After, 200, |_v, _s| {
        Err(Veto {
            module: "mod-b".into(),
            reason: "after must not veto".into(),
        })
    })
    .expect("after");
    eng.freeze().expect("freeze");
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let probe = CollectingSink::new();
    let finalized = Arc::clone(&probe.finalized);
    let sink = probe.into_box();
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let err = eng
        .transition(&mut tx, sink, &doc, "release", None, &NoSignatures, &ctx)
        .await
        .expect_err("after-hook Err");
    assert!(
        matches!(err, Error::AfterHookCannotVeto { ref module } if module == "mod-b"),
        "got {err:?}"
    );
    assert!(
        finalized.load(std::sync::atomic::Ordering::SeqCst),
        "CONTRACT §6.2 rule 1: finalize after every hook including after-hook Err"
    );
    tx.rollback().await.expect("rollback");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn transition_requires_frozen_registry() {
    let db = db_case!("sm_freeze");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(false).await;
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let persist_err = eng.persist(&mut tx).await.expect_err("persist unfrozen");
    assert!(
        matches!(persist_err, Error::NotFrozen),
        "got {persist_err:?}"
    );
    let spawn_err = eng
        .spawn(&mut tx, &doc, "Draft")
        .await
        .expect_err("spawn unfrozen");
    assert!(matches!(spawn_err, Error::NotFrozen), "got {spawn_err:?}");
    let trans_err = eng
        .transition(
            &mut tx,
            Box::new(NoPostings),
            &doc,
            "release",
            None,
            &NoSignatures,
            &ctx,
        )
        .await
        .expect_err("transition unfrozen");
    assert!(matches!(trans_err, Error::NotFrozen), "got {trans_err:?}");
    tx.rollback().await.expect("rollback");
    eng.freeze().expect("freeze");
    let frozen_err = eng
        .register_hook(
            "mod-a",
            "wo",
            "release",
            HookPhase::Before,
            200,
            |_v, _s| Ok(()),
        )
        .expect_err("register after freeze");
    assert!(matches!(frozen_err, Error::Frozen), "got {frozen_err:?}");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn concurrent_transition_is_rejected() {
    let db = db_case!("sm_lostupd");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (mut eng, doc) = engine_ready(false).await;
    eng.freeze().expect("freeze");
    let (_, ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    persist_spawn(&eng, &write, &ctx, &doc).await;
    let ctx_a = ctx.clone();
    let ctx_b = ctx.clone();
    let (r1, r2) = tokio::join!(
        async {
            let mut tx = Tx::begin(&write, &ctx_a).await.expect("begin a");
            let r = eng
                .transition(
                    &mut tx,
                    Box::new(NoPostings),
                    &doc,
                    "release",
                    None,
                    &NoSignatures,
                    &ctx_a,
                )
                .await;
            match &r {
                Ok(_) => tx.commit().await.expect("commit a"),
                Err(_) => tx.rollback().await.expect("rollback a"),
            }
            r
        },
        async {
            let mut tx = Tx::begin(&write, &ctx_b).await.expect("begin b");
            let r = eng
                .transition(
                    &mut tx,
                    Box::new(NoPostings),
                    &doc,
                    "release",
                    None,
                    &NoSignatures,
                    &ctx_b,
                )
                .await;
            match &r {
                Ok(_) => tx.commit().await.expect("commit b"),
                Err(_) => tx.rollback().await.expect("rollback b"),
            }
            r
        }
    );
    let wins = r1.is_ok() as u8 + r2.is_ok() as u8;
    assert_eq!(
        wins, 1,
        "exactly one concurrent transition must win; r1={r1:?} r2={r2:?}"
    );
    let err = match (r1, r2) {
        (Err(e), Ok(_)) | (Ok(_), Err(e)) => e,
        (r1, r2) => panic!("expected one Ok and one Err; r1={r1:?} r2={r2:?}"),
    };
    assert!(
        matches!(err, Error::InvalidState { .. }),
        "lost-update UPDATE … AND state = from AND version = expected → InvalidState, got {err:?}"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn build_twice_same_declaration_is_idempotent() {
    let db = db_case!("sm_idem");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let ctx = system_ctx("sm.persist");

    let (mut eng, _doc) = engine_ready(false).await;
    eng.freeze().expect("freeze");
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin 1");
    eng.persist(&mut tx).await.expect("persist 1");
    tx.commit().await.expect("commit 1");

    let id1 = machine_id_for(db.app_pool(), "wo").await;
    assert_eq!(
        table_count(db.app_pool(), "SELECT count(*) FROM sm.machine").await,
        1,
        "first persist writes one sm.machine row"
    );
    let audit_before = catalog_audit_count(db.app_pool()).await;
    assert!(
        audit_before > 0,
        "first persist must write catalog audit rows"
    );

    let (mut eng2, _doc2) = engine_ready(false).await;
    eng2.freeze().expect("freeze 2");
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin 2");
    eng2.persist(&mut tx).await.expect("persist 2");
    tx.commit().await.expect("commit 2");

    assert_eq!(
        table_count(db.app_pool(), "SELECT count(*) FROM sm.machine").await,
        1,
        "two builds on one database, one sm.machine row"
    );
    assert_eq!(
        machine_id_for(db.app_pool(), "wo").await,
        id1,
        "second persist returns the existing machine id"
    );
    assert_eq!(
        catalog_audit_count(db.app_pool()).await,
        audit_before,
        "second build writes no audit row"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn changed_declaration_is_refused() {
    let db = db_case!("sm_changed");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let ctx = system_ctx("sm.persist");

    let (mut eng, _doc) = engine_ready(false).await;
    eng.freeze().expect("freeze");
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin 1");
    eng.persist(&mut tx).await.expect("persist 1");
    tx.commit().await.expect("commit 1");

    let id1 = machine_id_for(db.app_pool(), "wo").await;
    let states_before = table_count(db.app_pool(), "SELECT count(*) FROM sm.state").await;
    let audit_before = catalog_audit_count(db.app_pool()).await;

    let changed = Machine::builder("wo")
        .state("Draft")
        .state("Released")
        .state("Void")
        .edge(
            EdgeBuilder::new("Draft", "Released", "release", "wo.release")
                .not_required("plain-shop; no signature on release"),
        )
        .edge(
            EdgeBuilder::new("Draft", "Void", "void", "wo.void")
                .not_required("void is not a quality decision"),
        )
        .build()
        .expect("changed machine");
    let mut eng2 = Engine::new();
    eng2.set_module_graph(diamond_graph()).expect("graph");
    eng2.register_machine(changed).expect("reg");
    eng2.freeze().expect("freeze 2");
    let mut tx = Tx::begin(&write, &ctx).await.expect("begin 2");
    let err = eng2
        .persist(&mut tx)
        .await
        .expect_err("changed declaration");
    assert!(
        matches!(err, Error::MachineChanged { ref doc_type } if doc_type == "wo"),
        "got {err:?}"
    );
    tx.rollback().await.expect("rollback");

    assert_eq!(
        table_count(db.app_pool(), "SELECT count(*) FROM sm.machine").await,
        1
    );
    assert_eq!(machine_id_for(db.app_pool(), "wo").await, id1);
    assert_eq!(
        table_count(db.app_pool(), "SELECT count(*) FROM sm.state").await,
        states_before,
        "catalog must be unchanged"
    );
    assert_eq!(
        catalog_audit_count(db.app_pool()).await,
        audit_before,
        "refused persist must not write"
    );
    db.finish().await.expect("finish");
}
