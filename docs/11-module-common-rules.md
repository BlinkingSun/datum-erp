# Module common rules and Wave 2s rulings

Audience: contributor. Status: shipped.

Promoted at the Wave 2s close. The rulings table lives in `research/decisions/w2s-rulings.md`.

## Acceptance addendum (2026-09-12, CONTRACT §5a as amended at batch 2.3)

- `sqlx::query` / `query_as` / `query_scalar` are allowed in this crate (the sealed `Tx` helpers take sqlx query
  objects); the session-protocol surface (`set_config`, `current_setting`, `raw_sql`, `copy_in_raw`,
  `QueryBuilder`, the `query!` macros) stays confined to `wicket-db` / `wicket-audit` / `wicket-test`.
- **Named test `writes_go_through_tx`**: a write to one of this crate's audited tables on a raw pool connection
  (not through `Tx::begin`) aborts with SQLSTATE `42501` and the table is unchanged. Reads through `ReadPool` are
  legitimate; every write path in the crate goes through `wicket_db::Tx`.

## ADDENDUM — fence is a law, not a grep (2026-09-12 05:40)

CONTRACT §5a.1 binds this crate: no `allow(clippy::disallowed_*)` at any scope, no `sqlx` token in any `build.rs`, no generated query code, no runtime DDL/GRANT/CREATE EXTENSION from Rust (all DDL in migrations run as `wicket_migrate`). Plain `sqlx::query`/`query_as`/`query_scalar` through the sealed `wicket_db::Tx` are the only SQL entry points; `just clippy` and `just lint-sql` must be green with the fence exactly as landed on main. Circumvention is a Fail-class finding (measured on wicket-uom attempt 1).

## ADDENDUM — kernel seams every slice module must respect (2026-09-12 10:56, from the batch 2.6 glue adjudication)

- Hooks run under the sealed, synchronous `HookView` ABI: a hook cannot call `wicket_uom` `to_stock`/`convert`
  or emit events from inside the hook. Do the unit conversion and the event emission through the `Kernel`
  methods around the transition (`Kernel::to_stock`/`convert` on the caller's `Tx` before/after
  `Kernel::transition`, `Kernel::publish_event`); a module that needs in-hook conversion or emission reports
  it as a seam for Wave 2b (statemachine ABI change), never works around it.
- Event subscriptions come from the module manifest and are registered through the kernel's subscribe path
  (`register_enabled_from_manifests` / `events.subscribe`), never by a constant in module code.
- The `PostingSink` a hook receives is bound to the live transaction; postings reach the ledger through
  `wicket_ledger::post` inside that transaction after `finalize`; an unfinalized sink poisons the transaction.
- `wicket_db::Tx::commit` does not consult the in-process poison flag (ledger design); modules commit through
  the kernel's transition/commit path, never by calling `Tx::commit` after a hook posted.
- **Lot-less receipts vs `inventory.lot_received` v1 (kernel-e2e, 2026-09-12 11:20).** The standard events registry marks `lot_id` required on `inventory.lot_received` v1, so a lot-less receipt (plain-shop, non-lot-controlled item) has no honest event to publish; the kernel proof used a dummy lot id. Wave 2s.1 (items/lots) and 2s.2 (inventory) build to this: a lot-less receipt publishes `inventory.receipt_posted` v1 (no `lot_id`; carries `item_id`, `location_id`, `qty`, `uom`, `posting_group_id`), and `inventory.lot_received` stays lot-only with `lot_id` required. Registering the new event is an additive registry change in `wicket-events` (schema + fixture), not a v2 of `lot_received`. Never publish a placeholder lot id.

## ADDENDUM — Wave 2s rulings R-2s-1 / R-2s-2 (2026-09-12 12:45, from the 2s.1 audits and the lots adjudication)

