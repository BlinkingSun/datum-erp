//! Export bundle: electronic copies plus a documented PDF hook.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::Result;
use crate::sha256::{digest, hex};

/// Selects the rows an investigator asked for.
#[derive(Debug, Clone, Default)]
pub struct Selector {
    /// Document type (`work_order`, ...).
    pub doc_type: Option<String>,
    /// Document id.
    pub doc_id: Option<Uuid>,
    /// Inclusive lower bound on `at`.
    pub from: Option<DateTime<Utc>>,
    /// Exclusive upper bound on `at`.
    pub to: Option<DateTime<Utc>>,
}

/// One exported bundle's manifest (without the PDF, which Wave 2b renders).
#[derive(Debug, Clone, Serialize)]
pub struct Manifest {
    /// Stable identifier shared by every member of the bundle.
    pub export_id: Uuid,
    /// Frozen export format version.
    pub export_schema_version: u32,
    /// Wicket crate version that produced the bundle.
    pub wicket_version: String,
    /// PostgreSQL `server_version`.
    pub postgresql_version: String,
    /// Chain algorithm id (`wicket-audit-1`).
    pub chain_algo: String,
    /// Query predicate used to select rows.
    pub predicate: serde_json::Value,
    /// Number of audit events in the bundle.
    pub row_count: u64,
    /// Inclusive seal sequence range covering the events, if any.
    pub seal_range: Option<SealRange>,
    /// When the bundle was generated (UTC).
    pub generated_at: DateTime<Utc>,
    /// Actor recorded on the export, if the caller supplied one.
    pub exporting_actor: Option<String>,
    /// Per-file SHA-256; `report.pdf` is listed as absent.
    pub files: serde_json::Value,
}

/// Inclusive `[from, to]` sequence range.
#[derive(Debug, Clone, Serialize)]
pub struct SealRange {
    /// First seal sequence.
    pub from: i64,
    /// Last seal sequence.
    pub to: i64,
}

#[derive(sqlx::FromRow)]
struct EventRow {
    event_id: Uuid,
    at: DateTime<Utc>,
    stmt_at: DateTime<Utc>,
    xid: String,
    actor_id: Uuid,
    actor_kind: String,
    actor_display: String,
    acting_for_id: Option<Uuid>,
    session_id: Option<Uuid>,
    request_id: Option<Uuid>,
    source_kind: String,
    source_device_id: Option<String>,
    source_ip: Option<String>,
    client_app: Option<String>,
    action: String,
    reason: Option<String>,
    doc_type: Option<String>,
    doc_id: Option<Uuid>,
    esign_id: Option<Uuid>,
    schema_name: Option<String>,
    table_name: Option<String>,
    op: Option<String>,
    row_key: Option<serde_json::Value>,
    old_row: Option<serde_json::Value>,
    new_row: Option<serde_json::Value>,
    changed_columns: Option<Vec<String>>,
    app_version: String,
    config_version: String,
}

#[derive(sqlx::FromRow)]
struct SealRow {
    seq: i64,
    xid: String,
    sealed_at: DateTime<Utc>,
    tz: String,
    row_count: i32,
    rows_digest: Vec<u8>,
    prev_hash: Vec<u8>,
    hash: Vec<u8>,
    chain_algo: String,
}

#[derive(sqlx::FromRow)]
struct AnchorRow {
    seq: i64,
    hash: Vec<u8>,
    anchored_at: DateTime<Utc>,
    sink: String,
    receipt: Option<String>,
}

