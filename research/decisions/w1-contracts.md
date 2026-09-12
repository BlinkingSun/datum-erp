# DECISION D-W1 — two Wave 1 contract conflicts, ruled

**Authority:** decision lane, task `erp`. **Date:** 2026-09-12.
**Inputs read:** `research/decisions/core-quantity.md` §§1, 2.5, 4.1 (D1);
`research/decisions/ledger-invariant.md` §§2 P2, 5.2, 7 R1–R6 (D2);
`research/decisions/audit-persistence.md` §§1.1, 1.3, 1.4 (D3);
`PLAN.md` §6b item 16; `_team/specs/SPEC-doc-datamodel.md` items 1–2;
`_team/specs/SPEC-harness.md` §`dev/sql` and tests.
`research/` is evidence and is **not edited**; the amendment notes below are the
operative text wherever they conflict with it.

---

## QUESTION 1 — the money column scale

### 1.1 The ruling

| Column | Was | Is |
|---|---|---|
| `ledger.posting.amount` | `numeric(19,4)` (D2 §5.2) | **`numeric(24,6)`** |
| `ledger.posting.unit_cost_applied` | `numeric(19,8)` (D2 §5.2) | **`numeric(24,8)`** |

**P2 value conservation is asserted at the stored scale**, not at the currency's minor
scale. The predicate in D2 §2 P2 is unchanged: `SUM(amount) = 0` over slice A
`(group_id, currency_id)` and slice B `(group_id, currency_id, cost_element)`.

### 1.2 Which decision wins, and why

**D1 wins on the column type.** D2's `numeric(19,4)` is a conventional general-ledger
money column, and nothing in D2's own invariants depends on the `4` — P0–P4 are sums and
counts, R1–R5 are about *quantity* scale, and R6's argument is about where a value
residual is *posted*, not how wide the column is. The `19,4` is incidental to every
claim D2 makes. D1's `6` is load-bearing: §2.5 declares `MONEY_MAX_SCALE = 6` as a
private type invariant, and §4.2 gives rounding authority to exactly two functions.
A storage column narrower than the type it stores is a **third, unnamed rounding site
living inside the database**: PostgreSQL coerces a scale-6 `Money` into `numeric(19,4)`
silently, with no error and no residual. That is not merely lossy — it is the one
failure mode D2 §7 R3 says destroys an invariant, transposed to the value side. Two rows
that Rust computed to sum to zero at scale 6 can, after independent silent rounding to
scale 4, fail `SUM(amount) = 0` at commit; an invariant that fails intermittently on a
rounding edge is the invariant that gets an escape hatch bolted on within six months. A
wider column can never lose information a narrower type produced. A narrower column
always can. `unit_cost_applied` follows the same rule at `RATE_MAX_SCALE = 8`; the
widening from 19 to 24 also puts it alongside `conversion_factor numeric(38,18)`, already
in the same provenance block, so the row has no unexplained precision cliff.

### 1.3 Why the stored scale, and not the minor scale, for P2

Costs first. Asserting at the minor scale means one of two predicates, and both are
worse than what we have:

- `SUM(round(amount, minor)) = 0` **rejects correct groups.** Exact conservation at
  scale 6 does not imply conservation of the per-row rounded values; a three-leg group
  balanced to the microcent can have its rounded legs miss by one minor unit. This is
  D2 §7 R3's failure mode exactly.
- `round(SUM(amount), minor) = 0` is **strictly weaker than what D2 already has.** It
  tolerates up to half a minor unit of unexplained value per group. Across a million
  groups that is real money, and it is invisible in every total — which is precisely the
  laundering P2 slice B exists to catch.

Against that, the stored-scale predicate `SUM(amount) = 0` is exact, deterministic, and
already written. PostgreSQL `numeric` addition is exact; no rounding occurs inside the
aggregate. **The currency's minor unit is a settlement scale, not a storage scale.**

**No new constraint is added.** A `CHECK (amount = round(amount, 6))` would be a
tautology — `numeric(24,6)` already coerces on assignment — and there is no per-currency
narrower storage requirement to enforce, because we have just ruled the minor scale out
of storage. This is the asymmetry with quantity: `quantity_exact_at_scale` exists because
an item's `stock_scale` is genuinely narrower than the `numeric(24,8)` column (D2 §7 R1).
Money has no such per-row narrower scale. The constraint list of §5.2 is unchanged.

### 1.4 Consequence for `Money::settle` (D1 §2.5)

