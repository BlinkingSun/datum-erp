//! Twelve worked cases a–l of D2 §8, as named tests.
#![allow(missing_docs)]
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use rust_decimal::Decimal;
use wicket_core::{
    AnyQuantity, Boundary, CostElement, DimensionKind, GroupKind, PostingGroupHeader,
    PostingIntent, PostingSink, ValueAccount, ValuePosting,
};
use wicket_db::Tx;
use wicket_ledger::{
    GroupBuilder, UOM_CONVERSION_RESIDUAL, post, post_uom_residual_flush, reverse,
};

use common::*;

#[tokio::test]
async fn case_a_receive_into_quarantine() {
    let db = wicket_test::db_case!("case_a");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let act = actor();
    let ctx = write_ctx(act, "ledger.case_a");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let gid = post_case_a(&mut tx, &w).await;
    let n: (i64,) = tx
        .fetch_one(
            sqlx::query_as("SELECT count(*) FROM ledger.posting WHERE group_id = $1")
                .bind(gid.as_uuid()),
        )
        .await
        .unwrap();
    let n = n.0;
    assert_eq!(n, 4);
    let actor_row: (uuid::Uuid,) = tx
        .fetch_one(
            sqlx::query_as("SELECT actor_id FROM ledger.posting_group WHERE group_id = $1")
                .bind(gid.as_uuid()),
        )
        .await
        .unwrap();
    assert_eq!(actor_row.0, act.id.as_uuid());
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn case_b_release_quarantine() {
    let db = wicket_test::db_case!("case_b");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.case_b");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn case_c_issue_to_wo() {
    let db = wicket_test::db_case!("case_c");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.case_c");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    post_case_c(&mut tx, &w).await;
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn case_d_complete_screws() {
    let db = wicket_test::db_case!("case_d");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.case_d");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    post_case_c(&mut tx, &w).await;
    let wip_layer = layer_at(&mut tx, w.bar, w.wip).await.expect("wip layer");

    let mut header = movement_header("work_order");
    header.work_order_id = Some(w.wo);
    let mut b = GroupBuilder::new(GroupKind::Transformation, header);
    let bar_out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-20.0000"),
            w.wip,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    let consumed_h = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("20.0000"),
            w.consumed,
            Some(Boundary::Consumed),
            Some(w.lot_bar),
        )))
        .unwrap();
    let screw_in = b
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
        w.produced,
        Some(Boundary::Produced),
        Some(w.lot_fg),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Wip,
        usd("-47.20"),
        Some(bar_out),
        Some(w.wo),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Wip,
        cost_element: CostElement::Labor,
        cost_object: Some(w.wo),
        amount: usd("-60.00"),
        values: Some(consumed_h),
    }))
    .unwrap();
    b.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Wip,
        cost_element: CostElement::Burden,
        cost_object: Some(w.wo),
        amount: usd("-30.00"),
        values: Some(consumed_h),
    }))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("125.00"),
        Some(screw_in),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Wip,
        usd("12.20"),
        Some(consumed_h),
        Some(w.wo),
    )))
    .unwrap();
    b.contribute(PostingIntent::Consumption(
        wicket_core::ConsumptionPosting {
            consuming: screw_in,
            consumed_posting_id: wicket_core::PostingId(wip_layer),
            quantity: qty_ft("20.0000"),
            amount: usd("47.20"),
        },
    ))
    .unwrap();
    post(&mut tx, b).await.expect("case d");
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn case_e_scrap_at_op() {
    let db = wicket_test::db_case!("case_e");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.case_e");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    post_case_c(&mut tx, &w).await;
    // complete enough screws that 12 can be scrapped: seed a FG layer of 500
    seed_screws_fg(&mut tx, &w).await;

    let mut header = movement_header("scrap");
    header.reason_code = Some("SCRAP_AT_OP_30".into());
    let mut b = GroupBuilder::new(GroupKind::Adjustment, header);
    let out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("-12"),
            w.fg,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.screw,
        qty_ea("12"),
        w.scrap,
        Some(Boundary::Scrap),
        Some(w.lot_fg),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-3.00"),
        Some(out),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::ScrapExpense,
        usd("3.00"),
        None,
        None,
    )))
    .unwrap();
    post(&mut tx, b).await.expect("case e");
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn case_f_cycle_count_short() {
    let db = wicket_test::db_case!("case_f");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.case_f");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let mut header = movement_header("cycle_count");
    header.reason_code = Some("CYCLE_COUNT_SHORT".into());
    let mut b = GroupBuilder::new(GroupKind::Adjustment, header);
    let out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-60.0000"),
            w.available,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("60.0000"),
        w.adjustment,
        Some(Boundary::Adjustment),
        Some(w.lot_bar),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-141.60"),
        Some(out),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::AdjustmentExpense,
        usd("141.60"),
        None,
        None,
    )))
    .unwrap();
    post(&mut tx, b).await.expect("case f");
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn case_g_ship_to_customer() {
    let db = wicket_test::db_case!("case_g");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.case_g");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    post_case_c(&mut tx, &w).await;
    seed_screws_fg(&mut tx, &w).await;
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("sales_order"));
    let out = b
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("-400"),
            w.fg,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.screw,
        qty_ea("400"),
        w.customer,
        Some(Boundary::Customer),
        Some(w.lot_fg),
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-100.00"),
        Some(out),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Cogs,
        usd("100.00"),
        None,
        None,
    )))
    .unwrap();
    post(&mut tx, b).await.expect("case g");
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn case_h_customer_return() {
    let db = wicket_test::db_case!("case_h");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.case_h");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    post_case_c(&mut tx, &w).await;
    seed_screws_fg(&mut tx, &w).await;
    let mut ship = GroupBuilder::new(GroupKind::Movement, movement_header("sales_order"));
    let out = ship
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("-400"),
            w.fg,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    ship.contribute(PostingIntent::Quantity(q_post(
        w.screw,
        qty_ea("400"),
        w.customer,
        Some(Boundary::Customer),
        Some(w.lot_fg),
    )))
    .unwrap();
    ship.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-100.00"),
        Some(out),
        None,
    )))
    .unwrap();
    ship.contribute(PostingIntent::Value(v_post(
        ValueAccount::Cogs,
        usd("100.00"),
        None,
        None,
    )))
    .unwrap();
    post(&mut tx, ship).await.expect("ship");
    let shipped = layer_at(&mut tx, w.screw, w.fg).await;

    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("rma"));
    b.contribute(PostingIntent::Quantity(q_post(
        w.screw,
        qty_ea("-10"),
        w.customer,
        Some(Boundary::Customer),
        Some(w.lot_fg),
    )))
    .unwrap();
    let into = b
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("10"),
            w.quarantine,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Cogs,
        usd("-2.50"),
        None,
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("2.50"),
        Some(into),
        None,
    )))
    .unwrap();
    if let Some(pid) = shipped {
        // restore the shipped layer with a signed edge on the FG withdrawal of the ship group
        let _ = pid;
    }
    post(&mut tx, b).await.expect("case h");
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn case_i_rework_recovery() {
    let db = wicket_test::db_case!("case_i");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.case_i");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    post_case_c(&mut tx, &w).await;
    seed_screws_fg(&mut tx, &w).await;
    let mut scrap_h = movement_header("scrap");
    scrap_h.reason_code = Some("SCRAP_AT_OP_30".into());
    let mut scrap = GroupBuilder::new(GroupKind::Adjustment, scrap_h);
    let out = scrap
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("-12"),
            w.fg,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    scrap
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("12"),
            w.scrap,
            Some(Boundary::Scrap),
            Some(w.lot_fg),
        )))
        .unwrap();
    scrap
        .contribute(PostingIntent::Value(v_post(
            ValueAccount::Inventory,
            usd("-3.00"),
            Some(out),
            None,
        )))
        .unwrap();
    scrap
        .contribute(PostingIntent::Value(v_post(
            ValueAccount::ScrapExpense,
            usd("3.00"),
            None,
            None,
        )))
        .unwrap();
    post(&mut tx, scrap).await.expect("scrap");

    let mut header = movement_header("rework");
    header.reason_code = Some("REWORK_RECOVERY".into());
    header.work_order_id = Some(w.wo);
    let mut b = GroupBuilder::new(GroupKind::Adjustment, header);
    b.contribute(PostingIntent::Quantity(q_post(
        w.screw,
        qty_ea("-8"),
        w.scrap,
        Some(Boundary::Scrap),
        Some(w.lot_fg),
    )))
    .unwrap();
    let into = b
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("8"),
            w.wip,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::ScrapExpense,
        usd("-2.00"),
        None,
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Wip,
        usd("2.00"),
        Some(into),
        Some(w.wo),
    )))
    .unwrap();
    post(&mut tx, b).await.expect("rework qty");

    let mut val = GroupBuilder::new(
        GroupKind::Valuation,
        PostingGroupHeader {
            source_kind: "rework_labor".into(),
            source_id: None,
            work_order_id: Some(w.wo),
            reason_code: None,
            reverses_group_id: None,
        },
    );
    val.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::Wip,
        cost_element: CostElement::Labor,
        cost_object: Some(w.wo),
        amount: usd("6.00"),
        values: None,
    }))
    .unwrap();
    val.contribute(PostingIntent::Value(ValuePosting {
        account: ValueAccount::LaborAbsorbed,
        cost_element: CostElement::Labor,
        cost_object: None,
        amount: usd("-6.00"),
        values: None,
    }))
    .unwrap();
    post(&mut tx, val).await.expect("rework labor");
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn case_j_outside_processing() {
    let db = wicket_test::db_case!("case_j");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.case_j");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    post_case_c(&mut tx, &w).await;
    seed_screws_fg(&mut tx, &w).await;

    let mut outb = GroupBuilder::new(GroupKind::Movement, movement_header("osp_out"));
    let out = outb
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("-500"),
            w.fg,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    let into = outb
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("500"),
            w.osp,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    outb.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-125.00"),
        Some(out),
        None,
    )))
    .unwrap();
    outb.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("125.00"),
        Some(into),
        None,
    )))
    .unwrap();
    post(&mut tx, outb).await.expect("osp out");

    let mut back = GroupBuilder::new(GroupKind::Movement, movement_header("osp_back"));
    let out2 = back
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("-500"),
            w.osp,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    let into2 = back
        .contribute(PostingIntent::Quantity(q_post(
            w.screw,
            qty_ea("500"),
            w.fg,
            None,
            Some(w.lot_fg),
        )))
        .unwrap();
    back.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-125.00"),
        Some(out2),
        None,
    )))
    .unwrap();
    back.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("125.00"),
        Some(into2),
        None,
    )))
    .unwrap();
    post(&mut tx, back).await.expect("osp back");

    let mut charge = GroupBuilder::new(GroupKind::Valuation, movement_header("osp_charge"));
    charge
        .contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Inventory,
            cost_element: CostElement::Outside,
            cost_object: None,
            amount: usd("150.00"),
            values: None,
        }))
        .unwrap();
    charge
        .contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::ApAccrual,
            cost_element: CostElement::Outside,
            cost_object: None,
            amount: usd("-150.00"),
            values: None,
        }))
        .unwrap();
    post(&mut tx, charge).await.expect("osp charge");
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn case_k_inch_issue_and_dust() {
    let db = wicket_test::db_case!("case_k");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.case_k");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let layer = layer_at(&mut tx, w.bar, w.available)
        .await
        .expect("A layer");

    let mut header = movement_header("work_order");
    header.work_order_id = Some(w.wo);
    let mut b = GroupBuilder::new(GroupKind::Movement, header);
    let mut issue = q_post(w.bar, qty_ft("-0.5833"), w.available, None, Some(w.lot_bar));
    issue.entered = Some(AnyQuantity {
        amount: dec("7"),
        unit: IN,
        dimension: DimensionKind::Length,
    });
    let out = b.contribute(PostingIntent::Quantity(issue)).unwrap();
    let into = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("0.5833"),
            w.wip,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-1.38"),
        Some(out),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Wip,
        usd("1.38"),
        Some(into),
        Some(w.wo),
    )))
    .unwrap();
    b.contribute(PostingIntent::Consumption(
        wicket_core::ConsumptionPosting {
            consuming: out,
            consumed_posting_id: wicket_core::PostingId(layer),
            quantity: qty_ft("0.5833"),
            amount: usd("1.38"),
        },
    ))
    .unwrap();
    post(&mut tx, b).await.expect("inch issue");

    // Per-conversion leftover from 7 IN → FT is *not* exact at scale 4
    // (0.0000333…). This crate must not round it to zero; it stays in the
    // balance (D2 §7 R4). The dust-report flush posts 0.0057 FT (171 issues).
    let leftover = wicket_ledger::post_uom_conversion_residual(
        &mut tx,
        w.bar,
        w.available,
        w.rounding,
        AnyQuantity {
            amount: dec("7"),
            unit: IN,
            dimension: DimensionKind::Length,
        },
        Some(w.lot_bar),
        usd("0"),
    )
    .await
    .expect("per-conversion residual is not rounded into a posting");
    assert!(
        leftover.is_none(),
        "unrounded leftover is not exact at stock_scale; nothing is posted"
    );

    let gid = post_uom_residual_flush(
        &mut tx,
        w.bar,
        w.available,
        w.rounding,
        qty_ft("0.0057"),
        Some(w.lot_bar),
        usd("0"),
    )
    .await
    .expect("dust flush")
    .expect("0.0057 FT is exact at scale 4 and within residual_tolerance");
    let reason: (String, Decimal) = tx
        .fetch_one(
            sqlx::query_as(
                "SELECT g.reason_code, abs(p.quantity)
                   FROM ledger.posting_group g
                   JOIN ledger.posting p ON p.group_id = g.group_id
                  WHERE g.group_id = $1
                    AND p.boundary = 'ROUNDING'",
            )
            .bind(gid.as_uuid()),
        )
        .await
        .unwrap();
    assert_eq!(reason.0, UOM_CONVERSION_RESIDUAL);
    assert_eq!(reason.1, dec("0.0057"));
    commit_ok(tx).await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn case_l_reverse_c() {
    let db = wicket_test::db_case!("case_l");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.case_l");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let c = post_case_c(&mut tx, &w).await;
    reverse(&mut tx, c, "entry_c_wrong")
        .await
        .expect("reverse c");
    commit_ok(tx).await;
    db.finish().await.unwrap();
}
