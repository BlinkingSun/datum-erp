//! Forward and reverse migration, enum round-trip, ZL000–ZL007, canary, fence.
#![allow(missing_docs)]
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use sqlx::migrate::Migrator;
use wicket_core::{Boundary, GroupKind, PostingIntent, PostingSink, ValueAccount};
use wicket_db::Tx;
use wicket_ledger::{
    GroupBuilder, boundary_variants, cost_element_variants, group_kind_variants, measure_variants,
    post, value_account_variants,
};

use common::*;

fn reversible_ledger_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(wicket_ledger::MIGRATOR.iter().cloned().collect());
    // schema `ledger` is app-class; a version table there cannot be INSERTed
    // without Tx::begin. `transient` is skipped by audit_attach.
    migrator.dangerous_set_table_name("transient._sqlx_migrations_ledger");
    migrator
}

async fn query_seam_fn_exists(pool: &sqlx::PgPool, name: &str) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pg_proc p
            JOIN pg_namespace n ON n.oid = p.pronamespace
            WHERE n.nspname = 'ledger' AND p.proname = $1
         )",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("fn exists")
}

#[tokio::test]
async fn reverse_migration_tested() {
    let db = wicket_test::db_case!("mig_rev");
    common::migrate(&db).await;
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE n.nspname = 'ledger' AND c.relname = 'posting'",
    )
    .fetch_one(db.migrate_pool())
    .await
    .unwrap();
    assert_eq!(n, 1);
    const DOWN: &[&str] = &[
        "DROP TRIGGER IF EXISTS consumption_group_invariants ON ledger.consumption",
        "DROP TRIGGER IF EXISTS posting_group_invariants ON ledger.posting",
        "DROP FUNCTION IF EXISTS ledger.children_of(uuid)",
        "DROP FUNCTION IF EXISTS ledger.has_quantity_at(uuid)",
        "DROP FUNCTION IF EXISTS ledger.has_postings(uuid)",
        "DROP FUNCTION IF EXISTS ledger.enforce_group_invariants()",
        "DROP TABLE IF EXISTS ledger.consumption",
        "DROP TABLE IF EXISTS ledger.posting",
        "DROP TABLE IF EXISTS ledger.posting_group",
        "DROP TABLE IF EXISTS ledger.stock_item",
        "DROP TABLE IF EXISTS ledger.location",
        "DROP TABLE IF EXISTS transient.layer_projection",
        "DROP TABLE IF EXISTS transient.balance_projection",
        "DROP TYPE IF EXISTS ledger.value_account",
        "DROP TYPE IF EXISTS ledger.cost_element",
        "DROP TYPE IF EXISTS ledger.boundary",
        "DROP TYPE IF EXISTS ledger.measure",
        "DROP TYPE IF EXISTS ledger.group_kind",
        "DELETE FROM wicket.schema_class WHERE nspname = 'ledger'",
        "DROP SCHEMA IF EXISTS ledger",
    ];
    for stmt in DOWN {
        sqlx::query(*stmt)
            .execute(db.migrate_pool())
            .await
            .unwrap_or_else(|e| panic!("down stmt {stmt:?}: {e}"));
    }
    let n2: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_namespace WHERE nspname = 'ledger'")
        .fetch_one(db.migrate_pool())
        .await
        .unwrap();
    assert_eq!(n2, 0);
    sqlx::query("DELETE FROM wicket.schema_history WHERE crate = 'wicket-ledger'")
        .execute(db.migrate_pool())
        .await
        .expect("clear history");
    wicket_db::migrate::run(
        db.migrate_pool(),
        &[("wicket-ledger", &wicket_ledger::MIGRATOR)],
    )
    .await
    .expect("up again");
    db.finish().await.unwrap();
}

