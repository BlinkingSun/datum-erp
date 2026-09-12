#![allow(dead_code)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use datum_core::{
    Actor, ActorKind, AnyQuantity, Boundary, CostElement, CurrencyId, DimensionKind, Identifier,
    ItemId, LocationId, LotId, Money, PostingGroupHeader, QuantityPosting, UnitId, ValueAccount,
    ValuePosting,
};
use datum_db::{Tx, WriteContext, WritePool};
use datum_ledger::{CostMethod, GroupBuilder, upsert_location, upsert_stock_item};
use rust_decimal::Decimal;
use sqlx::PgPool;

pub const USD: CurrencyId = CurrencyId(840);
pub const EA: UnitId = UnitId(1);
pub const IN: UnitId = UnitId(3);
pub const FT: UnitId = UnitId(4);

pub fn dec(s: &str) -> Decimal {
    s.parse().expect("decimal")
}

pub fn usd(s: &str) -> Money {
    Money::new(dec(s), USD).expect("money")
}

pub fn qty_ft(s: &str) -> AnyQuantity {
    AnyQuantity {
        amount: dec(s),
        unit: FT,
        dimension: DimensionKind::Length,
    }
}

pub fn qty_ea(s: &str) -> AnyQuantity {
    AnyQuantity {
        amount: dec(s),
        unit: EA,
        dimension: DimensionKind::Count,
    }
}

pub fn actor() -> Actor {
    Actor {
        id: Identifier::generate(),
        kind: ActorKind::User,
    }
}

pub fn write_ctx(actor: Actor, action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(actor, action, "ui");
    ctx.actor_display = Some("M. Reyes".into());
    ctx.reason = Some("test".into());
    ctx
}

pub async fn migrate(db: &datum_test::TestDb) {
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
            ("datum-uom", &datum_uom::MIGRATOR),
            ("datum-ledger", &datum_ledger::MIGRATOR),
        ],
    )
    .await
    .expect("migrate db+audit+uom+ledger");
}

pub fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub fn pg_code_sqlx(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
}

pub fn pg_code_db(err: &datum_db::Error) -> String {
    match err {
        datum_db::Error::Refused(s) => s.as_str().to_string(),
        datum_db::Error::Sqlx(e) => pg_code_sqlx(e),
        other => other.to_string(),
    }
}

pub fn corruption_code_ledger(err: &datum_ledger::Error) -> String {
    use datum_core::PostingError;
    match err {
        datum_ledger::Error::Posting(PostingError::IneligibleLayer { .. }) => {
            "IneligibleLayer".into()
        }
        datum_ledger::Error::Posting(PostingError::AllocationRequired(_)) => {
            "AllocationRequired".into()
        }
        other => pg_code_ledger(other),
    }
}

pub fn pg_code_ledger(err: &datum_ledger::Error) -> String {
    match err {
        datum_ledger::Error::GroupHasNoHeader => "ZL000".into(),
        datum_ledger::Error::GroupExtended => "ZL001".into(),
        datum_ledger::Error::QuantityNotConserved => "ZL002".into(),
        datum_ledger::Error::ValueNotConserved => "ZL003".into(),
        datum_ledger::Error::CostElementReclassified => "ZL004".into(),
        datum_ledger::Error::AllocationIncomplete => "ZL005".into(),
        datum_ledger::Error::ReversalNotExact => "ZL006".into(),
        datum_ledger::Error::LayersNotRestored => "ZL007".into(),
        datum_ledger::Error::AlreadyReversed => "23505".into(),
        datum_ledger::Error::ParentMustExist => "23503".into(),
        datum_ledger::Error::Db(e) => pg_code_db(e),
        other => other.to_string(),
    }
}

pub struct World {
    pub actor: Actor,
    pub bar: ItemId,
    pub screw: ItemId,
    pub supplier: LocationId,
    pub customer: LocationId,
    pub scrap: LocationId,
    pub adjustment: LocationId,
    pub rounding: LocationId,
    pub consumed: LocationId,
    pub produced: LocationId,
    pub quarantine: LocationId,
    pub available: LocationId,
    pub fg: LocationId,
    pub wip: LocationId,
    pub osp: LocationId,
    pub wo: Identifier,
    pub lot_bar: LotId,
    pub lot_fg: LotId,
}