- **R-2s-1 schema per module.** A module owns exactly one PostgreSQL schema named after it (`items`, `locations`, `lots`, `inventory`, …), registered as class `app`; working state goes in `<module>_transient` (class `transient`). "schema `app`" in older text means the class, not a name. The cross-module lint (invariant 6) is what makes this a law: a crate reads only its own schema; anything else goes through a published crate API or events.
- **R-2s-2 one name triple.** Crate `wicket-mod-<x>` (Cargo `[package] name`), manifest id `mod-<x>` (`module.toml` `id`, `profiles/*.toml`, `wicket-module` compiled-in list), schema `<x>`. 2s.1 landed with three different conventions (items/items/items, wicket-mod-locations/mod-locations/locations, lots/lots/lots); the 2s1-glue lane aligns them to the triple. Every later module ships aligned.
- **R-2s-6 residual/variance groups (2026-09-12 14:10, inventory adjudication residual 5).** A posting group has one kind. A UoM/cost residual that a movement or count produces is posted as its own `ADJUSTMENT` group in the SAME transaction, carrying `parent_group_id` = the movement group, and the named test asserts both groups conserve and the link exists. SPEC-mod-inventory "same group" wording is superseded. (To be copied into research/decisions/w2s-rulings.md at the next docs commit.)
- **R-2s-6 amendment (2026-09-12 14:40, audit-mod-inventory-c1).** `ledger.posting_group` has no `parent_group_id` yet. wicket-ledger adds it (nullable FK, `GroupBuilder::parent(group_id)`, migration up/down) in lane w2-carry-3; until that lands, the module links the residual `ADJUSTMENT` group to its movement group through the group's `source_kind`/reference tag and the named test asserts BOTH groups conserve and the link resolves. Once the column lands, modules switch to it (a one-line rework).
- **R-2s-7 composite mutations are one transition (2026-09-12 18:10, from server-slice-c2-r1 item 3 BLOCKED).** A `Tx` binds ONE business action (`WriteContext`) and cannot rebind; therefore a composite business mutation exposed as one HTTP call (e.g. `issue_wo` = issue material + start the work order) is modelled as ONE kernel transition on the owning record (the work order's `start`), and its secondary effects (the material issue postings) run as hooks on that transition's bound `PostingSink` inside the same transaction. The server never chains two bound actions in one request; if the owning module lacks the hook, the module gains it (module rework) — the server does not work around it. Two sequential transactions for one request is fail-class (docs/10 §4: a request is atomic).
- **R-2s-8 SECURITY DEFINER exemption set (2026-09-12 18:45, from lint-migrations findings).** `CREATE FUNCTION … SECURITY DEFINER` is allowed only in the migrations of `wicket-db`, `wicket-audit`, `wicket-ledger` and `wicket-numbering` (gap-free counter and exemption helpers, D3 §8), each documented in that crate's README. No module and no other kernel crate may define one; a cross-schema read or GRANT inside such a function is a fence bypass. `uom.item_has_postings` and `items.item_has_postings` are removed in favour of `wicket_ledger::has_postings` (R-2s-3). `just lint-sql` scans migrations as well as sources.

## ADDENDUM — R-2b-1 one install order (2026-09-13)

**R-2b-1 one install order (2026-09-13).** `wicket_module::order::CANONICAL_ORDER` is the only install order. A crate's migrations must run green **both** at their canonical position and as the last crate on a fresh database, with the `audit_attach` event trigger installed and never dropped, and must leave an **identical** set of audit-attached relations either way. A migration therefore declares `ALTER DEFAULT PRIVILEGES FOR ROLE wicket_migrate IN SCHEMA <x> GRANT TRIGGER ON TABLES TO wicket_owner` before its first `CREATE TABLE`, ends with an idempotent `SELECT audit.attach(…)` for every audited table, registers any exemption in `audit.exempt` before the table exists, and carries the six-GUC migration preamble if it writes an attached row. A test harness that hand-rolls a migrator list instead of calling `install_upto`, a migration that requires the trigger to be absent, and an install order that changes which relations end up audited are each **Fail-class**.