/// Write a self-describing bundle under `dir`.
///
/// Members: `events.ndjson`, `events.csv`, `seals.ndjson`, `manifest.json`,
/// `dictionary.md`. `report.pdf` is a documented hook for `wicket-print`
/// (Wave 2b) and is listed as absent in the manifest.
pub async fn bundle(pool: &PgPool, selector: &Selector, dir: impl AsRef<Path>) -> Result<Manifest> {
    let dir = dir.as_ref();
    fs::create_dir_all(dir)?;

    let events = fetch_events(pool, selector).await?;
    let xids: Vec<String> = {
        let mut v: Vec<String> = events.iter().map(|e| e.xid.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    let seals = fetch_seals(pool, &xids).await?;
    let seqs: Vec<i64> = seals.iter().map(|s| s.seq).collect();
    let anchors = fetch_anchors(pool, &seqs).await?;

    let ndjson = events_ndjson(&events);
    let csv = events_csv(&events);
    let seals_doc = seals_ndjson(&seals, &anchors);
    let dictionary = DICTIONARY;

    let events_path = dir.join("events.ndjson");
    let csv_path = dir.join("events.csv");
    let seals_path = dir.join("seals.ndjson");
    let dict_path = dir.join("dictionary.md");
    fs::write(&events_path, &ndjson)?;
    fs::write(&csv_path, &csv)?;
    fs::write(&seals_path, &seals_doc)?;
    fs::write(&dict_path, dictionary)?;

    let export_id = Uuid::now_v7();
    let generated_at = Utc::now();
    let pg_version: (String,) = sqlx::query_as("SELECT current_setting('server_version')")
        .fetch_one(pool)
        .await?;
    let seal_range = if let (Some(from), Some(to)) = (seqs.iter().min(), seqs.iter().max()) {
        Some(SealRange {
            from: *from,
            to: *to,
        })
    } else {
        None
    };
    let predicate = serde_json::json!({
        "doc_type": selector.doc_type,
        "doc_id": selector.doc_id,
        "from": selector.from,
        "to": selector.to,
    });
    let files = serde_json::json!({
        "events.ndjson": { "sha256": hex(&digest(ndjson.as_bytes())) },
        "events.csv": { "sha256": hex(&digest(csv.as_bytes())) },
        "seals.ndjson": { "sha256": hex(&digest(seals_doc.as_bytes())) },
        "dictionary.md": { "sha256": hex(&digest(dictionary.as_bytes())) },
        "report.pdf": {
            "absent": true,
            "hook": "wicket-print Wave 2b; the print primitive renders PDF/A from this bundle"
        }
    });
    let manifest = Manifest {
        export_id,
        export_schema_version: 1,
        wicket_version: env!("CARGO_PKG_VERSION").to_string(),
        postgresql_version: pg_version.0,
        chain_algo: "wicket-audit-1".to_string(),
        predicate,
        row_count: events.len() as u64,
        seal_range,
        generated_at,
        exporting_actor: None,
        files,
    };
    let manifest_path = dir.join("manifest.json");
    let body = serde_json::to_string_pretty(&manifest)?;
    fs::write(manifest_path, body)?;
    Ok(manifest)
}

async fn fetch_events(pool: &PgPool, selector: &Selector) -> Result<Vec<EventRow>> {
    Ok(sqlx::query_as::<_, EventRow>(
        r#"
        SELECT event_id, at, stmt_at, xid::text AS xid,
               actor_id, actor_kind, actor_display, acting_for_id,
               session_id, request_id, source_kind, source_device_id,
               source_ip::text AS source_ip, client_app, action, reason,
               doc_type, doc_id, esign_id,
               schema_name::text AS schema_name, table_name::text AS table_name,
               op, row_key, old_row, new_row, changed_columns,
               app_version, config_version
          FROM audit.event
         WHERE ($1::text IS NULL OR doc_type = $1)
           AND ($2::uuid IS NULL OR doc_id = $2)
           AND ($3::timestamptz IS NULL OR at >= $3)
           AND ($4::timestamptz IS NULL OR at < $4)
         ORDER BY at, stmt_at, event_id
        "#,
    )
    .bind(selector.doc_type.as_deref())
    .bind(selector.doc_id)
    .bind(selector.from)
    .bind(selector.to)
    .fetch_all(pool)
    .await?)
}

async fn fetch_seals(pool: &PgPool, xids: &[String]) -> Result<Vec<SealRow>> {
    if xids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_as::<_, SealRow>(
        r#"
        SELECT seq, xid::text AS xid, sealed_at, tz, row_count,
               rows_digest, prev_hash, hash, chain_algo
          FROM audit.tx_seal
         WHERE xid::text = ANY($1)
         ORDER BY seq
        "#,
    )
    .bind(xids)
    .fetch_all(pool)
    .await?)
}

async fn fetch_anchors(pool: &PgPool, seqs: &[i64]) -> Result<Vec<AnchorRow>> {
    if seqs.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_as::<_, AnchorRow>(
        r#"
        SELECT seq, hash, anchored_at, sink, receipt
          FROM audit.anchor
         WHERE seq = ANY($1)
         ORDER BY seq, anchored_at, sink
        "#,
    )
    .bind(seqs)
    .fetch_all(pool)
    .await?)
}

fn events_ndjson(events: &[EventRow]) -> String {
    let mut out = String::new();
    for e in events {
        let obj = event_json(e);
        out.push_str(&obj.to_string());
        out.push('\n');
    }
    out
}

