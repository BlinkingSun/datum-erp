//! Finding 2: composed kernel path as `datum-module` would drive it.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_core::{
    AnyQuantity, Boundary, ConversionContext, CostElement, CountDim, CurrencyId, DimensionKind,
    Identifier, ItemId, LocationId, Money, NoPostings, PermissionKey, PostingIntent, PostingSink,
    Quantity, QuantityPosting, SignatureError, SignatureGate, SignatureId, SignatureMeaning,
    SignatureRequirement, SignatureToken, UnitId, UnitRef, ValueAccount, ValuePosting,
};
use datum_db::Tx;
use datum_esign::{InstanceTriple, LiveDoc, MintRequest, identity_projection, mint};
use datum_identity::{PrincipalStatus, deactivate_principal};
use datum_ledger::{CostMethod, rebuild, upsert_location, upsert_stock_item, verify_projection};
use datum_statemachine::{DocRef, EdgeBuilder, Machine, Veto};
use datum_test::db_case;
use rust_decimal::Decimal;
use serde_json::json;
use sqlx::query_scalar;

use datum_module::{Error, KERNEL_ORDER, Kernel, Profile, SignatureEdge};

use common::{
    SIGNING_SECRET, actor_with_perms, boot_ctx, has_zz_audit, migrate_and_install, pg_code,
    signer_with_perms,
};

const DOC_TYPE: &str = "e2e.doc";
const RECEIVE: &str = "receive";
const ISSUE: &str = "issue";
const APPROVE: &str = "approve";
const EA: UnitId = UnitId(1);

fn qty_ea(n: i64) -> AnyQuantity {
    AnyQuantity {
        amount: Decimal::from(n),
        unit: EA,
        dimension: DimensionKind::Count,
    }
}

fn usd(n: i64) -> Money {
    Money::new(Decimal::from(n), CurrencyId(840)).expect("usd")
}

fn q_post(
    item: ItemId,
    qty: AnyQuantity,
    location: LocationId,
    boundary: Option<Boundary>,
) -> QuantityPosting {
    QuantityPosting {
        item,
        quantity: qty,
        location,
        boundary,
        lot: None,
        serial: None,
        entered: None,
    }
}

fn veto_of(err: datum_core::PostingError) -> Veto {
    Veto {
        module: "mod-inventory".into(),
        reason: err.to_string(),
    }
}

fn contribute_receipt(
    sink: &mut dyn PostingSink,
    item: ItemId,
    stock: LocationId,
    supplier: LocationId,
) -> core::result::Result<(), Veto> {
    let recv = sink
        .contribute(PostingIntent::Quantity(q_post(
            item,
            qty_ea(1),
            stock,
            None,
        )))
        .map_err(veto_of)?;
    sink.contribute(PostingIntent::Quantity(q_post(
        item,
        qty_ea(-1),
        supplier,
        Some(Boundary::Supplier),
    )))
    .map_err(veto_of)?;
    sink.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Inventory,
        cost_element: CostElement::Material,
        cost_object: None,
        amount: usd(10),
        values: Some(recv),
    }))
    .map_err(veto_of)?;
    sink.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::ApAccrual,
        cost_element: CostElement::Material,
        cost_object: None,
        amount: usd(-10),
        values: None,
    }))
    .map_err(veto_of)?;
    Ok(())
}

fn contribute_issue(
    sink: &mut dyn PostingSink,
    item: ItemId,
    stock: LocationId,
    customer: LocationId,
) -> core::result::Result<(), Veto> {
    let out = sink
        .contribute(PostingIntent::Quantity(q_post(
            item,
            qty_ea(-1),
            stock,
            None,
        )))
        .map_err(veto_of)?;
    sink.contribute(PostingIntent::Quantity(q_post(
        item,
        qty_ea(1),
        customer,
        Some(Boundary::Customer),
    )))
    .map_err(veto_of)?;
    sink.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Inventory,
        cost_element: CostElement::Material,
        cost_object: None,
        amount: usd(-10),
        values: Some(out),
    }))
    .map_err(veto_of)?;
    sink.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Cogs,
        cost_element: CostElement::Material,
        cost_object: None,
        amount: usd(10),
        values: None,
    }))
    .map_err(veto_of)?;
    Ok(())
}

/// One regulated machine: total `SignatureDeclaration` on every edge; one Required,
/// two sequential NotRequired (receipt then issue). A single plain edge cannot
/// sequence two postings under `NoSignatures`.
fn e2e_machine() -> Machine {
    Machine::builder(DOC_TYPE)
        .regulated(true)
        .state("Open")
        .state("Received")
        .state("Issued")
        .state("Closed")
        .edge(
            EdgeBuilder::new("Open", "Received", RECEIVE, "wo.release")
                .not_required("lot-less receipt; unsigned under NoSignatures"),
        )
        .edge(
            EdgeBuilder::new("Received", "Issued", ISSUE, "wo.release")
                .not_required("lot-less issue; unsigned under NoSignatures"),
        )
        .edge(
            EdgeBuilder::new("Open", "Closed", APPROVE, "calibration.approve").required(
                SignatureRequirement {
                    meaning: SignatureMeaning("Approved".into()),
                    permission: PermissionKey("calibration.approve".into()),
                },
            ),
        )
        .build()
        .unwrap()
}

fn dummy_token(doc: &DocRef, version: i64, actor: datum_core::Actor) -> SignatureToken {
    SignatureToken {
        signature: SignatureId::generate(),
        signer: actor,
        meaning: SignatureMeaning("Approved".into()),
        record: datum_core::RecordRef {
            table: "sm.instance".into(),
            id: doc.doc_id,
            version,
        },
        record_content_hash: [0; 32],
    }
}

async fn assert_migrators_and_history(db: &datum_test::TestDb) {
    assert!(
        has_zz_audit(db.migrate_pool(), "datum", "schema_history").await,
        "schema_history attached after install_upto"
    );
    let crates: Vec<String> = sqlx::query_scalar("SELECT DISTINCT crate FROM datum.schema_history")
        .fetch_all(db.app_pool())
        .await
        .expect("schema_history");
    for name in KERNEL_ORDER.iter().copied() {
        assert!(
            crates.iter().any(|c| c == name),
            "schema_history missing {name}, have {crates:?}"
        );
    }
}

async fn assert_builtins(db: &datum_test::TestDb) {
    let n: i64 = query_scalar(
        "SELECT count(*) FROM identity.principal WHERE username IN ('system', 'migration')",
    )
    .fetch_one(db.app_pool())
    .await
    .expect("builtins");
    assert_eq!(n, 2, "Kernel::build seeds identity builtins");
}

