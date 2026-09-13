//! Named commit-mode tests (SPEC).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use wicket_core::{ItemId, LotId};
use wicket_db::Tx;
use wicket_mod_lots::{
    CreateLot, CreateLotBody, Expiry, ExpiryPrecision, ExpiryWire, LotStatus, PackageLevel,
    StatusTarget, UdiTarget, attach_udi, create_lot, create_package, create_serials,
    list_serials as list_serials_http, package_hierarchy, resolve, set_status, trace_keys,
    validate_identifier,
};
use wicket_module::{Kernel, Profile};
use wicket_test::db_case;

#[tokio::test]
async fn lot_number_charset_and_length_enforced() {
    assert!(validate_identifier("LOT-BAR-24-4412").is_ok());
    assert!(validate_identifier("lot-bar-24-4412").is_err());
    assert!(validate_identifier("LOT BAR").is_err());
    assert!(validate_identifier("ABCDEFGHIJKLMNOPQRSTU").is_err());

    let db = db_case!("lot_charset");
    let kernel = common::boot_kernel(&db).await;
    let write = common::write_pool(&db);
    let ctx = common::write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();

    let err = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            number: Some("lot-bar-24-4412".into()),
            ..CreateLot::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, wicket_mod_lots::Error::InvalidIdentifier(_)));

    let err = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            template: Some("lot-bar-{0000}".into()),
            ..CreateLot::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, wicket_mod_lots::Error::Numbering(_)));

    let err = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            template: Some("LOT BAR-{0000}".into()),
            ..CreateLot::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, wicket_mod_lots::Error::Numbering(_)));

    let err = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            template: Some("ABCDEFGHIJKLMNOPQRSTU".into()),
            ..CreateLot::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, wicket_mod_lots::Error::Numbering(_)));

    tx.rollback().await.unwrap();

    for bad in ["lot-bar-24-4412", "LOT BAR", "ABCDEFGHIJKLMNOPQRSTU"] {
        let mut tx = Tx::begin(&write, &ctx).await.unwrap();
        let err = tx
            .execute(
                sqlx::query(
                    r#"INSERT INTO lots.lot (
                           id, item_id, number, status, application_version, configuration_version
                       ) VALUES ($1, $2, $3, 'quarantine', '0.1.0', '')"#,
                )
                .bind(LotId::generate().as_uuid())
                .bind(ItemId::generate().as_uuid())
                .bind(bad),
            )
            .await
            .unwrap_err();
        assert_eq!(
            common::pg_code_db(&err),
            "23514",
            "CHECK must refuse {bad:?}"
        );
        tx.rollback().await.unwrap();
    }

    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let defs: Vec<(String,)> = tx
        .fetch_all(sqlx::query_as(
            r#"SELECT pg_get_constraintdef(c.oid)
                 FROM pg_constraint c
                 JOIN pg_class t ON t.oid = c.conrelid
                 JOIN pg_namespace n ON n.oid = t.relnamespace
                WHERE n.nspname = 'lots' AND t.relname = 'lot' AND c.contype = 'c'"#,
        ))
        .await
        .unwrap();
    assert!(
        defs.iter()
            .any(|(d,)| d.contains("^[0-9A-Z-]{1,20}$") || d.contains("lot_number_charset")),
        "CHECK on lots.lot.number missing: {defs:?}"
    );

    tx.rollback().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn supplier_lot_is_a_cross_reference_not_the_id() {
    let db = db_case!("supplier_xref");
    let kernel = common::boot_kernel(&db).await;
    let write = common::write_pool(&db);
    let ctx = common::write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let lot = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            number: Some("LOT-BAR-24-4412".into()),
            supplier_lot: Some("ATI-HEAT-XYZ".into()),
            heat_or_source_ref: Some("HT-ATI-24-8831".into()),
            status: LotStatus::Quarantine,
            ..CreateLot::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(lot.number, "LOT-BAR-24-4412");
    assert_eq!(lot.supplier_lot.as_deref(), Some("ATI-HEAT-XYZ"));
    let id = resolve(&mut tx, "LOT-BAR-24-4412").await.unwrap();
    assert_eq!(id, lot.id);
    let err = resolve(&mut tx, "ATI-HEAT-XYZ").await.unwrap_err();
    assert!(matches!(err, wicket_mod_lots::Error::UnknownNumber(_)));
    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn serial_is_a_unit_within_a_lot() {
    let db = db_case!("serial_unit");
    let kernel = common::boot_kernel(&db).await;
    let write = common::write_pool(&db);
    let ctx = common::write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let heat = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            number: Some("HT-ATI-24-8831".into()),
            status: LotStatus::Quarantine,
            ..CreateLot::default()
        },
    )
    .await
    .unwrap();
    let lot = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: heat.item,
            number: Some("LOT-WO-1847".into()),
            heat_or_source_ref: Some("HT-ATI-24-8831".into()),
            status: LotStatus::Quarantine,
            ..CreateLot::default()
        },
    )
    .await
    .unwrap();
    let serials = create_serials(&mut tx, &kernel, lot.id, 2, Some("SN-450-{000000}"))
        .await
        .unwrap();
    assert_eq!(serials.len(), 2);
    assert!(serials.iter().all(|s| s.lot == lot.id));
    let (lot_id, keys) = trace_keys(&mut tx, lot.id).await.unwrap();
    assert_eq!(lot_id, lot.id);
    assert_eq!(keys.len(), 2);

    let err = tx
        .execute(
            sqlx::query(
                r#"INSERT INTO lots.serial (
                       id, lot_id, number, status, application_version, configuration_version
                   ) VALUES ($1, NULL, 'SN-450-000134', 'available', '0.1.0', '')"#,
            )
            .bind(uuid::Uuid::now_v7()),
        )
        .await
        .unwrap_err();
    let code = common::pg_code_db(&err);
    assert!(
        code == "23502" || code == "23503" || code.contains("not-null") || code == "23514",
        "null lot_id must fail, got {code}"
    );

    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn expiry_month_precision_survives_round_trip() {
    let db = db_case!("expiry_month");
    let kernel = common::boot_kernel(&db).await;
    let write = common::write_pool(&db);
    let ctx = common::write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let expiry = Expiry::from_year_month(2026, 9).unwrap();
    let lot = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            number: Some("LOT-BAR-24-4412".into()),
            expiry: Some(expiry),
            status: LotStatus::Quarantine,
            ..CreateLot::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(lot.expiry.unwrap().date.to_string(), "2026-09-01");
    assert_eq!(lot.expiry.unwrap().precision, ExpiryPrecision::Month);

    let body = wicket_mod_lots::LotBody::from(lot.clone());
    assert_eq!(body.expiry.as_ref().unwrap().value, "2026-09");
    assert_eq!(
        body.expiry.as_ref().unwrap().precision,
        ExpiryPrecision::Month
    );
    let json = serde_json::to_value(&body).unwrap();
    assert_eq!(json["expiry"]["value"], "2026-09");
    assert_eq!(json["expiry"]["precision"], "month");
    assert!(!json["expiry"]["value"].as_str().unwrap().contains("01"));

    let wire = ExpiryWire {
        value: "2026-09".into(),
        precision: ExpiryPrecision::Month,
    };
    let parsed = wire.into_expiry().unwrap();
    assert_eq!(parsed.date.to_string(), "2026-09-01");
    assert_eq!(parsed.precision, ExpiryPrecision::Month);

    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn two_cases_of_24_record_48_pieces_with_parent_links() {
    let db = db_case!("two_cases");
    let kernel = common::boot_kernel(&db).await;
    let write = common::write_pool(&db);
    let ctx = common::write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let lot = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            number: Some("LOT-WO-1847".into()),
            status: LotStatus::Quarantine,
            ..CreateLot::default()
        },
    )
    .await
    .unwrap();
    let mut pieces = 0i64;
    for case_n in 1..=2 {
        let case = create_package(
            &mut tx,
            lot.id,
            None,
            PackageLevel::Case,
            common::qty_ea(24),
            Some(&format!("CASE-{case_n}")),
        )
        .await
        .unwrap();
        for _ in 0..24 {
            create_package(
                &mut tx,
                lot.id,
                Some(case.id),
                PackageLevel::Each,
                common::qty_ea(1),
                None,
            )
            .await
            .unwrap();
            pieces += 1;
        }
    }
    assert_eq!(pieces, 48);
    let tree = package_hierarchy(&mut tx, lot.id).await.unwrap();
    let cases: Vec<_> = tree
        .iter()
        .filter(|p| p.level == PackageLevel::Case)
        .collect();
    let eaches: Vec<_> = tree
        .iter()
        .filter(|p| p.level == PackageLevel::Each)
        .collect();
    assert_eq!(cases.len(), 2);
    assert_eq!(eaches.len(), 48);
    assert!(eaches.iter().all(|e| e.parent.is_some()));
    assert!(
        cases
            .iter()
            .all(|c| c.contained.amount == rust_decimal::Decimal::from(24))
    );
    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn status_change_is_a_history_row_and_audited() {
    let db = db_case!("status_hist");
    let kernel = common::boot_kernel(&db).await;
    let write = common::write_pool(&db);
    let actor = common::actor_with_lots_perms(&write).await;
    let ctx = common::write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let lot = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            number: Some("LOT-BAR-24-4412".into()),
            status: LotStatus::Quarantine,
            ..CreateLot::default()
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let audit_before = common::count_audit(db.app_pool(), "status_history").await;
    let rel_ctx = common::edge_ctx(&kernel, actor, lot.id, "release");
    let mut tx = Tx::begin(&write, &rel_ctx).await.unwrap();
    set_status(
        &mut tx,
        &kernel,
        actor,
        StatusTarget::Lot(lot.id),
        LotStatus::Available,
        "released from quarantine",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let n: (i64,) = sqlx::query_as("SELECT count(*) FROM lots.status_history WHERE lot_id = $1")
        .bind(lot.id.as_uuid())
        .fetch_one(db.app_pool())
        .await
        .unwrap();
    assert_eq!(n.0, 1);
    let (app_v, cfg_v) = common::history_stamps(db.app_pool(), lot.id).await;
    assert!(!app_v.is_empty());
    assert_eq!(cfg_v, kernel.profile.spec_version);
    let audit_after = common::count_audit(db.app_pool(), "status_history").await;
    assert_eq!(
        audit_after - audit_before,
        1,
        "one audit row per transition"
    );
    assert!(common::count_audit_action(db.app_pool(), "lot.release", "lot").await >= 1);
    db.finish().await.unwrap();
}

#[tokio::test]
async fn udi_attachment_columns_are_nullable_and_settable() {
    let db = db_case!("udi_attach");
    let kernel = common::boot_kernel(&db).await;
    let write = common::write_pool(&db);
    let ctx = common::write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let lot = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            number: Some("LOT-BAR-24-4412".into()),
            ..CreateLot::default()
        },
    )
    .await
    .unwrap();
    assert!(lot.udi_device_identifier.is_none());
    let serials = create_serials(&mut tx, &kernel, lot.id, 1, Some("SN-450-{000000}"))
        .await
        .unwrap();
    assert!(serials[0].udi_production_identifier.is_none());
    attach_udi(
        &mut tx,
        UdiTarget::Lot(lot.id),
        Some("00850027865010"),
        None,
    )
    .await
    .unwrap();
    attach_udi(
        &mut tx,
        UdiTarget::Serial(serials[0].id),
        None,
        Some("LOT-BAR-24-4412"),
    )
    .await
    .unwrap();
    let lot = wicket_mod_lots::load_lot(&mut tx, lot.id).await.unwrap();
    assert_eq!(lot.udi_device_identifier.as_deref(), Some("00850027865010"));
    let serial = wicket_mod_lots::load_serial(&mut tx, serials[0].id)
        .await
        .unwrap();
    assert_eq!(
        serial.udi_production_identifier.as_deref(),
        Some("LOT-BAR-24-4412")
    );
    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn every_lots_table_is_audited_and_owned_by_wicket_owner() {
    let db = db_case!("lots_audited");
    common::migrate(&db).await;
    for table in ["lot", "serial", "package", "status_history"] {
        assert!(
            common::has_zz_audit(db.migrate_pool(), "lots", table).await,
            "{table} missing zz_audit_row"
        );
        assert_eq!(
            common::table_owner(db.migrate_pool(), "lots", table).await,
            "wicket_owner",
            "{table} owner"
        );
    }
    db.finish().await.unwrap();
}

