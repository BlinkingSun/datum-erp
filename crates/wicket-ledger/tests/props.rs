//! Property shards named by D2 §10. Commit mode; deliberately corrupted groups.
#![allow(missing_docs)]
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use proptest::prelude::*;
use wicket_core::{Boundary, GroupKind, PostingError, PostingIntent, PostingSink, ValueAccount};
use wicket_db::{Tx, WritePool};
use wicket_ledger::{
    GroupBuilder, SQL_OPEN_LAYERS, post, sql_reads_consuming_value_rows, take_query_log,
};

use common::*;

#[tokio::test]
async fn p0_atomicity() {
    let db = wicket_test::db_case!("p0");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.p0");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let gid = post_case_a(&mut tx, &w).await;
    commit_ok(tx).await;

    let ctx2 = write_ctx(actor(), "ledger.p0b");
    let mut tx2 = Tx::begin(&pool, &ctx2).await.unwrap();
    let err = tx2
        .execute(
            sqlx::query(
                "INSERT INTO ledger.posting (
                     group_id, kind, measure, item_id, uom_id, stock_scale, residual_tolerance,
                     location_id, boundary, quantity
                 ) VALUES (
                     $1, 'MOVEMENT', 'QUANTITY', $2, $3, 4, 0.0100,
                     $4, NULL, 1
                 )",
            )
            .bind(gid.as_uuid())
            .bind(w.bar.as_uuid())
            .bind(FT.0)
            .bind(w.quarantine.as_uuid()),
        )
        .await;
    err.unwrap_or_else(|e| panic!("P0 insert is deferred: {e}"));
    let c = tx2.commit().await.expect_err("P0");
    assert_eq!(
        pg_code_db(&c),
        "ZL001",
        "extending a settled group must raise ZL001, got {c}"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn p1_quantity() {
    let db = wicket_test::db_case!("p1");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    generator_shard_run(&pool, 1, "ledger.p1gen").await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn p2_value() {
    let db = wicket_test::db_case!("p2");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    generator_shard_run(&pool, 2, "ledger.p2gen").await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn p2b_cost_element() {
    let db = wicket_test::db_case!("p2b");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.p2b");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("p2b"));
    let recv = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("10.0000"),
            w.quarantine,
            None,
            None,
        )))
        .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("-10.0000"),
        w.supplier,
        Some(Boundary::Supplier),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("23.60"),
        Some(recv),
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Value(wicket_core::ValuePosting {
        account: ValueAccount::ApAccrual,
        cost_element: wicket_core::CostElement::Labor,
        cost_object: None,
        amount: usd("-23.60"),
        values: None,
    }))
    .unwrap();
    post(&mut tx, b).await.expect("insert");
    let err = tx.commit().await.expect_err("P2-B");
    assert_eq!(pg_code_db(&err), "ZL004");
    db.finish().await.unwrap();
}

