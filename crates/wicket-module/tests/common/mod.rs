//! Shared helpers for commit-mode wicket-module tests.

#![allow(dead_code, unused_imports)]

use sqlx::{PgPool, query_scalar as sql_query_scalar};
use wicket_core::{Actor, ActorKind, Identifier};
use wicket_db::{Tx, WriteContext, WritePool};
use wicket_identity::rbac::{RoleBundle, assign_role, seed_bundles};
use wicket_identity::{Principal, PrincipalKind, create_principal, set_signing_credential};
use wicket_statemachine::{DocRef, with_action};

use wicket_module::{install_kernel, install_slice};

pub async fn migrate_and_install(db: &wicket_test::TestDb) {
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    install_kernel(db.migrate_pool(), &boot)
        .await
        .unwrap_or_else(|e| panic!("install_kernel: {e:#}"));
    boot.close().await;
}

pub async fn migrate_and_install_slice(db: &wicket_test::TestDb) {
    let boot = db.bootstrap_pool().await.expect("bootstrap pool");
    install_slice(db.migrate_pool(), &boot)
        .await
        .unwrap_or_else(|e| panic!("install_slice: {e:#}"));
    boot.close().await;
}

pub fn write_pool(db: &wicket_test::TestDb) -> WritePool {
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

pub fn toy_manifest(id: &str, deps: &[(&str, &str)]) -> wicket_module::ModuleManifest {
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
    wicket_module::ModuleManifest::parse(&src).expect("toy manifest")
}

pub fn toy_manifest_regulated(id: &str) -> wicket_module::ModuleManifest {
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
    wicket_module::ModuleManifest::parse(&src).expect("regulated toy")
}

/// Principal holding `permission`, and a WriteContext for `doc`/`edge`.
pub async fn actor_with_perm(
    write: &WritePool,
    permission: &str,
    doc: &DocRef,
    edge: &str,
) -> (Actor, WriteContext) {
    let actor = actor_with_perms(write, &[permission]).await;
    let mut ctx = WriteContext::new(actor, "pending", "ui");
    ctx.actor_display = Some("Operator".into());
    ctx.reason = Some("module-glue-test".into());
    let ctx = with_action(ctx, doc, edge);
    (actor, ctx)
}

/// Signing secret used by kernel e2e mint (never the login credential).
pub const SIGNING_SECRET: &str = "signing-secret-ok";

/// Principal holding every listed permission (no bound action).
pub async fn actor_with_perms(write: &WritePool, permissions: &[&str]) -> Actor {
    signer_with_perms(write, permissions).await.actor()
}

/// Principal holding `permissions` and a signing credential (D-2b-3).
pub async fn signer_with_perms(write: &WritePool, permissions: &[&str]) -> Principal {
    let slug = Identifier::generate().to_string();
    let short: String = slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    let mut tx = Tx::begin(write, &boot_ctx()).await.expect("begin actor");
    let p = create_principal(
        &mut tx,
        PrincipalKind::User,
        &format!("u{short}"),
        "Operator",
    )
    .await
    .expect("principal");
    set_signing_credential(&mut tx, p.id, SIGNING_SECRET)
        .await
        .expect("signing cred");
    let roles = seed_bundles(
        &mut tx,
        &[RoleBundle {
            name: format!("r{short}"),
            permissions: permissions.iter().map(|s| (*s).to_owned()).collect(),
        }],
    )
    .await
    .expect("role");
    assign_role(&mut tx, p.id, roles[0]).await.expect("assign");
    tx.commit().await.expect("commit actor");
    p
}

pub fn boot_ctx() -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::from_uuid(wicket_identity::SYSTEM_ID),
            kind: ActorKind::ServicePrincipal,
        },
        "module.boot",
        "maintenance",
    );
    ctx.actor_display = Some("system".into());
    ctx.reason = Some("module-test".into());
    ctx
}