async fn seed_item_world(
    write: &datum_db::WritePool,
    item: ItemId,
    stock: LocationId,
    supplier: LocationId,
    customer: LocationId,
) {
    let mut tx = Tx::begin(write, &boot_ctx()).await.expect("seed world");
    upsert_stock_item(&mut tx, item, EA, 0, Decimal::ZERO, CostMethod::Fifo, None)
        .await
        .expect("stock item");
    tx.execute(
        sqlx::query(
            "INSERT INTO uom.item_stock (item_id, stock_unit_id, stock_scale)
             VALUES ($1, $2, $3)",
        )
        .bind(item.as_uuid())
        .bind(EA.0)
        .bind(0_i16),
    )
    .await
    .expect("uom.item_stock");
    upsert_location(&mut tx, stock, None)
        .await
        .expect("stock loc");
    upsert_location(&mut tx, supplier, Some(Boundary::Supplier))
        .await
        .expect("supplier");
    upsert_location(&mut tx, customer, Some(Boundary::Customer))
        .await
        .expect("customer");
    tx.commit().await.expect("commit seed");
}

async fn count_audit(pool: &sqlx::PgPool, actor: Identifier, table: &str, op: &str) -> i64 {
    query_scalar(
        "SELECT count(*) FROM audit.event
          WHERE actor_id = $1 AND table_name = $2 AND op = $3",
    )
    .bind(actor.as_uuid())
    .bind(table)
    .bind(op)
    .fetch_one(pool)
    .await
    .expect("audit count")
}

async fn assert_conservation(pool: &sqlx::PgPool) {
    let bad_qty: i64 = query_scalar(
        "SELECT count(*) FROM (
            SELECT group_id, item_id, uom_id
              FROM ledger.posting
             WHERE measure = 'QUANTITY'
             GROUP BY group_id, item_id, uom_id
            HAVING SUM(quantity) <> 0
         ) t",
    )
    .fetch_one(pool)
    .await
    .expect("p1");
    assert_eq!(
        bad_qty, 0,
        "per-slice quantity conservation (group, item, uom)"
    );
    let bad_val: i64 = query_scalar(
        "SELECT count(*) FROM (
            SELECT group_id
              FROM ledger.posting
             WHERE measure = 'VALUE'
             GROUP BY group_id
            HAVING SUM(amount) <> 0
         ) t",
    )
    .fetch_one(pool)
    .await
    .expect("p2");
    assert_eq!(bad_val, 0, "per-group value conservation");
}

async fn assert_actor_audit(pool: &sqlx::PgPool, actor: Identifier, spec_version: &str) {
    assert_eq!(count_audit(pool, actor, "instance", "INSERT").await, 2);
    assert_eq!(count_audit(pool, actor, "instance", "UPDATE").await, 2);
    assert_eq!(count_audit(pool, actor, "posting_group", "INSERT").await, 2);
    assert_eq!(count_audit(pool, actor, "posting", "INSERT").await, 8);
    assert_eq!(count_audit(pool, actor, "consumption", "INSERT").await, 1);
    assert_eq!(count_audit(pool, actor, "event", "INSERT").await, 1);
    let unstamped: i64 = query_scalar(
        "SELECT count(*) FROM audit.event
          WHERE actor_id = $1
            AND (app_version IS NULL OR app_version = ''
                 OR config_version IS DISTINCT FROM $2)",
    )
    .bind(actor.as_uuid())
    .bind(spec_version)
    .fetch_one(pool)
    .await
    .expect("stamps");
    assert_eq!(
        unstamped, 0,
        "invariants 3/5/17: actor audit rows carry app_version and config_version"
    );
}

async fn instance_state(pool: &sqlx::PgPool, doc: &DocRef) -> String {
    query_scalar("SELECT state FROM sm.instance WHERE doc_type = $1 AND doc_id = $2")
        .bind(&doc.doc_type)
        .bind(doc.doc_id.as_uuid())
        .fetch_one(pool)
        .await
        .expect("instance state")
}

