//! Superuser finish step: writer-role function owners and event triggers.

use sqlx::PgPool;

use crate::Result;

const ALTER_OWNERS: &str = r#"
ALTER FUNCTION audit.row_change() OWNER TO datum_audit_row;
ALTER FUNCTION audit.stmt_truncate() OWNER TO datum_audit_row;
ALTER FUNCTION audit.log_event(text, text, text, text, text, text, jsonb)
  OWNER TO datum_audit_event;
"#;

const ATTACH_ET: &str = r#"
CREATE EVENT TRIGGER audit_attach ON ddl_command_end
  WHEN TAG IN ('CREATE TABLE', 'CREATE TABLE AS', 'SELECT INTO')
  EXECUTE FUNCTION audit.attach_new_tables();
"#;

const PROTECT_ET: &str = r#"
CREATE EVENT TRIGGER audit_protect ON ddl_command_start
  WHEN TAG IN (
    'ALTER TABLE', 'ALTER TRIGGER', 'DROP TRIGGER',
    'DROP FUNCTION', 'DROP ROUTINE', 'DROP PROCEDURE'
  )
  EXECUTE FUNCTION audit.protect();
CREATE EVENT TRIGGER audit_protect_drop ON sql_drop
  WHEN TAG IN (
    'DROP TRIGGER', 'DROP FUNCTION', 'DROP ROUTINE',
    'DROP PROCEDURE'
  )
  EXECUTE FUNCTION audit.protect();
"#;

/// Apply writer-role ownership and create `audit_attach` / `audit_protect`.
///
/// Must run as a superuser (the bootstrap role) against the target database.
pub async fn install_privileged(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(ALTER_OWNERS).execute(pool).await?;
    let attach: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_event_trigger WHERE evtname = 'audit_attach')",
    )
    .fetch_one(pool)
    .await?;
    if !attach {
        sqlx::raw_sql(ATTACH_ET).execute(pool).await?;
    }
    let protect: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_event_trigger WHERE evtname = 'audit_protect')",
    )
    .fetch_one(pool)
    .await?;
    let protect_drop: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_event_trigger WHERE evtname = 'audit_protect_drop')",
    )
    .fetch_one(pool)
    .await?;
    if !protect || !protect_drop {
        if protect {
            sqlx::raw_sql("DROP EVENT TRIGGER IF EXISTS audit_protect")
                .execute(pool)
                .await?;
        }
        if protect_drop {
            sqlx::raw_sql("DROP EVENT TRIGGER IF EXISTS audit_protect_drop")
                .execute(pool)
                .await?;
        }
        sqlx::raw_sql(PROTECT_ET).execute(pool).await?;
    }
    Ok(())
}

/// Drop the event triggers so a reverse migration can drop audit objects.
/// Reverse migrations that drop `zz_audit_*` run as the bootstrap superuser
/// (`TestDb` already migrates down through it).
pub async fn uninstall_privileged(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
        "DROP EVENT TRIGGER IF EXISTS audit_protect_drop; DROP EVENT TRIGGER IF EXISTS audit_protect; DROP EVENT TRIGGER IF EXISTS audit_attach;",
    )
    .execute(pool)
    .await?;
    Ok(())
}
