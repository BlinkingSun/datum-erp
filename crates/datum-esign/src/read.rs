//! Manifestation and archival bundle (D-2b-2, D-2b-8). Reads through a pool.

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use datum_core::{RecordRef, SignatureId};
use datum_db::{ReadPool, Tx};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::query_as as sql_query_as;
use uuid::Uuid;

use crate::hash::{canonical_bytes, content_hash, hex};
use crate::{Error, Result};

/// D-2b-2 manifestation object (the inner `signature` member).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignatureManifest {
    /// Signature id.
    pub id: String,
    /// Signer principal id.
    pub signer_id: String,
    /// Printed name snapshot.
    pub printed_name: String,
    /// Meaning snapshot.
    pub meaning: String,
    /// Reason snapshot.
    pub reason: Option<String>,
    /// UTC instant (`…Z`).
    pub signed_at: String,
    /// Signer's IANA zone.
    pub signed_at_zone: String,
    /// Derived local stamp with offset.
    pub signed_at_local: String,
    /// Record reference including `doc_type`.
    pub record: ManifestRecord,
    /// Hex SHA-256.
    pub record_content_hash: String,
    /// Credential kind.
    pub credential_kind: String,
    /// Components used.
    pub components_used: Vec<String>,
    /// True when the live `sm.instance.version` is greater than `record.version`.
    pub superseded: bool,
    /// Live instance version when [`Self::superseded`]; otherwise `null`.
    #[serde(default)]
    pub superseded_by_version: Option<i64>,
}

/// Record object inside the manifestation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestRecord {
    /// Table name.
    pub table: String,
    /// Display document type.
    pub doc_type: String,
    /// Record id.
    pub id: String,
    /// Record version.
    pub version: i64,
}

/// Exact wire shape: `{ "signature": { … } }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifestation {
    /// Signature manifestation.
    pub signature: SignatureManifest,
}

type ManifestRow = (
    Uuid,
    Uuid,
    String,
    String,
    Option<String>,
    DateTime<Utc>,
    String,
    String,
    String,
    Uuid,
    i64,
    String,
    Vec<u8>,
    String,
    Vec<String>,
    Option<i64>,
);

/// D-2b-2 SELECT list + live `sm.instance.version` overlay (D-2b-7).
/// Duplicated in the record-keyed query because sqlx 0.9 `query_as`
/// requires `'static` SQL.
const MANIFESTATION_BY_ID_SQL: &str = r#"SELECT
               signature_id, signer_id, signer_printed_name, meaning, reason,
               signed_at, signed_at_zone,
               (
                 to_char(timezone(signed_at_zone, signed_at), 'YYYY-MM-DD"T"HH24:MI:SS')
                 || CASE
                      WHEN timezone(signed_at_zone, signed_at)
                           >= timezone('UTC', signed_at)
                      THEN '+' ELSE '-'
                    END
                 || to_char(
                      (abs(extract(epoch from (
                         timezone(signed_at_zone, signed_at)
                         - timezone('UTC', signed_at)
                       )))::int / 3600),
                      'FM00'
                    )
                 || ':'
                 || to_char(
                      ((abs(extract(epoch from (
                         timezone(signed_at_zone, signed_at)
                         - timezone('UTC', signed_at)
                       )))::int % 3600) / 60),
                      'FM00'
                    )
               ) AS signed_at_local,
               record_table, record_id, record_version, doc_type,
               record_content_hash, credential_kind, components_used,
               esign.live_instance_version(
                 CASE WHEN record_table = 'sm.instance' THEN doc_type END,
                 CASE WHEN record_table = 'sm.instance' THEN record_id END
               ) AS live_version
          FROM esign.signature
         WHERE signature_id = $1"#;

