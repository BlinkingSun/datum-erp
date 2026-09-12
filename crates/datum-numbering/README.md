# datum-numbering

Gap-free document numbering and kernel lot/serial identifiers. Numbers are
allocated from `numbering.counter` by `UPDATE … RETURNING` inside the caller's
`datum_db::Tx` (D3 §8). There is no `void_number`: cancellation is the document's
status. A committed number is never reused. Period keys come from `ResetPolicy`
applied to server `now()`, never the client clock (inv. 4).

## Public API (`src/lib.rs`)

- `Error` / `Result` — sequence, template, identifier, bind failures; `Unimplemented`
- `MIGRATOR` — `placeholder` + `0001_numbering`
- `ResetPolicy` — `Never` / `Yearly` / `Monthly` (`period_key` derivation)
- `SequenceId` — `(doc_type, reset)`; `doc_type` stored, `period_key` derived at allocate
- `define` — create the counter row; never rewinds `next_value`
- `next_number` — allocate the next formatted number in `tx`
- `server_now` — transaction-local server `now()` (UTC text)
- `lot` — `validate` / `validate_template` / `generate` (`^[0-9A-Z-]{1,20}$`, inv. 9)
- `serial` — same charset via `lot`; `validate` / `validate_template` / `generate`

## Migrations

- `00000000000000_placeholder` — no-op
- `00000000000001_numbering` — schema `numbering` (app): `numbering.counter` (app;
  registered in `audit.exempt` — allocation is evidenced by the audited document)

## Tests (`tests/`)

- `abort_then_reallocate_returns_same_number`
- `concurrent_allocation_is_contiguous` (inv. 19)
- `period_rollover_uses_server_time`
- `format_templates_render_exactly`
- `lot_id_charset_and_length_enforced` (inv. 9)
- `counter_is_audit_exempt`
- `migrate_down_then_up` / `no_postgres_sequence_or_identity` / `runner_reassigns_owner`

## Frozen / seams

Frozen: `define` / `next_number` / `server_now` / `lot::validate` (CONTRACT §4;
inv. 9, 19). No PostgreSQL `SEQUENCE` or `IDENTITY` on the counter.
`numbering.counter` is the documented audit exemption (D3 §8).
