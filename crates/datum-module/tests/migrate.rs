//! Reversible module-registry migration (PLAN §6 invariant 8).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

use std::collections::BTreeSet;

use datum_module::{
    CANONICAL_ORDER, MIGRATOR, attach_kernel_audit, attach_slice_audit, canonical_migrators,
    install_upto,
};
use datum_test::db_case;
use sqlx::migrate::Migrator;

fn reversible_migrator() -> Migrator {
    let mut migrator = Migrator::with_migrations(MIGRATOR.iter().cloned().collect());
    // `module` is app-class; a version table there cannot be INSERTed without
    // Tx::begin. `transient` is skipped by audit_attach.
    migrator.dangerous_set_table_name("transient._sqlx_migrations_module");
    migrator
}

async fn catalog_module(pool: &sqlx::PgPool) -> String {
    let cols: Vec<(String, String, String, bool)> = sqlx::query_as(
        r#"
        SELECT c.relname::text, a.attname::text, t.typname::text, a.attnotnull
        FROM pg_attribute a
        JOIN pg_class c ON c.oid = a.attrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_type t ON t.oid = a.atttypid
        WHERE n.nspname = 'module'
          AND a.attnum > 0
          AND NOT a.attisdropped
          AND c.relkind IN ('r', 'p')
        ORDER BY c.relname, a.attnum
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("cols");
    let cons: Vec<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT c.relname::text, con.contype::text, con.conname::text
        FROM pg_constraint con
        JOIN pg_class c ON c.oid = con.conrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'module'
        ORDER BY 1, 2, 3
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("cons");
    let owners: Vec<(String, String, bool)> = sqlx::query_as(
        r#"
        SELECT c.relname::text, r.rolname::text, r.rolcanlogin
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_roles r ON r.oid = c.relowner
        WHERE n.nspname = 'module' AND c.relkind IN ('r', 'p')
        ORDER BY 1
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("owners");
    let class: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT nspname::text, class::text
        FROM datum.schema_class
        WHERE nspname = 'module'
        ORDER BY 1
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("schema_class");
    format!("{cols:?}\n{cons:?}\n{owners:?}\n{class:?}")
}

#[tokio::test]
async fn migrate_down_then_up() {
    let db = db_case!("mod_down_up");
    let boot = db.bootstrap_pool().await.expect("bootstrap");
    install_upto(db.migrate_pool(), &boot, "datum-print")
        .await
        .expect("install_upto through predecessor");
    boot.close().await;

    let migrator = reversible_migrator();
    migrator.run(db.migrate_pool()).await.expect("up");
    let before = catalog_module(db.migrate_pool()).await;

    migrator
        .undo(db.migrate_pool(), 0)
        .await
        .expect("down to placeholder");
    let gone: bool = sqlx::query_scalar("SELECT to_regclass('module.installed') IS NULL")
        .fetch_one(db.migrate_pool())
        .await
        .expect("dropped");
    assert!(gone, "0001 down must drop module.installed");
    let class_gone: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM datum.schema_class WHERE nspname = 'module')",
    )
    .fetch_one(db.migrate_pool())
    .await
    .expect("class gone");
    assert!(class_gone, "0001 down must drop module from schema_class");

    migrator.run(db.migrate_pool()).await.expect("up again");
    let after = catalog_module(db.migrate_pool()).await;
    assert_eq!(
        before, after,
        "catalogue must be identical after down-then-up"
    );
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn wave_2s1_migrate_down_then_up() {
    let db = db_case!("w2s1_down_up");
    let boot = db.bootstrap_pool().await.expect("bootstrap");
    install_upto(db.migrate_pool(), &boot, "datum-module")
        .await
        .expect("install_upto through datum-module");
    boot.close().await;

    let order = datum_module::wave_2s1_order().expect("order");
    assert_eq!(
        order,
        vec![
            "mod-items".to_string(),
            "mod-locations".to_string(),
            "mod-lots".to_string(),
        ],
        "lots runs after items and locations"
    );

    for (name, migrator) in datum_module::wave_2s1_migrators().expect("migrators") {
        let rel = match name {
            "datum-mod-items" => "items.item",
            "datum-mod-locations" => "locations.site",
            "datum-mod-lots" => "lots.lot",
            other => panic!("unexpected crate {other}"),
        };
        let mut m = Migrator::with_migrations(migrator.iter().cloned().collect());
        m.dangerous_set_table_name(format!(
            "transient._sqlx_migrations_{}",
            name.replace('-', "_")
        ));
        m.run(db.migrate_pool()).await.expect("up");
        let present: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(rel)
            .fetch_one(db.migrate_pool())
            .await
            .expect("present");
        assert!(present, "{name} must exist after up");

        m.undo(db.migrate_pool(), 0).await.expect("down");
        let gone: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NULL")
            .bind(rel)
            .fetch_one(db.migrate_pool())
            .await
            .expect("gone");
        assert!(gone, "{name} down must drop {rel}");

        m.run(db.migrate_pool()).await.expect("up again");
        let again: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(rel)
            .fetch_one(db.migrate_pool())
            .await
            .expect("again");
        assert!(again, "{name} second up must recreate {rel}");
    }
    db.finish().await.expect("finish");
}

async fn zz_audit_set(pool: &sqlx::PgPool) -> BTreeSet<(String, String)> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT n.nspname::text, c.relname::text
          FROM pg_trigger t
          JOIN pg_class c ON c.oid = t.tgrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
         ORDER BY 1, 2
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("zz_audit_row set");
    rows.into_iter().collect()
}