pub async fn seed_world(tx: &mut Tx<'_>) -> World {
    let bar = ItemId::generate();
    let screw = ItemId::generate();
    upsert_stock_item(tx, bar, FT, 4, dec("0.0100"), CostMethod::Fifo, None)
        .await
        .expect("bar stock");
    upsert_stock_item(
        tx,
        screw,
        EA,
        0,
        Decimal::ZERO,
        CostMethod::Standard,
        Some(usd("0.25")),
    )
    .await
    .expect("screw stock");
    tx.execute(
        sqlx::query(
            "INSERT INTO uom.item_stock (item_id, stock_unit_id, stock_scale, residual_tolerance)
             VALUES ($1, $2, $3, $4), ($5, $6, $7, $8)",
        )
        .bind(bar.as_uuid())
        .bind(FT.0)
        .bind(4_i16)
        .bind(dec("0.0100"))
        .bind(screw.as_uuid())
        .bind(EA.0)
        .bind(0_i16)
        .bind(Decimal::ZERO),
    )
    .await
    .expect("uom.item_stock");

    async fn loc(tx: &mut Tx<'_>, b: Option<Boundary>) -> LocationId {
        let id = LocationId::generate();
        upsert_location(tx, id, b).await.expect("location");
        id
    }

    let supplier = loc(tx, Some(Boundary::Supplier)).await;
    let customer = loc(tx, Some(Boundary::Customer)).await;
    let scrap = loc(tx, Some(Boundary::Scrap)).await;
    let adjustment = loc(tx, Some(Boundary::Adjustment)).await;
    let rounding = loc(tx, Some(Boundary::Rounding)).await;
    let consumed = loc(tx, Some(Boundary::Consumed)).await;
    let produced = loc(tx, Some(Boundary::Produced)).await;
    let quarantine = loc(tx, None).await;
    let available = loc(tx, None).await;
    let fg = loc(tx, None).await;
    let wip = loc(tx, None).await;
    let osp = loc(tx, None).await;

    World {
        actor: actor(),
        bar,
        screw,
        supplier,
        customer,
        scrap,
        adjustment,
        rounding,
        consumed,
        produced,
        quarantine,
        available,
        fg,
        wip,
        osp,
        wo: Identifier::generate(),
        lot_bar: LotId::generate(),
        lot_fg: LotId::generate(),
    }
}

pub fn movement_header(source: &str) -> PostingGroupHeader {
    PostingGroupHeader {
        source_kind: source.into(),
        source_id: None,
        work_order_id: None,
        reason_code: None,
        reverses_group_id: None,
    }
}

pub fn q_post(
    item: ItemId,
    qty: AnyQuantity,
    location: LocationId,
    boundary: Option<Boundary>,
    lot: Option<LotId>,
) -> QuantityPosting {
    QuantityPosting {
        item,
        quantity: qty,
        location,
        boundary,
        lot,
        serial: None,
        entered: None,
    }
}

pub fn v_post(
    account: ValueAccount,
    amount: Money,
    values: Option<datum_core::PostingHandle>,
    cost_object: Option<Identifier>,
) -> ValuePosting {
    ValuePosting {
        account,
        cost_element: CostElement::Material,
        cost_object,
        amount,
        values,
    }
}

pub async fn commit_ok(tx: Tx<'_>) {
    tx.commit().await.expect("commit");
}

pub async fn count_groups(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM ledger.posting_group")
        .fetch_one(pool)
        .await
        .expect("count groups")
}

pub async fn posting_ids_for_group(tx: &mut Tx<'_>, group: Identifier) -> Vec<i64> {
    let rows: Vec<(i64,)> = tx
        .fetch_all(
            sqlx::query_as(
                "SELECT posting_id FROM ledger.posting
                  WHERE group_id = $1 AND measure = 'QUANTITY'
                  ORDER BY posting_id",
            )
            .bind(group.as_uuid()),
        )
        .await
        .expect("posting ids");
    rows.into_iter().map(|r| r.0).collect()
}

