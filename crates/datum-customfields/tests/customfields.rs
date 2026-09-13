//! Named commit-mode tests (SPEC).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use datum_core::Identifier;
use datum_customfields::{
    DefinitionSpec, Error, FieldType, ManifestCustomField, ManifestCustomFields, Value, define,
    definitions_for, get, gtin_valid, register_from_manifest, retire, set, validate,
};
use datum_db::Tx;
use sqlx::query_scalar;

use common::{PROFILES, migrate, open_db, write_ctx, write_pool};

const VALID_GTIN: &str = "4006381333931";

async fn for_each_profile<F, Fut>(f: F)
where
    F: Fn(&'static str) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    for profile in PROFILES {
        f(profile).await;
    }
}

#[tokio::test]
async fn define_from_manifest() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("cf_manifest", profile).await else {
            return;
        };
        migrate(&db).await;
        let pool = write_pool(&db);
        let mut tx = Tx::begin(&pool, &write_ctx("customfields.define", profile))
            .await
            .unwrap();
        let manifest = ManifestCustomFields {
            fields: vec![ManifestCustomField {
                entity: "items.item".into(),
                key: "shop_note".into(),
                field_type: FieldType::Text,
                label: "Shop note".into(),
                validate: String::new(),
                audit: true,
                required: false,
                indexed: false,
                owner: "mod-items".into(),
            }],
        };
        register_from_manifest(&mut tx, &manifest).await.unwrap();
        register_from_manifest(&mut tx, &manifest).await.unwrap();
        let defs = definitions_for(&mut tx, "items.item").await.unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].key, "shop_note");
        tx.rollback().await.unwrap();
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn define_rejects_unknown_rule() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("cf_unk_rule", profile).await else {
            return;
        };
        migrate(&db).await;
        let pool = write_pool(&db);
        let mut tx = Tx::begin(&pool, &write_ctx("customfields.define", profile))
            .await
            .unwrap();
        let err = define(
            &mut tx,
            DefinitionSpec {
                entity: "items.item".into(),
                key: "x".into(),
                field_type: FieldType::String,
                label: "X".into(),
                validation_rule: "not-a-rule".into(),
                required: false,
                indexed: false,
                owner_module: "mod-items".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::UnknownValidationRule { .. }));
        tx.rollback().await.unwrap();
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn typed_value_rejects_wrong_type() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("cf_type", profile).await else {
            return;
        };
        migrate(&db).await;
        let pool = write_pool(&db);
        let mut tx = Tx::begin(&pool, &write_ctx("customfields.define", profile))
            .await
            .unwrap();
        define(
            &mut tx,
            DefinitionSpec {
                entity: "items.item".into(),
                key: "qty".into(),
                field_type: FieldType::Integer,
                label: "Qty".into(),
                validation_rule: String::new(),
                required: false,
                indexed: false,
                owner_module: "mod-items".into(),
            },
        )
        .await
        .unwrap();
        let record = Identifier::generate();
        let err = set(
            &mut tx,
            "items.item",
            record,
            "qty",
            Value::String("nope".into()),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::TypeMismatch { .. }));
        tx.rollback().await.unwrap();
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn validate_gs1_gtin() {
    assert!(gtin_valid(VALID_GTIN));
    assert!(!gtin_valid("4006381333930"));
    for_each_profile(|profile| async move {
        let Some(db) = open_db("cf_gtin", profile).await else {
            return;
        };
        migrate(&db).await;
        let pool = write_pool(&db);
        let mut tx = Tx::begin(&pool, &write_ctx("customfields.define", profile))
            .await
            .unwrap();
        let def = DefinitionSpec {
            entity: "items.item".into(),
            key: "gtin".into(),
            field_type: FieldType::String,
            label: "GTIN".into(),
            validation_rule: "gs1-gtin".into(),
            required: false,
            indexed: false,
            owner_module: "mod-items".into(),
        };
        define(&mut tx, def.clone()).await.unwrap();
        let loaded = definitions_for(&mut tx, "items.item").await.unwrap()[0].clone();
        validate(&loaded, &Value::String(VALID_GTIN.into())).unwrap();
        let err = validate(&loaded, &Value::String("bad".into())).unwrap_err();
        assert!(matches!(err, Error::ValidationFailed { .. }));
        tx.rollback().await.unwrap();
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn set_is_audited_once() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("cf_audit", profile).await else {
            return;
        };
        migrate(&db).await;
        let pool = write_pool(&db);
        let mut tx = Tx::begin(&pool, &write_ctx("customfields.define", profile))
            .await
            .unwrap();
        define(
            &mut tx,
            DefinitionSpec {
                entity: "items.item".into(),
                key: "note".into(),
                field_type: FieldType::String,
                label: "Note".into(),
                validation_rule: String::new(),
                required: false,
                indexed: false,
                owner_module: "mod-items".into(),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let record = Identifier::generate();
        let mut tx = Tx::begin(&pool, &write_ctx("customfields.set", profile))
            .await
            .unwrap();
        set(
            &mut tx,
            "items.item",
            record,
            "note",
            Value::String("hello".into()),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let n: i64 =
            query_scalar("SELECT count(*) FROM audit.event WHERE table_name = 'value_string'")
                .fetch_one(db.app_pool())
                .await
                .unwrap();
        assert_eq!(n, 1, "exactly one audit row for one set");
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn definition_change_is_versioned_and_type_change_refused() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("cf_ver", profile).await else {
            return;
        };
        migrate(&db).await;
        let pool = write_pool(&db);
        let mut tx = Tx::begin(&pool, &write_ctx("customfields.define", profile))
            .await
            .unwrap();
        let spec = DefinitionSpec {
            entity: "items.item".into(),
            key: "code".into(),
            field_type: FieldType::String,
            label: "Code".into(),
            validation_rule: String::new(),
            required: false,
            indexed: false,
            owner_module: "mod-items".into(),
        };
        define(&mut tx, spec.clone()).await.unwrap();
        let v2 = DefinitionSpec {
            label: "Item code".into(),
            ..spec.clone()
        };
        define(&mut tx, v2).await.unwrap();
        let defs = definitions_for(&mut tx, "items.item").await.unwrap();
        assert_eq!(defs[0].version, 2);
        let err = define(
            &mut tx,
            DefinitionSpec {
                field_type: FieldType::Integer,
                ..spec.clone()
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::TypeChangeRefused { .. }));
        tx.rollback().await.unwrap();
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn retired_definition_values_still_read() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("cf_retired", profile).await else {
            return;
        };
        migrate(&db).await;
        let pool = write_pool(&db);
        let mut tx = Tx::begin(&pool, &write_ctx("customfields.define", profile))
            .await
            .unwrap();
        let id = define(
            &mut tx,
            DefinitionSpec {
                entity: "items.item".into(),
                key: "legacy".into(),
                field_type: FieldType::String,
                label: "Legacy".into(),
                validation_rule: String::new(),
                required: false,
                indexed: false,
                owner_module: "mod-items".into(),
            },
        )
        .await
        .unwrap();
        let record = Identifier::generate();
        set(
            &mut tx,
            "items.item",
            record,
            "legacy",
            Value::String("kept".into()),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let mut tx = Tx::begin(&pool, &write_ctx("mod-items.retire", profile))
            .await
            .unwrap();
        retire(&mut tx, id).await.unwrap();
        tx.commit().await.unwrap();

        let mut tx = Tx::begin(&pool, &write_ctx("customfields.get", profile))
            .await
            .unwrap();
        let v = get(&mut tx, "items.item", record, "legacy")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v, Value::String("kept".into()));
        tx.rollback().await.unwrap();
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn no_json_blob_in_schema() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("cf_nojson", profile).await else {
            return;
        };
        migrate(&db).await;
        let n: i64 = query_scalar(
            "SELECT count(*) FROM information_schema.columns
             WHERE table_schema = 'customfields' AND udt_name = 'jsonb'",
        )
        .fetch_one(db.migrate_pool())
        .await
        .unwrap();
        assert_eq!(n, 0);
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn module_column_on_kernel_table() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("cf_udi", profile).await else {
            return;
        };
        migrate(&db).await;
        let pool = write_pool(&db);
        let mut tx = Tx::begin(&pool, &write_ctx("customfields.define", profile))
            .await
            .unwrap();
        let manifest = ManifestCustomFields {
            fields: vec![ManifestCustomField {
                entity: "items.item".into(),
                key: "udi_device_identifier".into(),
                field_type: FieldType::String,
                label: "UDI Device Identifier".into(),
                validate: "gs1-gtin".into(),
                audit: true,
                required: false,
                indexed: false,
                owner: "mod-udi".into(),
            }],
        };
        register_from_manifest(&mut tx, &manifest).await.unwrap();
        let record = Identifier::generate();
        set(
            &mut tx,
            "items.item",
            record,
            "udi_device_identifier",
            Value::String(VALID_GTIN.into()),
        )
        .await
        .unwrap();
        let read = get(&mut tx, "items.item", record, "udi_device_identifier")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(read, Value::String(VALID_GTIN.into()));
        let defs = definitions_for(&mut tx, "items.item").await.unwrap();
        let id = defs[0].id;
        tx.commit().await.unwrap();

        let mut tx = Tx::begin(&pool, &write_ctx("mod-udi.retire", profile))
            .await
            .unwrap();
        retire(&mut tx, id).await.unwrap();
        tx.commit().await.unwrap();
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn required_field_missing_is_typed_error() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("cf_req", profile).await else {
            return;
        };
        migrate(&db).await;
        let pool = write_pool(&db);
        let mut tx = Tx::begin(&pool, &write_ctx("customfields.define", profile))
            .await
            .unwrap();
        let def = DefinitionSpec {
            entity: "items.item".into(),
            key: "must".into(),
            field_type: FieldType::String,
            label: "Must".into(),
            validation_rule: String::new(),
            required: true,
            indexed: false,
            owner_module: "mod-items".into(),
        };
        define(&mut tx, def).await.unwrap();
        let loaded = definitions_for(&mut tx, "items.item").await.unwrap()[0].clone();
        let err = validate(&loaded, &Value::String(String::new())).unwrap_err();
        assert!(matches!(err, Error::RequiredFieldMissing { .. }));
        tx.rollback().await.unwrap();
        db.finish().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn writes_go_through_tx() {
    for_each_profile(|profile| async move {
        let Some(db) = open_db("cf_fence", profile).await else {
            return;
        };
        migrate(&db).await;
        let err = sqlx::query(
            "INSERT INTO customfields.definition
             (definition_id, version, entity, key, field_type, label, owner_module, status)
             VALUES (gen_random_uuid(), 1, 'x', 'y', 'string', 'L', 'mod-x', 'active')",
        )
        .execute(db.app_pool())
        .await
        .unwrap_err();
        assert_eq!(
            err.as_database_error()
                .and_then(|d| d.code().map(|c| c.to_string()))
                .unwrap_or_default(),
            "42501"
        );
        db.finish().await.unwrap();
    })
    .await;
}