async fn instance_row_count(pool: &sqlx::PgPool, doc: &DocRef) -> i64 {
    query_scalar("SELECT count(*) FROM sm.instance WHERE doc_type = $1 AND doc_id = $2")
        .bind(&doc.doc_type)
        .bind(doc.doc_id.as_uuid())
        .fetch_one(pool)
        .await
        .expect("instance exists")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LedgerSnap {
    posting: i64,
    posting_group: i64,
    consumption: i64,
    instance_state: String,
}

async fn ledger_snap(pool: &sqlx::PgPool, doc: &DocRef) -> LedgerSnap {
    let posting: i64 = query_scalar("SELECT count(*) FROM ledger.posting")
        .fetch_one(pool)
        .await
        .expect("posting count");
    let posting_group: i64 = query_scalar("SELECT count(*) FROM ledger.posting_group")
        .fetch_one(pool)
        .await
        .expect("posting_group count");
    let consumption: i64 = query_scalar("SELECT count(*) FROM ledger.consumption")
        .fetch_one(pool)
        .await
        .expect("consumption count");
    LedgerSnap {
        posting,
        posting_group,
        consumption,
        instance_state: instance_state(pool, doc).await,
    }
}

async fn composed_path(db: &datum_test::TestDb, profile: Profile, rebuild_projections: bool) {
    assert_migrators_and_history(db).await;

    let item = ItemId::generate();
    let stock = LocationId::generate();
    let supplier = LocationId::generate();
    let customer = LocationId::generate();

    let mut builder = Kernel::builder(db.app_pool().clone(), profile);
    builder.register_machine(e2e_machine()).unwrap();
    builder.register_projection(DOC_TYPE, identity_projection);
    builder.register_hook("mod-inventory", DOC_TYPE, RECEIVE, move |_v, sink| {
        contribute_receipt(sink, item, stock, supplier)
    });
    builder.register_hook("mod-inventory", DOC_TYPE, ISSUE, move |_v, sink| {
        contribute_issue(sink, item, stock, customer)
    });
    let mut kernel = builder.build().await.expect("Kernel::build");
    assert_builtins(db).await;

    let late = Machine::builder("late.e2e")
        .edge(EdgeBuilder::new("X", "Y", "n", "wo.release"))
        .build()
        .unwrap();
    let frozen = kernel.engine.register_machine(late).expect_err("frozen");
    assert!(
        matches!(frozen, datum_statemachine::Error::Frozen),
        "got {frozen:?}"
    );

    let stored = datum_module::export_manifest(db.app_pool())
        .await
        .expect("manifest");
    assert!(
        kernel.profile.required_edges().iter().any(|e| matches!(
            e,
            SignatureEdge::Required { edge, module, .. }
                if edge == APPROVE && module == DOC_TYPE
        )),
        "live Required set lists {DOC_TYPE}.{APPROVE}"
    );
    assert!(
        stored.signature_edges.iter().any(|e| e.is_required()
            && matches!(
                e,
                SignatureEdge::Required { edge, module, .. }
                    if edge == APPROVE && module == DOC_TYPE
            )),
        "must-assert 7: manifest lists the Required edge"
    );

    let write = kernel.write_pool();
    seed_item_world(&write, item, stock, supplier, customer).await;

    let actor = actor_with_perms(&write, &["wo.release", "calibration.approve"]).await;
    let flow = DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: Identifier::generate(),
    };
    let mut recv_ctx = kernel.transition_context(actor, &flow, RECEIVE);
    recv_ctx.actor_display = Some("Operator".into());
    recv_ctx.reason = Some("kernel-e2e".into());

    let pool = db.app_pool();
    let mut tx = Tx::begin(&write, &recv_ctx).await.expect("spawn flow");
    kernel
        .spawn(&mut tx, &flow, "Open")
        .await
        .expect("spawn flow");
    tx.commit().await.expect("commit spawn flow");

    assert_eq!(
        instance_row_count(pool, &flow).await,
        1,
        "spawned instance row"
    );
    assert_eq!(
        instance_state(pool, &flow).await,
        "Open",
        "spawned instance is Open"
    );
    let spawn_inserts = count_audit(pool, actor.id, "instance", "INSERT").await;
    assert_eq!(
        spawn_inserts, 1,
        "actor instance INSERT rows immediately after spawn, got {spawn_inserts}"
    );

    let mut tx = Tx::begin(&write, &recv_ctx).await.expect("receive tx");
    let inst = kernel
        .transition(&mut tx, &flow, RECEIVE, None, &recv_ctx)
        .await
        .expect("plain receive");
    assert_eq!(inst.state.0, "Received");
    tx.commit().await.expect("commit receive");

    assert_eq!(
        instance_state(pool, &flow).await,
        "Received",
        "receive committed state"
    );
    let recv_group: sqlx::types::Uuid =
        query_scalar("SELECT group_id FROM ledger.posting_group WHERE source_id = $1")
            .bind(flow.doc_id.as_uuid())
            .fetch_one(pool)
            .await
            .expect("receive group id");
    let recv_postings: i64 =
        query_scalar("SELECT count(*) FROM ledger.posting WHERE group_id = $1")
            .bind(recv_group)
            .fetch_one(pool)
            .await
            .expect("receive postings");
    assert_eq!(recv_postings, 4, "receive group {recv_group} posting count");

    let mut tx = Tx::begin(&write, &recv_ctx).await.expect("immut tx");
    let err = datum_uom::update_item_stock(
        &mut tx,
        item,
        datum_uom::ItemStockMeasure {
            stock_unit: EA,
            stock_scale: 2,
            residual_tolerance: Decimal::ZERO,
        },
    )
    .await
    .expect_err("stock unit immutable while ledger.posting rows exist");
    assert!(
        matches!(err, datum_uom::Error::StockMeasureImmutable),
        "got {err:?}"
    );
    tx.rollback().await.expect("rollback immut");

    let mut tx = Tx::begin(&write, &recv_ctx).await.expect("uom/event tx");
    let catalog = datum_uom::load_catalog(&mut tx).await.expect("catalog");
    let ea = UnitRef::<CountDim>::checked(EA, DimensionKind::Count).unwrap();
    let entered = AnyQuantity::from(Quantity::new(Decimal::from(1), ea).unwrap());
    let conv = kernel
        .to_stock::<CountDim>(
            &mut tx,
            &catalog,
            item,
            entered,
            &ConversionContext { item, lot: None },
        )
        .await
        .expect("to_stock after receive commit");
    assert_eq!(conv.canonical.amount(), Decimal::ONE);
    assert_eq!(conv.factor, Decimal::ONE);

    let event = datum_events::Event::builder()
        .name("inventory.lot_received")
        .version(1)
        .payload(json!({
            "item_id": item.as_uuid().to_string(),
            "lot_id": Identifier::generate().as_uuid().to_string(),
        }))
        .build_with(&kernel.event_schemas)
        .expect("event");
    kernel
        .publish_event(&mut tx, event)
        .await
        .expect("publish after receive commit");
    tx.commit().await.expect("commit uom/event");

    let event_rows: i64 = query_scalar("SELECT count(*) FROM app.event WHERE actor_id = $1")
        .bind(actor.id.as_uuid())
        .fetch_one(pool)
        .await
        .expect("app.event");
    assert_eq!(event_rows, 1, "one app.event for the actor");
    assert_eq!(count_audit(pool, actor.id, "event", "INSERT").await, 1);

    let dispatched = kernel
        .dispatch_tick(Kernel::service_actor())
        .await
        .expect("dispatch");
    assert!(dispatched >= 1, "genealogy bridge ran, got {dispatched}");
    let ran = kernel
        .worker_tick(Kernel::service_actor())
        .await
        .expect("worker");
    assert_eq!(ran, 1, "one job through datum-jobs");
    let done: i64 =
        query_scalar("SELECT count(*) FROM transient.job WHERE kind = $1 AND state = 'succeeded'")
            .bind(datum_jobs::events::bridge_job_kind())
            .fetch_one(pool)
            .await
            .expect("job done");
    assert_eq!(done, 1, "one genealogy.refresh succeeded");

    let mut issue_ctx = kernel.transition_context(actor, &flow, ISSUE);
    issue_ctx.actor_display = Some("Operator".into());
    issue_ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&write, &issue_ctx).await.expect("issue tx");
    let inst = kernel
        .transition(&mut tx, &flow, ISSUE, None, &issue_ctx)
        .await
        .expect("plain issue");
    assert_eq!(inst.state.0, "Issued");
    tx.commit().await.expect("commit issue");

    assert_eq!(
        instance_state(pool, &flow).await,
        "Issued",
        "issue committed state"
    );
    let cons: i64 = query_scalar("SELECT count(*) FROM ledger.consumption")
        .fetch_one(pool)
        .await
        .expect("consumption");
    assert_eq!(cons, 1, "issue wrote one ledger.consumption");

    assert_conservation(pool).await;

    let sig = DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id: Identifier::generate(),
    };
    let mut appr_ctx = kernel.transition_context(actor, &sig, APPROVE);
    appr_ctx.actor_display = Some("Operator".into());
    appr_ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&write, &appr_ctx).await.expect("spawn sig");
    kernel
        .spawn(&mut tx, &sig, "Open")
        .await
        .expect("spawn sig");
    tx.commit().await.expect("commit spawn sig");

    assert_actor_audit(pool, actor.id, &kernel.profile.spec_version).await;

    let before = ledger_snap(pool, &sig).await;
    let token = dummy_token(&sig, 1, actor);
    let mut tx = Tx::begin(&write, &appr_ctx).await.expect("approve tx");
    let err = kernel
        .transition(&mut tx, &sig, APPROVE, Some(&token), &appr_ctx)
        .await
        .expect_err("Required must refuse a dummy token");
    if kernel.gate_is_noop() {
        assert!(
            matches!(
                err,
                datum_module::Error::Statemachine(datum_statemachine::Error::Signature(
                    SignatureError::NoProvider
                ))
            ),
            "must-assert 7 typed error, got {err:?}"
        );
    } else {
        assert!(
            matches!(
                err,
                datum_module::Error::Statemachine(datum_statemachine::Error::Signature(
                    SignatureError::Invalid(_)
                ))
            ),
            "esign-bound dummy token is Invalid, got {err:?}"
        );
    }
    tx.rollback().await.ok();
    let after = ledger_snap(pool, &sig).await;
    assert_eq!(
        before, after,
        "Required refusal left posting/posting_group/consumption and instance state unchanged"
    );

    if rebuild_projections {
        let mut tx = Tx::begin(&write, &boot_ctx()).await.expect("rebuild tx");
        verify_projection(&mut tx)
            .await
            .expect("live projection equals fold");
        rebuild(&mut tx).await.expect("rebuild from scratch");
        verify_projection(&mut tx)
            .await
            .expect("rebuilt projection equals fold");
        tx.commit().await.expect("commit rebuild");
    }
}

