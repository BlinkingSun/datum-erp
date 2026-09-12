//! Catalogue lints for D-W1-2 / PLAN invariant 16.

use crate::Pool;
use crate::error::{Error, Result, is_undefined_table};

/// Read the catalogue and fail on banned DDL.
///
/// Fails on:
/// - any `ON DELETE CASCADE` in any schema
/// - any table outside `audit` / `transient` / `datum` / app-class schemas
/// - any `DELETE` grant to `datum_app` outside `transient`
/// - any explicit `TRUNCATE` grant to a login role other than the table owner
pub async fn check(pool: &Pool) -> Result<()> {
    let mut violations = Vec::new();
    cascade(pool, &mut violations).await?;
    stray_tables(pool, &mut violations).await?;
    app_delete(pool, &mut violations).await?;
    truncate_grants(pool, &mut violations).await?;
    if violations.is_empty() {
        Ok(())
    } else {
        Err(Error::Ddl(violations))
    }
}

async fn cascade(pool: &Pool, out: &mut Vec<String>) -> Result<()> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT n.nspname::text, c.relname::text, con.conname::text
        FROM pg_constraint con
        JOIN pg_class c ON c.oid = con.conrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE con.contype = 'f'
          AND con.confdeltype = 'c'
        ORDER BY 1, 2, 3
        "#,
    )
    .fetch_all(pool)
    .await?;
    for (schema, table, constraint) in rows {
        out.push(format!(
            "ON DELETE CASCADE {schema}.{table} constraint {constraint}"
        ));
    }
    Ok(())
}

async fn stray_tables(pool: &Pool, out: &mut Vec<String>) -> Result<()> {
    let mut allowed: Vec<String> = vec![
        "audit".into(),
        "transient".into(),
        "datum".into(),
        "app".into(),
    ];
    match sqlx::query_as::<_, (String,)>(
        "SELECT nspname::text FROM datum.schema_class WHERE class = 'app'",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => {
            for (name,) in rows {
                if !allowed.iter().any(|a| a == &name) {
                    allowed.push(name);
                }
            }
        }
        Err(e) if is_undefined_table(&e) => {}
        Err(e) => return Err(Error::from(e)),
    }

    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT n.nspname::text, c.relname::text
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind IN ('r', 'p')
          AND n.nspname NOT IN ('pg_catalog', 'information_schema')
          AND n.nspname NOT LIKE 'pg\_%'
          AND c.relname IS DISTINCT FROM '_sqlx_migrations'
        ORDER BY 1, 2
        "#,
    )
    .fetch_all(pool)
    .await?;
    for (schema, table) in rows {
        if allowed.iter().any(|a| a == &schema) {
            continue;
        }
        out.push(format!(
            "table {schema}.{table} outside audit/transient/datum/app-class schemas"
        ));
    }
    Ok(())
}

async fn app_delete(pool: &Pool, out: &mut Vec<String>) -> Result<()> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT n.nspname::text, c.relname::text
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace,
        LATERAL aclexplode(c.relacl) acl
        JOIN pg_roles r ON r.oid = acl.grantee
        WHERE c.relacl IS NOT NULL
          AND c.relkind IN ('r', 'p')
          AND r.rolname = 'datum_app'
          AND acl.privilege_type = 'DELETE'
          AND n.nspname IS DISTINCT FROM 'transient'
        ORDER BY 1, 2
        "#,
    )
    .fetch_all(pool)
    .await?;
    for (schema, table) in rows {
        out.push(format!(
            "DELETE grant to datum_app on {schema}.{table} outside transient"
        ));
    }
    Ok(())
}

async fn truncate_grants(pool: &Pool, out: &mut Vec<String>) -> Result<()> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT r.rolname::text, n.nspname::text, c.relname::text
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace,
        LATERAL aclexplode(c.relacl) acl
        JOIN pg_roles r ON r.oid = acl.grantee
        WHERE c.relacl IS NOT NULL
          AND c.relkind IN ('r', 'p')
          AND r.rolcanlogin
          AND acl.privilege_type = 'TRUNCATE'
          AND c.relowner IS DISTINCT FROM r.oid
        ORDER BY 1, 2, 3
        "#,
    )
    .fetch_all(pool)
    .await?;
    for (role, schema, table) in rows {
        out.push(format!(
            "TRUNCATE grant to login role {role} on {schema}.{table}"
        ));
    }
    Ok(())
}
