//! Shared helpers for commit-mode datum-module tests.

#![allow(dead_code, unused_imports)]

use datum_db::WritePool;
use sqlx::{PgPool, query_scalar as sql_query_scalar};

use datum_module::{attach_kernel_audit, migrate_prefix, migrate_suffix};

pub async fn migrate_and_install(db: &datum_test::TestDb) {
    migrate_prefix(db.migrate_pool())
        .await
        .expect("migrate prefix");
    migrate_suffix(db.migrate_pool())
        .await
        .unwrap_or_else(|e| panic!("migrate suffix: {e:#}"));
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    datum_audit::install_privileged(&boot)
        .await
        .expect("install_privileged");
    boot.close().await;
    attach_kernel_audit(db.migrate_pool())
        .await
        .expect("attach_kernel_audit");
}

pub fn write_pool(db: &datum_test::TestDb) -> WritePool {
    WritePool::new(db.app_pool().clone())
}

pub fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_else(|| format!("{err}"))
}

pub async fn has_zz_audit(pool: &PgPool, schema: &str, table: &str) -> bool {
    sql_query_scalar(
        r#"SELECT EXISTS (
             SELECT 1 FROM pg_trigger t
             JOIN pg_class c ON c.oid = t.tgrelid
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND c.relname = $2
               AND t.tgname = 'zz_audit_row' AND NOT t.tgisinternal
           )"#,
    )
    .bind(schema)
    .bind(table)
    .fetch_one(pool)
    .await
    .expect("trigger")
}

pub async fn table_count(pool: &PgPool, sql: &'static str) -> i64 {
    sql_query_scalar(sql).fetch_one(pool).await.expect("count")
}

pub async fn module_enabled(pool: &PgPool, id: &str) -> Option<bool> {
    sql_query_scalar("SELECT enabled FROM module.installed WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .expect("enabled")
}

pub async fn module_hash(pool: &PgPool, id: &str) -> Option<String> {
    sql_query_scalar("SELECT manifest_hash FROM module.installed WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .expect("hash")
}

pub fn toy_manifest(id: &str, deps: &[(&str, &str)]) -> datum_module::ModuleManifest {
    let mut deps_toml = String::from("kernel = \"^0.1\"\n");
    for (dep, range) in deps {
        deps_toml.push_str(&format!("{dep} = \"{range}\"\n"));
    }
    let src = format!(
        r#"
[module]
id = "{id}"
version = "0.1.0"
name = "{id}"
description = "toy"

[dependencies]
{deps_toml}
[permissions]
"{id}.view" = "View {id}"

[capabilities]
requires-signature = []
regulated = false
"#
    );
    datum_module::ModuleManifest::parse(&src).expect("toy manifest")
}

pub fn toy_manifest_regulated(id: &str) -> datum_module::ModuleManifest {
    let src = format!(
        r#"
[module]
id = "{id}"
version = "0.1.0"
name = "{id}"
description = "regulated toy"

[dependencies]
kernel = "^0.1"

[permissions]
"{id}.view" = "View {id}"
"{id}.approve" = "Approve {id}"

[capabilities]
requires-signature = ["{id}.approve"]
regulated = true
"#
    );
    datum_module::ModuleManifest::parse(&src).expect("regulated toy")
}
