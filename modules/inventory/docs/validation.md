# Inventory module — validation protocol

Intended use: receipts, issues, moves, adjustments, and cycle counts over the
kernel ledger. On-hand, allocated, and available are rebuildable projections
(`wicket_ledger::balance_at`). Lots and serials are kernel lot entities, never
text columns (PLAN §3). Quarantine is not available until released (PLAN §3
item 4). Conversion happens once at the boundary (D2 R2); quantity residual
lives in the balance and is flushed as `ADJUSTMENT` / `UOM_CONVERSION_RESIDUAL`
(D2 R4). Over-receipt beyond source-document tolerance is a document error,
never a ledger invariant.

## Executable protocol

| Test | D2 / PLAN |
|---|---|
| `case_a_receive_into_quarantine_posts_movement_from_supplier` | D2 §8 a; PLAN §3 item 4 (quarantine not available) |
| `case_b_release_quarantine_posts_and_changes_status` | D2 §8 b; PLAN §3 items 4, 13 |
| `case_c_issue_one_bar_to_wip_with_explicit_lot_pick_contributes_consumption` | D2 §8 c; D-W1-3 (c); PLAN §3 item 1 (lot entity) |
| `case_e_scrap_is_adjustment_with_reason` | D2 §8 e |
| `case_f_cycle_count_variance_is_adjustment` | D2 §8 f |
| `case_h_customer_return_into_quarantine` | D2 §8 h |
| `case_k_issue_by_the_inch_posts_uom_rounding_residual` | D2 §8 k; D2 R2/R4 |
| `over_receipt_beyond_tolerance_is_a_document_error_not_a_ledger_error` | D2 §6 / PLAN §6 (tolerance vs ledger) |
| `no_balance_column_exists_in_module_schema` | PLAN §6 invariant 1 |
| `on_hand_equals_ledger_fold_after_every_document` | PLAN §3 item 9 |
| `posting_without_actor_aborts_and_leaves_no_document` | PLAN §3 item 6; invariant 5 |
| `every_inventory_table_is_audited_and_owned_by_wicket_owner` | PLAN §3 item 13; invariants 3, 16 |
| `writes_go_through_tx` | CONTRACT §5a |
| `reversible_migration_drops_inventory_schema` | PLAN §6 invariant 8 |