#[tokio::test]
async fn kernel_e2e_plain_shop() {
    let db = db_case!("e2e_ps");
    migrate_and_install(&db).await;
    composed_path(&db, Profile::plain_shop().unwrap(), false).await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn kernel_e2e_regulated_device() {
    let db = db_case!("e2e_rd");
    migrate_and_install(&db).await;
    composed_path(&db, Profile::regulated_device().unwrap(), false).await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn kernel_e2e_no_actor_aborts() {
    let db = db_case!("e2e_na");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::plain_shop().unwrap())
        .await
        .expect("build");
    let _ = kernel;
    let before: i64 = query_scalar("SELECT count(*) FROM ledger.location")
        .fetch_one(db.app_pool())
        .await
        .expect("before");
    let err = sqlx::query(
        "INSERT INTO ledger.location (location_id, boundary_class)
         VALUES ($1, NULL)",
    )
    .bind(Identifier::generate().as_uuid())
    .execute(db.app_pool())
    .await
    .expect_err("raw write must fail");
    assert_eq!(pg_code(&err), "42501", "must-assert 6 err={err}");
    let after: i64 = query_scalar("SELECT count(*) FROM ledger.location")
        .fetch_one(db.app_pool())
        .await
        .expect("after");
    assert_eq!(before, after, "write with no actor left nothing behind");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn kernel_e2e_projection_rebuild_equals_fold() {
    let db = db_case!("e2e_pr");
    migrate_and_install(&db).await;
    composed_path(&db, Profile::plain_shop().unwrap(), true).await;
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn kernel_e2e_regulated_release_with_real_signature() {
    let db = db_case!("e2e_sig");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::regulated_device().unwrap())
        .await
        .expect("build regulated");
    assert!(!kernel.gate_is_noop(), "regulated-device binds datum-esign");

    let write = kernel.write_pool();
    let principal = signer_with_perms(&write, &["calibration.approve"]).await;
    let doc = DocRef {
        doc_type: "calibration.certificate".into(),
        doc_id: Identifier::generate(),
    };
    let mut spawn_ctx = kernel.transition_context(principal.actor(), &doc, "approve");
    spawn_ctx.actor_display = Some(principal.display_name.clone());
    spawn_ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&write, &spawn_ctx).await.expect("spawn begin");
    kernel
        .spawn(&mut tx, &doc, "Open")
        .await
        .expect("spawn certificate");
    tx.commit().await.expect("spawn commit");

    let (state, version): (String, i64) = sqlx::query_as(
        r#"SELECT state, version FROM sm.instance
            WHERE doc_type = $1 AND doc_id = $2"#,
    )
    .bind(&doc.doc_type)
    .bind(doc.doc_id.as_uuid())
    .fetch_one(db.app_pool())
    .await
    .expect("instance");
    assert_eq!(state, "Open");

    let inst = InstanceTriple {
        doc_type: doc.doc_type.clone(),
        doc_id: doc.doc_id,
        state: state.clone(),
        version,
    };
    let rec = datum_core::RecordRef {
        table: "sm.instance".into(),
        id: doc.doc_id,
        version,
    };
    let mut mint_tx = Tx::begin(&write, &boot_ctx()).await.expect("mint begin");
    let projection = kernel
        .live_record(&mut mint_tx, &doc.doc_type, doc.doc_id)
        .await
        .expect("live record");
    let sig = mint(
        &mut mint_tx,
        &MintRequest {
            components: vec!["code".into(), "secret".into()],
            code: Some(principal.username.clone()),
            secret: SIGNING_SECRET.into(),
            meaning: SignatureMeaning("Approved".into()),
            reason: None,
            record: rec.clone(),
            doc_type: doc.doc_type.clone(),
            projection: projection.clone(),
            instance: inst.clone(),
            permission: PermissionKey("calibration.approve".into()),
            signed_at_zone: kernel.profile.seeded_permissions.display_timezone.clone(),
            policy: kernel.profile.session_policy.clone(),
            principal: principal.clone(),
            login_session_id: None,
            device_fingerprint: Some("e2e-tablet".into()),
            source_ip: Some("127.0.0.1".into()),
            boot_epoch: "1".into(),
            credential_kind: "signing_password".into(),
        },
    )
    .await
    .expect("mint both components");
    mint_tx.commit().await.expect("mint commit");

    let token = sig.token(principal.actor());
    let mut ctx = kernel.transition_context(principal.actor(), &doc, "approve");
    ctx.actor_display = Some(principal.display_name.clone());
    ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&write, &ctx).await.expect("transition begin");
    let inst_out = kernel
        .transition(&mut tx, &doc, "approve", Some(&token), &ctx)
        .await
        .expect("Required edge consumes the minted signature");
    assert_eq!(inst_out.state.0, "Approved");
    let xid = tx.pg_txid().await.expect("xid");
    tx.commit().await.expect("transition commit");

    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        r#"SELECT xid::text, esign_id::text FROM audit.event
            WHERE xid = $1::xid8"#,
    )
    .bind(&xid)
    .fetch_all(db.app_pool())
    .await
    .expect("audit events");
    assert!(
        !rows.is_empty(),
        "transition wrote audit rows for xid {xid}"
    );
    assert!(
        rows.iter()
            .all(|(_, e)| e.as_deref() == Some(&sig.id.to_string())),
        "every audit row of the transition carries esign_id: {rows:?}"
    );
    let seals: i64 = query_scalar("SELECT count(*) FROM audit.tx_seal WHERE xid = $1::xid8")
        .bind(&xid)
        .fetch_one(db.app_pool())
        .await
        .expect("seals");
    assert_eq!(seals, 1, "one audit.tx_seal row for the shared xid");

    let live = LiveDoc {
        record: rec.clone(),
        doc_type: doc.doc_type.clone(),
        projection,
        instance: inst,
        signer_status: PrincipalStatus::Active,
    };
    let mut tx = Tx::begin(&write, &ctx).await.expect("second begin");
    let gate = kernel
        .signature_gate_factory()
        .prepare(&mut tx, &token, &live)
        .await
        .expect("second prepare");
    let err = gate
        .verify(
            &token,
            &SignatureRequirement {
                meaning: SignatureMeaning("Approved".into()),
                permission: PermissionKey("calibration.approve".into()),
            },
            &rec,
        )
        .expect_err("second use");
    assert_eq!(err, SignatureError::Consumed);
    tx.rollback().await.ok();
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn regulated_release_refused_without_signature_succeeds_with_two_component_signature() {
    let db_plain = db_case!("e2e_cmpp");
    migrate_and_install(&db_plain).await;

    let plain = Kernel::build(db_plain.app_pool(), Profile::plain_shop().unwrap())
        .await
        .expect("plain");
    let plain_write = plain.write_pool();
    let wo = DocRef {
        doc_type: "wo".into(),
        doc_id: Identifier::generate(),
    };
    let actor = actor_with_perms(&plain_write, &["wo.release"]).await;
    let mut ctx = plain.transition_context(actor, &wo, "release");
    ctx.actor_display = Some("Operator".into());
    ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&plain_write, &ctx).await.expect("spawn wo");
    plain.spawn(&mut tx, &wo, "Draft").await.expect("spawn wo");
    tx.commit().await.expect("spawn wo commit");
    let mut tx = Tx::begin(&plain_write, &ctx).await.expect("plain rel");
    plain
        .transition(&mut tx, &wo, "release", None, &ctx)
        .await
        .expect("plain-shop unsigned release");
    tx.commit().await.expect("plain commit");
    assert!(
        matches!(
            plain.signature_gate().verify(
                &dummy_token(&wo, 1, actor),
                &SignatureRequirement {
                    meaning: SignatureMeaning("Released".into()),
                    permission: PermissionKey("wo.release".into()),
                },
                &datum_core::RecordRef {
                    table: "sm.instance".into(),
                    id: wo.doc_id,
                    version: 1,
                },
            ),
            Err(SignatureError::NoProvider)
        ),
        "plain-shop signature_gate stays NoSignatures"
    );
    db_plain.finish().await.expect("plain finish");

    let db = db_case!("e2e_cmpr");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::regulated_device().unwrap())
        .await
        .expect("regulated");
    let write = kernel.write_pool();
    let principal = signer_with_perms(&write, &["calibration.approve"]).await;
    let doc = DocRef {
        doc_type: "calibration.certificate".into(),
        doc_id: Identifier::generate(),
    };
    let mut spawn_ctx = kernel.transition_context(principal.actor(), &doc, "approve");
    spawn_ctx.actor_display = Some(principal.display_name.clone());
    spawn_ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&write, &spawn_ctx).await.expect("spawn");
    kernel.spawn(&mut tx, &doc, "Open").await.expect("spawn");
    tx.commit().await.expect("spawn commit");

    let dummy = dummy_token(&doc, 1, principal.actor());
    let required = SignatureRequirement {
        meaning: SignatureMeaning("Approved".into()),
        permission: PermissionKey("calibration.approve".into()),
    };
    let rec = datum_core::RecordRef {
        table: "sm.instance".into(),
        id: doc.doc_id,
        version: 1,
    };
    let gate_err = kernel
        .signature_gate()
        .verify(&dummy, &required, &rec)
        .expect_err("bound esign");
    assert!(
        matches!(gate_err, SignatureError::Invalid(_)),
        "inventory-style Engine::transition callers see the bound gate, got {gate_err:?}"
    );

    let mut tx = Tx::begin(&write, &spawn_ctx).await.expect("dummy begin");
    let dummy_err = kernel
        .engine
        .transition(
            &mut tx,
            Box::new(NoPostings),
            &doc,
            "approve",
            Some(&dummy),
            kernel.signature_gate(),
            &spawn_ctx,
        )
        .await
        .expect_err("dummy Engine::transition");
    assert!(
        matches!(
            dummy_err,
            datum_statemachine::Error::Signature(SignatureError::Invalid(_))
        ),
        "Engine::transition through signature_gate refuses dummy, got {dummy_err:?}"
    );
    tx.rollback().await.ok();

    let mut tx = Tx::begin(&write, &spawn_ctx).await.expect("kt dummy");
    let kt_err = kernel
        .transition(&mut tx, &doc, "approve", Some(&dummy), &spawn_ctx)
        .await
        .expect_err("Kernel dummy");
    assert!(
        matches!(
            kt_err,
            datum_module::Error::Statemachine(datum_statemachine::Error::Signature(
                SignatureError::Invalid(_)
            ))
        ),
        "Kernel::transition dummy, got {kt_err:?}"
    );
    tx.rollback().await.ok();

    let (state, version): (String, i64) = sqlx::query_as(
        r#"SELECT state, version FROM sm.instance
            WHERE doc_type = $1 AND doc_id = $2"#,
    )
    .bind(&doc.doc_type)
    .bind(doc.doc_id.as_uuid())
    .fetch_one(db.app_pool())
    .await
    .expect("instance");
    let inst = InstanceTriple {
        doc_type: doc.doc_type.clone(),
        doc_id: doc.doc_id,
        state,
        version,
    };
    let rec = datum_core::RecordRef {
        table: "sm.instance".into(),
        id: doc.doc_id,
        version,
    };
    let mint_ctx = boot_ctx();
    let mut mint_tx = Tx::begin(&write, &mint_ctx).await.expect("mint");
    let projection = kernel
        .live_record(&mut mint_tx, &doc.doc_type, doc.doc_id)
        .await
        .expect("live record");
    let sig = mint(
        &mut mint_tx,
        &MintRequest {
            components: vec!["code".into(), "secret".into()],
            code: Some(principal.username.clone()),
            secret: SIGNING_SECRET.into(),
            meaning: SignatureMeaning("Approved".into()),
            reason: None,
            record: rec.clone(),
            doc_type: doc.doc_type.clone(),
            projection,
            instance: inst,
            permission: PermissionKey("calibration.approve".into()),
            signed_at_zone: kernel.profile.seeded_permissions.display_timezone.clone(),
            policy: kernel.profile.session_policy.clone(),
            principal: principal.clone(),
            login_session_id: None,
            device_fingerprint: Some("e2e-tablet".into()),
            source_ip: Some("127.0.0.1".into()),
            boot_epoch: "1".into(),
            credential_kind: "signing_password".into(),
        },
    )
    .await
    .expect("mint both components");
    mint_tx.commit().await.expect("mint commit");

    let token = sig.token(principal.actor());
    let mut ctx = kernel.transition_context(principal.actor(), &doc, "approve");
    ctx.actor_display = Some(principal.display_name.clone());
    ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&write, &ctx).await.expect("signed begin");
    let out = kernel
        .transition(&mut tx, &doc, "approve", Some(&token), &ctx)
        .await
        .expect("two-component signature is consumed");
    assert_eq!(out.state.0, "Approved");
    tx.commit().await.expect("signed commit");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn kernel_e2e_regulated_document_approve_with_real_signature() {
    let db = db_case!("e2e_doc");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::regulated_device().unwrap())
        .await
        .expect("regulated");
    assert!(!kernel.gate_is_noop(), "regulated-device binds datum-esign");

    let write = kernel.write_pool();
    let principal = signer_with_perms(
        &write,
        &[
            "documents.view",
            "documents.edit",
            "documents.approve",
            "documents.release",
        ],
    )
    .await;
    let actor = principal.actor();

    let mut create_ctx = boot_ctx();
    create_ctx.actor = actor;
    create_ctx.actor_display = Some(principal.display_name.clone());
    let from = chrono::Utc::now() - chrono::Duration::days(1);
    let mut manifest = datum_documents::Manifest::content(json!({}));
    manifest.effective_from = Some(from);
    manifest.from_precision = Some(datum_documents::DatePrecision::Day);

    let mut tx = Tx::begin(&write, &create_ctx).await.expect("create begin");
    let id = kernel
        .create_document(&mut tx, "SOP", "Regulated SOP", "quality")
        .await
        .expect("create");
    let rev = kernel
        .new_document_revision(&mut tx, id, "A", manifest)
        .await
        .expect("revision");
    tx.commit().await.expect("create commit");

    let doc = DocRef {
        doc_type: datum_documents::DOC_TYPE.into(),
        doc_id: id.0,
    };
    let mut submit_ctx = kernel.transition_context(actor, &doc, "submit");
    submit_ctx.actor_display = Some(principal.display_name.clone());
    submit_ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&write, &submit_ctx).await.expect("submit begin");
    kernel
        .transition(&mut tx, &doc, "submit", None, &submit_ctx)
        .await
        .expect("submit is NotRequired");
    tx.commit().await.expect("submit commit");

    let mut approve_ctx = kernel.transition_context(actor, &doc, "approve");
    approve_ctx.actor_display = Some(principal.display_name.clone());
    approve_ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&write, &approve_ctx)
        .await
        .expect("unsigned approve begin");
    let refused = kernel
        .transition(&mut tx, &doc, "approve", None, &approve_ctx)
        .await
        .expect_err("regulated approve without a signature");
    assert!(
        matches!(
            refused,
            datum_module::Error::Statemachine(datum_statemachine::Error::Signature(
                SignatureError::NoProvider | SignatureError::Invalid(_)
            )) | datum_module::Error::Documents(datum_documents::Error::Signature(
                SignatureError::NoProvider | SignatureError::Invalid(_)
            ))
        ),
        "unsigned Required approve must refuse, got {refused:?}"
    );
    tx.rollback().await.ok();

    let mut tx = Tx::begin(&write, &approve_ctx)
        .await
        .expect("load after refuse");
    let after_refuse = datum_documents::load(&mut tx, id)
        .await
        .expect("load after refuse");
    tx.rollback().await.ok();
    assert_eq!(
        after_refuse.status,
        datum_documents::Status::InReview,
        "unsigned approve writes nothing"
    );

    let token_approve = mint_document_token(
        &kernel,
        &write,
        &principal,
        &doc,
        "Approved",
        "documents.approve",
    )
    .await;
    let mut tx = Tx::begin(&write, &approve_ctx)
        .await
        .expect("signed approve begin");
    let approved = kernel
        .transition(&mut tx, &doc, "approve", Some(&token_approve), &approve_ctx)
        .await
        .expect("two-component signature on approve");
    assert_eq!(approved.state.0, "Approved");
    tx.commit().await.expect("approve commit");

    let mut effective_ctx = kernel.transition_context(actor, &doc, "make_effective");
    effective_ctx.actor_display = Some(principal.display_name.clone());
    effective_ctx.reason = Some("kernel-e2e".into());
    let token_effective = mint_document_token(
        &kernel,
        &write,
        &principal,
        &doc,
        "Responsible",
        "documents.release",
    )
    .await;
    let mut tx = Tx::begin(&write, &effective_ctx)
        .await
        .expect("make_effective begin");
    let made = kernel
        .transition(
            &mut tx,
            &doc,
            "make_effective",
            Some(&token_effective),
            &effective_ctx,
        )
        .await
        .expect("two-component signature on make_effective");
    assert_eq!(made.state.0, "Effective");
    let live = datum_documents::effective_at(&mut tx, id, chrono::Utc::now())
        .await
        .expect("effective_at");
    tx.commit().await.expect("make_effective commit");
    let live = live.expect("effective revision retrievable");
    assert_eq!(live.id, rev);
    assert_eq!(live.document_id, id);

    db.finish().await.expect("finish");
}