#[tokio::test]
async fn migrate_down_then_up() {
    let db = wicket_test::db_case!("led_down_up");
    common::migrate_predecessors(&db).await;

    let migrator = reversible_ledger_migrator();
    migrator.run(db.migrate_pool()).await.expect("ledger up");
    assert!(
        query_seam_fn_exists(db.migrate_pool(), "has_postings").await,
        "0002 must create ledger.has_postings"
    );
    assert!(query_seam_fn_exists(db.migrate_pool(), "has_quantity_at").await);
    assert!(
        query_seam_fn_exists(db.migrate_pool(), "children_of").await,
        "0003 must create ledger.children_of"
    );
    let parent_col: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.columns
             WHERE table_schema = 'ledger' AND table_name = 'posting_group'
               AND column_name = 'parent_group_id'
         )",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("parent_group_id column");
    assert!(parent_col, "0003 must add posting_group.parent_group_id");
    let definer: bool = sqlx::query_scalar(
        "SELECT p.prosecdef FROM pg_proc p
          JOIN pg_namespace n ON n.oid = p.pronamespace
         WHERE n.nspname = 'ledger' AND p.proname = 'has_postings'",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("prosecdef");
    assert!(!definer, "has_postings must be invoker-rights");
    let app_exec: bool = sqlx::query_scalar(
        "SELECT has_function_privilege('wicket_app', 'ledger.has_postings(uuid)', 'EXECUTE')",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("app execute");
    assert!(app_exec, "EXECUTE granted to wicket_app");
    let public_exec: bool = sqlx::query_scalar(
        "SELECT has_function_privilege('public', 'ledger.has_postings(uuid)', 'EXECUTE')",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("public execute");
    assert!(!public_exec, "EXECUTE revoked from PUBLIC");

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let posting_gone: bool = sqlx::query_scalar("SELECT to_regclass('ledger.posting') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("posting gone");
    assert!(posting_gone, "0001 down must drop ledger.posting");
    assert!(
        !query_seam_fn_exists(db.migrate_pool(), "has_postings").await,
        "0002 down must drop ledger.has_postings"
    );
    assert!(!query_seam_fn_exists(db.migrate_pool(), "has_quantity_at").await);
    assert!(
        !query_seam_fn_exists(db.migrate_pool(), "children_of").await,
        "0003 down must drop ledger.children_of"
    );

    migrator.run(db.migrate_pool()).await.expect("up again");
    assert!(query_seam_fn_exists(db.migrate_pool(), "has_postings").await);
    assert!(query_seam_fn_exists(db.migrate_pool(), "has_quantity_at").await);
    let definer_after: bool = sqlx::query_scalar(
        "SELECT p.prosecdef FROM pg_proc p
          JOIN pg_namespace n ON n.oid = p.pronamespace
         WHERE n.nspname = 'ledger' AND p.proname = 'has_quantity_at'",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("prosecdef after");
    assert!(!definer_after, "has_quantity_at must stay invoker-rights");

    db.finish().await.unwrap();
}

async fn has_zz_audit_row(pool: &sqlx::PgPool, rel: &str) -> bool {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM pg_trigger t
             WHERE t.tgrelid = $1::regclass
               AND t.tgname = 'zz_audit_row'
               AND NOT t.tgisinternal
        )
        "#,
    )
    .bind(rel)
    .fetch_one(pool)
    .await
    .expect("zz_audit_row")
}

#[tokio::test]
async fn migrates_at_canonical_position() {
    let db = wicket_test::db_case!("led_canon");
    common::migrate(&db).await;
    assert!(
        has_zz_audit_row(db.migrate_pool(), "ledger.stock_item").await,
        "ledger.stock_item must carry zz_audit_row at canonical position"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn migrates_as_last_crate() {
    let db = wicket_test::db_case!("led_last");
    common::migrate_as_last_crate(&db).await;
    assert!(
        has_zz_audit_row(db.migrate_pool(), "ledger.stock_item").await,
        "ledger.stock_item must carry zz_audit_row as last crate with audit_attach up"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn enum_bijection_round_trip() {
    let db = wicket_test::db_case!("enums");
    common::migrate(&db).await;
    for (k, s) in group_kind_variants() {
        let got: String = sqlx::query_scalar("SELECT $1::text::ledger.group_kind::text")
            .bind(*s)
            .fetch_one(db.app_pool())
            .await
            .unwrap_or_else(|_| panic!("group_kind {s}"));
        assert_eq!(got, *s, "{k:?}");
    }
    for (b, s) in boundary_variants() {
        let got: String = sqlx::query_scalar("SELECT $1::text::ledger.boundary::text")
            .bind(*s)
            .fetch_one(db.app_pool())
            .await
            .unwrap_or_else(|_| panic!("boundary {s}"));
        assert_eq!(got, *s, "{b:?}");
    }
    for (e, s) in cost_element_variants() {
        let got: String = sqlx::query_scalar("SELECT $1::text::ledger.cost_element::text")
            .bind(*s)
            .fetch_one(db.app_pool())
            .await
            .unwrap();
        assert_eq!(got, *s, "{e:?}");
    }
    for (a, s) in value_account_variants() {
        let got: String = sqlx::query_scalar("SELECT $1::text::ledger.value_account::text")
            .bind(*s)
            .fetch_one(db.app_pool())
            .await
            .unwrap();
        assert_eq!(got, *s, "{a:?}");
    }
    for (m, s) in measure_variants() {
        let got: String = sqlx::query_scalar("SELECT $1::text::ledger.measure::text")
            .bind(*s)
            .fetch_one(db.app_pool())
            .await
            .unwrap();
        assert_eq!(got, *s, "{m:?}");
    }
    db.finish().await.unwrap();
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = wicket_test::db_case!("writes_tx");
    common::migrate(&db).await;
    let err = sqlx::query(
        "INSERT INTO ledger.location (location_id, boundary_class) VALUES (gen_random_uuid(), NULL)",
    )
    .execute(db.app_pool())
    .await
    .expect_err("raw write");
    assert_eq!(pg_code_sqlx(&err), "42501");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger.location")
        .fetch_one(db.migrate_pool())
        .await
        .unwrap();
    assert_eq!(n, 0);
    db.finish().await.unwrap();
}

#[tokio::test]
async fn canary_ledger_constraints_are_armed() {
    let db = wicket_test::db_case!("canary_led");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "ledger.canary");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header("canary"));
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("1.0000"),
        w.quarantine,
        None,
        None,
    )))
    .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("-2.0000"),
        w.supplier,
        Some(Boundary::Supplier),
        None,
    )))
    .unwrap();
    post(&mut tx, b)
        .await
        .expect("insert must succeed; trigger is deferred");
    let err = tx
        .commit()
        .await
        .expect_err("canary: deferred trigger must fire");
    assert_eq!(pg_code_db(&err), "ZL002", "canary err={err}");
    db.finish().await.unwrap();
}