const MANIFESTATION_BY_RECORD_SQL: &str = r#"SELECT
               signature_id, signer_id, signer_printed_name, meaning, reason,
               signed_at, signed_at_zone,
               (
                 to_char(timezone(signed_at_zone, signed_at), 'YYYY-MM-DD"T"HH24:MI:SS')
                 || CASE
                      WHEN timezone(signed_at_zone, signed_at)
                           >= timezone('UTC', signed_at)
                      THEN '+' ELSE '-'
                    END
                 || to_char(
                      (abs(extract(epoch from (
                         timezone(signed_at_zone, signed_at)
                         - timezone('UTC', signed_at)
                       )))::int / 3600),
                      'FM00'
                    )
                 || ':'
                 || to_char(
                      ((abs(extract(epoch from (
                         timezone(signed_at_zone, signed_at)
                         - timezone('UTC', signed_at)
                       )))::int % 3600) / 60),
                      'FM00'
                    )
               ) AS signed_at_local,
               record_table, record_id, record_version, doc_type,
               record_content_hash, credential_kind, components_used,
               esign.live_instance_version(
                 CASE WHEN record_table = 'sm.instance' THEN doc_type END,
                 CASE WHEN record_table = 'sm.instance' THEN record_id END
               ) AS live_version
          FROM esign.signature
         WHERE record_table = $1 AND record_id = $2 AND record_version = $3
         ORDER BY signed_at ASC, signature_id ASC"#;

fn row_to_manifestation(row: ManifestRow) -> Manifestation {
    let mut hash = [0u8; 32];
    if row.12.len() == 32 {
        hash.copy_from_slice(&row.12);
    }
    Manifestation {
        signature: SignatureManifest {
            id: row.0.to_string(),
            signer_id: row.1.to_string(),
            printed_name: row.2,
            meaning: row.3,
            reason: row.4,
            signed_at: row.5.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            signed_at_zone: row.6,
            signed_at_local: row.7,
            record: ManifestRecord {
                table: row.8,
                doc_type: row.11,
                id: row.9.to_string(),
                version: row.10,
            },
            record_content_hash: hex(&hash),
            credential_kind: row.13,
            components_used: row.14,
            superseded: row.15.is_some_and(|live| live > row.10),
            superseded_by_version: row.15.filter(|live| *live > row.10),
        },
    }
}

/// Read the D-2b-2 manifestation through the published [`ReadPool`]
/// fetch surface (CONTRACT §5a).
pub async fn manifestation(pool: &ReadPool, id: SignatureId) -> Result<Manifestation> {
    let row: Option<ManifestRow> = pool
        .fetch_optional(sql_query_as(MANIFESTATION_BY_ID_SQL).bind(id.as_uuid()))
        .await?;
    let Some(row) = row else {
        return Err(Error::NotFound);
    };
    Ok(row_to_manifestation(row))
}

/// D-2b-2 manifestations for a record version, oldest first (D-2b-7 overlay).
///
/// `datum-print` is the named consumer (R-2s-3): it must not SELECT
/// `esign.signature`. Empty when the record has no signatures.
pub async fn manifestation_for_record(
    tx: &mut Tx<'_>,
    record: &RecordRef,
) -> Result<Vec<Manifestation>> {
    let rows: Vec<ManifestRow> = tx
        .fetch_all(
            sql_query_as(MANIFESTATION_BY_RECORD_SQL)
                .bind(&record.table)
                .bind(record.id.as_uuid())
                .bind(record.version),
        )
        .await?;
    Ok(rows.into_iter().map(row_to_manifestation).collect())
}

/// [`manifestation_for_record`] through a [`ReadPool`] (no actor bound).
pub async fn manifestation_for_record_on(
    pool: &ReadPool,
    record: &RecordRef,
) -> Result<Vec<Manifestation>> {
    let rows: Vec<ManifestRow> = pool
        .fetch_all(
            sql_query_as(MANIFESTATION_BY_RECORD_SQL)
                .bind(&record.table)
                .bind(record.id.as_uuid())
                .bind(record.version),
        )
        .await?;
    Ok(rows.into_iter().map(row_to_manifestation).collect())
}

