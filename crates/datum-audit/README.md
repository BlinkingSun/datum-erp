# datum-audit

The audit trail written by the database: `audit.event`, row-change and
truncate triggers attached at `CREATE TABLE`, fail-closed context,
per-transaction hash chain, off-box anchors, verify, redaction, and export.

Rust is not trusted to produce an audit row. `Tx::begin` supplies actor and
intent as transaction-local settings; the trigger writes the row.

## IQ tests (stable names)

The integration binary is `tests/iq.rs`. The documented home of the stable
names is `tests/iq/` (`tests/iq/README.md`). These names never change after
the first release (PLAN §7):

- `author_forgets_audit_still_audited`
- `app_cannot_insert_audit_event`
- `app_cannot_update_audit_event`
- `app_cannot_delete_audit_event`
- `log_event_cannot_forge_row_change`
- `tamper_breaks_verify_at_seq`
- `reseal_diverges_from_anchor`
- `aborted_tx_leaves_no_seal_and_same_head`
- `export_bundle_is_self_describing`
- `require_context_refuses_missing_actor`
- `require_context_refuses_foreign_txid`
- `require_context_refuses_missing_action`
- `scrub_redacts_listed_columns`
- `transient_tables_carry_no_trigger`
- `truncate_is_recorded_when_owner_truncates`
- `at_is_same_for_all_rows_of_one_transaction`
- `stmt_at_orders_within_transaction`
- `seal_trigger_fires_at_commit_not_insert`
- `migrate_login_cannot_drop_or_disable_audit_trigger`
- `module_trigger_can_be_disabled_by_migrate_login`
- `superuser_can_drop_audit_trigger_for_migrate_down`
- `verify_is_independent_of_session_timezone`

Also: `migrate_down_then_up`, `grants_match_d3_section_1_3`.

## Install

Migrations run as `datum_migrate`. PostgreSQL requires a superuser to
`CREATE EVENT TRIGGER` and to `ALTER FUNCTION ... OWNER TO` the writer
roles (`datum_audit_row`, `datum_audit_event`). After the SQL migrator,
call `datum_audit::install_privileged` on a bootstrap/superuser
connection to the same database.

`audit_protect` (and `audit_protect_drop`) refuse `DROP TRIGGER` of
`zz_audit_*`, `ALTER TABLE ... DISABLE TRIGGER` / `ENABLE [REPLICA|ALWAYS]
TRIGGER` of `zz_audit_*` (and `DISABLE TRIGGER ALL` / `USER` on an audited
table), and `DROP FUNCTION` of the audit functions unless `session_user` is a
superuser (`pg_roles.rolsuper`). A module's own trigger on an audited table
remains manageable by `datum_migrate`. Membership in `datum_owner` grants
nothing here: `datum_owner` cannot log in, and the migrate login is the
role the defence exists against. Reverse migrations that must drop
audit triggers on audited tables run under the bootstrap superuser
(`TestDb` already migrates down through it; call
`datum_audit::uninstall_privileged` first).

## Export

`export::bundle` writes `events.ndjson`, `events.csv`, `seals.ndjson`,
`manifest.json`, and `dictionary.md`. `report.pdf` is `datum-print`
(Wave 2b); the manifest lists it as absent.
