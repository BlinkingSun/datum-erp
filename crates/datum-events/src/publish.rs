//! Insert into `app.event` inside the caller's transaction.

use chrono::{DateTime, Utc};
use datum_core::Identifier;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::event::Event;

use crate::sql::query_as;

/// Publish `event` inside `tx`. Visible to subscribers only after `tx` commits.
///
/// `occurred_at` is `now()` (transaction server time). `tx_xid` is `pg_current_xact_id()`,
/// so a consumer can join the row to the audit seal of the same transaction. `actor_id`
/// and `source_kind` are taken from the transaction-local context, not from the caller.
pub async fn publish(tx: &mut datum_db::Tx<'_>, event: Event) -> Result<Identifier> {
    crate::schema::global()?.validate(&event.name, event.version, &event.payload)?;

    let actor_raw = tx.setting("datum.actor_id").await?;
    let actor_uuid = Uuid::parse_str(actor_raw.trim()).map_err(|e| {
        Error::Invariant(format!("datum.actor_id is not a uuid ({actor_raw}): {e}"))
    })?;
    let source_kind = tx.setting("datum.source_kind").await?;
    if source_kind.is_empty() {
        return Err(Error::Invariant(
            "datum.source_kind is empty; begin through Tx::begin".into(),
        ));
    }

    let doc_type = match event.doc_type.clone() {
        Some(d) if !d.is_empty() => Some(d),
        _ => {
            let s = tx.setting("datum.doc_type").await?;
            if s.is_empty() { None } else { Some(s) }
        }
    };
    let doc_id = match event.doc_id {
        Some(id) => Some(id.as_uuid()),
        None => {
            let s = tx.setting("datum.doc_id").await?;
            if s.is_empty() {
                None
            } else {
                Some(Uuid::parse_str(s.trim()).map_err(|e| {
                    Error::Invariant(format!("datum.doc_id is not a uuid ({s}): {e}"))
                })?)
            }
        }
    };

    let id = event.id.as_uuid();
    let row: (Uuid, DateTime<Utc>) = tx
        .fetch_one(
            query_as(
                r#"
                INSERT INTO app.event (
                    id, name, version, occurred_at, actor_id, source_kind,
                    doc_type, doc_id, payload, tx_xid
                )
                VALUES (
                    $1, $2, $3, now(), $4, $5,
                    $6, $7, $8, pg_catalog.pg_current_xact_id()
                )
                RETURNING id, occurred_at
                "#,
            )
            .bind(id)
            .bind(&event.name)
            .bind(event.version)
            .bind(actor_uuid)
            .bind(&source_kind)
            .bind(doc_type.as_deref())
            .bind(doc_id)
            .bind(&event.payload),
        )
        .await?;
    let _ = row;
    Ok(Identifier::from_uuid(id))
}
