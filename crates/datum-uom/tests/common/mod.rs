#![allow(dead_code)]

use datum_core::{Actor, ActorKind, Identifier, ItemId, UnitId};
use datum_db::WriteContext;
use rust_decimal::Decimal;

pub fn write_ctx(action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::generate(),
            kind: ActorKind::User,
        },
        action,
        "ui",
    );
    ctx.actor_display = Some("test".into());
    ctx.reason = Some("integration test".into());
    ctx
}

/// D-2b-10 order through `datum-uom`. Glue rider flips this to
/// `datum_module::order::install_upto(..., "datum-uom")`.
/// TODO(2b-migorder-glue): replace this fallback with `install_upto`.
pub async fn migrate(db: &datum_test::TestDb) {
    migrate_prefix_privileged(db).await;
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-identity", &datum_identity::MIGRATOR),
            ("datum-numbering", &datum_numbering::MIGRATOR),
            ("datum-uom", &datum_uom::MIGRATOR),
        ],
    )
    .await
    .expect("canonical through datum-uom");
}

/// D-2b-10 order through `datum-ledger` (uom tests that post).
/// TODO(2b-migorder-glue): flip to `install_upto(..., "datum-ledger")`.
pub async fn migrate_with_ledger(db: &datum_test::TestDb) {
    migrate_prefix_privileged(db).await;
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-identity", &datum_identity::MIGRATOR),
            ("datum-numbering", &datum_numbering::MIGRATOR),
            ("datum-uom", &datum_uom::MIGRATOR),
            ("datum-events", &datum_events::MIGRATOR),
            ("datum-jobs", &datum_jobs::MIGRATOR),
            ("datum-ledger", &datum_ledger::MIGRATOR),
        ],
    )
    .await
    .expect("canonical through datum-ledger");
}

/// Predecessors of `datum-uom` in D-2b-10 order, event trigger already up.
pub async fn migrate_predecessors(db: &datum_test::TestDb) {
    migrate_prefix_privileged(db).await;
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-identity", &datum_identity::MIGRATOR),
            ("datum-numbering", &datum_numbering::MIGRATOR),
        ],
    )
    .await
    .expect("canonical predecessors of datum-uom");
}

/// Apply this crate last: predecessors with the trigger down, then privileged, then uom.
pub async fn migrate_as_last_crate(db: &datum_test::TestDb) {
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
            ("datum-identity", &datum_identity::MIGRATOR),
            ("datum-numbering", &datum_numbering::MIGRATOR),
        ],
    )
    .await
    .expect("predecessors with trigger down");
    install_privileged(db).await;
    datum_db::migrate::run(db.migrate_pool(), &[("datum-uom", &datum_uom::MIGRATOR)])
        .await
        .expect("datum-uom last with audit_attach up");
}

pub async fn migrate_prefix_privileged(db: &datum_test::TestDb) {
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
        ],
    )
    .await
    .expect("migrate db+audit");
    install_privileged(db).await;
}

pub async fn install_privileged(db: &datum_test::TestDb) {
    let boot = db.bootstrap_pool().await.expect("bootstrap");
    datum_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
}

pub async fn post_one_quantity_for_item(
    tx: &mut datum_db::Tx<'_>,
    item: ItemId,
    stock_unit: UnitId,
) {
    use datum_core::{
        AnyQuantity, Boundary, CostElement, DimensionKind, GroupKind, LocationId, Money,
        PostingGroupHeader, PostingIntent, PostingSink, QuantityPosting, ValueAccount,
        ValuePosting,
    };
    use datum_ledger::{
        CostMethod as LedgerCostMethod, GroupBuilder, upsert_location, upsert_stock_item,
    };
    use rust_decimal::Decimal;

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
    let usd = |n: Decimal| Money::new(n, datum_core::CurrencyId(840)).unwrap();
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
    datum_ledger::post(tx, b).await.expect("post group");
}

pub async fn insert_item_stock(
    tx: &mut datum_db::Tx<'_>,
    item: ItemId,
    stock_unit: UnitId,
    scale: i16,
) {
    datum_uom::pin_item_stock(
        tx,
        item,
        datum_uom::ItemStockMeasure {
            stock_unit,
            stock_scale: scale,
            residual_tolerance: Decimal::ZERO,
        },
    )
    .await
    .expect("item_stock");
}

pub fn pg_code(err: &datum_db::Error) -> String {
    match err {
        datum_db::Error::Sqlx(e) => e
            .as_database_error()
            .and_then(|d| d.code().map(|c| c.into_owned()))
            .unwrap_or_else(|| format!("{e}")),
        other => other.to_string(),
    }
}