/// Predecessors with the trigger down, then `install_privileged`, then `crate_name`.
/// The bootstrap pair has no event trigger until `datum-audit` itself is applied,
/// so both trigger states collapse to [`install_upto`].
async fn install_crate_last(migrate: &sqlx::PgPool, bootstrap: &sqlx::PgPool, crate_name: &str) {
    if matches!(crate_name, "datum-db" | "datum-audit") {
        install_upto(migrate, bootstrap, crate_name)
            .await
            .unwrap_or_else(|e| panic!("install_upto {crate_name}: {e:#}"));
        return;
    }
    let crates = canonical_migrators().expect("canonical migrators");
    let idx = crates
        .iter()
        .position(|(name, _)| *name == crate_name)
        .unwrap_or_else(|| panic!("{crate_name} not in CANONICAL_ORDER"));
    for (name, migrator) in crates.iter().take(idx) {
        datum_db::migrate::run(migrate, &[(*name, *migrator)])
            .await
            .unwrap_or_else(|e| panic!("predecessor {name}: {e:#}"));
    }
    datum_audit::install_privileged(bootstrap)
        .await
        .expect("install_privileged after predecessors");
    let (name, migrator) = crates[idx];
    datum_db::migrate::run(migrate, &[(name, migrator)])
        .await
        .unwrap_or_else(|e| panic!("crate last {name}: {e:#}"));
    attach_kernel_audit(migrate)
        .await
        .expect("belt-and-braces kernel attach");
    attach_slice_audit(migrate)
        .await
        .expect("belt-and-braces slice attach");
}

#[tokio::test]
async fn every_crate_migrates_in_both_trigger_states() {
    for crate_name in CANONICAL_ORDER {
        let slug = crate_name.replace('-', "_");

        let db_canon = db_case!(&format!("c_{slug}"));
        let boot_canon = db_canon.bootstrap_pool().await.expect("bootstrap canon");
        install_upto(db_canon.migrate_pool(), &boot_canon, crate_name)
            .await
            .unwrap_or_else(|e| panic!("canonical {crate_name}: {e:#}"));
        boot_canon.close().await;
        let canon = zz_audit_set(db_canon.migrate_pool()).await;

        let db_last = db_case!(&format!("l_{slug}"));
        let boot_last = db_last.bootstrap_pool().await.expect("bootstrap last");
        install_crate_last(db_last.migrate_pool(), &boot_last, crate_name).await;
        boot_last.close().await;
        let last = zz_audit_set(db_last.migrate_pool()).await;

        assert_eq!(
            canon, last,
            "{crate_name}: zz_audit_row set must be identical at canonical position vs last-crate"
        );
        db_canon.finish().await.expect("finish canon");
        db_last.finish().await.expect("finish last");
    }
}
