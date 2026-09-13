# Kernel data model

**Conforms to:** [ADR 0004](adr/0004-append-only-ledger.md), [ADR 0005](adr/0005-compliance-in-kernel.md), [ADR 0008](adr/0008-single-tenant.md); `research/decisions/core-quantity.md` (D1), `research/decisions/ledger-invariant.md` (D2), `research/decisions/audit-persistence.md` (D3/D4), `research/decisions/install-story.md` (D5); `_team/reports/DECISION-w1-contracts.md` **D-W1-1** (money column scale, AMENDMENT A1) and **D-W1-2** (schema classes `app` / `transient` / `audit`).

This document is what a Wave 2 lane writes migrations from and what a contributor reads to learn what a record *is*. SQL restated from the decisions is byte-faithful unless noted; AMENDMENT A1 lines are footnoted **D-W1-1**. Storage money columns use **`numeric(24,6)`** only (D1 §4.1, D-W1-1).

---

## 1. Ledger

Source: `research/decisions/ledger-invariant.md` §§2, 4, 5, 7; AMENDMENT A1 in `_team/reports/DECISION-w1-contracts.md`.

### 1.1 Balance slices and predicates

A posting carries `measure`: `QUANTITY` or `VALUE`. Multi-row predicates live in a deferred constraint trigger; the rest are immediate single-row `CHECK`s.

| Id | Slice | Predicate (unchanged text) |
|---|---|---|
| **P0** | The group | `count(DISTINCT created_xid) = 1` over postings and equal to the header’s |
| **P1** | `(group_id, item_id, uom_id)` where `measure = 'QUANTITY'` | `SUM(quantity) = 0` |
| **P2-A** | `(group_id, currency_id)` where `measure = 'VALUE'` | `SUM(amount) = 0` |
| **P2-B** | `(group_id, currency_id, cost_element)` — **MOVEMENT only** | `SUM(amount) = 0` |
| **P3** | Each `QUANTITY` row with `boundary IS NULL` and `quantity < 0` in `MOVEMENT`, `ADJUSTMENT`, or `TRANSFORMATION` | Consumption quantity and value sums match the withdrawal and its value rows (see D2 §2) |
| **P4** | `REVERSAL` union with target group | Exact negation per dimension tuple; consumption edges net to zero |

**P2 value conservation** uses the predicates above **unchanged** and is **asserted at the stored scale** (`numeric(24,6)` for `amount`). PostgreSQL `numeric` addition in aggregates is exact at that scale. A currency’s minor unit is a **settlement** scale reached through `Money::settle` (D1 §2.5), never a storage scale — GL-facing totals must settle, not sum raw postings (D-W1-1 §1.3–1.4).

### 1.2 Five group kinds and dispatch

Kinds: `MOVEMENT`, `ADJUSTMENT`, `TRANSFORMATION`, `VALUATION`, `REVERSAL`. Kind is **recorded** on `ledger.posting_group`, denormalised onto each posting, and **pinned** by composite foreign key.

**Rule D1 — dispatch is monotone.** P0, P1, P2-A, and P4 apply to every kind. A kind may only **add** a predicate.

**Rule D2 — what varies by kind is which boundary accounts the kind may touch**, not which base law applies.

| kind | may touch | forbidden | adds |
|---|---|---|---|
| `MOVEMENT` | `SUPPLIER`, `CUSTOMER` | `SCRAP`, `ADJUSTMENT`, `ROUNDING`, `CONSUMED`, `PRODUCED` | **P2-B** |
| `ADJUSTMENT` | `SCRAP`, `ADJUSTMENT`, `ROUNDING` | `SUPPLIER`, `CUSTOMER`, `CONSUMED`, `PRODUCED` | `reason_code NOT NULL`; `ROUNDING` bounded by item dust tolerance |
| `TRANSFORMATION` | `CONSUMED`, `PRODUCED` | all external boundaries | `work_order_id NOT NULL`; P3 on boundary-crossing quantity |
| `VALUATION` | none | all | no `QUANTITY` rows |
| `REVERSAL` | whatever its target touched | — | **P4** |