async fn mint_document_token(
    kernel: &Kernel,
    write: &datum_db::WritePool,
    principal: &datum_identity::Principal,
    doc: &DocRef,
    meaning: &str,
    permission: &str,
) -> SignatureToken {
    let (state, version): (String, i64) = sqlx::query_as(
        r#"SELECT state, version FROM sm.instance
            WHERE doc_type = $1 AND doc_id = $2"#,
    )
    .bind(&doc.doc_type)
    .bind(doc.doc_id.as_uuid())
    .fetch_one(kernel.pool())
    .await
    .expect("instance");
    let inst = InstanceTriple {
        doc_type: doc.doc_type.clone(),
        doc_id: doc.doc_id,
        state,
        version,
    };
    let rec = datum_core::RecordRef {
        table: "sm.instance".into(),
        id: doc.doc_id,
        version,
    };
    let mut mint_tx = Tx::begin(write, &boot_ctx()).await.expect("mint begin");
    let projection = kernel
        .live_record(&mut mint_tx, &doc.doc_type, doc.doc_id)
        .await
        .expect("live record");
    let sig = mint(
        &mut mint_tx,
        &MintRequest {
            components: vec!["code".into(), "secret".into()],
            code: Some(principal.username.clone()),
            secret: SIGNING_SECRET.into(),
            meaning: SignatureMeaning(meaning.into()),
            reason: None,
            record: rec,
            doc_type: doc.doc_type.clone(),
            projection,
            instance: inst,
            permission: PermissionKey(permission.into()),
            signed_at_zone: kernel.profile.seeded_permissions.display_timezone.clone(),
            policy: kernel.profile.session_policy.clone(),
            principal: principal.clone(),
            login_session_id: None,
            device_fingerprint: Some("e2e-tablet".into()),
            source_ip: Some("127.0.0.1".into()),
            boot_epoch: "1".into(),
            credential_kind: "signing_password".into(),
        },
    )
    .await
    .expect("mint both components");
    mint_tx.commit().await.expect("mint commit");
    sig.token(principal.actor())
}