`settle`'s signature is unchanged; its **call sites are narrowed and named**. Under
`numeric(19,4)`, every `Money` would have had to be settled before insert or be silently
mangled by the server — settle would have been a storage obligation discharged everywhere,
and the `Settled` residual would have been discarded at hundreds of sites. Under
`numeric(24,6)` the kernel writes what it computed, and settle is called only where a
currency's minor unit is actually owed: document totals, AR/AP and payment boundaries,
GL export, and anything printed. At each of those the returned `Settled` is
`#[must_use]`, and the residual it carries is posted to the `ROUNDING` value account
**inside the same group**, where P2-A already enforces it. `MONEY_MAX_SCALE = 6` stands
and the database no longer contradicts it.

**The accepted cost, stated:** a posting may now carry sub-minor value, so a naive
trial balance read straight off `ledger.posting` can show fractions of a penny. That is
correct behaviour, not a defect — the penny-exact view is the settled one — but it means
any GL-facing report or export **must** go through settle rather than `SELECT SUM(amount)`.
Wave 2 owns that; it is a reporting rule, not a schema rule, and no invariant is added
for it here.

### 1.5 Consequence for R6

R6's normative claim survives intact and is strengthened: value residuals are in-group,
so the database does catch them. What changes is that at scale 6 `Money::allocate`
(largest-remainder, D1 §2.5) produces parts that sum exactly, so allocation creates **no
residual at all** — a residual now appears only when someone deliberately settles, which
is a named act with an un-ignorable return. R6's asymmetry blockquote ("a value residual
is a discrepancy between postings … a quantity residual is a discrepancy between the
postings and the physical world") is unaffected and still belongs verbatim in the
authoring guide.

### 1.6 AMENDMENT A1 — to be recorded against `research/decisions/ledger-invariant.md`

> **AMENDMENT A1**, recorded by DECISION D-W1-1, 2026-09-12. `research/` is evidence and
> is not edited; this note is operative where it conflicts.
>
> **§5.2, VALUE dimensions block — replace**
> `  amount            numeric(19,4),`
> **with**
> `  amount            numeric(24,6),   -- D1 §2.5 MONEY_MAX_SCALE = 6; the column is never a rounding site (D-W1-1)`
>
> **§5.2, provenance block — replace**
> `  unit_cost_applied numeric(19,8),`
> **with**
> `  unit_cost_applied numeric(24,8),   -- D1 §2.5 RATE_MAX_SCALE = 8 (D-W1-1)`
>
> **§5.2, constraint list — unchanged.** No money-scale `CHECK` is added; see §1.3 above.
>
> **§7 R6, first sentence — replace** "Money is `numeric(19,4)`." **with**
> "Money is `numeric(24,6)`, matching `MONEY_MAX_SCALE` in D1 §2.5. The currency's minor
> unit is a *settlement* scale, reached through `Money::settle`, never a storage scale."
>
> **§7 R6, worked example — replace** the `$47.20`-across-three-lots sentence **with**
> "Allocating a $47.20 receipt across three lots with `Money::allocate` at scale 6 gives
> 15.733334 / 15.733333 / 15.733333, which sums to 47.200000 exactly: at the stored scale
> the allocator creates no residual. A residual arises only when a value is deliberately
> settled to the currency's minor unit, and the un-ignorable `Settled` it returns is
> posted to the `ROUNDING` value account inside the same group — P2-A fails if it is
> dropped."
>
> **§2 P2 — unchanged**, and asserted at the stored scale.

### 1.7 Exact replacement for `SPEC-doc-datamodel.md` item 1

> 1. The ledger, verbatim in substance from `research/decisions/ledger-invariant.md`
>    §§2, 4, 5: the balance slices, the five group kinds and their dispatch rule, the
>    `posting_group` header, the `posting` table with the measure-shape constraint, the
>    consumption edge (§5.3), the deferred constraint trigger's obligations (§5.4), and
>    the boundary matrix. **Restate §5.2 and §7 R6 with AMENDMENT A1 of
>    `_team/reports/DECISION-w1-contracts.md` applied**: `amount` is `numeric(24,6)` and
>    `unit_cost_applied` is `numeric(24,8)`, not the `numeric(19,4)` / `numeric(19,8)`
>    printed in the decision. Footnote each amended line with `D-W1-1`; every other line
>    of §5.2 stays byte-faithful, and no money-scale `CHECK` is invented. State P2's
>    predicate unchanged and **asserted at the stored scale**, and state that a currency's
>    minor scale is a settlement scale reached through `Money::settle` (D1 §2.5), never a
>    storage scale — this is what reconciles item 1 with item 2's `numeric(24,6)` money
>    column, and the document must not print both widths. The decision's §10 bar — slice
>    definitions, boundary matrix and §7 verbatim before any `datum-ledger` migration —
>    stands, as amended.