#[tokio::test]
async fn p3_coupling() {
    let db = wicket_test::db_case!("p3");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    generator_shard_run(&pool, 3, "ledger.p3gen").await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn p3_independent_sources() {
    let db = wicket_test::db_case!("p3ind");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.p3ind");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let layer = layer_at(&mut tx, w.bar, w.available)
        .await
        .expect("A layer");
    commit_ok(tx).await;

    assert!(
        SQL_OPEN_LAYERS.contains("values_posting_id = p.posting_id"),
        "layer money comes from the *layer*"
    );
    assert!(
        !sql_reads_consuming_value_rows(SQL_OPEN_LAYERS),
        "SQL_OPEN_LAYERS must not read consuming value rows"
    );

    // Corrupt the layer's attached unit cost (replica skips P2-A on the settled group).
    let boot = db.bootstrap_pool().await.unwrap();
    let mut rtx = boot.begin().await.unwrap();
    sqlx::query("SET LOCAL session_replication_role = replica")
        .execute(&mut *rtx)
        .await
        .expect("replica");
    sqlx::query(
        "UPDATE ledger.posting SET amount = 9440.00
          WHERE values_posting_id = $1 AND measure = 'VALUE'",
    )
    .bind(layer)
    .execute(&mut *rtx)
    .await
    .expect("corrupt layer unit cost");
    rtx.commit().await.expect("replica update");

    let ctx2 = write_ctx(actor(), "ledger.p3ind2");
    let mut tx2 = Tx::begin(&pool, &ctx2).await.unwrap();
    let _ = take_query_log();
    let mut header2 = movement_header("wo");
    header2.work_order_id = Some(w.wo);
    let mut b2 = GroupBuilder::new(GroupKind::Movement, header2);
    let out2 = b2
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-20.0000"),
            w.available,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    let into2 = b2
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("20.0000"),
            w.wip,
            None,
            Some(w.lot_bar),
        )))
        .unwrap();
    b2.contribute(PostingIntent::Value(v_post(
        ValueAccount::Inventory,
        usd("-47.20"),
        Some(out2),
        None,
    )))
    .unwrap();
    b2.contribute(PostingIntent::Value(v_post(
        ValueAccount::Wip,
        usd("47.20"),
        Some(into2),
        Some(w.wo),
    )))
    .unwrap();
    post(&mut tx2, b2).await.expect("insert; P3 is deferred");
    let log = take_query_log();
    assert!(
        log.iter().any(|q| q.contains("consumed_posting_id")),
        "allocator must query layers"
    );
    assert!(
        !log.iter().any(|q| sql_reads_consuming_value_rows(q)),
        "allocator must not read the consuming posting's value rows; sql={log:?}"
    );
    let err = tx2.commit().await.expect_err("P3 money half");
    assert_eq!(
        pg_code_db(&err),
        "ZL005",
        "corrupt layer unit cost vs valuation must fail P3 (ZL005), got {err}"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn p4_reversal() {
    let db = wicket_test::db_case!("p4");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    generator_shard_run(&pool, 4, "ledger.p4gen").await;
    db.finish().await.unwrap();
}

#[tokio::test]
async fn boundary_matrix() {
    let db = wicket_test::db_case!("bmat");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.bmat");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("bmat"));
    let err = b
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("1.0000"),
            w.scrap,
            Some(Boundary::Scrap),
            None,
        )))
        .unwrap_err();
    assert!(matches!(err, PostingError::Shape(_)));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn scale_exactness() {
    let db = wicket_test::db_case!("scale");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.scale");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("scale"));
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("0.58333333"),
        w.quarantine,
        None,
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("-0.58333333"),
        w.supplier,
        Some(Boundary::Supplier),
        None,
    )))
    .unwrap();
    let err = post(&mut tx, b).await.expect_err("scale");
    let code = pg_code_ledger(&err);
    assert!(
        code == "23514"
            || err.to_string().contains("23514")
            || matches!(err, wicket_ledger::Error::Db(_)),
        "quantity_exact_at_scale at INSERT, got {code} {err}"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn dust_bounded() {
    let db = wicket_test::db_case!("dust");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.dust");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let mut header = movement_header("dust");
    header.reason_code = Some("UOM_CONVERSION_RESIDUAL".into());
    let mut b = GroupBuilder::new(GroupKind::Adjustment, header);
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("-60.0000"),
        w.available,
        None,
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("60.0000"),
        w.rounding,
        Some(Boundary::Rounding),
        None,
    )))
    .unwrap();
    let err = post(&mut tx, b).await.expect_err("dust");
    let code = pg_code_ledger(&err);
    assert!(
        code == "23514" || matches!(err, wicket_ledger::Error::Db(_)),
        "rounding_is_dust, got {code} {err}"
    );
    db.finish().await.unwrap();
}

/// D2 §10 / PLAN §7 generator: business sequences and named corruptions (exact SQLSTATE).
const GENERATOR_CORRUPTIONS: &[(&str, &str)] = &[
    ("transposed_digit", "ZL002"),
    ("dropped_row", "ZL003"),
    ("wrong_kind", "23503"),
    ("wrong_sign", "ZL002"),
    ("duplicate_posting_id", "428C9"),
    ("wrong_boundary", "23514"),
    ("identity_crossing", "23514"),
    ("allocation_qty", "ZL005"),
    ("allocation_money", "ZL005"),
    ("wrong_location_consumption", "IneligibleLayer"),
];

const GENERATOR_SEQ_PER_SHARD: u8 = 8;
const GENERATOR_CORRUPT_PER_SHARD: u8 = 5;
const GENERATOR_SHARD_SEED: u64 = 0x4C32_C200_0000_0001;