fn keep_n_projection(value: &serde_json::Value) -> serde_json::Value {
    json!({ "n": value.get("n").cloned().unwrap_or(serde_json::Value::Null) })
}

fn glue_signed_machine(doc_type: &str) -> Machine {
    Machine::builder(doc_type)
        .regulated(true)
        .state("Open")
        .state("Closed")
        .edge(
            EdgeBuilder::new("Open", "Closed", "approve", "calibration.approve").required(
                SignatureRequirement {
                    meaning: SignatureMeaning("Approved".into()),
                    permission: PermissionKey("calibration.approve".into()),
                },
            ),
        )
        .build()
        .unwrap()
}

async fn spawn_open(
    kernel: &Kernel,
    write: &datum_db::WritePool,
    principal: &datum_identity::Principal,
    doc: &DocRef,
) {
    let mut ctx = kernel.transition_context(principal.actor(), doc, "approve");
    ctx.actor_display = Some(principal.display_name.clone());
    ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(write, &ctx).await.expect("spawn begin");
    kernel.spawn(&mut tx, doc, "Open").await.expect("spawn");
    tx.commit().await.expect("spawn commit");
}

async fn mint_for_doc(
    kernel: &Kernel,
    write: &datum_db::WritePool,
    principal: &datum_identity::Principal,
    doc: &DocRef,
) -> (SignatureToken, datum_core::RecordRef, i64) {
    let mut mint_tx = Tx::begin(write, &boot_ctx()).await.expect("mint begin");
    let loaded = kernel
        .load_sm_instance(&mut mint_tx, doc.doc_id)
        .await
        .expect("instance")
        .expect("spawned");
    let inst = InstanceTriple {
        doc_type: loaded.0.clone(),
        doc_id: doc.doc_id,
        state: loaded.1,
        version: loaded.2,
    };
    let rec = datum_core::RecordRef {
        table: "sm.instance".into(),
        id: doc.doc_id,
        version: loaded.2,
    };
    let projection = kernel
        .live_record(&mut mint_tx, &doc.doc_type, doc.doc_id)
        .await
        .expect("live record");
    let sig = mint(
        &mut mint_tx,
        &MintRequest {
            components: vec!["code".into(), "secret".into()],
            code: Some(principal.username.clone()),
            secret: SIGNING_SECRET.into(),
            meaning: SignatureMeaning("Approved".into()),
            reason: None,
            record: rec.clone(),
            doc_type: doc.doc_type.clone(),
            projection,
            instance: inst,
            permission: PermissionKey("calibration.approve".into()),
            signed_at_zone: kernel.profile.seeded_permissions.display_timezone.clone(),
            policy: kernel.profile.session_policy.clone(),
            principal: principal.clone(),
            login_session_id: None,
            device_fingerprint: Some("e2e-tablet".into()),
            source_ip: Some("127.0.0.1".into()),
            boot_epoch: "1".into(),
            credential_kind: "signing_password".into(),
        },
    )
    .await
    .expect("mint");
    mint_tx.commit().await.expect("mint commit");
    (sig.token(principal.actor()), rec, loaded.2)
}