**Handed to Wave 2 ledger:** confirm D2 §5.4's deferred trigger has no literal scale or
`numeric(19,4)` cast in its aggregate expressions. The predicate is scale-agnostic, but I
did not read §5.4 and am not asserting it is clean.

---

## QUESTION 2 — the application role's DELETE grant

### 2.1 The ruling

`datum_app` holds `DELETE` on **schema `transient` only**. It holds no `DELETE` on
schema `app`, no `INSERT`/`UPDATE`/`DELETE` on schema `audit` (D3 §1.3 unchanged), and
no `TRUNCATE` anywhere. The rule is expressed as **default privileges by schema class**,
not per table.

A table belongs in `transient` only if it carries **no audit trigger** and **no
history-bearing table references it** — sessions, idempotency keys, completed job-queue
rows, projection caches. Everything else is `app`.

### 2.2 Which decision wins, and why

**PLAN §6b invariant 16 wins over D3 §1.1, scoped.** D3's own normative SQL — §1.3, in
the same decision — never emits a `DELETE` grant to anyone; the `DELETE` in the §1.1
roles table is a descriptive cell the decision never implements, so treating it as
binding elevates prose over the SQL beside it. Invariant 16, by contrast, is the
regulatory-facing commitment ("it is found at inspection") that D3's entire hash-chain
apparatus exists to serve: D3 §1.4 shows the trigger logs a `DELETE`, but logging the
destruction of a row is not keeping it, and `old_row` alone cannot reconstruct a record
once its siblings and foreign keys are gone. Where invariant 16 over-reaches is the word
"anywhere": a session row, an idempotency key and a finished job carry no history to
destroy, and forcing them to be soft-deleted buys nothing while guaranteeing monotonic
growth, index bloat and a reaper that must hold the *migration* credential to clean up —
handing DDL rights to a runtime job, which is strictly worse than the grant it avoids.
So invariant 16 is amended to name its subject (*records*) rather than the physical
operation, and the operation-level prohibition moves to where it is mechanical: the
privilege system, by schema.

### 2.3 Schema-class default privileges versus per-table decision — the cost case

Per-table is cheaper **once** and more expensive **forever**. It requires no new schema,
but it has no backstop: every Wave 2+ migration is a fresh opportunity to hand-write a
`DELETE` grant, the pattern must be re-audited at every migration, and `grants_pattern_holds`
can only prove something about tables that already exist. Schema-class default privileges
cost two schemas in Wave 1 and hold for **tables that do not exist yet** — which is
exactly the property `SPEC-harness.md` §`dev/sql` already names as the reason to use
default privileges. The residual cost is real and worth stating: placing a table in
`transient` is now a privilege decision disguised as a naming decision, and a module
author who puts a business table there has silently given it a hard-delete path. That is
mitigated by making the membership rule checkable (no audit trigger, no inbound reference
from a history-bearing table) rather than by review, and it is the reason `ON DELETE
CASCADE` stays banned in `transient` too.

### 2.4 Exact grant text for `dev/sql/02-grants.sql`

```sql
-- 02-grants.sql — grant pattern per audit-persistence §1.3, PLAN §6b inv 16 (amended),
-- and DECISION D-W1-2. Applied as bootstrap superuser; idempotent.
--
-- Schema classes:
--   app       = records with history            -> no DELETE
--   transient = working state with no history   -> DELETE allowed
--   audit     = the trail                       -> SELECT only

CREATE SCHEMA IF NOT EXISTS app       AUTHORIZATION datum_migrate;
CREATE SCHEMA IF NOT EXISTS transient AUTHORIZATION datum_migrate;
CREATE SCHEMA IF NOT EXISTS audit     AUTHORIZATION datum_migrate;

REVOKE ALL   ON SCHEMA app, transient, audit FROM PUBLIC;
GRANT  USAGE ON SCHEMA app, transient, audit TO   datum_app;

-- app: records with history. No DELETE. (PLAN §6b inv 16, D-W1-2)
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA app
  GRANT SELECT, INSERT, UPDATE ON TABLES TO datum_app;

-- transient: sessions, idempotency keys, job queue, projection caches. (D-W1-2)
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA transient
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO datum_app;

-- audit: SELECT and nothing else, for every role. (audit-persistence §1.3)
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA audit
  GRANT SELECT ON TABLES TO datum_app;

-- sequences: draw, never set.
ALTER DEFAULT PRIVILEGES FOR ROLE datum_migrate IN SCHEMA app, transient
  GRANT USAGE ON SEQUENCES TO datum_app;

-- TRUNCATE appears in no GRANT above, in any schema, and PUBLIC holds nothing.
-- The absence is deliberate: TRUNCATE fires no row trigger, so it would erase
-- history with no audit row. (audit-persistence §1.1, D-W1-2)
```

