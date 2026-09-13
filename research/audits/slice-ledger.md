# Plan-audit slice 1: ADR 0004 deferred zero-sum constraint (PostgreSQL)

**Role:** SWEET-SPOT adversarial researcher · **Date:** 2026-09-11 · **Scope:** read-only audit of PLAN + ADRs vs PostgreSQL 15–17 mechanics

---

## Executive verdict (first paragraph)

**The thesis is only half right.** PostgreSQL can enforce a **commit-time, multi-row balance invariant** at ADR’s target volumes (5k business transactions/day, ~2–20 postings/group, 10M-row rebuild budget, revisit at ~50M rows/year) without blocking the 10-minute rebuild target—but **not** as the prose invariant “`SUM(quantity)` over all rows sharing `group_id` equals zero.” That aggregate is **wrong for any real BOM completion, mixed-UOM group, or multi-ledger atomic posting** and is the case that **forces a non-zero group-wide sum** unless the plan is narrowed. What *is* implementable is a **deferred constraint trigger** (there is no `CREATE ASSERTION`; SQL feature F521 is unimplemented per [PostgreSQL D.2](https://www.postgresql.org/docs/17/unsupported-features-sql-standard.html)) that, at `COMMIT`, asserts zero net per **balance slice**—at minimum `(group_id, ledger)` and, for inventory, per **(item_id, uom)** (and whatever dimension keys define a conservation law: location, lot, serial as required by the posting schema). Virtual locations fix **external** appearance/disappearance; they do **not** fix **unlike items** or **unlike units** in one scalar sum. Until PLAN/ADR state that slice explicitly, Wave 2 `wicket-ledger` risks encoding a constraint that either rejects legitimate production or gives a false sense of safety.

---

## 1. PostgreSQL deferrability (docs, not folklore)

Source: [SET CONSTRAINTS](https://www.postgresql.org/docs/17/sql-setconstraints.html), [CREATE TABLE](https://www.postgresql.org/docs/17/sql-createtable.html), [CREATE TRIGGER / CONSTRAINT TRIGGER](https://www.postgresql.org/docs/17/sql-createtrigger.html), [pg_constraint](https://www.postgresql.org/docs/17/catalog-pg-constraint.html).

| Mechanism | DEFERRABLE? | Can express “group sums to 0 at COMMIT”? |
|-----------|-------------|------------------------------------------|
| `CHECK` | **No** — checked immediately on row insert/update | No (single-row only) |
| `NOT NULL` | **No** | No |
| `UNIQUE` / `PRIMARY KEY` | **Yes** (optional) | No |
| `FOREIGN KEY` | **Yes** (optional) | No |
| `EXCLUDE` | **Yes** (optional) | No (not a sum) |
| **`CONSTRAINT TRIGGER`** (`CREATE CONSTRAINT TRIGGER`, `AFTER` `FOR EACH ROW`) | **Yes** | **Yes** — trigger body runs when deferred constraints run (commit or `SET CONSTRAINTS … IMMEDIATE`) |
| **`CREATE ASSERTION`** | N/A | **Not implemented** (F521 in D.2) |

**`SET CONSTRAINTS`** explicitly states it applies to UNIQUE, PK, FK, EXCLUDE, and that **constraint triggers fire on the same schedule** as their associated deferrable constraints. CHECK/NOT NULL are **never** deferred.

**Implication:** The ADR’s “database constraint” for zero-sum **must** be a **deferrable `INITIALLY DEFERRED` constraint trigger** (or equivalent enforced only in a single `COMMIT` path—worse for grants story). A table-level `CHECK (sum…)` is impossible; a generated “balance” column maintained by ordinary triggers is **mutable state** and conflicts with “postings are the truth” unless it is clearly a non-authoritative helper (PLAN forbids disguising balances as truth).

**Transition tables:** [CREATE TRIGGER](https://www.postgresql.org/docs/17/sql-createtrigger.html) — `REFERENCING NEW TABLE` / `OLD TABLE` is allowed only for **`AFTER` triggers that are not constraint triggers**. Therefore you **cannot** combine “defer to COMMIT” (constraint trigger) with “one statement-level aggregate over all rows touched this transaction” (transition table). Commit-time validation is **row-trigger semantics**: one queued firing per inserted/updated/deleted posting row at commit (unless you add a separate non-deferred pattern).

**Assertion substitute in practice:** Deferred **constraint trigger** whose function aggregates `postings` for the affected `group_id` (and balance slice) and `RAISE EXCEPTION` if any slice ≠ 0. This is the standard PostgreSQL replacement for assertions. It is **not** serializable cross-transaction mutual exclusion: concurrent transactions on **different** `group_id` values are fine; two writers on the **same** `group_id` should be impossible by design (one UUID per atomic business transaction, single writer). Cybertec and list-archive discussions apply to **shared-row** predicates (e.g. “at least one guard on duty”); **not** to one-writer-per-group if that invariant is enforced in the API.

---

## 2. The REAL constraint (SQL sketch)

**Posting table (conceptual):**

```sql
CREATE TABLE ledger.posting (
  id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  group_id    uuid NOT NULL,
  ledger      text NOT NULL CHECK (ledger IN ('inventory','cost','labor')),
  posted_at   timestamptz NOT NULL DEFAULT clock_timestamp(),
  posted_by   uuid NOT NULL,
  item_id     uuid,          -- inventory slice
  uom_id      uuid,          -- inventory slice
  location_id uuid,          -- inventory slice (incl. virtual)
  lot_id      uuid,
  serial_id   uuid,
  quantity    numeric(24,8), -- inventory; NULL if amount-only row
  amount      numeric(24,8), -- cost/labor money or hours encoding; NULL if qty-only
  -- source, reason, dimensions JSONB as needed
  CONSTRAINT posting_measure CHECK (
    (ledger = 'inventory' AND quantity IS NOT NULL AND amount IS NULL)
    OR (ledger IN ('cost','labor') AND amount IS NOT NULL AND quantity IS NULL)
  )
);

CREATE INDEX posting_group_id_idx ON ledger.posting (group_id);
-- Rebuild / time-range scans (not for commit check):
CREATE INDEX posting_posted_at_idx ON ledger.posting USING brin (posted_at);
-- Per-slice partial indexes if needed for property tests at scale
```

**Balance slices (required PLAN amendment):**

1. **Inventory:** ∀ `(group_id, item_id, uom_id)`: `SUM(quantity) = 0` (locations/lots appear as signed rows; virtual locations close the system boundary).
2. **Cost / labor:** ∀ `(group_id, ledger)`: `SUM(amount) = 0` (or per `cost_element` / `work_order_id` if labor and burden must not net across jobs—another decision).

**Constraint trigger (one function; fire on INSERT/UPDATE/DELETE):**

```sql
CREATE OR REPLACE FUNCTION ledger.enforce_group_balance()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
  gid uuid := COALESCE(NEW.group_id, OLD.group_id);
  bad_inventory record;
  bad_amount record;
BEGIN
  -- Inventory slices
  SELECT item_id, uom_id, SUM(quantity) AS s
    INTO bad_inventory
    FROM ledger.posting
   WHERE group_id = gid AND ledger = 'inventory'
   GROUP BY item_id, uom_id
  HAVING SUM(quantity) <> 0
   LIMIT 1;
  IF FOUND THEN
    RAISE EXCEPTION 'inventory slice not balanced in group %: item % uom % sum %',
      gid, bad_inventory.item_id, bad_inventory.uom_id, bad_inventory.s;
  END IF;

  -- Cost/labor (example: per ledger only; tighten if needed)
  FOR bad_amount IN
    SELECT ledger AS lg, SUM(amount) AS s
      FROM ledger.posting
     WHERE group_id = gid AND ledger IN ('cost','labor')
     GROUP BY ledger
    HAVING SUM(amount) <> 0
  LOOP
    RAISE EXCEPTION 'ledger % not balanced in group %: sum %',
      bad_amount.lg, gid, bad_amount.s;
  END LOOP;

  RETURN NULL; -- AFTER trigger
END;
$$;

CREATE CONSTRAINT TRIGGER posting_group_balance
  AFTER INSERT OR UPDATE OR DELETE ON ledger.posting
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW
  EXECUTE FUNCTION ledger.enforce_group_balance();
```

**Application contract:** `BEGIN; SET CONSTRAINTS posting_group_balance DEFERRED;` (default if `INITIALLY DEFERRED`); insert all postings; update projections in same transaction; `COMMIT` → trigger runs.

**Optimization note:** With 2–20 rows/group, **O(n²)** repeated `SUM` at commit (one trigger firing per row) is still **microseconds to low milliseconds** per transaction with `posting_group_id_idx`. At scale, a **single commit-time pass** could use a `TEMP` table of touched `group_id`s maintained by a **non-deferred** `AFTER STATEMENT` trigger—but that trigger **cannot** be deferrable; only the constraint trigger can. Accept duplicate aggregates in v1 or add a **second** deferred constraint trigger that no-ops unless `pg_trigger_depth()` / session GUC says “final pass” (fragile). **Spike item:** measure duplicate-`SUM` cost with `EXPLAIN (ANALYZE)` at 20 rows/group.

---

## 3. Write-path cost at stated volumes

| Factor | Assessment |
|--------|------------|
| **5k tx/day** | ~25k–100k posting inserts/day → trivial WAL/CPU vs interactive targets |
| **2–20 postings/group** | Commit: 2–20 deferred trigger invocations × indexed aggregate over ≤20 rows |
| **10M historical rows** | Does **not** amplify per-commit work (index seeks on `group_id`, not table scan) |
| **WAL** | One insert record per posting; append-only → no HOT update win; `fillfactor` 100 on `posting` is fine |
| **TOAST** | Wide `dimensions` JSONB may TOAST; does not affect balance trigger if slice keys are columns |
| **BRIN on `posted_at`** | Helps rebuild/time-window reports; **irrelevant** to commit check |
| **Partitioning by `posted_at`** | Compatible if **all rows for a `group_id` share one partition** (same `posted_at` within txn). Cross-partition group would break locality—**forbid** by setting `posted_at` once per group in kernel |
| **50M rows/year revisit** | Constraint cost still per-txn; pressure moves to **projection maintenance**, **rebuild**, **index bloat**, **vacuum**—needs partition + archival ADR, not weaker balance |

**Rebuild &lt;10 min @ 10M rows:** ~17k rows/s sustained aggregate—parallel seq scan + `GROUP BY` dimensions, not constraint-trigger path. Independent of deferrable enforcement.

---

## 4. SQLx + deferred constraints

- **Explicit transaction:** Inserts succeed inside `tx`; **violation surfaces on `tx.commit().await?`** for deferred FK and constraint triggers (SQL standard behavior).
- **API:** Use `&mut *tx` as `Executor` (SQLx 0.7+); map `DatabaseError::constraint()` to domain errors in `wicket-ledger`.
- **Autocommit + `RETURNING`:** SQLx tests include `test_error_handling_with_deferred_constraints` (deferred FK fails on single-statement autocommit path). Historical issue #1370 (silent drop) — **mandate explicit transactions for all multi-posting groups** and integration-test `commit()` failure; do not rely on autocommit for ledger groups.
- **PLAN gap:** Acceptance criterion: “unbalanced group returns `LedgerBalanceError` with `group_id` on **commit**, never partial projection visible.”

---

## 5. Virtual-location trick: where it breaks

| Scenario | Breaks? | Notes |
|----------|---------|-------|
| Receipt / ship / scrap / adjust | **No** | `SUPPLIER`, `CUSTOMER`, `SCRAP`, `ADJUSTMENT` close the boundary per item/uom slice |
| **BOM completion (4 screws + 1 housing → 1 assembly)** | **Yes for group-wide `SUM(quantity)`** | Net +1 assembly −4 −1 ≠ 0. **Designable** with per-`(item_id,uom)` slices + WIP virtual locations per component |
| **Mixed UOM in one group** (primer §9) | **Yes** | Feet vs inches vs pounds cannot share one scalar; **fatal** unless kernel converts to **canonical UOM per item** before insert or stores separate slices |
| Yield / kerf / phantom / backflush | **Designable** | Loss: move to `SCRAP` or variance location; phantom BOM: timing of issue vs backflush is **posting choreography**, not constraint weakening |
| Serial-split of lot | **Designable** | Two transfers, same item/uom, lot dimension changes |
| **Negative inventory (on-hand)** | **Not a group-balance issue** | Projection can go negative if business allows; constraint only nets **group**; allocation/available is separate |
| Catch-weight | **Designable** | Two measures → two items or qty in canonical UOM + optional weight attribute not in sum |
| **Labor hours + inventory qty in one sum** | **Fatal** | Separate `ledger` + `amount` vs `quantity` slices (architecture already hints; ADR prose does not) |
| **Multi-ledger same group** | **Fatal for single scalar** | Atomic WO completion: inventory rows + cost rows + labor rows — **must** balance **per ledger** (and likely per WO for cost) |

---

## 6. Breakdown cases (adversarial matrix)

| # | Case | Fatal vs designable |
|---|------|---------------------|
| 1 | Mixed units in one group | **Fatal** without canonical UOM per item at posting time |
| 2 | BOM completion | **Fatal** for naive group sum; **designable** with per-item slices + virtual WIP |
| 3 | Yield/kerf/phantom/backflush/serial-split/negative/catch-weight | Mostly **designable**; negative on-hand is policy; catch-weight needs explicit model |
| 4 | Labor hours vs inventory qty | **Fatal** if one sum; **designable** with separate ledgers/measures |
| 5 | Multi-ledger groups | **Fatal** for one sum; **designable** with per-`(group_id, ledger)` and possibly finer cost dimensions |
| 6 | Projection invalidation under concurrent shop-floor scans | **Not fatal** to DB constraint; **hard** for &lt;500ms read if projections updated outside same txn or without row locks. ADR rejected CQRS **eventual** read lag—not the same as “projection in same commit as postings.” Risk: **race on projection cache**, not zero-sum trigger. Needs `SELECT … FOR UPDATE` on projection rows or derive read from ledger for floor (too slow)—**spike + acceptance test under concurrent issues** |

**Forcing case that FORCES non-zero group (plain English):** *Single `group_id`, single `SUM(quantity)` across rows:* **WO completion** posting −4 EA screws, −1 EA housing, +1 EA assembly → sum = −4.

---

## 7. PLAN / ADR gaps

| Gap | Recommendation |
|-----|----------------|
| **Invariant definition** | Amend ADR 0004 + PLAN §6.2 to: “posting group balances to zero **per balance slice**” with normative slice keys (inventory: `item_id`+`uom_id`; cost/labor: `ledger`+TBD dimensions) |
| **Spike** | `spike-ledger-constraint`: pg17 `EXPLAIN ANALYZE` commit path; duplicate trigger firings; sqlx commit error mapping; concurrent projection update |
| **Acceptance criteria** | Unbalanced slice rejected at commit; balanced multi-item WO accepted; mixed-UOM rejected at insert (UOM kernel) not at obscure commit; property tests generate **per-slice** not per-group scalar |
| **Partition / archival** | No task for 50M/year — add before revisit threshold: monthly partitions, detach/archive, rebuild job scoped per partition |
| **Grant story** | App role: INSERT on `posting` only; no UPDATE/DELETE; constraint trigger owned by superuser/migration role |
| **docs/05-data-model** | Must land slice definition before `wicket-ledger` migrations |

---

## 8. Dispatch metadata

| Field | Value |
|-------|--------|
| **EXECUTOR** | **cursor** for spike + `wicket-ledger` implementation; **grok** for parallel property-test generation; **blind-race** only after slice definition is frozen |
| **Opus DECISION before crates?** | **YES** — narrow balance slice (inventory vs cost/labor, WO-level cost netting, canonical UOM rule). Wrong trigger encoding is expensive to unwind and touches every module posting path |
| **SPLIT** | (A) Opus decision + ADR amendment; (B) spike-ledger-constraint; (C) `wicket-ledger` crate + migrations; (D) projection/concurrency spike separate from constraint |
| **AUDIT TIER** | **deep** for `wicket-ledger` and migrations; **standard** for ADR text-only amendment |
| **SHARD notes (property / rebuild suite)** | Shard generators by: `ledger` enum; inventory vs cost; single-item transfer vs multi-item WO; virtual location class; UOM conversion present/absent; group size 2 vs 20. Rebuild tests: shard by `posted_at` year-month partitions. Tag tests `slice_inventory`, `slice_cost`, `multi_item_wo`, `mixed_uom_forbidden` |

---

## References (PostgreSQL 15–17)

- [SET CONSTRAINTS](https://www.postgresql.org/docs/17/sql-setconstraints.html) — deferrable types; constraint trigger timing
- [CREATE TRIGGER](https://www.postgresql.org/docs/17/sql-createtrigger.html) — CONSTRAINT TRIGGER; transition tables incompatible with constraint triggers
- [D.2 Unsupported Features — F521 Assertions](https://www.postgresql.org/docs/17/unsupported-features-sql-standard.html)
- [pg_constraint catalog](https://www.postgresql.org/docs/17/catalog-pg-constraint.html) — `contype` includes `t` for constraint triggers