Value that moves **with** matter is posted in the matter’s group. Value that **enters** without matter (outside processing charges, absorbed labour, landed cost, and similar) is a separate **`VALUATION`** group (D2 §4.3).

### 1.3 `posting_group` header

```sql
CREATE TYPE ledger.group_kind AS ENUM
  ('MOVEMENT','ADJUSTMENT','TRANSFORMATION','VALUATION','REVERSAL');

CREATE TABLE ledger.posting_group (
  group_id          uuid PRIMARY KEY,
  kind              ledger.group_kind NOT NULL,
  posted_at         timestamptz NOT NULL DEFAULT clock_timestamp(),  -- server time, PLAN §6.4
  created_xid       xid8        NOT NULL DEFAULT pg_current_xact_id(),
  actor_id          uuid NOT NULL,
  source_kind       text NOT NULL,          -- 'purchase_order','work_order','cycle_count',...
  source_id         uuid,
  work_order_id     uuid,
  reason_code       text,
  reverses_group_id uuid,
  reverses_kind     ledger.group_kind,

  UNIQUE (group_id, kind),                       -- FK target: pins kind onto every posting
  UNIQUE (group_id, work_order_id),              -- FK target: pins the WIP cost object

  CONSTRAINT reason_required
    CHECK (kind <> 'ADJUSTMENT' OR reason_code IS NOT NULL),
  CONSTRAINT wo_required
    CHECK (kind <> 'TRANSFORMATION' OR work_order_id IS NOT NULL),
  CONSTRAINT reversal_shape
    CHECK ((kind = 'REVERSAL') = (reverses_group_id IS NOT NULL)),
  CONSTRAINT no_reversal_of_reversal
    CHECK (reverses_kind IS DISTINCT FROM 'REVERSAL'),

  FOREIGN KEY (reverses_group_id, reverses_kind)
    REFERENCES ledger.posting_group (group_id, kind)
);

-- a fact may be reversed at most once; re-post rather than reverse twice
CREATE UNIQUE INDEX posting_group_reversed_once
  ON ledger.posting_group (reverses_group_id)
  WHERE reverses_group_id IS NOT NULL;
```

`actor_id` is **not** supplied by posting intents; it is read from transaction-local audit context (D3 §2.1), same fact as ledger attribution (CONTRACT §6.2).

### 1.4 Supporting types and registry (summary)

Types `ledger.measure`, `ledger.boundary`, `ledger.cost_element`, `ledger.value_account`, and tables `ledger.stock_item`, `ledger.location` are as in D2 §5.1 (verbatim in the decision). Quantity storage is `numeric(24,8)`; money storage is `numeric(24,6)` per D1 and D-W1-1.

### 1.5 `posting` table (measure shape and pins)