/// One seal in an archival bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SealRef {
    /// Gap-free sequence.
    pub seq: i64,
    /// Transaction id as text.
    pub xid: String,
    /// Seal hash.
    pub hash: Vec<u8>,
    /// Previous hash.
    pub prev_hash: Option<Vec<u8>>,
    /// Sealed at.
    pub sealed_at: DateTime<Utc>,
}

/// Off-box anchor, if any.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnchorRef {
    /// Sink name.
    pub sink: String,
    /// Receipt.
    pub receipt: Option<String>,
}

/// Archival bundle (D-2b-8).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivalBundle {
    /// Manifestation.
    pub manifestation: Manifestation,
    /// Canonical snapshot that was hashed.
    pub record_snapshot: Value,
    /// Content hash.
    pub record_content_hash: [u8; 32],
    /// Audit event ids covering this signature (row-change + `esign_id` link).
    pub audit_event_ids: Vec<Uuid>,
    /// Seals covering those events (`prev_hash` chain from `audit.tx_seal`).
    pub seals: Vec<SealRef>,
    /// Anchor, if recorded.
    pub anchor: Option<AnchorRef>,
}

/// Result of [`verify_bundle`] (pure, no database).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleVerification {
    /// Snapshot hashes to `record_content_hash`.
    pub hash_ok: bool,
    /// Seal `prev_hash` chain is consistent. Empty seals are not a pass.
    pub chain_ok: bool,
    /// An off-box anchor is present.
    pub anchored: bool,
}

/// Load an archival bundle.
pub async fn archival_bundle(pool: &datum_db::ReadPool, id: SignatureId) -> Result<ArchivalBundle> {
    let manifestation = manifestation(pool, id).await?;
    let row: Option<(Value, Vec<u8>, DateTime<Utc>)> = pool
        .fetch_optional(
            sql_query_as(
                r#"SELECT record_snapshot, record_content_hash, signed_at
             FROM esign.signature WHERE signature_id = $1"#,
            )
            .bind(id.as_uuid()),
        )
        .await?;
    let Some((record_snapshot, hash_bytes, signed_at)) = row else {
        return Err(Error::NotFound);
    };
    let mut record_content_hash = [0u8; 32];
    if hash_bytes.len() == 32 {
        record_content_hash.copy_from_slice(&hash_bytes);
    }
    let (audit_event_ids, seals, anchor) = load_signature_trail(pool, id, signed_at).await?;
    Ok(ArchivalBundle {
        manifestation,
        record_snapshot,
        record_content_hash,
        audit_event_ids,
        seals,
        anchor,
    })
}

/// Verify a bundle with no database (D-2b-8).
///
/// `chain_ok` requires a non-empty seal list whose consecutive `prev_hash`
/// equals the previous seal's `hash`. Empty seals are not a pass (no ≤1 shortcut).
pub fn verify_bundle(bundle: &ArchivalBundle) -> BundleVerification {
    let hash_ok = content_hash(&bundle.record_snapshot)
        .ok()
        .is_some_and(|h| h == bundle.record_content_hash)
        && canonical_bytes(&bundle.record_snapshot).is_ok();
    let chain_ok = !bundle.seals.is_empty()
        && bundle
            .seals
            .windows(2)
            .all(|w| w[1].prev_hash.as_ref().is_some_and(|p| p == &w[0].hash));
    BundleVerification {
        hash_ok,
        chain_ok,
        anchored: bundle.anchor.is_some(),
    }
}

