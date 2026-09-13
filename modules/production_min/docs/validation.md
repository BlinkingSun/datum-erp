# Production-min module — validation protocol

Intended use: the minimal work-order surface the Wave 2s slice needs — create,
release, issue material, complete, and receive the finished lot. Not the Phase 3
`production` module. Completion is a priced `TRANSFORMATION` (D2 §8 case d)
whose consumption edges reproduce issued quantity and money; the remainder is
named `MFG_VARIANCE`. Scrap at an operation is a separate `ADJUSTMENT` (case e).
Lots and serials are kernel lot entities, never text columns (PLAN §3).

The machine is `draft → released → in_process → completed` (`cancelled` from
draft/released), `regulated = false`, every edge `NotRequired` with reason
"v1 minimal work order; signature points belong to the Phase 3 production module".

## Executable protocol

| Test | Asserts |
|---|---|
| `release_allocates_gap_free_number_late` | Number is null at create; `WO-…` allocated at release, consecutive. |
| `release_creates_wip_location_once` | `locations::ensure_wip` is idempotent per work order. |
| `complete_posts_priced_transformation_case_d` | TRANSFORMATION; `work_order_id`; `MFG_VARIANCE`; audit `production.complete` / `doc_id`. |
| `complete_contributes_produced_lineage_edges` | Forward trace from the bar lot reaches the finished lot (PLAN §3 item 8). |
| `complete_without_issued_material_is_lineage_required_error` | `LineageRequired` (CONTRACT §6.2 rule 6). |
| `complete_creates_finished_lot_with_kernel_identifier` | Finished lot `LOT-WO-1847` is a kernel identifier (inv. 9). |
| `transition_edges_all_declare_not_required_with_reason` | Every edge `NotRequired` with the SPEC reason. |
| `abort_mid_completion_leaves_no_group_no_lot_no_transition` | Rollback leaves no group, no finished lot, status not completed. |
| `every_production_table_is_audited_and_owned_by_wicket_owner` | `zz_audit_row` and owner `wicket_owner` (PLAN §3 item 13). |
| `writes_go_through_tx` | Raw-pool write aborts SQLSTATE `42501`; table unchanged. |
| `reversible_migration_drops_production_min_schema` | `0001` down drops `production_min.work_order`. |