```sql
CREATE TABLE ledger.posting (
  posting_id  bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  group_id    uuid    NOT NULL,
  kind        ledger.group_kind NOT NULL,   -- denormalised, FK-pinned
  measure     ledger.measure    NOT NULL,
  created_xid xid8    NOT NULL DEFAULT pg_current_xact_id(),

  -- QUANTITY dimensions
  item_id            uuid,
  uom_id             uuid,
  stock_scale        smallint,
  residual_tolerance numeric(24,8),
  location_id        uuid,
  boundary           ledger.boundary,       -- NULL = real location
  lot_id             uuid,
  serial_id          uuid,
  quantity           numeric(24,8),

  -- VALUE dimensions
  account           ledger.value_account,
  cost_element      ledger.cost_element,
  cost_object_id    uuid,                   -- work order for WIP, sales order for COGS
  currency_id       smallint,
  amount            numeric(24,6),   -- D1 §2.5 MONEY_MAX_SCALE = 6; the column is never a rounding site (D-W1-1)
  values_posting_id bigint,                 -- the QUANTITY row this money prices

  -- provenance. Recorded, audited, NEVER summed by any invariant.
  entered_quantity  numeric(24,8),
  entered_uom_id    uuid,
  conversion_factor numeric(38,18),
  unit_cost_applied numeric(24,8),   -- D1 §2.5 RATE_MAX_SCALE = 8 (D-W1-1)

  UNIQUE (posting_id, group_id),            -- FK target for values_posting_id

  ---------------- shape ----------------
  CONSTRAINT measure_shape CHECK (
    (measure = 'QUANTITY'
       AND quantity IS NOT NULL AND item_id IS NOT NULL AND uom_id IS NOT NULL
       AND stock_scale IS NOT NULL AND residual_tolerance IS NOT NULL
       AND location_id IS NOT NULL
       AND amount IS NULL AND account IS NULL AND cost_element IS NULL
       AND currency_id IS NULL AND values_posting_id IS NULL)
    OR
    (measure = 'VALUE'
       AND amount IS NOT NULL AND account IS NOT NULL AND cost_element IS NOT NULL
       AND currency_id IS NOT NULL
       AND quantity IS NULL AND location_id IS NULL AND boundary IS NULL
       AND stock_scale IS NULL AND residual_tolerance IS NULL)
  ),

  -- a zero row carries no information and is a cheap way to fake a counterpart
  CONSTRAINT no_zero_rows CHECK (COALESCE(quantity, amount) <> 0),

  ---------------- §7 R1: quantity is exact at the item's declared scale ----------------
  CONSTRAINT quantity_exact_at_scale CHECK (
    measure <> 'QUANTITY' OR quantity = round(quantity, stock_scale::int)
  ),

  ---------------- §4.2: the boundary matrix, immediate ----------------
  CONSTRAINT boundary_permitted CHECK (
    boundary IS NULL
    OR (kind = 'MOVEMENT'       AND boundary IN ('SUPPLIER','CUSTOMER'))
    OR (kind = 'ADJUSTMENT'     AND boundary IN ('SCRAP','ADJUSTMENT','ROUNDING'))
    OR (kind = 'TRANSFORMATION' AND boundary IN ('CONSUMED','PRODUCED'))
    OR  kind = 'REVERSAL'                       -- P4 pins a reversal exactly
  ),
  CONSTRAINT valuation_moves_no_matter CHECK (
    kind <> 'VALUATION' OR measure = 'VALUE'
  ),

  ---------------- §7 R4: dust must actually be dust ----------------
  CONSTRAINT rounding_is_dust CHECK (
    boundary IS DISTINCT FROM 'ROUNDING' OR abs(quantity) <= residual_tolerance
  ),

  ---------------- value must attach to matter, except in VALUATION ----------------
  CONSTRAINT value_attaches_to_matter CHECK (
    measure <> 'VALUE'
    OR kind = 'VALUATION'
    OR account NOT IN ('INVENTORY','WIP')
    OR values_posting_id IS NOT NULL
  ),
  CONSTRAINT wip_names_its_cost_object CHECK (
    account IS DISTINCT FROM 'WIP' OR cost_object_id IS NOT NULL
  ),

  ---------------- the five pins ----------------
  FOREIGN KEY (group_id, kind)
    REFERENCES ledger.posting_group (group_id, kind),
  FOREIGN KEY (item_id, uom_id, stock_scale, residual_tolerance)
    REFERENCES ledger.stock_item (item_id, stock_uom_id, stock_scale, residual_tolerance),
  FOREIGN KEY (location_id, boundary)
    REFERENCES ledger.location (location_id, boundary_class),
  FOREIGN KEY (group_id, cost_object_id)
    REFERENCES ledger.posting_group (group_id, work_order_id),   -- MATCH SIMPLE: skipped when NULL
  FOREIGN KEY (values_posting_id, group_id)
    REFERENCES ledger.posting (posting_id, group_id)             -- value stays in its own group
);

CREATE INDEX posting_group_idx   ON ledger.posting (group_id);
CREATE INDEX posting_values_idx  ON ledger.posting (values_posting_id)
  WHERE values_posting_id IS NOT NULL;
CREATE INDEX posting_balance_idx ON ledger.posting (item_id, location_id, lot_id)
  WHERE measure = 'QUANTITY';
CREATE INDEX posting_xid_brin    ON ledger.posting USING brin (created_xid);
```