pub async fn layer_at(tx: &mut Tx<'_>, item: ItemId, location: LocationId) -> Option<i64> {
    let row: Option<(i64,)> = tx
        .fetch_optional(
            sqlx::query_as(
                "SELECT p.posting_id FROM ledger.posting p
                  WHERE p.item_id = $1 AND p.location_id = $2
                    AND p.measure = 'QUANTITY' AND p.quantity > 0 AND p.boundary IS NULL
                  ORDER BY p.posting_id DESC LIMIT 1",
            )
            .bind(item.as_uuid())
            .bind(location.as_uuid()),
        )
        .await
        .expect("layer");
    row.map(|r| r.0)
}

/// Receive 100 bars / 2000 FT into quarantine (D2 §8 case a).
pub async fn post_case_a(tx: &mut Tx<'_>, w: &World) -> Identifier {
    use datum_core::{GroupKind, PostingIntent, PostingSink};
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("purchase_order"));
    let recv = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("2000.0000"),
            w.quarantine,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("-2000.0000"),
        w.supplier,
        Some(Boundary::Supplier),
        Some(w.lot_bar),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("4720.00"),
        Some(recv),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::ApAccrual,
        usd("-4720.00"),
        None,
        None,
    )))
    .unwrap();
    datum_ledger::post(tx, b).await.expect("case a")
}

/// Release quarantine → available (D2 §8 case b).
pub async fn post_case_b(tx: &mut Tx<'_>, w: &World) -> Identifier {
    use datum_core::{GroupKind, PostingIntent, PostingSink};
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("inspection"));
    let out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-2000.0000"),
            w.quarantine,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    let into = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("2000.0000"),
            w.available,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-4720.00"),
        Some(out),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("4720.00"),
        Some(into),
        None,
    )))
    .unwrap();
    datum_ledger::post(tx, b).await.expect("case b")
}

pub async fn seed_screws_fg(tx: &mut Tx<'_>, w: &World) {
    use datum_core::{GroupKind, PostingIntent, PostingSink};
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("seed_fg"));
    let into = b
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("500"),
            w.fg,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.screw,
        qty_ea("-500"),
        w.supplier,
        Some(Boundary::Supplier),
        Some(w.lot_fg),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("125.00"),
        Some(into),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::ApAccrual,
        usd("-125.00"),
        None,
        None,
    )))
    .unwrap();
    datum_ledger::post(tx, b).await.expect("seed screws");
}

/// Issue one bar to the work order (D2 §8 case c).
pub async fn post_case_c(tx: &mut Tx<'_>, w: &World) -> Identifier {
    use datum_core::{GroupKind, PostingIntent, PostingSink};
    let mut header = movement_header("work_order");
    header.work_order_id = Some(w.wo);
    let mut b = GroupBuilder::new(GroupKind::Movement, header);
    let out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-20.0000"),
            w.available,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    let into = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("20.0000"),
            w.wip,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-47.20"),
        Some(out),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Wip,
        usd("47.20"),
        Some(into),
        Some(w.wo),
    )))
    .unwrap();
    datum_ledger::post(tx, b).await.expect("case c")
}

pub async fn actor_uuid(tx: &mut Tx<'_>) -> uuid::Uuid {
    uuid::Uuid::parse_str(&tx.setting("datum.actor_id").await.expect("actor")).expect("uuid")
}

pub async fn raw_group(
    tx: &mut Tx<'_>,
    kind: &str,
    source: &str,
    reason: Option<&str>,
    work_order: Option<uuid::Uuid>,
    reverses: Option<uuid::Uuid>,
    reverses_kind: Option<&str>,
) -> uuid::Uuid {
    let gid = datum_core::Identifier::generate().as_uuid();
    let actor = actor_uuid(tx).await;
    tx.execute(
        sqlx::query(
            "INSERT INTO ledger.posting_group (
                 group_id, kind, actor_id, source_kind, work_order_id,
                 reason_code, reverses_group_id, reverses_kind
             ) VALUES (
                 $1, $2::ledger.group_kind, $3, $4, $5, $6, $7, $8::ledger.group_kind
             )",
        )
        .bind(gid)
        .bind(kind)
        .bind(actor)
        .bind(source)
        .bind(work_order)
        .bind(reason)
        .bind(reverses)
        .bind(reverses_kind),
    )
    .await
    .expect("raw group");
    gid
}

