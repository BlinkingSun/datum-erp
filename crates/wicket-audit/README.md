# wicket-audit

The audit trail written by the database: `audit.event`, row-change and truncate
triggers attached at `CREATE TABLE`, fail-closed context, per-transaction hash
chain, off-box anchors, verify, redaction, and export. Rust is not trusted to
produce an audit row; `Tx::begin` supplies actor and intent as GUCs.

## Public API (`src/lib.rs`)

- `Error` / `Result` — `Unimplemented` (PDF hook until `wicket-print`), `Core`/`Db`/`Io`/`Json`
- `MIGRATOR` — `placeholder` + `0001_audit`
- `AuditCtx` — actor / reason / source for `record`
- `AuditEntry` — typed view of one `audit.event` row
- `record` — kernel `app_event` via `audit.log_event`
- `attach` — attach `zz_audit_row` / `zz_audit_truncate` to `schema.table`
- `verify` — recompute seals in `[from_seq, to_seq]`; first divergent `seq` or `None`
- `Head` — chain head (`seq`, hash, xid, sealed_at, row_count, chain_algo)
- `head` — `audit.head()`
- `anchor::record` — persist an off-box anchor of `(seq, hash)` at `sink`
- `bundle` — export bundle (`events.ndjson`/`csv`, `seals.ndjson`, `manifest.json`, `dictionary.md`)
- `install_privileged` / `uninstall_privileged` — event triggers and writer-role owners (superuser)
- `export` / `install` / `sha256` — modules (`sha256::digest`, `sha256::hex`)

## Migrations

- `00000000000000_placeholder` — no-op
- `00000000000001_audit` — schema `audit` (class audit): `audit.event`, `audit.redact`,
  `audit.reason_policy`, `audit.exempt`, `audit.tx_seal`, `audit.anchor`

## Tests (`tests/`)

IQ (`tests/iq.rs`; PLAN §7 names): `author_forgets_audit_still_audited`,
`app_cannot_insert_audit_event`, `app_cannot_update_audit_event`,
`app_cannot_delete_audit_event`, `log_event_cannot_forge_row_change`,
`tamper_breaks_verify_at_seq`, `reseal_diverges_from_anchor`,
`aborted_tx_leaves_no_seal_and_same_head`, `export_bundle_is_self_describing`,
`require_context_refuses_missing_actor`, `require_context_refuses_foreign_txid`,
`require_context_refuses_missing_action`, `scrub_redacts_listed_columns`,
`transient_tables_carry_no_trigger`, `truncate_is_recorded_when_owner_truncates`,
`at_is_same_for_all_rows_of_one_transaction`, `stmt_at_orders_within_transaction`,
`seal_trigger_fires_at_commit_not_insert`, `migrate_login_cannot_drop_or_disable_audit_trigger`,
`module_trigger_can_be_disabled_by_migrate_login`,
`superuser_can_drop_audit_trigger_for_migrate_down`,
`verify_is_independent_of_session_timezone`.

Also: `migrate_down_then_up`, `grants_match_d3_section_1_3`.

## Frozen / seams

Frozen: `attach`, `install_privileged`, `require_context` (inv. 3, 4, 5, 17).
`report.pdf` in the export bundle is `wicket-print` (Wave 2b); the manifest lists
it as absent. `Error::Unimplemented` is that PDF hook. Event-trigger create is
superuser-only; migrations run as `wicket_migrate`, then `install_privileged`.
