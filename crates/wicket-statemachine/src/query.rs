//! Published live-state reads for `sm.instance` / `sm.machine`.
//!
//! Kernel crates that cannot depend on `wicket-module` (CONTRACT §4) call these
//! instead of selecting `sm.instance` themselves (R-2s-3). Backed by the
//! invoker-rights SQL helpers in `0002_query_seam`. Reads are offered on the
//! sealed [`Tx`] and on [`ReadPool`].

use sqlx::query_as;
use uuid::Uuid;
use wicket_core::Identifier;
use wicket_db::{ReadPool, Tx};

use crate::Result;
use crate::decl::{DocRef, MachineId, State};

/// Current state of the instance identified by `doc`, or `None` if no row.
///
/// Reads through `sm.current_state(text, uuid)`.
pub async fn current_state(tx: &mut Tx<'_>, doc: &DocRef) -> Result<Option<State>> {
    let (state,): (Option<String>,) = tx
        .fetch_one(
            query_as("SELECT sm.current_state($1, $2)")
                .bind(&doc.doc_type)
                .bind(doc.doc_id.as_uuid()),
        )
        .await?;
    Ok(state.map(State))
}

/// [`current_state`] through a [`ReadPool`] (no actor bound on the connection).
pub async fn current_state_on(pool: &ReadPool, doc: &DocRef) -> Result<Option<State>> {
    let (state,): (Option<String>,) = pool
        .fetch_one(
            query_as("SELECT sm.current_state($1, $2)")
                .bind(&doc.doc_type)
                .bind(doc.doc_id.as_uuid()),
        )
        .await?;
    Ok(state.map(State))
}

/// Whether an `sm.instance` row exists for `doc`.
///
/// Reads through `sm.instance_exists(text, uuid)`.
pub async fn instance_exists(tx: &mut Tx<'_>, doc: &DocRef) -> Result<bool> {
    let (exists,): (bool,) = tx
        .fetch_one(
            query_as("SELECT sm.instance_exists($1, $2)")
                .bind(&doc.doc_type)
                .bind(doc.doc_id.as_uuid()),
        )
        .await?;
    Ok(exists)
}

/// [`instance_exists`] through a [`ReadPool`].
pub async fn instance_exists_on(pool: &ReadPool, doc: &DocRef) -> Result<bool> {
    let (exists,): (bool,) = pool
        .fetch_one(
            query_as("SELECT sm.instance_exists($1, $2)")
                .bind(&doc.doc_type)
                .bind(doc.doc_id.as_uuid()),
        )
        .await?;
    Ok(exists)
}

/// Catalog machine id for `doc_type`, or `None` if no machine is persisted.
///
/// Reads through `sm.machine_id_for(text)`. Documents (and other kernel crates
/// that cannot hold an [`crate::Engine`]) use this instead of selecting
/// `sm.machine`.
pub async fn machine_id_for(tx: &mut Tx<'_>, doc_type: &str) -> Result<Option<MachineId>> {
    let (id,): (Option<Uuid>,) = tx
        .fetch_one(query_as("SELECT sm.machine_id_for($1)").bind(doc_type))
        .await?;
    Ok(id.map(|u| MachineId(Identifier::from_uuid(u))))
}

/// [`machine_id_for`] through a [`ReadPool`].
pub async fn machine_id_for_on(pool: &ReadPool, doc_type: &str) -> Result<Option<MachineId>> {
    let (id,): (Option<Uuid>,) = pool
        .fetch_one(query_as("SELECT sm.machine_id_for($1)").bind(doc_type))
        .await?;
    Ok(id.map(|u| MachineId(Identifier::from_uuid(u))))
}