fn event_json(e: &EventRow) -> serde_json::Value {
    serde_json::json!({
        "event_id": e.event_id,
        "at": e.at.to_rfc3339(),
        "stmt_at": e.stmt_at.to_rfc3339(),
        "xid": e.xid,
        "actor_id": e.actor_id,
        "actor_kind": e.actor_kind,
        "actor_display": e.actor_display,
        "acting_for_id": e.acting_for_id,
        "session_id": e.session_id,
        "request_id": e.request_id,
        "source_kind": e.source_kind,
        "source_device_id": e.source_device_id,
        "source_ip": e.source_ip,
        "client_app": e.client_app,
        "action": e.action,
        "reason": e.reason,
        "doc_type": e.doc_type,
        "doc_id": e.doc_id,
        "esign_id": e.esign_id,
        "schema_name": e.schema_name,
        "table_name": e.table_name,
        "op": e.op,
        "row_key": e.row_key,
        "old_row": e.old_row,
        "new_row": e.new_row,
        "changed_columns": e.changed_columns,
        "app_version": e.app_version,
        "config_version": e.config_version,
    })
}

const CSV_COLUMNS: &[&str] = &[
    "event_id",
    "at",
    "stmt_at",
    "xid",
    "actor_id",
    "actor_kind",
    "actor_display",
    "acting_for_id",
    "session_id",
    "request_id",
    "source_kind",
    "source_device_id",
    "source_ip",
    "client_app",
    "action",
    "reason",
    "doc_type",
    "doc_id",
    "esign_id",
    "schema_name",
    "table_name",
    "op",
    "row_key",
    "old_row",
    "new_row",
    "changed_columns",
    "app_version",
    "config_version",
];