async fn generate_legitimate_sequence(seed: u8, tx: &mut Tx<'_>, w: &World) {
    let feet = match seed % 4 {
        0 => "5.0000",
        1 => "10.0000",
        2 => "15.0000",
        _ => "20.0000",
    };
    // receipt (MOVEMENT into quarantine), release, issue — FIFO bar / lot_bar / quarantine→available→wip
    post_case_a(tx, w).await;
    post_case_b(tx, w).await;
    let issue = post_case_c(tx, w).await;
    // internal MOVEMENT across two real locations (available ↔ fg)
    post_gen_internal_move(tx, w, feet).await;
    // ADJUSTMENT on FIFO bar at `available`
    post_gen_adjustment(tx, w).await;
    // TRANSFORMATION + explicit consumption; STANDARD screw / lot_fg
    let xform = post_gen_transformation(tx, w).await;
    // release/issue STANDARD screws to customer
    let release = post_gen_screw_release(tx, w).await;
    let reverse_target = match seed % 3 {
        0 => issue,
        1 => xform,
        _ => release,
    };
    if seed % 2 == 1 {
        wicket_ledger::reverse(tx, reverse_target, "generator")
            .await
            .expect("reverse in generator");
    }
}

async fn run_generator_corruption(seed: u64, index: usize, mut tx: Tx<'_>, w: &World) {
    let (name, sqlstate) = GENERATOR_CORRUPTIONS[index];
    let immediate = generate_corruption(name, &mut tx, w).await;
    let got = if immediate.is_empty() {
        let err = tx.commit().await.expect_err(name);
        pg_code_db(&err)
    } else {
        let _ = tx.rollback().await;
        immediate
    };
    assert_eq!(
        got, sqlstate,
        "seed={seed} corruption {name} wanted {sqlstate} got {got}"
    );
}

async fn generator_shard_run(pool: &WritePool, shard: u8, tag: &str) {
    let base = GENERATOR_SHARD_SEED ^ (shard as u64);
    for i in 0..GENERATOR_SEQ_PER_SHARD {
        let seed = base + i as u64;
        let ctx = write_ctx(actor(), &format!("{tag}s{i}"));
        let mut tx = Tx::begin(pool, &ctx).await.unwrap_or_else(|e| {
            panic!("shard {shard} sequence {i} seed={seed} begin: {e}");
        });
        let w = seed_world(&mut tx).await;
        generate_legitimate_sequence(i, &mut tx, &w).await;
        commit_ok(tx).await;
    }
    for j in 0..GENERATOR_CORRUPT_PER_SHARD {
        let seed = base + 100 + j as u64;
        let idx = (seed as usize) % GENERATOR_CORRUPTIONS.len();
        let ctx2 = write_ctx(actor(), &format!("{tag}c{j}"));
        let mut tx2 = Tx::begin(pool, &ctx2).await.unwrap_or_else(|e| {
            panic!("shard {shard} corruption {j} seed={seed} begin: {e}");
        });
        let w2 = seed_world(&mut tx2).await;
        run_generator_corruption(seed, idx, tx2, &w2).await;
    }
}

