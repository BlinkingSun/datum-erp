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
| R-2s-7 | Composite mutations are ONE kernel transition plus hooks: a `Tx` binds one business action (`WriteContext`) and never rebinds the write GUC; a server handler that needs a movement plus a status change calls one transition and lets module hooks post within it. | server-slice-c2-r1 item 3 BLOCKED → lane 2s4-onetx | 2026-09-12 |
| R-2s-8 | `SECURITY DEFINER` functions are allowed only in the migrations of `datum-db`, `datum-audit`, `datum-ledger` and `datum-numbering`; `just lint-sql` scans migrations and fails any other occurrence, any `datum.*` GUC write outside datum-db/datum-audit (0001 preambles grandfathered), and a DROP that cancels a CREATE. | lint-migrations findings → lanes lint-migrations, lint-migrations-c1 | 2026-09-12 |