#[allow(clippy::too_many_arguments)]
pub async fn raw_qty(
    tx: &mut Tx<'_>,
    gid: uuid::Uuid,
    kind: &str,
    item: uuid::Uuid,
    uom: i64,
    scale: i16,
    tol: Decimal,
    location: uuid::Uuid,
    boundary: Option<&str>,
    lot: Option<uuid::Uuid>,
    qty: Decimal,
) -> i64 {
    let row: (i64,) = tx
        .fetch_one(
            sqlx::query_as(
                "INSERT INTO ledger.posting (
                     group_id, kind, measure, item_id, uom_id, stock_scale, residual_tolerance,
                     location_id, boundary, lot_id, quantity
                 ) VALUES (
                     $1, $2::ledger.group_kind, 'QUANTITY', $3, $4, $5, $6,
                     $7, $8::ledger.boundary, $9, $10
                 ) RETURNING posting_id",
            )
            .bind(gid)
            .bind(kind)
            .bind(item)
            .bind(uom)
            .bind(scale)
            .bind(tol)
            .bind(location)
            .bind(boundary)
            .bind(lot)
            .bind(qty),
        )
        .await
        .expect("raw qty");
    row.0
}

#[allow(clippy::too_many_arguments)]
pub async fn raw_val(
    tx: &mut Tx<'_>,
    gid: uuid::Uuid,
    kind: &str,
    account: &str,
    element: &str,
    amount: Decimal,
    currency: i16,
    values: Option<i64>,
    cost_object: Option<uuid::Uuid>,
) -> i64 {
    let row: (i64,) = tx
        .fetch_one(
            sqlx::query_as(
                "INSERT INTO ledger.posting (
                     group_id, kind, measure, account, cost_element, cost_object_id,
                     currency_id, amount, values_posting_id
                 ) VALUES (
                     $1, $2::ledger.group_kind, 'VALUE',
                     $3::ledger.value_account, $4::ledger.cost_element, $5,
                     $6, $7, $8
                 ) RETURNING posting_id",
            )
            .bind(gid)
            .bind(kind)
            .bind(account)
            .bind(element)
            .bind(cost_object)
            .bind(currency)
            .bind(amount)
            .bind(values),
        )
        .await
        .expect("raw val");
    row.0
}

pub async fn raw_cons(
    tx: &mut Tx<'_>,
    gid: uuid::Uuid,
    consuming: i64,
    consumed: i64,
    qty: Decimal,
    amount: Decimal,
) {
    tx.execute(
        sqlx::query(
            "INSERT INTO ledger.consumption (
                 consuming_posting_id, consumed_posting_id, group_id, quantity, amount
             ) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(consuming)
        .bind(consumed)
        .bind(gid)
        .bind(qty)
        .bind(amount),
    )
    .await
    .expect("raw cons");
}

/// P1/P2-balanced 20 FT available→wip withdrawal with no consumption (P3 / ZL005).
pub async fn raw_uncovered_issue(tx: &mut Tx<'_>, w: &World, source: &str) -> uuid::Uuid {
    let gid = raw_group(
        tx,
        "MOVEMENT",
        source,
        None,
        Some(w.wo.as_uuid()),
        None,
        None,
    )
    .await;
    let out = raw_qty(
        tx,
        gid,
        "MOVEMENT",
        w.bar.as_uuid(),
        FT.0,
        4,
        dec("0.0100"),
        w.available.as_uuid(),
        None,
        Some(w.lot_bar.as_uuid()),
        dec("-20.0000"),
    )
    .await;
    let into = raw_qty(
        tx,
        gid,
        "MOVEMENT",
        w.bar.as_uuid(),
        FT.0,
        4,
        dec("0.0100"),
        w.wip.as_uuid(),
        None,
        Some(w.lot_bar.as_uuid()),
        dec("20.0000"),
    )
    .await;
    raw_val(
        tx,
        gid,
        "MOVEMENT",
        "INVENTORY",
        "MATERIAL",
        dec("-47.20"),
        840,
        Some(out),
        None,
    )
    .await;
    raw_val(
        tx,
        gid,
        "MOVEMENT",
        "WIP",
        "MATERIAL",
        dec("47.20"),
        840,
        Some(into),
        Some(w.wo.as_uuid()),
    )
    .await;
    gid
}