fn events_csv(events: &[EventRow]) -> String {
    let mut out = CSV_COLUMNS.join(",");
    out.push('\n');
    for e in events {
        let row_key = e
            .row_key
            .as_ref()
            .map(std::string::ToString::to_string)
            .unwrap_or_default();
        let old_row = e
            .old_row
            .as_ref()
            .map(std::string::ToString::to_string)
            .unwrap_or_default();
        let new_row = e
            .new_row
            .as_ref()
            .map(std::string::ToString::to_string)
            .unwrap_or_default();
        let changed = e
            .changed_columns
            .as_ref()
            .map(|c| c.join(","))
            .unwrap_or_default();
        let fields = [
            e.event_id.to_string(),
            e.at.to_rfc3339(),
            e.stmt_at.to_rfc3339(),
            e.xid.clone(),
            e.actor_id.to_string(),
            e.actor_kind.clone(),
            e.actor_display.clone(),
            opt_uuid(e.acting_for_id),
            opt_uuid(e.session_id),
            opt_uuid(e.request_id),
            e.source_kind.clone(),
            e.source_device_id.clone().unwrap_or_default(),
            e.source_ip.clone().unwrap_or_default(),
            e.client_app.clone().unwrap_or_default(),
            e.action.clone(),
            e.reason.clone().unwrap_or_default(),
            e.doc_type.clone().unwrap_or_default(),
            opt_uuid(e.doc_id),
            opt_uuid(e.esign_id),
            e.schema_name.clone().unwrap_or_default(),
            e.table_name.clone().unwrap_or_default(),
            e.op.clone().unwrap_or_default(),
            row_key,
            old_row,
            new_row,
            changed,
            e.app_version.clone(),
            e.config_version.clone(),
        ];
        out.push_str(
            &fields
                .into_iter()
                .map(|f| csv_field(&f))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}

fn opt_uuid(v: Option<Uuid>) -> String {
    v.map(|u| u.to_string()).unwrap_or_default()
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        let mut out = String::from("\"");
        for c in s.chars() {
            if c == '"' {
                out.push('"');
            }
            out.push(c);
        }
        out.push('"');
        out
    } else {
        s.to_string()
    }
}

fn seals_ndjson(seals: &[SealRow], anchors: &[AnchorRow]) -> String {
    let mut out = String::new();
    for s in seals {
        let obj = serde_json::json!({
            "seq": s.seq,
            "xid": s.xid,
            "sealed_at": s.sealed_at.to_rfc3339(),
            "tz": s.tz,
            "row_count": s.row_count,
            "rows_digest": hex(&s.rows_digest),
            "prev_hash": hex(&s.prev_hash),
            "hash": hex(&s.hash),
            "chain_algo": s.chain_algo,
        });
        out.push_str(&obj.to_string());
        out.push('\n');
    }
    for a in anchors {
        let obj = serde_json::json!({
            "kind": "anchor",
            "seq": a.seq,
            "hash": hex(&a.hash),
            "anchored_at": a.anchored_at.to_rfc3339(),
            "sink": a.sink,
            "receipt": a.receipt,
        });
        out.push_str(&obj.to_string());
        out.push('\n');
    }
    out
}

/// Column meanings, enums, and the frozen canonical-row byte layout.
pub const DICTIONARY: &str = r#"# Wicket audit export dictionary (wicket-audit-1)

This bundle is readable with no Wicket installation. Events are one JSON object
per line in `events.ndjson` and the same rows flattened in `events.csv`.
`seals.ndjson` holds the per-transaction hash-chain links covering those events
and any off-box anchors for those sequence numbers. `report.pdf` is produced by
`wicket-print` (Wave 2b); this exporter leaves that member absent.

## `audit.event` columns

| column | meaning |
|---|---|
| event_id | uuid of the audit row (server-assigned) |
| at | `now()` / transaction_timestamp; identical for every row of one transaction |
| stmt_at | `clock_timestamp()`; intra-transaction order |
| xid | `pg_current_xact_id()`; joins the row to its seal |
| actor_id | uuid of the operator (no FK in this batch) |
| actor_kind | `user` / `service` / `migration` |
| actor_display | denormalised name as of the change |
| acting_for_id | principal being acted for, if any |
| session_id, request_id | request correlation |
| source_kind | `ui` `api` `job` `import` `migration` `maintenance` `app_event` |
| source_device_id, source_ip, client_app | device / network / client |
| action | declared business action (`work_order.complete`, ...) |
| reason | reason for change; required by `audit.reason_policy` for UPDATE/DELETE by default |
| doc_type, doc_id | regulated document this change belongs to |
| esign_id | signature that authorised the transaction, if any |
| schema_name, table_name, op | trigger-only; `op` in INSERT/UPDATE/DELETE/TRUNCATE |
| row_key | primary-key columns as jsonb |
| old_row, new_row | scrubbed row images; redacted values are the JSON string `[redacted]` |
| changed_columns | UPDATE only; lexicographically sorted keys that changed |
| app_version, config_version | producing build and configuration (invariant 17); may be empty string |

`event_shape`: an `app_event` row has `op` and `table_name` null; every other
row has both set. `wicket_app` holds SELECT only. Row-change columns are written
only by `audit.row_change` / `audit.stmt_truncate` (owner `wicket_audit_row`).
Kernel events go through `audit.log_event` (owner `wicket_audit_event`), which
cannot write `op` / `old_row` / `new_row` / `table_name`.

## Canonical row (`chain_algo = wicket-audit-1`)

All fields UTF-8. Unit separator is byte `0x1F`. SQL NULL is empty string.
`timestamptz` is `audit.ts_canon`: `to_char(t AT TIME ZONE 'UTC',
'YYYY-MM-DD"T"HH24:MI:SS.US') || '+00'`. jsonb uses
`audit.canon_jsonb`: keys sorted lexicographically, no insignificant whitespace,
numbers as their JSON text form, recursive.

```
canon(row) =
    'v1' || 0x1F || event_id || 0x1F || at || 0x1F || stmt_at || 0x1F || xid
    || 0x1F || actor_id || 0x1F || actor_kind || 0x1F || actor_display
    || 0x1F || acting_for_id || 0x1F || session_id || 0x1F || request_id
    || 0x1F || source_kind || 0x1F || source_device_id || 0x1F || source_ip
    || 0x1F || client_app || 0x1F || action || 0x1F || reason
    || 0x1F || doc_type || 0x1F || doc_id || 0x1F || esign_id
    || 0x1F || schema_name || 0x1F || table_name || 0x1F || op
    || 0x1F || canon_jsonb(row_key)
    || 0x1F || canon_jsonb(old_row)
    || 0x1F || canon_jsonb(new_row)
    || 0x1F || array_to_string(changed_columns, ',')
    || 0x1F || app_version
    || 0x1F || config_version
```

```
rows_digest = sha256( concat of (canon(row) || 0x1E) over the transaction's
                      audit rows, ordered by (stmt_at, table_name, op,
                      row_key::text, event_id) )
```

```
hash = sha256(
    prev_hash || 0x1F || seq::text || 0x1F || xid::text
    || 0x1F || ts_canon(sealed_at)
    || 0x1F || rows_digest || 0x1F || row_count::text
    || 0x1F || chain_algo
)
```

Genesis `prev_hash` is 32 zero bytes. `seq` is `prev.seq + 1` under
`pg_advisory_xact_lock(hashtextextended('audit.chain', 0))`. An aborted
transaction leaves the head unchanged.
"#;

/// Files written by [`bundle`].
pub fn members(dir: impl AsRef<Path>) -> [PathBuf; 5] {
    let dir = dir.as_ref();
    [
        dir.join("events.ndjson"),
        dir.join("events.csv"),
        dir.join("seals.ndjson"),
        dir.join("manifest.json"),
        dir.join("dictionary.md"),
    ]
}