#[tokio::test]
async fn no_delete_path_on_lot_or_serial() {
    let db = db_case!("no_delete");
    let kernel = common::boot_kernel(&db).await;
    let write = common::write_pool(&db);
    let ctx = common::write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let lot = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            number: Some("LOT-BAR-24-4412".into()),
            ..CreateLot::default()
        },
    )
    .await
    .unwrap();
    let serials = create_serials(&mut tx, &kernel, lot.id, 1, Some("SN-450-{000000}"))
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let err = sqlx::query("DELETE FROM lots.lot WHERE id = $1")
        .bind(lot.id.as_uuid())
        .execute(db.app_pool())
        .await
        .unwrap_err();
    assert_eq!(common::pg_code(&err), "42501");
    let err = sqlx::query("DELETE FROM lots.serial WHERE id = $1")
        .bind(serials[0].id.as_uuid())
        .execute(db.app_pool())
        .await
        .unwrap_err();
    assert_eq!(common::pg_code(&err), "42501");

    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM lots.lot WHERE id = $1")
        .bind(lot.id.as_uuid())
        .fetch_one(db.migrate_pool())
        .await
        .unwrap();
    assert_eq!(n, 1);
    db.finish().await.unwrap();
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = db_case!("writes_tx");
    common::migrate(&db).await;
    let id = LotId::generate();
    let err = sqlx::query(
        r#"INSERT INTO lots.lot (
               id, item_id, number, status, application_version, configuration_version
           ) VALUES ($1, $2, 'LOT-BAR-24-4412', 'quarantine', '0.1.0', '')"#,
    )
    .bind(id.as_uuid())
    .bind(ItemId::generate().as_uuid())
    .execute(db.app_pool())
    .await
    .unwrap_err();
    assert_eq!(common::pg_code(&err), "42501");
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM lots.lot WHERE id = $1")
        .bind(id.as_uuid())
        .fetch_one(db.migrate_pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    db.finish().await.unwrap();
}