/// FINDING 2: the live hash is the registered projection plus the instance triple.
/// A field dropped by the projection may change without HashMismatch; a kept
/// field may not. datum-module e2e (not server HTTP): Kernel::live_doc is the
/// composition-root consume path, and this lane cannot add server tests.
#[tokio::test]
async fn live_doc_hash_covers_registered_projection() {
    let db = db_case!("e2e_proj");
    migrate_and_install(&db).await;
    let mut builder = Kernel::builder(db.app_pool().clone(), Profile::regulated_device().unwrap());
    builder
        .register_machine(glue_signed_machine("glue.proj.doc"))
        .unwrap();
    builder.register_projection("glue.proj.doc", keep_n_projection);
    let kernel = builder.build().await.expect("build");
    let write = kernel.write_pool();
    let principal = signer_with_perms(&write, &["calibration.approve"]).await;

    let kept = DocRef {
        doc_type: "glue.proj.doc".into(),
        doc_id: Identifier::generate(),
    };
    spawn_open(&kernel, &write, &principal, &kept).await;
    kernel.set_live_record("glue.proj.doc", kept.doc_id, json!({"n": 1, "drop": "old"}));
    let (token, _, version) = mint_for_doc(&kernel, &write, &principal, &kept).await;
    kernel.set_live_record("glue.proj.doc", kept.doc_id, json!({"n": 1, "drop": "new"}));
    let mut ctx = kernel.transition_context(principal.actor(), &kept, "approve");
    ctx.actor_display = Some(principal.display_name.clone());
    ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&write, &ctx).await.expect("consume drop-field");
    let out = kernel
        .transition(&mut tx, &kept, "approve", Some(&token), &ctx)
        .await
        .expect("drop-field edit is outside the projection");
    assert_eq!(out.state.0, "Closed");
    tx.commit().await.expect("commit drop-field");

    let mismatched = DocRef {
        doc_type: "glue.proj.doc".into(),
        doc_id: Identifier::generate(),
    };
    spawn_open(&kernel, &write, &principal, &mismatched).await;
    kernel.set_live_record(
        "glue.proj.doc",
        mismatched.doc_id,
        json!({"n": 1, "drop": "old"}),
    );
    let (token2, _, version2) = mint_for_doc(&kernel, &write, &principal, &mismatched).await;
    kernel.set_live_record(
        "glue.proj.doc",
        mismatched.doc_id,
        json!({"n": 2, "drop": "old"}),
    );
    let mut ctx = kernel.transition_context(principal.actor(), &mismatched, "approve");
    ctx.actor_display = Some(principal.display_name.clone());
    ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&write, &ctx).await.expect("consume kept-field");
    let started = std::time::Instant::now();
    let err = kernel
        .transition(&mut tx, &mismatched, "approve", Some(&token2), &ctx)
        .await
        .expect_err("kept-field edit");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "HashMismatch refusal must not deadlock on audit.log_event"
    );
    assert!(
        matches!(
            err,
            Error::Statemachine(datum_statemachine::Error::Signature(
                SignatureError::HashMismatch
            ))
        ),
        "got {err:?}"
    );
    assert_eq!(
        err.to_string(),
        "signature content hash mismatch",
        "kernel error that maps to HTTP 409 CONFLICT (D-2b-8)"
    );
    tx.rollback().await.ok();
    let live_version: i64 =
        query_scalar("SELECT version FROM sm.instance WHERE doc_type = $1 AND doc_id = $2")
            .bind(&mismatched.doc_type)
            .bind(mismatched.doc_id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .expect("version");
    assert_eq!(live_version, version2, "instance version unchanged");
    assert_eq!(version, 1);
    db.finish().await.expect("finish");
}

