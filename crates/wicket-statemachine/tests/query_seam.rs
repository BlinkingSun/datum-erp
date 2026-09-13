//! Live-state query seam: current_state / instance_exists / machine_id_for
//! on the sealed Tx and on ReadPool under both LOGIN roles.
#![allow(missing_docs)]
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use wicket_core::{Identifier, NoPostings, NoSignatures, PostingSink};
use wicket_db::Tx;
use wicket_statemachine::{
    DocRef, Engine, current_state, current_state_on, instance_exists, instance_exists_on,
    machine_id_for, machine_id_for_on,
};
use wicket_test::db_case;

use common::{
    actor_with_perm, draft_release_machine, migrate_and_install, persist_spawn, read_pool_app,
    read_pool_migrate, system_ctx, write_pool,
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

#[tokio::test]
async fn current_state_none_then_some_after_spawn_then_released() {
    let db = db_case!("sm_qs_state");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (eng, doc) = frozen_engine();
    let ctx = system_ctx("sm.query_seam");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    assert!(
        current_state(&mut tx, &doc).await.unwrap().is_none(),
        "no instance yet"
    );
    assert!(
        !instance_exists(&mut tx, &doc).await.unwrap(),
        "no instance yet"
    );
    assert!(
        machine_id_for(&mut tx, "wo").await.unwrap().is_none(),
        "catalog empty before persist"
    );
    eng.persist(&mut tx).await.expect("persist");
    let catalog_id = machine_id_for(&mut tx, "wo")
        .await
        .unwrap()
        .expect("catalog after persist");
    assert!(
        !instance_exists(&mut tx, &doc).await.unwrap(),
        "persist does not spawn"
    );
    let inst = eng.spawn(&mut tx, &doc, "Draft").await.expect("spawn");
    assert_eq!(inst.machine_id, catalog_id);
    assert_eq!(
        current_state(&mut tx, &doc).await.unwrap().map(|s| s.0),
        Some("Draft".into())
    );
    assert!(instance_exists(&mut tx, &doc).await.unwrap());
    tx.commit().await.expect("commit spawn");

    let (_, rel_ctx) = actor_with_perm(&write, "wo.release", &doc, "release").await;
    let mut tx = Tx::begin(&write, &rel_ctx).await.unwrap();
    let sink: Box<dyn PostingSink> = Box::new(NoPostings);
    eng.transition(
        &mut tx,
        sink,
        &doc,
        "release",
        None,
        &NoSignatures,
        &rel_ctx,
    )
    .await
    .expect("transition");
    assert_eq!(
        current_state(&mut tx, &doc).await.unwrap().map(|s| s.0),
        Some("Released".into()),
        "query seam follows the executor; transition behaviour is unchanged"
    );
    tx.commit().await.expect("commit release");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn query_seam_on_read_pool_under_app_and_migrate() {
    let db = db_case!("sm_qs_roles");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let (eng, doc) = frozen_engine();
    let ctx = system_ctx("sm.query_seam");
    persist_spawn(&eng, &write, &ctx, &doc).await;

    let missing = DocRef {
        doc_type: "wo".into(),
        doc_id: Identifier::generate(),
    };
    let app = read_pool_app(&db);
    let mig = read_pool_migrate(&db);
    for (label, pool) in [("wicket_app", &app), ("wicket_migrate", &mig)] {
        let state = current_state_on(pool, &doc)
            .await
            .unwrap_or_else(|e| panic!("{label} current_state: {e}"));
        assert_eq!(state.map(|s| s.0), Some("Draft".into()), "{label}");
        assert!(
            instance_exists_on(pool, &doc)
                .await
                .unwrap_or_else(|e| panic!("{label} instance_exists: {e}")),
            "{label}"
        );
        assert!(
            machine_id_for_on(pool, "wo")
                .await
                .unwrap_or_else(|e| panic!("{label} machine_id_for: {e}"))
                .is_some(),
            "{label}"
        );
        assert!(
            current_state_on(pool, &missing)
                .await
                .unwrap_or_else(|e| panic!("{label} missing state: {e}"))
                .is_none(),
            "{label} missing instance"
        );
        assert!(
            !instance_exists_on(pool, &missing)
                .await
                .unwrap_or_else(|e| panic!("{label} missing exists: {e}")),
            "{label} missing instance"
        );
        assert!(
            machine_id_for_on(pool, "no-such-doc")
                .await
                .unwrap_or_else(|e| panic!("{label} missing machine: {e}"))
                .is_none(),
            "{label} unknown doc_type"
        );
    }
    db.finish().await.expect("finish");
}