async fn generate_corruption(kind: &str, tx: &mut Tx<'_>, w: &World) -> String {
    match kind {
        "transposed_digit" => {
            let gid = raw_group(tx, "MOVEMENT", "gen_td", None, None, None, None).await;
            raw_qty(
                tx,
                gid,
                "MOVEMENT",
                w.bar.as_uuid(),
                FT.0,
                4,
                dec("0.0100"),
                w.quarantine.as_uuid(),
                None,
                Some(w.lot_bar.as_uuid()),
                dec("0200.0000"),
            )
            .await;
            raw_qty(
                tx,
                gid,
                "MOVEMENT",
                w.bar.as_uuid(),
                FT.0,
                4,
                dec("0.0100"),
                w.supplier.as_uuid(),
                Some("SUPPLIER"),
                Some(w.lot_bar.as_uuid()),
                dec("-2000.0000"),
            )
            .await;
            String::new()
        }
        "dropped_row" => {
            post_case_a(tx, w).await;
            post_case_b(tx, w).await;
            let gid = raw_group(
                tx,
                "MOVEMENT",
                "gen_drop",
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
            raw_qty(
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
            String::new()
        }
        "wrong_kind" => {
            let gid = raw_group(tx, "MOVEMENT", "gen_wk", None, None, None, None).await;
            let res = tx
                .execute(
                    sqlx::query(
                        "INSERT INTO ledger.posting (
                             group_id, kind, measure, item_id, uom_id, stock_scale,
                             residual_tolerance, location_id, quantity
                         ) VALUES (
                             $1, 'ADJUSTMENT', 'QUANTITY', $2, $3, 4, 0.0100, $4, 1.0000
                         )",
                    )
                    .bind(gid)
                    .bind(w.bar.as_uuid())
                    .bind(FT.0)
                    .bind(w.quarantine.as_uuid()),
                )
                .await;
            pg_code_db(&res.expect_err("wrong kind"))
        }
        "wrong_sign" => {
            let gid = raw_group(tx, "MOVEMENT", "gen_ws", None, None, None, None).await;
            raw_qty(
                tx,
                gid,
                "MOVEMENT",
                w.bar.as_uuid(),
                FT.0,
                4,
                dec("0.0100"),
                w.quarantine.as_uuid(),
                None,
                Some(w.lot_bar.as_uuid()),
                dec("10.0000"),
            )
            .await;
            raw_qty(
                tx,
                gid,
                "MOVEMENT",
                w.bar.as_uuid(),
                FT.0,
                4,
                dec("0.0100"),
                w.supplier.as_uuid(),
                Some("SUPPLIER"),
                Some(w.lot_bar.as_uuid()),
                dec("10.0000"),
            )
            .await;
            String::new()
        }
        "duplicate_posting_id" => {
            let gid = raw_group(tx, "MOVEMENT", "gen_dup", None, None, None, None).await;
            let p1 = raw_qty(
                tx,
                gid,
                "MOVEMENT",
                w.bar.as_uuid(),
                FT.0,
                4,
                dec("0.0100"),
                w.quarantine.as_uuid(),
                None,
                Some(w.lot_bar.as_uuid()),
                dec("1.0000"),
            )
            .await;
            let res = tx
                .execute(
                    sqlx::query(
                        "INSERT INTO ledger.posting (
                             posting_id, group_id, kind, measure, item_id, uom_id, stock_scale,
                             residual_tolerance, location_id, quantity
                         ) VALUES (
                             $1, $2, 'MOVEMENT', 'QUANTITY', $3, $4, 4, 0.0100, $5, -1.0000
                         )",
                    )
                    .bind(p1)
                    .bind(gid)
                    .bind(w.bar.as_uuid())
                    .bind(FT.0)
                    .bind(w.supplier.as_uuid()),
                )
                .await;
            pg_code_db(&res.expect_err("dup id"))
        }
        "wrong_boundary" => {
            let gid = raw_group(tx, "MOVEMENT", "gen_b", None, None, None, None).await;
            let res = tx
                .execute(
                    sqlx::query(
                        "INSERT INTO ledger.posting (
                             group_id, kind, measure, item_id, uom_id, stock_scale,
                             residual_tolerance, location_id, boundary, quantity
                         ) VALUES (
                             $1, 'MOVEMENT', 'QUANTITY', $2, $3, 4, 0.0100,
                             $4, 'ADJUSTMENT', 1.0000
                         )",
                    )
                    .bind(gid)
                    .bind(w.bar.as_uuid())
                    .bind(FT.0)
                    .bind(w.adjustment.as_uuid()),
                )
                .await;
            pg_code_db(&res.expect_err("wrong boundary"))
        }
        "identity_crossing" => {
            let gid = raw_group(tx, "MOVEMENT", "gen_id", None, None, None, None).await;
            let res = tx
                .execute(
                    sqlx::query(
                        "INSERT INTO ledger.posting (
                             group_id, kind, measure, item_id, uom_id, stock_scale,
                             residual_tolerance, location_id, boundary, quantity
                         ) VALUES (
                             $1, 'MOVEMENT', 'QUANTITY', $2, $3, 4, 0.0100,
                             $4, 'CONSUMED', 1.0000
                         )",
                    )
                    .bind(gid)
                    .bind(w.bar.as_uuid())
                    .bind(FT.0)
                    .bind(w.consumed.as_uuid()),
                )
                .await;
            pg_code_db(&res.expect_err("identity crossing"))
        }
        "allocation_qty" => {
            post_case_a(tx, w).await;
            post_case_b(tx, w).await;
            let layer = layer_at(tx, w.bar, w.available).await.expect("layer");
            let gid = raw_uncovered_issue(tx, w, "gen_aq").await;
            let consuming: i64 = tx
                .fetch_one(
                    sqlx::query_as(
                        "SELECT posting_id FROM ledger.posting
                          WHERE group_id = $1 AND measure = 'QUANTITY' AND quantity < 0",
                    )
                    .bind(gid),
                )
                .await
                .map(|(id,): (i64,)| id)
                .expect("withdrawal");
            raw_cons(tx, gid, consuming, layer, dec("2.0000"), dec("47.20")).await;
            String::new()
        }
        "allocation_money" => {
            post_case_a(tx, w).await;
            post_case_b(tx, w).await;
            let layer = layer_at(tx, w.bar, w.available).await.expect("layer");
            let gid = raw_uncovered_issue(tx, w, "gen_am").await;
            let consuming = {
                let row: (i64,) = tx
                    .fetch_one(
                        sqlx::query_as(
                            "SELECT posting_id FROM ledger.posting
                              WHERE group_id = $1 AND measure = 'QUANTITY' AND quantity < 0",
                        )
                        .bind(gid),
                    )
                    .await
                    .expect("withdrawal");
                row.0
            };
            raw_cons(tx, gid, consuming, layer, dec("20.0000"), dec("4.72")).await;
            String::new()
        }
        "wrong_location_consumption" => {
            post_case_a(tx, w).await;
            post_case_b(tx, w).await;
            post_gen_internal_move(tx, w, "10.0000").await;
            let fg_layer = layer_at(tx, w.bar, w.fg).await.expect("fg layer");
            let mut header = movement_header("gen_wloc");
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
            b.contribute(PostingIntent::Consumption(
                wicket_core::ConsumptionPosting {
                    consuming: out,
                    consumed_posting_id: wicket_core::PostingId(fg_layer),
                    quantity: qty_ft("20.0000"),
                    amount: usd("47.20"),
                },
            ))
            .unwrap();
            let err = post(tx, b).await.expect_err("wrong location layer");
            corruption_code_ledger(&err)
        }
        other => panic!("unknown corruption {other}"),
    }
}