/// FINDING 2: body-only edit between mint and consume is HashMismatch.
/// datum-module e2e (not server HTTP): the live hash is Kernel::live_doc, and
/// this lane may only touch the mint hunk in datum-server.
#[tokio::test]
async fn mint_then_body_edit_is_hash_mismatch() {
    let db = db_case!("e2e_body");
    migrate_and_install(&db).await;
    let mut builder = Kernel::builder(db.app_pool().clone(), Profile::regulated_device().unwrap());
    builder
        .register_machine(glue_signed_machine("glue.body.doc"))
        .unwrap();
    builder.register_projection("glue.body.doc", identity_projection);
    let kernel = builder.build().await.expect("build");
    let write = kernel.write_pool();
    let principal = signer_with_perms(&write, &["calibration.approve"]).await;
    let doc = DocRef {
        doc_type: "glue.body.doc".into(),
        doc_id: Identifier::generate(),
    };
    spawn_open(&kernel, &write, &principal, &doc).await;
    kernel.set_live_record(
        "glue.body.doc",
        doc.doc_id,
        json!({"wo": "WO-1", "rev": "C"}),
    );
    let (token, _, version) = mint_for_doc(&kernel, &write, &principal, &doc).await;
    kernel.set_live_record(
        "glue.body.doc",
        doc.doc_id,
        json!({"wo": "WO-1", "rev": "D"}),
    );
    let live_version: i64 =
        query_scalar("SELECT version FROM sm.instance WHERE doc_type = $1 AND doc_id = $2")
            .bind(&doc.doc_type)
            .bind(doc.doc_id.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .expect("version");
    assert_eq!(live_version, version, "body edit must not bump sm.instance");

    let mut ctx = kernel.transition_context(principal.actor(), &doc, "approve");
    ctx.actor_display = Some(principal.display_name.clone());
    ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&write, &ctx).await.expect("consume");
    let started = std::time::Instant::now();
    let err = kernel
        .transition(&mut tx, &doc, "approve", Some(&token), &ctx)
        .await
        .expect_err("body edit");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "HashMismatch refusal must not deadlock on audit.log_event (pool max_connections=2)"
    );
    assert!(
        matches!(
            err,
            Error::Statemachine(datum_statemachine::Error::Signature(
                SignatureError::HashMismatch
            ))
        ),
        "got {err:?}"
    );
    assert_eq!(
        err.to_string(),
        "signature content hash mismatch",
        "kernel error that maps to HTTP 409 CONFLICT (D-2b-8 / datum-server from_signature)"
    );
    let refusals: i64 = query_scalar(
        r#"SELECT count(*) FROM audit.event
            WHERE source_kind = 'app_event'
              AND reason = 'signature content hash mismatch'"#,
    )
    .fetch_one(db.app_pool())
    .await
    .expect("refusal audit");
    assert!(
        refusals >= 1,
        "D-2b-5: refusal audit lands after rollback, while the claim Tx handle is still held"
    );
    tx.rollback().await.ok();
    db.finish().await.expect("finish");
}

/// Extra bound machines must register a projection; first-party types may default.
#[tokio::test]
async fn bound_machine_without_projection_fails_build() {
    let db = db_case!("e2e_noproj");
    migrate_and_install(&db).await;
    let mut builder = Kernel::builder(db.app_pool().clone(), Profile::regulated_device().unwrap());
    builder
        .register_machine(glue_signed_machine("glue.noproj.doc"))
        .unwrap();
    let err = match builder.build().await {
        Ok(_) => panic!("missing projection must fail Kernel::build"),
        Err(e) => e,
    };
    assert!(
        matches!(err, Error::MissingProjection(ref t) if t == "glue.noproj.doc"),
        "got {err:?}"
    );
    db.finish().await.expect("finish");
}

/// FINDING 5: Active is read on the claim Tx, so a same-Tx deactivation is seen
/// even when LiveDoc.signer_status is still Active.
#[tokio::test]
async fn signer_deactivated_on_claim_tx_is_refused() {
    let db = db_case!("e2e_deact");
    migrate_and_install(&db).await;
    let kernel = Kernel::build(db.app_pool(), Profile::regulated_device().unwrap())
        .await
        .expect("build");
    let write = kernel.write_pool();
    let principal = signer_with_perms(&write, &["calibration.approve"]).await;
    let doc = DocRef {
        doc_type: "calibration.certificate".into(),
        doc_id: Identifier::generate(),
    };
    spawn_open(&kernel, &write, &principal, &doc).await;
    let (token, rec, version) = mint_for_doc(&kernel, &write, &principal, &doc).await;

    let mut ctx = kernel.transition_context(principal.actor(), &doc, "approve");
    ctx.actor_display = Some(principal.display_name.clone());
    ctx.reason = Some("kernel-e2e".into());
    let mut tx = Tx::begin(&write, &ctx).await.expect("claim tx");
    deactivate_principal(&mut tx, principal.id)
        .await
        .expect("deactivate on claim tx");
    let live = LiveDoc {
        record: rec.clone(),
        doc_type: doc.doc_type.clone(),
        projection: json!({}),
        instance: InstanceTriple {
            doc_type: doc.doc_type.clone(),
            doc_id: doc.doc_id,
            state: "Open".into(),
            version,
        },
        signer_status: PrincipalStatus::Active,
    };
    let gate = kernel
        .signature_gate_factory()
        .prepare(&mut tx, &token, &live)
        .await
        .expect("prepare");
    let err = gate
        .verify(
            &token,
            &SignatureRequirement {
                meaning: SignatureMeaning("Approved".into()),
                permission: PermissionKey("calibration.approve".into()),
            },
            &rec,
        )
        .expect_err("inactive");
    assert_eq!(err, SignatureError::Invalid("signer inactive".into()));
    tx.rollback().await.ok();
    db.finish().await.expect("finish");
}