Production note: where D3 §1.1's five-role model is in force, tables are created by
`datum_owner` — repeat each `ALTER DEFAULT PRIVILEGES` with `FOR ROLE datum_owner`, since
default privileges key on the *creating* role and `datum_migrate`'s membership in
`datum_owner` does not cover it.

### 2.5 Amended `PLAN.md` §6b invariant 16

> 16. **No hard deletes of any record, and no cascade deletes.** A record — anything an
>     audit trigger attests to, or that a history-bearing table references — is retired by
>     state change, never by `DELETE`. This is a privilege fact, not a convention:
>     `datum_app` holds no `DELETE` on schema `app` and no `TRUNCATE` in any schema
>     (D-W1-2). Working state that carries no history — sessions, idempotency keys,
>     completed job rows, projection caches — lives in schema `transient`, where `DELETE`
>     is granted and expected; a table qualifies for `transient` only if it has no audit
>     trigger and no history-bearing table references it. `ON DELETE CASCADE` remains
>     prohibited in **every** schema, `transient` included, because one cascade
>     permanently removes the history of rows the author never looked at, and it is found
>     at inspection.

### 2.6 Wording of the harness test `grants_pattern_holds`

> - `grants_pattern_holds`: as `datum_migrate`, create `audit.probe`, `app.probe` and
>   `transient.probe` inside a case schema triple, each with one seeded row. As
>   `datum_app` (second pool from a URL derived from `DATABASE_URL` with the app role),
>   assert: on `audit.probe`, `SELECT` succeeds and `INSERT` is refused; on `app.probe`,
>   `SELECT`, `INSERT` and `UPDATE` succeed and `DELETE` is refused; on
>   `transient.probe`, `SELECT`, `INSERT`, `UPDATE` and `DELETE` all succeed; and
>   `TRUNCATE` is refused on all three. **Every refusal must be asserted on SQLSTATE
>   `42501`, not on a zero row count** — a `DELETE` that matches no rows also "succeeds"
>   with zero rows and would pass a naive test while proving nothing. The probe tables
>   must be created *after* `02-grants.sql` has run, since the whole point is that default
>   privileges reach tables that did not exist when the grant was written.

Update `SPEC-harness.md` §`dev/sql` `02-grants.sql` accordingly (the file currently says
`datum_app` never gets `DELETE` on application tables, which stays true and is now
mechanised per schema), and the acceptance checkbox to:
`grants_pattern_holds` proves SELECT-only on `audit`, no DELETE on `app`, DELETE
permitted on `transient`, and no TRUNCATE anywhere — each by SQLSTATE.

---

## Scope note

No invariant is created beyond the two amendments above. Q1 adds no constraint and leaves
P2, P3 and R1–R5 untouched. Q2 adds one schema and one membership rule, and removes a
`DELETE` grant that D3's own SQL never issued.

DECISION D-W1-1: `ledger.posting.amount` is `numeric(24,6)` and `unit_cost_applied` is `numeric(24,8)`, matching D1 §2.5's `MONEY_MAX_SCALE = 6` and `RATE_MAX_SCALE = 8`, with P2 value conservation asserted exactly at that stored scale — a currency's minor unit is a settlement scale reached through `Money::settle`, never a storage scale, because a column narrower than the type it stores is an unnamed rounding site that would make P2 fail intermittently on groups the kernel computed as balanced.
DECISION D-W1-2: `datum_app` holds `DELETE` on schema `transient` only (sessions, idempotency keys, job rows, projection caches) and never on schema `app` or `audit`, and holds `TRUNCATE` nowhere — expressed as default privileges by schema class so the rule binds tables that do not exist yet, with invariant 16 amended to prohibit hard deletion of *records* rather than the `DELETE` verb, and `ON DELETE CASCADE` still banned in every schema.