#[tokio::test]
async fn zl000_group_has_no_header() {
    let db = wicket_test::db_case!("zl000");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "zl000");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    commit_ok(tx).await;

    // Replica INSERT skips the deferred trigger *and* FKs, so it never yields
    // ZL000. Drop the posting_group FKs as bootstrap, then INSERT an orphan
    // under origin so the deferred trigger fires at commit.
    let boot = db.bootstrap_pool().await.unwrap();
    let fks: Vec<(String,)> = sqlx::query_as(
        "SELECT conname FROM pg_constraint
          WHERE conrelid = 'ledger.posting'::regclass
            AND confrelid = 'ledger.posting_group'::regclass",
    )
    .fetch_all(&boot)
    .await
    .unwrap();
    assert!(!fks.is_empty(), "expected posting → posting_group FKs");
    for (name,) in &fks {
        assert!(
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "unsafe constraint name {name}"
        );
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE ledger.posting DROP CONSTRAINT {name}"
        )))
        .execute(&boot)
        .await
        .unwrap_or_else(|e| panic!("drop {name}: {e}"));
    }
    // Audit row trigger requires Tx context; keep the deferred constraint trigger.
    sqlx::query("ALTER TABLE ledger.posting DISABLE TRIGGER zz_audit_row")
        .execute(&boot)
        .await
        .expect("disable audit");
    let mut orphan = boot.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO ledger.posting (
             group_id, kind, measure, item_id, uom_id, stock_scale, residual_tolerance,
             location_id, quantity
         ) VALUES (
             $1, 'MOVEMENT', 'QUANTITY', $2, $3, 4, 0.0100, $4, 1
         )",
    )
    .bind(wicket_core::Identifier::generate().as_uuid())
    .bind(w.bar.as_uuid())
    .bind(FT.0)
    .bind(w.quarantine.as_uuid())
    .execute(&mut *orphan)
    .await
    .expect("orphan insert (FKs dropped)");
    let err = orphan.commit().await.expect_err("ZL000 at commit");
    assert_eq!(
        pg_code_sqlx(&err),
        "ZL000",
        "orphan posting must raise ZL000, got {err}"
    );
    boot.close().await;
    db.finish().await.unwrap();
}

