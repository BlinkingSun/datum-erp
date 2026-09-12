# Wave 2s (module slices) — rulings record

Companion to `w2-rulings.md`. Recorded by the orchestrator as slices land; promoted into `docs/` at the Wave 2s close.

| # | Ruling | Source | Date |
|---|---|---|---|
| R-2s-1 | A module owns one PostgreSQL schema named after it (class `app`); working state in `<module>_transient` (class `transient`). The cross-module lint makes this a law. | 2s.1 audits (all three modules did this), lots adjudication residual 3 | 2026-09-12 |
| R-2s-2 | One name triple per module: crate `datum-mod-<x>`, manifest id `mod-<x>` (profiles + compiled-in), schema `<x>`. 2s1-glue aligns the 2s.1 modules. | lots adjudication residual 2, items audit residual 3, locations audit residual 3 | 2026-09-12 |
| R-2s-3 | Ledger publishes `registry::has_postings(item)` and `has_quantity_at(location)`; modules never read `ledger.*`/`transient.*` directly (SECURITY DEFINER shims are not a workaround). | items audit residual 1, locations audit residual 2 → lane w2-carry-2 | 2026-09-12 |
| R-2s-4 | Lot-less receipts publish `inventory.receipt_posted` v1; `inventory.lot_received` stays lot-only. | kernel-e2e blocker 4 | 2026-09-12 |
| R-2s-5 | Module status changes drive the registered state machine through the kernel (`Kernel::spawn`/`transition`); a raw status UPDATE is a fail-class finding at the next audit. | lots adjudication residual 1 → lane mod-lots-c1 | 2026-09-12 |
| R-2s-6 | A posting group has one kind; a UoM/cost residual from a movement or count is its own `ADJUSTMENT` group in the same transaction with `parent_group_id` = the movement group; both conserve. Supersedes the SPEC-mod-inventory "same group" wording. | inventory adjudication residual 5 | 2026-09-12 |
