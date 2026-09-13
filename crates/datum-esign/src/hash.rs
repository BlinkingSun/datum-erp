//! Content hash: SHA-256 over the canonical JSON of the signed snapshot.

use serde_json::Value;

use crate::{Error, Result};

/// SHA-256 of the canonical JSONB bytes of `snapshot`.
pub fn content_hash(snapshot: &Value) -> Result<[u8; 32]> {
    let bytes = canonical_bytes(snapshot)?;
    Ok(datum_audit::sha256::digest(&bytes))
}

/// Canonical JSON bytes (object keys sorted; `serde_json::Map` is a `BTreeMap`).
pub fn canonical_bytes(value: &Value) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(Error::from)
}

/// Lowercase hex of a SHA-256 digest.
pub fn hex(hash: &[u8; 32]) -> String {
    datum_audit::sha256::hex(hash)
}

/// Build the signed snapshot: the registered projection plus the `sm.instance` triple.
pub fn snapshot(projection: &Value, instance: &crate::InstanceTriple) -> Result<Value> {
    Ok(serde_json::json!({
        "projection": projection,
        "instance": {
            "doc_type": instance.doc_type,
            "doc_id": instance.doc_id,
            "state": instance.state,
            "version": instance.version,
        },
    }))
}
