# datum-ledger

Append-only inventory and cost ledger. Groups, postings, the consumption edge
(cost layers and genealogy), the deferred constraint trigger (`ZL000`–`ZL007`),
`GroupBuilder` as `datum_core::PostingSink`, rebuildable projections, reversal,
and the `UOM_CONVERSION_RESIDUAL` rounding home.

Test names are stable from the first release (PLAN §7).

## Named tests

### Cases (D2 §8)

- `case_a_receive_into_quarantine`
- `case_b_release_quarantine`
- `case_c_issue_to_wo`
- `case_d_complete_screws`
- `case_e_scrap_at_op`
- `case_f_cycle_count_short`
- `case_g_ship_to_customer`
- `case_h_customer_return`
- `case_i_rework_recovery`
- `case_j_outside_processing`
- `case_k_inch_issue_and_dust`
- `case_l_reverse_c`

### PLAN §7 criteria

- `every_legitimate_sequence_commits`
- `transposed_digit_is_rejected_and_names_predicate`
- `dropped_counterpart_is_rejected`
- `identity_crossing_outside_transformation_is_rejected`
- `allocation_not_reproducing_quantity_is_rejected`
- `allocation_not_reproducing_money_is_rejected`
- `projections_equal_fold_after_every_sequence_and_after_rebuild`
- `balance_reconstructible_at_any_instant`
- `reversal_restores_state_without_deleting`

### Property shards (D2 §10)

- `p0_atomicity`
- `p1_quantity`
- `p2_value`
- `p2b_cost_element`
- `p3_coupling`
- `p3_independent_sources`
- `p4_reversal`
- `boundary_matrix`
- `scale_exactness`
- `dust_bounded`
- `generator_names_are_stable`
- `generator_corruption_index_is_stable`

### Trigger codes and canary

- `zl000_group_has_no_header`
- `zl001_group_extended`
- `zl002_quantity_not_conserved`
- `zl003_value_not_conserved`
- `zl004_cost_element_reclassified`
- `zl005_allocation_incomplete`
- `zl006_reversal_not_exact`
- `zl007_layers_not_restored`
- `canary_ledger_constraints_are_armed`
- `trace_backward_and_forward_follow_consumption`
- `writes_go_through_tx`
- `enum_bijection_round_trip`
- `reverse_migration_tested`
- `group_actor_matches_audit`
- `empty_group_and_unfinalized`

Lib: `unimplemented_formats`, `migrator_has_placeholder`, `zl_mapping_is_exhaustive`,
`allocator_sql_does_not_read_consuming_value_rows`,
`rust_sql_labels_cover_every_named_variant`.
