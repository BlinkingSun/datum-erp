//! SHA-256 hex for idempotency body hashes (via `datum-audit`).

use datum_audit::sha256::{digest, hex};

/// SHA-256 digest of `data` as lowercase hex (64 chars).
pub fn sha256_hex(data: &[u8]) -> String {
    hex(&digest(data))
}
