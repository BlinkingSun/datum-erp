//! Read path for GET handlers whose module APIs take `&mut Tx`.
//!
//! `Tx` has no public read constructor. This helper opens a sealed `Tx` on
//! the app pool and callers **must rollback** (never commit) so SELECT-only
//! module loads and genealogy `cache_put` do not persist. GET handler bodies
//! must not mention `Tx::begin` or `commit`.

use axum::http::HeaderMap;
use wicket_db::{Tx, WritePool};

use crate::boot::AppState;
use crate::error::Result;
use crate::session::{self, HttpSession};

/// App pool wrapped for the module `&mut Tx` load APIs.
pub fn pool(state: &AppState) -> WritePool {
    WritePool::new(state.pool().clone())
}

/// Begin a Tx for a GET. Caller rolls back.
pub async fn begin<'c>(
    write: &'c WritePool,
    session: &HttpSession,
    action: &str,
    request_id: &str,
    headers: &HeaderMap,
    spec_version: &str,
) -> Result<Tx<'c>> {
    let ctx = session::write_context(session, action, request_id, headers, spec_version);
    Ok(Tx::begin(write, &ctx).await?)
}