**What a posting is:** one immutable row in an append-only group — either a quantity movement at a location (with optional lot/serial) or a value leg priced against a quantity row (`values_posting_id`) or a pure valuation leg. **Why a group has a kind:** dispatch and the boundary matrix are keyed off kind; the trigger adds kind-specific predicates (P2-B, P4) without exempting any kind from P0/P1/P2-A.

### 1.6 Consumption edge (cost layers and genealogy)

```sql
CREATE TABLE ledger.consumption (
  consuming_posting_id bigint NOT NULL REFERENCES ledger.posting (posting_id),
  consumed_posting_id  bigint NOT NULL REFERENCES ledger.posting (posting_id),
  group_id             uuid   NOT NULL REFERENCES ledger.posting_group (group_id),
  quantity             numeric(24,8) NOT NULL CHECK (quantity <> 0),
  amount               numeric(24,6) NOT NULL,   -- D-W1-1 (money storage scale)
  PRIMARY KEY (consuming_posting_id, consumed_posting_id),
  CONSTRAINT no_self_consumption CHECK (consuming_posting_id <> consumed_posting_id)
);
CREATE INDEX consumption_forward_idx ON ledger.consumption (consumed_posting_id);
CREATE INDEX consumption_group_idx   ON ledger.consumption (group_id);
```

Signed quantity supports reversals. Remaining layer quantity is **derived** (`posting.quantity − SUM(edges)`); nothing stores a running balance (D2 §6). P3 makes the genealogy graph **total**.

### 1.7 Deferred constraint trigger (obligations)

Function `ledger.enforce_group_invariants()` (D2 §5.4) runs on **`CREATE CONSTRAINT TRIGGER … DEFERRABLE INITIALLY DEFERRED`** for **`INSERT OR UPDATE OR DELETE`** on **`ledger.posting`** and again on **`ledger.consumption`**. At commit it:

1. Loads the group header; fails if postings exist without header (`ZL000`).
2. **P0** — rejects mixed `created_xid` (`ZL001`).
3. **P1** — quantity slice nets (`ZL002`).
4. **P2-A** — value slice nets (`ZL003`).
5. **P2-B** — per cost element for `MOVEMENT` only (`ZL004`).
6. **P3** — withdrawal vs consumption vs value rows (`ZL005`).
7. **P4** — reversal exact negation and consumption net (`ZL006`, `ZL007`).

`SECURITY DEFINER` with pinned `search_path`. Multi-posting writes must use an explicit transaction; failures surface on `commit()`. Full function body: D2 §5.4 (byte-faithful; aggregates are scale-agnostic).

**Bar before migrations:** slice definitions, boundary matrix, and §7 rounding rules (below) must be accepted before any `wicket-ledger` migration (D2 §10), as amended by D-W1-1 for money columns.

### 1.8 Rounding and residuals (D2 §7)

**R1** — Canonical measure per item (`stock_uom_id`, `stock_scale`, `residual_tolerance`); posted quantity exact at `stock_scale` (`quantity_exact_at_scale`).

**R2** — Conversion once at the API boundary (`wicket-uom`, half-even at `stock_scale`); `entered_*` and `conversion_factor` are provenance only.

**R3** — Same canonical number on both legs of a transfer within a group so P1 cannot fail on rounding.

**R4** — Quantity residual lives in **balance**, not inside a balanced group; flushed via **`ADJUSTMENT`** to `ROUNDING` with reason **`UOM_CONVERSION_RESIDUAL`**, capped by `rounding_is_dust` and item tolerance.

**R5** — Item/lot conversion factors pinned; stock UOM/scale/tolerance immutable while postings reference them (composite FK).

**R6** — Value rounding (AMENDMENT A1 / D-W1-1):

Money is `numeric(24,6)`, matching `MONEY_MAX_SCALE` in D1 §2.5. The currency's minor unit is a *settlement* scale, reached through `Money::settle`, never a storage scale.

Allocating a $47.20 receipt across three lots with `Money::allocate` at scale 6 gives 15.733334 / 15.733333 / 15.733333, which sums to 47.200000 exactly: at the stored scale the allocator creates no residual. A residual arises only when a value is deliberately settled to the currency's minor unit, and the un-ignorable `Settled` it returns is posted to the `ROUNDING` value account inside the same group — P2-A fails if it is dropped.