/// Internal MOVEMENT between two real locations (same item/lot).
pub async fn post_gen_internal_move(tx: &mut Tx<'_>, w: &World, feet: &str) -> Identifier {
    use datum_core::{GroupKind, PostingIntent, PostingSink};
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("gen_move"));
    let out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            AnyQuantity {
                amount: -dec(feet),
                unit: FT,
                dimension: DimensionKind::Length,
            },
            w.available,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    let into = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft(feet),
            w.fg,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    let amt = dec(feet) * dec("2.36");
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        Money::new(-amt, USD).expect("money"),
        Some(out),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        Money::new(amt, USD).expect("money"),
        Some(into),
        None,
    )))
    .unwrap();
    datum_ledger::post(tx, b).await.expect("gen move")
}

/// ADJUSTMENT cycle-count short (FIFO bar, `available`).
pub async fn post_gen_adjustment(tx: &mut Tx<'_>, w: &World) -> Identifier {
    use datum_core::{GroupKind, PostingIntent, PostingSink};
    let mut header = movement_header("gen_adj");
    header.reason_code = Some("CYCLE_COUNT".into());
    let mut b = GroupBuilder::new(GroupKind::Adjustment, header);
    let out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-1.0000"),
            w.available,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("1.0000"),
        w.adjustment,
        Some(Boundary::Adjustment),
        Some(w.lot_bar),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-2.36"),
        Some(out),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::AdjustmentExpense,
        usd("2.36"),
        None,
        None,
    )))
    .unwrap();
    datum_ledger::post(tx, b).await.expect("gen adj")
}

/// TRANSFORMATION with explicit consumption edge (case d shape, STANDARD screws).
pub async fn post_gen_transformation(tx: &mut Tx<'_>, w: &World) -> Identifier {
    use datum_core::{CostElement, GroupKind, PostingIntent, PostingSink, ValuePosting};
    let wip_layer = layer_at(tx, w.bar, w.wip).await.expect("wip layer");
    seed_screws_fg(tx, w).await;
    let mut header = movement_header("gen_xform");
    header.work_order_id = Some(w.wo);
    let mut b = GroupBuilder::new(GroupKind::Transformation, header);
    let bar_out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-5.0000"),
            w.wip,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    let consumed_h = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("5.0000"),
            w.consumed,
            Some(Boundary::Consumed),
            Some(w.lot_bar),
        )))
        .unwrap();
    let screw_in = b
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("50"),
            w.fg,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.screw,
        qty_ea("-50"),
        w.produced,
        Some(Boundary::Produced),
        Some(w.lot_fg),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Wip,
        usd("-11.80"),
        Some(bar_out),
        Some(w.wo),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Wip,
        cost_element: CostElement::Labor,
        cost_object: Some(w.wo),
        amount: usd("-15.00"),
        values: Some(consumed_h),
    }))
    .unwrap();
    b.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Wip,
        cost_element: CostElement::Burden,
        cost_object: Some(w.wo),
        amount: usd("-7.50"),
        values: Some(consumed_h),
    }))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("12.50"),
        Some(screw_in),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Wip,
        usd("21.80"),
        Some(consumed_h),
        Some(w.wo),
    )))
    .unwrap();
    b.contribute(PostingIntent::Consumption(datum_core::ConsumptionPosting {
        consuming: screw_in,
        consumed_posting_id: datum_core::PostingId(wip_layer),
        quantity: qty_ft("5.0000"),
        amount: usd("11.80"),
    }))
    .unwrap();
    datum_ledger::post(tx, b).await.expect("gen xform")
}

/// Release/issue STANDARD screws to customer (second item / second lot).
pub async fn post_gen_screw_release(tx: &mut Tx<'_>, w: &World) -> Identifier {
    use datum_core::{GroupKind, PostingIntent, PostingSink};
    seed_screws_fg(tx, w).await;
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("gen_issue"));
    let out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("-10"),
            w.fg,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.screw,
        qty_ea("10"),
        w.customer,
        Some(Boundary::Customer),
        Some(w.lot_fg),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-2.50"),
        Some(out),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Cogs,
        usd("2.50"),
        None,
        None,
    )))
    .unwrap();
    datum_ledger::post(tx, b).await.expect("gen screw issue")
}