async fn load_signature_trail(
    pool: &datum_db::ReadPool,
    id: SignatureId,
    signed_at: DateTime<Utc>,
) -> Result<(Vec<Uuid>, Vec<SealRef>, Option<AnchorRef>)> {
    let dir = std::env::temp_dir().join(format!("datum-esign-bundle-{id}"));
    fs::create_dir_all(&dir).map_err(|e| Error::Invariant(e.to_string()))?;
    let selector = datum_audit::export::Selector {
        doc_type: None,
        doc_id: None,
        from: Some(signed_at - chrono::Duration::seconds(1)),
        to: Some(Utc::now() + chrono::Duration::seconds(2)),
    };
    let exported = datum_audit::bundle(pool.as_pool(), &selector, &dir)
        .await
        .map_err(|e| Error::Invariant(e.to_string()));
    let parsed = match exported {
        Ok(_) => parse_export_trail(&dir, id),
        Err(e) => Err(e),
    };
    let _ = fs::remove_dir_all(&dir);
    parsed
}

fn parse_export_trail(
    dir: &Path,
    id: SignatureId,
) -> Result<(Vec<Uuid>, Vec<SealRef>, Option<AnchorRef>)> {
    let want = id.as_uuid().to_string();
    let events_body = fs::read_to_string(dir.join("events.ndjson"))
        .map_err(|e| Error::Invariant(e.to_string()))?;
    let mut audit_event_ids = Vec::new();
    let mut xids = Vec::new();
    for line in events_body.lines().filter(|l| !l.is_empty()) {
        let v: Value = serde_json::from_str(line)?;
        if !event_belongs(&v, &want) {
            continue;
        }
        if let Some(eid) = v.get("event_id").and_then(Value::as_str)
            && let Ok(u) = Uuid::parse_str(eid)
        {
            audit_event_ids.push(u);
        }
        if let Some(xid) = v.get("xid").and_then(Value::as_str) {
            xids.push(xid.to_owned());
        }
    }
    xids.sort();
    xids.dedup();
    let seals_body = fs::read_to_string(dir.join("seals.ndjson"))
        .map_err(|e| Error::Invariant(e.to_string()))?;
    let mut seals = Vec::new();
    let mut anchor = None;
    for line in seals_body.lines().filter(|l| !l.is_empty()) {
        let v: Value = serde_json::from_str(line)?;
        if v.get("kind").and_then(Value::as_str) == Some("anchor") {
            if anchor.is_none() {
                anchor = Some(AnchorRef {
                    sink: v
                        .get("sink")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    receipt: v.get("receipt").and_then(Value::as_str).map(str::to_owned),
                });
            }
            continue;
        }
        let xid = v.get("xid").and_then(Value::as_str).unwrap_or_default();
        if !xids.iter().any(|x| x == xid) {
            continue;
        }
        let seq = v.get("seq").and_then(Value::as_i64).unwrap_or(0);
        let hash = v
            .get("hash")
            .and_then(Value::as_str)
            .and_then(unhex)
            .unwrap_or_default();
        let prev_hash = v.get("prev_hash").and_then(Value::as_str).and_then(unhex);
        let sealed_at = v
            .get("sealed_at")
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or(Utc::now());
        seals.push(SealRef {
            seq,
            xid: xid.to_owned(),
            hash,
            prev_hash,
            sealed_at,
        });
    }
    seals.sort_by_key(|s| s.seq);
    Ok((audit_event_ids, seals, anchor))
}

fn event_belongs(v: &Value, signature_id: &str) -> bool {
    if v.get("esign_id").and_then(Value::as_str) == Some(signature_id) {
        return true;
    }
    let table = v.get("table_name").and_then(Value::as_str).unwrap_or("");
    let key = v.get("row_key");
    match table {
        "signature" => {
            key.and_then(|k| k.get("signature_id"))
                .and_then(Value::as_str)
                == Some(signature_id)
        }
        "supersession" => {
            key.and_then(|k| k.get("old_signature_id"))
                .and_then(Value::as_str)
                == Some(signature_id)
                || key
                    .and_then(|k| k.get("new_signature_id"))
                    .and_then(Value::as_str)
                    == Some(signature_id)
        }
        _ => false,
    }
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = nibble(pair[0])?;
        let lo = nibble(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