> **A value residual is a discrepancy between postings, so the database enforces it. A quantity residual is a discrepancy between the postings and the physical world, so the database bounds it and a cycle count finds it.**

**R7** — Dust report is a **report**, not a constraint; drives an `ADJUSTMENT` with reason **`UOM_CONVERSION_RESIDUAL`**.

**Where rounding residual goes (value):** `ROUNDING` value account in the **same group** when settle returns `Settled`; P2-A catches drops.

---

## 2. Quantities, money, and wire formats

Source: D1 §4.1 and §6.

| Type | Rust | Max scale | PostgreSQL |
|---|---|---|---|
| `Quantity<D>` | `Decimal` | 8 | `numeric(24,8)` |
| `Money` | `Decimal` | 6 | `numeric(24,6)` |
| `UnitCost<D>` | `Decimal` | 8 | `numeric(24,8)` |

**Three representations:**

| Boundary | Types | Encoding |
|---|---|---|
| In-process | `Quantity<D>`, `Money`, `UnitCost<D>` | Not `Serialize`/`Deserialize` |
| HTTP API | `AnyQuantity`, `MoneyWire` | JSON; **decimal as string**, never JSON number |
| PostgreSQL | `AnyQuantity`, `MoneyWire` via `wicket-db` | `numeric(24,8)` / `numeric(24,6)`; unit as `bigint`; dimension as text or enum |

`wicket-core` does not round; `wicket-uom` owns conversion policy; `wicket-ledger` owns posting residuals (D1 §4.2–4.3).

---

## 3. Audit persistence

Source: D3 §§1.1–1.3, 2, 4, 5, 6, 8.

### 3.1 Roles (five)

| Role | Login | Purpose |
|---|---|---|
| `wicket_owner` | NO | Owns tables and trigger functions |
| `wicket_migrate` | YES | DDL; member of `wicket_owner` |
| `wicket_app` | YES | Application pool; business DML; **`SELECT` only** on `audit.*` |
| `wicket_audit_row` | NO | Owns `audit.row_change()`; column-limited `INSERT` on `audit.event` |
| `wicket_audit_event` | NO | Owns `audit.log_event()`; kernel events without row-change columns |

```sql
CREATE ROLE wicket_owner       NOLOGIN;
CREATE ROLE wicket_audit_row   NOLOGIN;
CREATE ROLE wicket_audit_event NOLOGIN;
CREATE ROLE wicket_migrate     LOGIN;
CREATE ROLE wicket_app         LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
GRANT wicket_owner TO wicket_migrate;
REVOKE SET ON PARAMETER session_replication_role FROM wicket_app;
```

**Who writes the audit row:** the **`audit.row_change()`** trigger (owner **`wicket_audit_row`**), not application code. `wicket_app` has no `INSERT` on `audit.event` (D3 §1.3, §3).

### 3.2 Schema classes (D-W1-2)

| Class | Contents | `wicket_app` |
|---|---|---|
| **`app`** | Records with history (audit trigger, or referenced by history-bearing tables) | `SELECT`, `INSERT`, `UPDATE`; **no `DELETE`** |
| **`transient`** | Sessions, idempotency keys, completed job rows, projection caches — **no audit trigger**, no inbound reference from history-bearing tables | `SELECT`, `INSERT`, `UPDATE`, **`DELETE`** |
| **`audit`** | Trail | **`SELECT` only** |

`wicket_app` holds **`TRUNCATE` nowhere**. **`ON DELETE CASCADE` is banned in every schema**, including `transient` (PLAN §6b item 16 as amended).

Default privileges pattern: `_team/reports/DECISION-w1-contracts.md` §2.4 (`dev/sql/02-grants.sql` owned by harness lane).

### 3.3 `audit.event` (shape)

Partitioned by `at`; columns for time, actor, provenance, business intent (`action`, `reason`, `doc_type`, `doc_id`, `esign_id`), and row change payload (`op`, `old_row`, `new_row`, …) as in D3 §1.2. Grants: D3 §1.3 (verbatim in decision).

