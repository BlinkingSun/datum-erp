//! Principals: never deleted, never reused (invariants 13, 15).

use chrono::{DateTime, Utc};
use datum_core::{Actor, ActorKind, Identifier};
use serde::{Deserialize, Serialize};
use sqlx::{query as sql_query, query_as as sql_query_as};
use uuid::Uuid;

use crate::{Error, Result, UserId, map_tx};

pub(crate) type PrincipalRow = (
    Uuid,
    String,
    String,
    String,
    String,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
);

/// Built-in service principal `system` (also `audit.log_event`'s unattributable actor).
pub const SYSTEM_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0001);
/// Built-in service principal `migration`.
pub const MIGRATION_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0002);

/// Kind stored on `identity.principal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrincipalKind {
    /// Human user.
    User,
    /// Named service.
    Service,
    /// Migration runner.
    Migration,
}

impl PrincipalKind {
    fn as_db(self) -> &'static str {
        match self {
            PrincipalKind::User => "user",
            PrincipalKind::Service => "service",
            PrincipalKind::Migration => "migration",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        match s {
            "user" => Ok(Self::User),
            "service" => Ok(Self::Service),
            "migration" => Ok(Self::Migration),
            other => Err(Error::Crypto(format!("unknown principal kind {other}"))),
        }
    }

    fn actor_kind(self) -> ActorKind {
        match self {
            PrincipalKind::User => ActorKind::User,
            PrincipalKind::Service | PrincipalKind::Migration => ActorKind::ServicePrincipal,
        }
    }
}

/// Activation status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrincipalStatus {
    /// May authenticate.
    Active,
    /// Deactivated; username still reserved.
    Inactive,
}

impl PrincipalStatus {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            other => Err(Error::Crypto(format!("unknown status {other}"))),
        }
    }
}

/// Authenticated principal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    /// Principal id.
    pub id: UserId,
    /// Actor kind for `Tx::begin` (stub field, derived from [`Self::principal_kind`]).
    pub kind: ActorKind,
    /// Precise kind stored in the database.
    pub principal_kind: PrincipalKind,
    /// Unique username; never recycled.
    pub username: String,
    /// Current display name.
    pub display_name: String,
    /// Active or inactive.
    pub status: PrincipalStatus,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Deactivated at, if inactive.
    pub deactivated_at: Option<DateTime<Utc>>,
}

impl Principal {
    /// Actor mapping for `Tx::begin`.
    pub fn actor(&self) -> Actor {
        Actor {
            id: self.id.0,
            kind: self.kind,
        }
    }

    /// Display name as of `at` (invariant 15). Reads `identity.display_name_history`.
    pub async fn display_name_at(
        &self,
        pool: &datum_db::Pool,
        at: DateTime<Utc>,
    ) -> Result<String> {
        display_name_at(pool, self.id, at).await
    }
}

/// Insert `system` and `migration` if missing. Migration `0001_identity` already
/// inserts both; this helper is an idempotent no-op when they are present.
pub async fn seed_builtins(tx: &mut datum_db::Tx<'_>) -> Result<()> {
    tx.execute(
        sql_query(
            r#"INSERT INTO identity.principal
                   (id, kind, username, display_name, status, created_at)
               SELECT x.id, x.kind, x.username, x.display_name, 'active', now()
                 FROM (VALUES
                   ($1::uuid, 'service',   'system',    'system'),
                   ($2::uuid, 'migration', 'migration', 'migration')
                 ) AS x(id, kind, username, display_name)
                WHERE NOT EXISTS (
                  SELECT 1 FROM identity.principal p WHERE p.id = x.id
                )"#,
        )
        .bind(SYSTEM_ID)
        .bind(MIGRATION_ID),
    )
    .await
    .map_err(map_tx)?;
    Ok(())
}

/// Look up a principal.
pub async fn load_principal(pool: &datum_db::Pool, id: UserId) -> Result<Principal> {
    let row: Option<PrincipalRow> = sql_query_as(
        r#"SELECT id, kind, username, display_name, status, created_at, deactivated_at
               FROM identity.principal WHERE id = $1"#,
    )
    .bind(id.as_uuid())
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Err(Error::NotFound);
    };
    row_to_principal(row)
}

/// Create a principal. Username uniqueness is enforced by history + unique index.
pub async fn create_principal(
    tx: &mut datum_db::Tx<'_>,
    kind: PrincipalKind,
    username: &str,
    display_name: &str,
) -> Result<Principal> {
    let id = UserId::generate();
    let row: PrincipalRow = tx
        .fetch_one(
            sql_query_as(
                r#"INSERT INTO identity.principal
                       (id, kind, username, display_name, status, created_at)
                   VALUES ($1, $2, $3, $4, 'active', now())
                   RETURNING id, kind, username, display_name, status, created_at, deactivated_at"#,
            )
            .bind(id.as_uuid())
            .bind(kind.as_db())
            .bind(username)
            .bind(display_name),
        )
        .await
        .map_err(map_tx)?;
    row_to_principal(row)
}

/// Deactivate. Status change; no DELETE.
pub async fn deactivate_principal(tx: &mut datum_db::Tx<'_>, id: UserId) -> Result<()> {
    let n = tx
        .execute(
            sql_query(
                r#"UPDATE identity.principal
                      SET status = 'inactive', deactivated_at = now()
                    WHERE id = $1 AND status = 'active'"#,
            )
            .bind(id.as_uuid()),
        )
        .await
        .map_err(map_tx)?;
    if n.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}

/// Change the printed name; history keeps every previous value.
pub async fn rename_principal(
    tx: &mut datum_db::Tx<'_>,
    id: UserId,
    display_name: &str,
) -> Result<()> {
    let n = tx
        .execute(
            sql_query("UPDATE identity.principal SET display_name = $2 WHERE id = $1")
                .bind(id.as_uuid())
                .bind(display_name),
        )
        .await
        .map_err(map_tx)?;
    if n.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}

/// Name as of `at`.
pub async fn display_name_at(
    pool: &datum_db::Pool,
    id: UserId,
    at: DateTime<Utc>,
) -> Result<String> {
    let row: Option<(String,)> = sql_query_as(
        r#"SELECT display_name
             FROM identity.display_name_history
            WHERE principal_id = $1 AND at <= $2
            ORDER BY at DESC
            LIMIT 1"#,
    )
    .bind(id.as_uuid())
    .bind(at)
    .fetch_optional(pool)
    .await?;
    if let Some((name,)) = row {
        return Ok(name);
    }
    // `at` is before the first history row: the name in force at `at` is the
    // first recorded name, never the live `principal.display_name`.
    let first: Option<(String,)> = sql_query_as(
        r#"SELECT display_name
             FROM identity.display_name_history
            WHERE principal_id = $1
            ORDER BY at ASC
            LIMIT 1"#,
    )
    .bind(id.as_uuid())
    .fetch_optional(pool)
    .await?;
    match first {
        Some((name,)) => Ok(name),
        None => Err(Error::NotFound),
    }
}

pub(crate) fn row_to_principal(row: PrincipalRow) -> Result<Principal> {
    let kind = PrincipalKind::parse(&row.1)?;
    Ok(Principal {
        id: UserId(Identifier::from_uuid(row.0)),
        kind: kind.actor_kind(),
        principal_kind: kind,
        username: row.2,
        display_name: row.3,
        status: PrincipalStatus::parse(&row.4)?,
        created_at: row.5,
        deactivated_at: row.6,
    })
}
