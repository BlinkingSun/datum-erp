#![allow(dead_code)]

use rust_decimal::Decimal;
use wicket_core::{Actor, ActorKind, Identifier, ItemId, UnitId};
use wicket_db::WriteContext;

pub fn write_ctx(action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(wicket_identity::SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        action,
        "ui",
    );
    ctx.actor_display = Some("test".into());
    ctx.reason = Some("integration test".into());
    ctx
}

/// D-2b-10 order through `wicket-uom`. Glue rider flips this to
/// `wicket_module::order::install_upto(..., "wicket-uom")`.
/// TODO(2b-migorder-glue): replace this fallback with `install_upto`.
pub async fn migrate(db: &wicket_test::TestDb) {
    migrate_prefix_privileged(db).await;
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-identity", &wicket_identity::MIGRATOR),
            ("wicket-numbering", &wicket_numbering::MIGRATOR),
            ("wicket-uom", &wicket_uom::MIGRATOR),
        ],
    )
    .await
    .expect("canonical through wicket-uom");
}

/// D-2b-10 order through `wicket-ledger` (uom tests that post).
/// TODO(2b-migorder-glue): flip to `install_upto(..., "wicket-ledger")`.
pub async fn migrate_with_ledger(db: &wicket_test::TestDb) {
    migrate_prefix_privileged(db).await;
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-identity", &wicket_identity::MIGRATOR),
            ("wicket-numbering", &wicket_numbering::MIGRATOR),
            ("wicket-uom", &wicket_uom::MIGRATOR),
            ("wicket-events", &wicket_events::MIGRATOR),
            ("wicket-jobs", &wicket_jobs::MIGRATOR),
            ("wicket-ledger", &wicket_ledger::MIGRATOR),
        ],
    )
    .await
    .expect("canonical through wicket-ledger");
}

/// Predecessors of `wicket-uom` in D-2b-10 order, event trigger already up.
pub async fn migrate_predecessors(db: &wicket_test::TestDb) {
    migrate_prefix_privileged(db).await;
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-identity", &wicket_identity::MIGRATOR),
            ("wicket-numbering", &wicket_numbering::MIGRATOR),
        ],
    )
    .await
    .expect("canonical predecessors of wicket-uom");
}

/// Apply this crate last: predecessors with the trigger down, then privileged, then uom.
pub async fn migrate_as_last_crate(db: &wicket_test::TestDb) {
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-db", &wicket_db::MIGRATOR),
            ("wicket-audit", &wicket_audit::MIGRATOR),
            ("wicket-identity", &wicket_identity::MIGRATOR),
            ("wicket-numbering", &wicket_numbering::MIGRATOR),
        ],
    )
    .await
    .expect("predecessors with trigger down");
    install_privileged(db).await;
    wicket_db::migrate::run(db.migrate_pool(), &[("wicket-uom", &wicket_uom::MIGRATOR)])
        .await
        .expect("wicket-uom last with audit_attach up");
}

pub async fn migrate_prefix_privileged(db: &wicket_test::TestDb) {
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[
            ("wicket-db", &wicket_db::MIGRATOR),
            ("wicket-audit", &wicket_audit::MIGRATOR),
        ],
    )
    .await
    .expect("migrate db+audit");
    install_privileged(db).await;
}

pub async fn install_privileged(db: &wicket_test::TestDb) {
    let boot = db.bootstrap_pool().await.expect("bootstrap");
    wicket_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
}

pub async fn post_one_quantity_for_item(
    tx: &mut wicket_db::Tx<'_>,
    item: ItemId,
    stock_unit: UnitId,
) {
    use rust_decimal::Decimal;
    use wicket_core::{
        AnyQuantity, Boundary, CostElement, DimensionKind, GroupKind, LocationId, Money,
        PostingGroupHeader, PostingIntent, PostingSink, QuantityPosting, ValueAccount,
        ValuePosting,
    };
    use wicket_ledger::{
        CostMethod as LedgerCostMethod, GroupBuilder, upsert_location, upsert_stock_item,
    };

    upsert_stock_item(
        tx,
        item,
        stock_unit,
        4,
        Decimal::ZERO,
        LedgerCostMethod::Fifo,
        None,
    )
    .await
    .expect("ledger stock_item");

    let quarantine = LocationId::generate();
    let supplier = LocationId::generate();
    upsert_location(tx, quarantine, None)
        .await
        .expect("quarantine");
    upsert_location(tx, supplier, Some(Boundary::Supplier))
        .await
        .expect("supplier");

    let qty = AnyQuantity {
        amount: Decimal::ONE,
        unit: stock_unit,
        dimension: DimensionKind::Length,
    };
    let recv = QuantityPosting {
        item,
        quantity: qty,
        location: quarantine,
        boundary: None,
        lot: None,
        serial: None,
        entered: None,
    };
    let supplier_qty = QuantityPosting {
        item,
        quantity: AnyQuantity {
            amount: -Decimal::ONE,
            unit: stock_unit,
            dimension: DimensionKind::Length,
        },
        location: supplier,
        boundary: Some(Boundary::Supplier),
        lot: None,
        serial: None,
        entered: None,
    };

    let mut b = GroupBuilder::new(
        GroupKind::Movement,
        PostingGroupHeader {
            source_kind: "test".into(),
            source_id: None,
            work_order_id: None,
            reason_code: None,
            reverses_group_id: None,
        },
    );
    let recv_id = b
        .contribute(PostingIntent::Quantity(recv))
        .expect("recv qty");
    b.contribute(PostingIntent::Quantity(supplier_qty))
        .expect("supplier qty");
    let usd = |n: Decimal| Money::new(n, wicket_core::CurrencyId(840)).unwrap();
    b.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Inventory,
        cost_element: CostElement::Material,
        cost_object: None,
        amount: usd(Decimal::ONE),
        values: Some(recv_id),
    }))
    .expect("inventory value");
    b.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::ApAccrual,
        cost_element: CostElement::Material,
        cost_object: None,
        amount: usd(-Decimal::ONE),
        values: None,
    }))
    .expect("ap value");
    wicket_ledger::post(tx, b).await.expect("post group");
}

pub async fn insert_item_stock(
    tx: &mut wicket_db::Tx<'_>,
    item: ItemId,
    stock_unit: UnitId,
    scale: i16,
) {
    wicket_uom::pin_item_stock(
        tx,
        item,
        wicket_uom::ItemStockMeasure {
            stock_unit,
            stock_scale: scale,
            residual_tolerance: Decimal::ZERO,
        },
    )
    .await
    .expect("item_stock");
}

pub fn pg_code(err: &wicket_db::Error) -> String {
    match err {
        wicket_db::Error::Sqlx(e) => e
            .as_database_error()
            .and_then(|d| d.code().map(|c| c.into_owned()))
            .unwrap_or_else(|| format!("{e}")),
        other => other.to_string(),
    }
}