### 3.4 Transaction-local actor (D3 §2)

`wicket_db::Tx::begin` sets **`wicket.*` context with `set_config(..., is_local => true)`** in one statement, including **`wicket.txid` = `pg_current_xact_id()`**. `audit.require_context()` fails closed if actor or txid mismatch (`42501`). Read pool never sets actor context.

### 3.5 Time (D3 §4)

Inside the trigger: **`at = now()`** (one time of record per transaction), **`stmt_at = clock_timestamp()`** (intra-transaction order), **`xid = pg_current_xact_id()`**. No client-supplied timestamps as time of record.

### 3.6 Row contents (D3 §5)

`reason` required by catalogue via `audit.reason_required`; `actor_display` denormalised; nullable device/session/request/`esign_id` columns reserved now.

### 3.7 Hash chain (D3 §6)

Per-transaction seal in `audit.tx_seal`; deferred trigger `audit.seal_tx()`; advisory lock on chain head; `prev_hash` / `hash` / `rows_digest` algorithm **`wicket-audit-1`**; anchors off-box. Tamper **evidence**, not proof against a superuser (D3 §7).

### 3.8 Automatic attachment (D3 §1.5)

`audit.attach(rel)` and event trigger `audit_attach` on `CREATE TABLE` (except `audit.exempt`). Module tables created in **`app`** get row and truncate triggers without author boilerplate.

### 3.9 Gap-free numbering (D3 §8)

Regulated document numbers use **`numbering.counter`** transactional `UPDATE … RETURNING`, not `nextval()` or human-visible `IDENTITY`. Allocate late; void instead of delete; counter table is **`audit.exempt`**.

---

## 4. Identity lifecycle (PLAN §6b items 13–15)

**PROPOSED** — exact `identity.*` DDL is not frozen in the four decisions; behaviour is binding:

| Invariant | Rule |
|---|---|
| **13** | Principals are never deleted or reused; deactivate instead; usernames not recycled |
| **14** | **Signing credential** is separate from login credential — reserve column(s) now (e.g. `signing_credential_id`) so SSO does not require retroactive DDL |
| **15** | Signature stores **printed name at signing**; audit/signing rows do not rely on live joins for display name |

---

## 5. Lot and serial identity (PLAN §6b items 9–12)

| Item | Rule |
|---|---|
| **9** | Lot/serial **identifiers at generation**: uppercase letters, digits, hyphen only; **≤ 20 characters** (`research/background/regulatory.md` §1.0.4: GS1 20-char cap, 21 CFR 830.20(c) ISO/IEC 646, HIBCC A–Z/0–9) |
| **10** | **Tracked entity** is a lot or a **unit within a lot** from the first posting — kernel lot entities, not ad hoc strings |
| **11** | **Package hierarchy** (each, inner, case, pallet, contained qty, parent link) is kernel inventory structure |
| **12** | **Expiry** stores **precision** (e.g. day vs month-end), not a bare `DATE` that invents a day |

**What a lot identifier may contain:** characters **`A–Z`**, **`0–9`**, **`-`**, length **1–20** after normalisation to uppercase.

---

## 6. Version stamping and delete conventions (items 16–17)

Every **`app`** record table carries **`application_version`** and **`configuration_version`** (semver or build id + config hash) set at insert/update from the running binary and enabled profile (PLAN §6b item 17; `docs/02-architecture.md` §2).

**No hard delete of records** (item 16, D-W1-2): retire by state change; enforcement is **`DELETE` privilege** — none on **`app`**, allowed on **`transient`** only for qualifying tables.

**Migrations may not:** grant `DELETE` on `app`, use `ON DELETE CASCADE`, disable audit triggers, or place history-bearing tables in `transient`.

---

## 7. Worked examples (canonical set)

Identifiers from `PLAN.md` §3 (same set intended for `docs/10-api-conventions.md`): finished item **`MDS-450-M4x12`** Rev C; raw **`RM-TI-BAR-12`**; heat **`HT-ATI-24-8831`**; bar lot **`LOT-BAR-24-4412`** on **`PO-2024-0841`**; work order **`WO-2026-1847`** (500 EA, lathe **`WC-LATHE-03`**); finished lot **`LOT-WO-1847`**; serials **`SN-450-000134`**…**`SN-450-000633`**; operator **`M. Reyes`**.

