# IQ test names

This directory is the documented home of the customer-facing
installation-qualification names (SPEC `tests/iq/`). The integration binary
stays `tests/iq.rs`; do not split one-test-per-file. Names are the
`#[tokio::test]` functions there and never change after the first release
(PLAN §7):

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

Also: `migrate_down_then_up`, `grants_match_d3_section_1_3` in `tests/migrate.rs`.