async fn zl_via_raw(name: &str, sqlstate: &str, build: impl FnOnce(&mut GroupBuilder, &World)) {
    let db = wicket_test::db_case!(name);
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), name);
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let mut b = GroupBuilder::new(GroupKind::Movement, movement_header(name));
    build(&mut b, &w);
    let _ = post(&mut tx, b).await;
    match tx.commit().await {
        Err(e) => assert_eq!(pg_code_db(&e), sqlstate, "wanted {sqlstate} got {e}"),
        Ok(()) => panic!("expected {sqlstate}"),
    }
    db.finish().await.unwrap();
}

#[tokio::test]
async fn zl001_group_extended() {
    p0_like().await;
}

async fn p0_like() {
    let db = wicket_test::db_case!("zl001");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "zl001");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let gid = post_case_a(&mut tx, &w).await;
    commit_ok(tx).await;
    let ctx2 = write_ctx(actor(), "zl001b");
    let mut tx2 = Tx::begin(&pool, &ctx2).await.unwrap();
    let res = tx2
        .execute(
            sqlx::query(
                "INSERT INTO ledger.posting (
                     group_id, kind, measure, item_id, uom_id, stock_scale, residual_tolerance,
                     location_id, quantity
                 ) SELECT $1, 'MOVEMENT', 'QUANTITY', $2, $3, 4, 0.0100, $4, 1",
            )
            .bind(gid.as_uuid())
            .bind(w.bar.as_uuid())
            .bind(FT.0)
            .bind(w.available.as_uuid()),
        )
        .await;
    res.unwrap_or_else(|e| {
        panic!("extending a settled group must insert; trigger is deferred: {e}")
    });
    let e = tx2.commit().await.expect_err("ZL001");
    assert_eq!(pg_code_db(&e), "ZL001", "P0 must raise ZL001, got {e}");
    db.finish().await.unwrap();
}

#[tokio::test]
async fn zl002_quantity_not_conserved() {
    zl_via_raw("zl002", "ZL002", |b, w| {
        b.contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("1.0000"),
            w.quarantine,
            None,
            None,
        )))
        .unwrap();
        b.contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-2.0000"),
            w.supplier,
            Some(Boundary::Supplier),
            None,
        )))
        .unwrap();
    })
    .await;
}

#[tokio::test]
async fn zl003_value_not_conserved() {
    zl_via_raw("zl003", "ZL003", |b, w| {
        let recv = b
            .contribute(PostingIntent::Quantity(q_post(
                w.bar,
                qty_ft("1.0000"),
                w.quarantine,
                None,
                None,
            )))
            .unwrap();
        b.contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-1.0000"),
            w.supplier,
            Some(Boundary::Supplier),
            None,
        )))
        .unwrap();
        b.contribute(PostingIntent::Value(v_post(
            ValueAccount::Inventory,
            usd("2.36"),
            Some(recv),
            None,
        )))
        .unwrap();
    })
    .await;
}

