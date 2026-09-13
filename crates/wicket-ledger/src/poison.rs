//! Unfinalized-sink poison keyed by `pg_current_xact_id()` (wicket-db has no `Tx::poison`).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

fn poisoned() -> &'static Mutex<HashSet<String>> {
    static SET: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SET.get_or_init(|| Mutex::new(HashSet::new()))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Mark `txid` so [`crate::commit`] refuses with [`crate::Error::Unfinalized`].
pub fn mark(txid: &str) {
    lock(poisoned()).insert(txid.to_string());
}

/// Clear after successful finalize.
pub fn clear(txid: &str) {
    lock(poisoned()).remove(txid);
}

/// Whether `txid` has an unfinalized sink.
pub fn is_marked(txid: &str) -> bool {
    lock(poisoned()).contains(txid)
}

/// Whether `txid` is poisoned (`test-utils` only).
#[cfg(feature = "test-utils")]
pub fn test_is_marked(txid: &str) -> bool {
    is_marked(txid)
}
