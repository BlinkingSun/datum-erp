# Lots module — validation protocol

Intended use: lot and serial identity for every shop that tracks material by
heat, batch, or serialized unit. Traceability is a business capability
(`regulated = false`, D-W1-5). The UDI attachment columns are nullable kernel
fields populated later by the Phase 6 UDI module; they are never custom fields.

## Requirements

| Id | Requirement | Invariant |
|---|---|---|
| L9 | Kernel lot/serial identifiers are `^[0-9A-Z-]{1,20}$` at generate and at CHECK | 9 |
| L10 | A serial is a unit within a lot; `lot_id` is NOT NULL with an FK | 10 |
| L11 | Package hierarchy (each/inner/case/pallet, contained qty, parent) records two cases of 24 as 48 pieces | 11 |
| L12 | Expiry stores precision; month `2026-09` round-trips as `{2026-09-01, month}` and the API invents no day | 12 |
| L16 | No DELETE on lot or serial (`wicket_app` → 42501) | 16 |
| LU | UDI-DI / UDI-PI columns are nullable and settable | 1.0.7 |

## Executable protocol

| Test | Proves |
|---|---|
| `lot_number_charset_and_length_enforced` | 9 — lowercase, space, 21 chars refused at generate and at CHECK |
| `supplier_lot_is_a_cross_reference_not_the_id` | 9 — supplier lot is not the kernel identifier |
| `serial_is_a_unit_within_a_lot` | 10 — no serial without a lot; FK; finished serial traces to its lot |
| `expiry_month_precision_survives_round_trip` | 12 — store `2026-09`, read `{2026-09-01, month}`; API renders precision |
| `two_cases_of_24_record_48_pieces_with_parent_links` | 11 |
| `status_change_is_a_history_row_and_audited` | status is a record + audit row (PLAN §3 item 4, this module's half; inventory posts) |
| `udi_attachment_columns_are_nullable_and_settable` | UDI attachment point |
| `every_lots_table_is_audited_and_owned_by_wicket_owner` | 3, 16 |
| `no_delete_path_on_lot_or_serial` | 16 — DELETE → 42501 |
| `writes_go_through_tx` | CONTRACT §5a — raw-pool write → 42501 |
| `reverse_migration_tested` | PLAN §6 item 8 |
| `lot_spawn_on_create_registers_sm_instance` | R-2s-5 — `Kernel::spawn` on lot create |
| `lot_edge_release_quarantine_to_available` | registered machine edge `release` |
| `lot_edge_hold_available_to_hold` | edge `hold` |
| `lot_edge_unhold_hold_to_available` | edge `unhold` |
| `lot_edge_reject_from_quarantine` | edge `reject_from_quarantine` |
| `lot_edge_reject_from_available` | edge `reject` |
| `lot_edge_reject_from_hold` | edge `reject_from_hold` |
| `lot_illegal_jump_quarantine_to_hold_is_refused` | illegal transition refused |
| `lot_release_plain_profile_not_required_under_no_signatures` | plain profile: release NotRequired |
| `lot_release_regulated_profile_refuses_under_no_signatures` | regulated Required + NoSignatures |
| `expiry_day_precision_survives_round_trip` | docs/10 `{value, precision}` day round-trip |
| `serial_list_paginates_at_page_boundary` | serial list `has_more` + cursor |
| `http_routes_declare_method_level_permissions` | GET `lots.view`, POST `lots.edit`, release `lots.release` |