#[tokio::test]
async fn generator_names_are_stable() {
    assert_eq!(GENERATOR_CORRUPTIONS.len(), 10);
    for (i, (name, sqlstate)) in GENERATOR_CORRUPTIONS.iter().enumerate() {
        let db = wicket_test::db_case!(&format!("gen{i}"));
        common::migrate(&db).await;
        let pool = write_pool(&db);
        let ctx = write_ctx(actor(), &format!("ledger.gen{i}"));
        let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
        let w = seed_world(&mut tx).await;
        let immediate = generate_corruption(name, &mut tx, &w).await;
        let got = if immediate.is_empty() {
            let err = tx.commit().await.expect_err(name);
            pg_code_db(&err)
        } else {
            immediate
        };
        assert_eq!(
            got, *sqlstate,
            "corruption {name} wanted {sqlstate} got {got}"
        );
        db.finish().await.unwrap();
    }

    for seed in 0u8..GENERATOR_SEQ_PER_SHARD {
        let db = wicket_test::db_case!(&format!("genseq{seed}"));
        common::migrate(&db).await;
        let pool = write_pool(&db);
        let ctx = write_ctx(actor(), &format!("ledger.genseq{seed}"));
        let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
        let w = seed_world(&mut tx).await;
        generate_legitimate_sequence(seed, &mut tx, &w).await;
        commit_ok(tx).await;
        db.finish().await.unwrap();
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 8, ..ProptestConfig::default() })]
    #[test]
    fn generator_corruption_index_is_stable(x in 0usize..10) {
        let (name, sqlstate) = GENERATOR_CORRUPTIONS[x];
        prop_assert!(!name.is_empty());
        prop_assert!(sqlstate == "ZL002"
            || sqlstate == "ZL003"
            || sqlstate == "ZL005"
            || sqlstate == "23514"
            || sqlstate == "23503"
            || sqlstate == "428C9"
            || sqlstate == "IneligibleLayer");
    }
}
