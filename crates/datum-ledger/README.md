# datum-ledger

Append-only inventory and cost ledger. Groups, postings, the consumption edge
(cost layers and genealogy), deferred constraint trigger `ZL000`–`ZL007`,
`GroupBuilder` as `datum_core::PostingSink`, rebuildable projections, reversal,
and the UoM rounding-residual home. Implements CONTRACT §6.2.

## Public API (`src/lib.rs`)

- `AllocationEdge` / `Layer` / `SQL_OPEN_LAYERS` / `allocate_withdrawal` / `load_open_layers`
- `sql_reads_consuming_value_rows` / `take_query_log` — allocator SQL probes
- `GroupBuilder` — `PostingSink` implementation (header, contribute, finalize)
- `UOM_CONVERSION_RESIDUAL` — reason code for conversion dust
- `CostMethod` / `Measure` — costing method and QUANTITY/VALUE measure
- `group_kind_sql` / `group_kind_from_sql` / `group_kind_variants`
- `boundary_sql` / `boundary_from_sql` / `boundary_variants` / `boundary_permitted`
- `cost_element_sql` / `cost_element_from_sql` / `cost_element_variants`
- `value_account_sql` / `value_account_from_sql` / `value_account_variants`
- `measure_sql` / `measure_from_sql` / `measure_variants`
- `cost_method_sql` / `cost_method_from_sql`
- `Error` / `Result` / `map_sqlstate` — ZL codes and posting errors
- `Node` / `TraceStart` / `trace_backward` / `trace_forward` — genealogy
- `bind_tx` / `post` / `commit` — bind sink to xid; insert header/postings/consumption; commit with poison check
- `BalanceSlice` / `apply_group` / `balance_at` / `rebuild` / `verify_projection`
- `StockItem` / `load_stock_item` / `upsert_location` / `upsert_stock_item`
- `post_uom_conversion_residual` / `post_uom_residual_flush`
- `reverse` — exact reversing group
- `GroupId` — `ledger.posting_group.group_id`
- `MIGRATOR` — `placeholder` + `0001_ledger`
- `#[cfg(feature = "test-utils")]` `test_inject_quantity_without_contributed_mark` /
  `test_poison_is_marked`

## Migrations

- `00000000000000_placeholder` — no-op
- `00000000000001_ledger` — schema `ledger` (app): `ledger.stock_item`, `ledger.location`,
  `ledger.posting_group`, `ledger.posting`, `ledger.consumption`;
  `transient.balance_projection`, `transient.layer_projection` (transient)

## Tests (`tests/`)

Cases: `case_a_receive_into_quarantine` … `case_l_reverse_c` (D2 §8).
Criteria: `every_legitimate_sequence_commits`, `transposed_digit_is_rejected_and_names_predicate`,
`dropped_counterpart_is_rejected`, `identity_crossing_outside_transformation_is_rejected`,
`allocation_not_reproducing_quantity_is_rejected`, `allocation_not_reproducing_money_is_rejected`,
`projections_equal_fold_after_every_sequence_and_after_rebuild`,
`balance_reconstructible_at_any_instant`, `reversal_restores_state_without_deleting`.
Props: `p0_atomicity` … `p4_reversal`, `boundary_matrix`, `scale_exactness`, `dust_bounded`,
`generator_names_are_stable`, `generator_corruption_index_is_stable`.
Trigger/canary: `zl000_group_has_no_header` … `zl007_layers_not_restored`,
`canary_ledger_constraints_are_armed`, `trace_backward_and_forward_follow_consumption`,
`writes_go_through_tx`, `enum_bijection_round_trip`, `reverse_migration_tested`,
`group_actor_matches_audit`, `empty_group_and_unfinalized`, `lineage_required_and_after_finalize`.
Addendum: `explicit_consumption_skips_auto_allocation`, `unfinalized_*_poisons`,
`group_builder_drop_does_not_poison_without_contributed_mark`, `poisoned_transaction_cannot_commit`,
`standard_costing_posts_ppv`, `balance_at_is_lot_and_serial_aware`, `reverse_kind_from_target_not_sink`,
`no_reversal_of_reversal`, `ineligible_layer_rejected`, `explicit_consumption_money_half_validated`,
`verify_projection_catches_each_column_poison`, `allocation_is_per_location`,
`apply_group_incremental_matches_rebuild`.

## Frozen / seams

Frozen: `GroupBuilder` as `PostingSink`, `post` / `bind_tx` / `commit`, enum
SQL labels (CONTRACT §6.2). The posting stub in `datum-uom` is replaced: this
crate owns `ledger.posting`. SM talks to this crate only through the trait.
`Kernel::build`'s factory currently does not call `post` after sink finalize
(FINDINGS-0 #2).