#[tokio::test]
async fn zl004_cost_element_reclassified() {
    zl_via_raw("zl004", "ZL004", |b, w| {
        let recv = b
            .contribute(PostingIntent::Quantity(q_post(
                w.bar,
                qty_ft("1.0000"),
                w.quarantine,
                None,
                None,
            )))
            .unwrap();
        b.contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-1.0000"),
            w.supplier,
            Some(Boundary::Supplier),
            None,
        )))
        .unwrap();
        b.contribute(PostingIntent::Value(v_post(
            ValueAccount::Inventory,
            usd("2.36"),
            Some(recv),
            None,
        )))
        .unwrap();
        b.contribute(PostingIntent::Value(wicket_core::ValuePosting {
            account: ValueAccount::ApAccrual,
            cost_element: wicket_core::CostElement::Outside,
            cost_object: None,
            amount: usd("-2.36"),
            values: None,
        }))
        .unwrap();
    })
    .await;
}

#[tokio::test]
async fn zl005_allocation_incomplete() {
    let db = wicket_test::db_case!("zl005");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "zl005");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    // Bypass the allocator: P1/P2-balanced withdrawal, zero consumption.
    raw_uncovered_issue(&mut tx, &w, "zl005").await;
    let err = tx.commit().await.expect_err("P3");
    assert_eq!(
        pg_code_db(&err),
        "ZL005",
        "uncovered withdrawal must raise ZL005, got {err}"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn zl006_reversal_not_exact() {
    let db = wicket_test::db_case!("zl006");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "zl006");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let c = post_case_c(&mut tx, &w).await;
    // Internally P1-balanced reversal that is not the exact negation of (c).
    let gid = raw_group(
        &mut tx,
        "REVERSAL",
        "zl006",
        None,
        None,
        Some(c.as_uuid()),
        Some("MOVEMENT"),
    )
    .await;
    raw_qty(
        &mut tx,
        gid,
        "REVERSAL",
        w.bar.as_uuid(),
        FT.0,
        4,
        dec("0.0100"),
        w.available.as_uuid(),
        None,
        Some(w.lot_bar.as_uuid()),
        dec("20.0000"),
    )
    .await;
    raw_qty(
        &mut tx,
        gid,
        "REVERSAL",
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
    let err = tx.commit().await.expect_err("ZL006");
    assert_eq!(
        pg_code_db(&err),
        "ZL006",
        "non-negation reversal must raise ZL006, got {err}"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn zl007_layers_not_restored() {
    let db = wicket_test::db_case!("zl007");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "zl007");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    let c = post_case_c(&mut tx, &w).await;
    let extra_layer = layer_at(&mut tx, w.bar, w.quarantine)
        .await
        .expect("receipt layer");
    wicket_ledger::reverse(&mut tx, c, "undo")
        .await
        .expect("exact reverse");
    let rev: uuid::Uuid = tx
        .fetch_one(
            sqlx::query_as(
                "SELECT group_id FROM ledger.posting_group WHERE reverses_group_id = $1",
            )
            .bind(c.as_uuid()),
        )
        .await
        .map(|(g,): (uuid::Uuid,)| g)
        .unwrap();
    let consuming: i64 = tx
        .fetch_one(
            sqlx::query_as(
                "SELECT posting_id FROM ledger.posting
                  WHERE group_id = $1 AND measure = 'QUANTITY' AND quantity > 0
                  ORDER BY posting_id LIMIT 1",
            )
            .bind(rev),
        )
        .await
        .map(|(id,): (i64,)| id)
        .unwrap();
    // Exact posting negation plus an extra consumption edge → ZL007 (P4 layers).
    raw_cons(
        &mut tx,
        rev,
        consuming,
        extra_layer,
        dec("1.0000"),
        dec("2.36"),
    )
    .await;
    let err = tx.commit().await.expect_err("ZL007");
    assert_eq!(
        pg_code_db(&err),
        "ZL007",
        "extra consumption must raise ZL007, got {err}"
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn group_actor_matches_audit() {
    let db = wicket_test::db_case!("actor_eq");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let act = actor();
    let ctx = write_ctx(act, "ledger.actor");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    let gid = post_case_a(&mut tx, &w).await;
    commit_ok(tx).await;
    let header: uuid::Uuid =
        sqlx::query_scalar("SELECT actor_id FROM ledger.posting_group WHERE group_id = $1")
            .bind(gid.as_uuid())
            .fetch_one(db.app_pool())
            .await
            .unwrap();
    let audit: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT actor_id FROM audit.event WHERE table_name = 'posting_group' ORDER BY at DESC LIMIT 1",
    )
    .fetch_optional(db.app_pool())
    .await
    .unwrap();
    assert_eq!(header, act.id.as_uuid());
    if let Some(a) = audit {
        assert_eq!(a, header);
    }
    db.finish().await.unwrap();
}

#[tokio::test]
async fn empty_group_and_unfinalized() {
    let db = wicket_test::db_case!("empty");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "empty");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let b = GroupBuilder::new(GroupKind::Movement, movement_header("empty"));
    let err = post(&mut tx, b).await.expect_err("empty");
    assert!(matches!(
        err,
        wicket_ledger::Error::Posting(wicket_core::PostingError::EmptyGroup)
    ));
    let mut b2 = GroupBuilder::new(GroupKind::Movement, movement_header("drop"));
    b2.contribute(PostingIntent::Quantity(q_post(
        wicket_core::ItemId::generate(),
        qty_ft("1.0000"),
        wicket_core::LocationId::generate(),
        None,
        None,
    )))
    .ok();
    wicket_ledger::bind_tx(&mut b2, &mut tx).await.unwrap();
    drop(b2);
    let err = wicket_ledger::commit(tx, &[]).await.expect_err("poison");
    assert!(matches!(err, wicket_ledger::Error::Unfinalized));
    db.finish().await.unwrap();
}

#[tokio::test]
async fn lineage_required_and_after_finalize() {
    let db = wicket_test::db_case!("lineage");
    common::migrate(&db).await;
    let pool = write_pool(&db);
    let ctx = write_ctx(actor(), "lineage");
    let mut tx = Tx::begin(&pool, &ctx).await.unwrap();
    let w = seed_world(&mut tx).await;
    post_case_a(&mut tx, &w).await;
    post_case_b(&mut tx, &w).await;
    post_case_c(&mut tx, &w).await;

    let mut header = movement_header("xf");
    header.work_order_id = Some(w.wo);
    let mut b = GroupBuilder::new(GroupKind::Transformation, header);
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("-20.0000"),
        w.wip,
        None,
        Some(w.lot_bar),
    )))
    .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("20.0000"),
        w.consumed,
        Some(Boundary::Consumed),
        Some(w.lot_bar),
    )))
    .unwrap();
    b.contribute(PostingIntent::Quantity(q_post(
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
    let err = post(&mut tx, b).await.expect_err("lineage");
    assert!(
        matches!(
            err,
            wicket_ledger::Error::Posting(wicket_core::PostingError::LineageRequired(_))
        ),
        "produced lot without consumption edge: {err}"
    );

    let mut b2 = GroupBuilder::new(GroupKind::Movement, movement_header("fin"));
    b2.contribute(PostingIntent::Quantity(q_post(
        w.bar,
        qty_ft("1.0000"),
        w.quarantine,
        None,
        None,
    )))
    .ok();
    let boxed: Box<dyn PostingSink> = Box::new(b2.clone());
    boxed.finalize().unwrap();
    let after = b2
        .contribute(PostingIntent::Quantity(q_post(
            w.bar,
            qty_ft("-1.0000"),
            w.supplier,
            Some(Boundary::Supplier),
            None,
        )))
        .unwrap_err();
    assert!(matches!(after, wicket_core::PostingError::AfterFinalize));
    db.finish().await.unwrap();
}