#[tokio::test]
async fn module_registers_through_kernel_extension_points() {
    let db = db_case!("lots_kernel");
    common::migrate_kernel(&db).await;
    let mut builder = Kernel::builder(db.app_pool().clone(), Profile::plain_shop().unwrap());
    wicket_mod_lots::register(&mut builder, &Profile::plain_shop().unwrap()).unwrap();
    let kernel = builder.build().await.expect("kernel build");
    assert!(
        kernel
            .routes
            .iter()
            .any(|r| r.path == "/api/v1/lots" && r.permission == "lots.view"),
        "lots routes missing: {:?}",
        kernel.routes
    );
    db.finish().await.unwrap();
}

#[tokio::test]
async fn http_create_renders_month_expiry() {
    let db = db_case!("http_expiry");
    let kernel = common::boot_kernel(&db).await;
    let write = common::write_pool(&db);
    let ctx = common::write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let body = wicket_mod_lots::create_lot_http(
        &mut tx,
        &kernel,
        &ctx,
        CreateLotBody {
            item_id: ItemId::generate(),
            identifier: Some("LOT-BAR-24-4412".into()),
            template: None,
            supplier_lot: None,
            heat: Some("HT-ATI-24-8831".into()),
            expiry: Some(ExpiryWire {
                value: "2026-09".into(),
                precision: ExpiryPrecision::Month,
            }),
            cert_ref: None,
            status: Some(LotStatus::Quarantine),
        },
    )
    .await
    .unwrap();
    assert_eq!(body.identifier, "LOT-BAR-24-4412");
    assert_eq!(body.expiry.as_ref().unwrap().value, "2026-09");
    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn expiry_day_precision_survives_round_trip() {
    let db = db_case!("expiry_day");
    let kernel = common::boot_kernel(&db).await;
    let write = common::write_pool(&db);
    let ctx = common::write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let wire = ExpiryWire {
        value: "2029-03-18".into(),
        precision: ExpiryPrecision::Day,
    };
    let lot = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            number: Some("LOT-BAR-24-4412".into()),
            expiry: Some(wire.into_expiry().unwrap()),
            ..CreateLot::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(lot.expiry.unwrap().date.to_string(), "2029-03-18");
    let body = wicket_mod_lots::LotBody::from(lot);
    assert_eq!(body.expiry.as_ref().unwrap().value, "2029-03-18");
    assert_eq!(
        body.expiry.as_ref().unwrap().precision,
        ExpiryPrecision::Day
    );
    tx.commit().await.unwrap();
    db.finish().await.unwrap();
}

#[tokio::test]
async fn serial_list_paginates_at_page_boundary() {
    let db = db_case!("serial_page");
    let kernel = common::boot_kernel(&db).await;
    let write = common::write_pool(&db);
    let ctx = common::write_ctx("lots.edit");
    let mut tx = Tx::begin(&write, &ctx).await.unwrap();
    let lot = create_lot(
        &mut tx,
        &kernel,
        &ctx,
        CreateLot {
            item: ItemId::generate(),
            number: Some("LOT-WO-1847".into()),
            ..CreateLot::default()
        },
    )
    .await
    .unwrap();
    create_serials(&mut tx, &kernel, lot.id, 3, Some("SN-450-{000000}"))
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let mut tx = Tx::begin(&write, &common::write_ctx("lots.view"))
        .await
        .unwrap();
    let page1 = list_serials_http(&mut tx, lot.id, Some(2), None)
        .await
        .unwrap();
    assert_eq!(page1.data.len(), 2);
    assert!(page1.has_more);
    assert!(page1.next_cursor.is_some());
    let page2 = list_serials_http(&mut tx, lot.id, Some(2), page1.next_cursor.as_deref())
        .await
        .unwrap();
    assert_eq!(page2.data.len(), 1);
    assert!(!page2.has_more);
    tx.rollback().await.unwrap();
    db.finish().await.unwrap();
}
