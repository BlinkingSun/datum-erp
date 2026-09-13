# datum-esign

Electronic signatures (Wave 2b). Mints a signature row bound to the exact
content of the record version it certifies, and lets a state transition consume
it atomically through a per-transaction prepared `SignatureGate`.

Composition-root wiring (`GateBinding` factory, `prepare` before
`Engine::transition`, `datum.esign_id` on audit rows, profile TOML flip) is the
follow-up lane `2b1-glue`. This crate publishes what that lane needs.

## Schema

Class `app`, schema `esign` (R-2s-1; kernel crate owns its schema):

| Table | Notes |
|---|---|
| `esign.signature` | Insert-only. `datum_app` holds `SELECT`, `INSERT`, and `UPDATE (consumed_at, consumed_xid)` only (D-2b-1). A `BEFORE UPDATE` trigger refuses any other column; `consumed_at` is monotone. No `DELETE`. |
| `esign.meaning_policy` | Reason-text policy (`requires_reason`, `permission_hint`). |
| `esign.supersession` | Insert-only link `old → new`. `supersede` INSERTs here (SECURITY INVOKER); it does not `UPDATE esign.signature.superseded_by`. |

Working state is `transient.signing_session` (D-2b-3; `DELETE` allowed, not
audited). This crate is not on the R-2s-3 exemption list, so production Rust
never names `transient.*`: session DML goes through invoker `esign.*`
functions.

Tables are owned by `datum_owner` (NOLOGIN). App-class tables are audited by
`zz_audit_row` from `CREATE TABLE`. Hash columns are redacted through
`audit.redact` (registered by the production migrator via a one-shot
`esign._register_hash_redact()` function dropped in `0002`, so
`lint-sql-migrations` treats the cross-schema insert as neutralized).

## D-2b-5 check table

| # | Check | Error | Audited? |
|---|---|---|---|
| 0 | provider bound | `NoProvider` | silent |
| 1 | row `FOR UPDATE`: exists; `expires_at > now()`; signer `Active`; `token.signer` == `signer_id` | `Invalid("no such signature" \| "expired" \| "signer inactive" \| "signer mismatch")` | yes |
| 2 | meaning (row vs required vs token) | `MeaningMismatch` | yes |
| 3 | `(record_table, record_id, record_version)` vs live `RecordRef` | `RecordMismatch` | yes |
| 4 | token hash vs row vs live projection | `HashMismatch` | yes |
| 5 | `required.permission` ∈ `permission_snapshot` | `SignerNotPermitted` | yes |
| 6 | conditional `UPDATE` row count | `Consumed` | yes |

A missing token on a `Required` edge is `Invalid("missing token")` (executor).
Audited failures are written after rollback via `esign::log_refusal` →
`audit.log_event` as `security.esign.refusal`.

`prepare` claims inside the caller's transaction. A refusal aborts that
transaction and the rollback un-claims the row.

## Manifestation wire shape (D-2b-2)

```json
{ "signature": {
  "id": "01932c5a-…-e2", "signer_id": "01932c5a-…-0b", "printed_name": "M. Reyes",
  "meaning": "Released", "reason": null,
  "signed_at": "2026-03-14T15:02:11Z", "signed_at_zone": "America/New_York",
  "signed_at_local": "2026-03-14T11:02:11-04:00",
  "record": { "table": "sm.instance", "doc_type": "production.work_order",
              "id": "01932c5a-…-06", "version": 3 },
  "record_content_hash": "e3b0c442…b855",
  "credential_kind": "signing_password", "components_used": ["code","secret"],
  "superseded": false } }
```

`signed_at_local` is derived, not stored. Snapshots only — never a live join.

## Route shapes (transport is `datum-server`)

| Method | Path | Body / result |
|---|---|---|
| `POST` | `/api/v1/esign/challenges` | `{ components_required: ["code","secret"] \| ["secret"], signing_session_expires_at, credential_kind }` |
| `POST` | `/api/v1/esign/signatures` | Idempotency-Key; identification `{ code, secret }`; returns the D-2b-2 body. 401 `SIGNATURE_REQUIRED` (`field = "identification.secret"`) when the principal has no signing credential. Login secret is `VALIDATION`. |
| `GET` | `/api/v1/esign/signatures/{id}` | Manifestation |
| `GET` | `/api/v1/esign/signatures/{id}/bundle` | Archival bundle; permission `esign.bundle.read` |

Error codes (docs/10): `SIGNATURE_REQUIRED`, `SIGNATURE_NO_PROVIDER`,
`VALIDATION`, `CONFLICT`. `Consumed` / `HashMismatch` are 409 `CONFLICT`.

## Relaxation keys (SPEC-profiles key 4 sub-keys)

```toml
[signature_gate_binding]
gate = "NoSignatures"          # or "datum-esign"
continuous_session = "off"     # shipped off in both profiles
idle_timeout_secs = 300
max_window_secs = 900
```

v1 requires `["code","secret"]` on every signing. Continuation with `["secret"]`
is accepted only when `continuous_session = "on"` and a live
`transient.signing_session` is inside both windows.

## API

- `mint(tx, MintRequest) -> Signature`
- `prepare(tx, token, doc) -> PreparedGate` (`LiveDoc.signer_status` ignored; signer `Active` is `load_principal_on` on the claim `Tx`)
- `PreparedGate: SignatureGate`, `GateFactory`, `BoundGate`
- `manifestation(&ReadPool, id)`, `archival_bundle(&ReadPool, id)`, `verify_bundle` (pure; `chain_ok` requires a non-empty seal chain)
- `manifestation_for_record(tx, record)` / `manifestation_for_record_on(pool, record)` — D-2b-2 list by record version including supersession. **Consumer: `datum-print`** (R-2s-3)
- `supersede(tx, old, new)` (INSERT into `esign.supersession`), `close_session(tx, reason)` (actor from the bound `WriteContext`), `log_refusal`
- `register_projection(doc_type, fn)`, default `identity_projection` for first-party machines; extra bound machines fail `Kernel::build` without a registration

Reads go through `datum_db::ReadPool` (`fetch_one` / `fetch_optional` /
`fetch_all`) and, for the print seam, the sealed `Tx`. `archival_bundle` still
calls `datum_audit::bundle`, which takes `&Pool`; that one call uses
`ReadPool::as_pool`.

### Render read seam (`datum-print` is the consumer)

Kernel crates must not SELECT `esign.*` (R-2s-3). `datum-print` is the named
consumer of the record-keyed manifestation list. Direct SELECT of this crate's
tables (invoker-rights; no `SECURITY DEFINER`).

| Function | Signature | Source of truth |
|---|---|---|
| `manifestation` | `async fn manifestation(pool: &ReadPool, id: SignatureId) -> Result<Manifestation>` | `esign.signature` + `esign.supersession` overlay |
| `manifestation_for_record` | `async fn manifestation_for_record(tx: &mut Tx<'_>, record: &RecordRef) -> Result<Vec<Manifestation>>` | same, filtered by `(record_table, record_id, record_version)`, oldest first |
| `manifestation_for_record_on` | `async fn manifestation_for_record_on(pool: &ReadPool, record: &RecordRef) -> Result<Vec<Manifestation>>` | same, through `ReadPool` (no actor) |

`record_content_hash` is SHA-256 over the canonical JSON of
`{ projection, instance: { doc_type, doc_id, state, version } }`. The caller
reads the `sm.instance` triple (this crate does not `SELECT sm.*`).
