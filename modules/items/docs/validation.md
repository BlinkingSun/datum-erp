# Items module — validation protocol

Intended use: part master for make / buy / service / phantom items. Stocking
unit is a kernel `UnitId` (D1). Expiry precision is a lot property, never an
item one (PLAN §6 invariant 12).

## Requirements

- Item `number` is A–Z / a–z / digits / hyphen / `.`, at most 40 characters
  (PLAN §3 `MDS-450-M4x12` is binding; a–z is accepted).
- `number` + `revision` is the human identity; `id` is the key.
- Stock unit, scale, and residual tolerance are immutable while ledger postings
  exist (D2 §7 R5). Written to `ledger.stock_item` through `datum_ledger::registry`.
- Lifecycle is the state machine `draft → released → obsolete`, declared
  `NotRequired { reason: "item release is not a regulated signature point in v1" }`
  on every edge.
- Every write goes through `datum_db::Tx`. Every `items.*` table is audited and
  owned by `datum_owner`. `item_revision_history` carries `application_version`
  and `configuration_version` like `items.item` (inv. 17).

## Executable tests

| Test | Asserts |
|---|---|
| `create_item_writes_ledger_registry` | Create writes `ledger.stock_item` through the published registry. |
| `item_number_charset_enforced` | Space / underscore / length > 40 are refused with a typed error. |
| `stock_unit_immutable_after_first_posting` | After one ledger receipt, `datum_ledger::has_postings` is true and a stock-unit update is `StockMeasureImmutable`. |
| `release_transitions_and_audits` | `draft → released`; one audit row with action `items.release`. |
| `obsolete_item_cannot_be_released_again` | An obsolete item refuses `release`. |
| `optimistic_version_conflict_is_typed` | Stale `version` is `VersionConflict`. |
| `list_paginates_stably` | Cursor pages do not overlap; order is stable by `id`. |
| `every_items_table_is_audited_and_owned_by_datum_owner` | `zz_audit_row` and owner `datum_owner` on every items table. |
| `writes_go_through_tx` | Raw-pool write aborts SQLSTATE `42501`; table unchanged. |
| `reversible_migration_drops_items_schema` | `0001` down drops `items.item`; a second up recreates it. |
| `migrate_down_then_up` | Down to placeholder then up; `items.item_has_postings` stays dropped (R-2s-8). |
| `http_routes_declare_method_level_permissions` | POST/PATCH `items.edit`; release edge `items.release`; manifest and `api::ROUTES` agree. |
| `revision_history_carries_version_stamps` | A revision-history row carries `application_version` and `configuration_version` (inv. 17). |
| `duplicate_number_is_conflict` | A second create with the same `number` is `Error::DuplicateNumber` / docs/10 `CONFLICT`. |
