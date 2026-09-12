# Genealogy module — validation protocol

Intended use: the quality user traces a lot or serial forward and backward over
the inventory ledger's consumption edges (D2 §5.3). The module is read-only
(`regulated = false`, D-W1-5). It does not keep a parallel graph; the cache in
`genealogy_transient` is rebuildable and never authoritative. Lots and serials are
kernel lot entities, never text columns (PLAN §3).

## Requirements

| Id | Requirement | Source |
|---|---|---|
| G1 | Reads only through `datum_ledger::trace_backward` / `trace_forward` | SPEC; R-2s-3 |
| G2 | Forward from the heat and backward from the finished lot are the same tree | PLAN §3 item 8 |
| G3 | `impact(lot)` is the forward closure to CUSTOMER (recall list) | D2 case g; mockup |
| G4 | Traversals over more than N postings run as `genealogy.trace` with progress | DESIGN §2 |
| G5 | Cache drop does not change results | SPEC named test |
| G6 | Reversal edges are signed negative and do not double-count | D2 §5.3 |

## Executable protocol

| Test | Proves |
|---|---|
| `forward_and_backward_traces_return_the_same_tree` | PLAN §3 item 8 |
| `trace_filters_by_lot_and_serial` | lot/serial are entity filters |
| `impact_lists_customer_shipments_and_units` | case g recall list |
| `reversal_edges_are_negative_and_do_not_double_count` | signed edges |
| `large_trace_runs_as_job_with_progress` | DESIGN §2; 202 + job |
| `cache_is_rebuildable_and_never_authoritative` | drop cache; identical |
| `module_reads_only_through_datum_ledger_api` | no SQL against ledger.* |
| `writes_go_through_tx` | CONTRACT §5a |
| `every_genealogy_table_is_audited_and_owned_by_datum_owner` | every table audited |
| `reversible_migration_drops_genealogy_schema` | PLAN §6 invariant 8 |
| `where_used_walks_item_revision` | `where_used(item, revision)` |