Narrative mapping to ledger cases (D2 §8):

1. **Receive bar stock** — `MOVEMENT`: mill heat **`HT-ATI-24-8831`** and **`LOT-BAR-24-4412`** as lot entities on **`PO-2024-0841`** (D2 §8 case a).
2. **Receive two cases** — package hierarchy: two cases post **48 EA** of screws (PLAN §6b item 11).
3. **Quarantine → available** — `MOVEMENT` with **P3** consumption from receipt layer (case b).
4. **Issue bar to `WO-2026-1847`** — `MOVEMENT`; WIP **`cost_object_id`** = work order id; FIFO layer edges (case c with WO-2026-1847).
5. **Complete 500 EA `MDS-450-M4x12`** — `TRANSFORMATION` on **`WO-2026-1847`**: bar through **`CONSUMED`/`PRODUCED`**, finished **`LOT-WO-1847`**, P2-A and P3 carry value across the identity seam (case d).
6. **Invalid lot id** — generation rejects lowercase, space, or 21st character (inv. 9).
7. **Reverse a mistaken issue** — `REVERSAL` group; **P4**; nothing deleted (case l).

Full numeric matrices: `research/decisions/ledger-invariant.md` §8 table (BAR/SCREW fixture); substitute WO-1041 → **`WO-2026-1847`** and part numbers above where illustrative.

---

## 8. Conventions for a module author

**Naming.** Tables live in schema **`app`** unless they meet **`transient`** rules (§3.2). Primary keys are **`uuid`** (**UUID v7** via `Identifier::generate()` / `uuid::Uuid` v7). Foreign keys name the referenced table and column explicitly; no `CASCADE`.

**Keys.** Surrogate ids are opaque v7 UUIDs. Regulated **document numbers** come from **`wicket-numbering`**, not sequences (§3.9). Lot and serial ids obey §5.

**Migrations (`wicket_migrate` only).** May create **`app`** / **`transient`** tables, constraints, indexes, and seed data; must call **`audit.attach`** only indirectly via create-table event trigger (or explicit attach in bootstrap). May **not** grant `wicket_app` broader privileges, add `ON DELETE CASCADE`, disable audit machinery, or put audited data in **`transient`**. Every migration has a tested **`.down.sql`**. Raw SQL in modules stays behind the fence (CONTRACT §5a).

**Audit for free.** Create table with primary key in **`app`** → **`zz_audit_row`** and truncate trigger attached (D3 §1.5). Exemption requires a migration inserting **`audit.exempt`** with reason. Row changes appear in **`audit.event`** with actor from **`Tx::begin`**, not from module code.

**Ledger.** Modules never write **`ledger.*`** directly except through published kernel APIs (`PostingSink` implemented by `wicket-ledger`). One sink per transaction; withdrawals must allocate (CONTRACT §6.2).

**Quantities and money.** Convert in **`wicket-uom`** before building postings; post **`numeric(24,8)`** / **`numeric(24,6)`**; expose API decimals as **strings**.

**Version stamp.** Include **`application_version`** and **`configuration_version`** on every mutable business record in **`app`**.

---

## 9. Quick reference

| Question | Answer |
|---|---|
| What is a posting? | One append-only quantity or value row in a typed group (§1.5) |
| Why does a group have a kind? | Monotone dispatch + boundary matrix + extra predicates (§1.2) |
| Where does a quantity rounding residual go? | Balance; **`ADJUSTMENT`** + `ROUNDING` boundary, dust-bounded (§1.8 R4) |
| Where does a value rounding residual go? | **`ROUNDING` account in the same group** after `Money::settle` (§1.8 R6) |
| Who writes the audit row? | **`audit.row_change()`** as **`wicket_audit_row`** (§3.1) |
| What may a lot id contain? | **`A–Z`, `0–9`, `-`, max 20 chars** (§5) |
