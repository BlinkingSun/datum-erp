# DECISION — The conservation invariant of the Wicket ledger

**Authority:** Opus (decision lane) · **Date:** 2026-09-11 · **Status:** decided, binding
**Inputs:** `_team/reports/sweep-plan-ledger.md`, `_team/reports/sweep-plan-typed-qty.md`,
ADR 0003, ADR 0004, ADR 0007, `PLAN.md` §5 §6, `docs/00-erp-primer.md` §9,
`docs/02-architecture.md` §2–§3.
**Amends:** `docs/adr/0004-append-only-ledger.md` (in place, this cycle).
**Binds:** `wicket-ledger`, `wicket-uom`, `wicket-core`, all Wave 2 migrations, `docs/05-data-model.md`.

---

## 0. The decision in one page

The original claim — "every posting group sums to zero" — is wrong twice over, and the
second error is the interesting one.

It is wrong *mechanically* because a scalar `SUM(quantity)` over a group adds feet to
each and pounds to dollars. The sweep is right: conservation holds **per balance slice**.

It is wrong *physically* because manufacturing is not a closed system in quantity. A
titanium bar becomes five hundred screws. No slicing of a quantity sum survives that,
because matter changes identity. The tempting patch — virtual `CONSUMED` and `PRODUCED`
locations that absorb each side — is correctly diagnosed in the brief as bookkeeping
theater: it makes every group balance by construction and the constraint stops
detecting anything.

**The resolution is not to weaken the invariant at the seam. It is to make access to
the seam the constrained resource, and to require value to cross it.**

> **Quantity is conserved in every posting group, per `(item, uom)`. Value is conserved
> in every posting group, per currency. Quantity may only cross an identity boundary
> inside a group whose declared kind is `TRANSFORMATION`, and every quantity that
> crosses must carry a value posting that does not cross — so the seam where matter is
> permitted to change identity is exactly the seam where value is forbidden to
> disappear.**

That is the first sentence of the law. The second sentence is what makes it bite:

> **Every withdrawal of matter from a real location must be allocated over named source
> postings by the costing engine, and that allocation must independently reproduce both
> the quantity the movement engine stated and the money the valuation engine stated.**

The first sentence is structural and catches missing, duplicated, and transposed rows.
The second sentence is the only part of the design with *genuine redundancy from two
independently computed sources*, and it is therefore the only part that can catch an
error in a number that is otherwise stated once. It is also — not coincidentally — the
structure that makes genealogy total and makes FIFO, moving average, and standard cost
all fall out of one table.

Five group kinds: `MOVEMENT`, `ADJUSTMENT`, `TRANSFORMATION`, `VALUATION`, `REVERSAL`.
One posting table with two measures. One append-only `consumption` edge table.
**`PLAN.md` §6.1 (no stored balances) survives intact — nothing below stores a running
total.**

---

## 1. Why the obvious patch fails, stated precisely

Take the virtual-location patch on its own terms. A work order issues one bar and
completes five hundred screws:

```
-20.0000 FT  BAR    @ WIP-WO1041          +20.0000 FT  BAR    @ CONSUMED
+500     EA  SCREW  @ WIP-WO1041          -500     EA  SCREW  @ PRODUCED
```

Both slices net zero. Now transpose a digit: write the counterpart of `-20.0000` as
`+02.0000`. The slice nets `-18` and the group is rejected. So the constraint is not
*entirely* vacuous — it catches single-row corruption.

But it is vacuous against the error that matters, because in the posting path above
the `CONSUMED` row is produced by negating the `WIP` row. One number, written twice, by
one line of code. The constraint proves that a `numeric` survived a round trip through
WAL. That is not an invariant; it is a checksum with delusions.

**The general principle, which governs the rest of this document:**

> A conservation constraint is load-bearing exactly to the degree that the two sides of
> the slice originate from *different* computations. Where both sides come from one
> variable, the constraint detects storage corruption and nothing else. Where they come
> from two engines that had to agree, it detects a disagreement — which is a real bug.

So the design work is not choosing a slice key. It is arranging the posting schema so
that the numbers which must agree are produced by parties that could have disagreed.
Three places in an ERP have that property, and this design uses all three:

| Redundancy | Source A | Source B | Enforced by |
|---|---|---|---|
| Matter vs. cost layers | movement engine states the quantity moved | costing engine resolves it against open layers and returns its own total | **P3** |
| Matter vs. money | costing engine's layer allocation sums to an amount | valuation engine posts an amount against the inventory account | **P3** |
| Correction vs. original | the reversal group, written now | the original group, written days ago and immutable | **P4** |

Everything else in the design — P0, P1, P2, and the immediate `CHECK`s — is structural
hygiene: it catches missing counterparts, boundary abuse, unreasoned adjustments,
off-scale quantities, and late tampering. That is worth having and it is honest to call
it what it is.

**What no group-level invariant can catch, stated up front so nobody believes
otherwise:** an operator who scans `550` when the physical count is `500`, in a posting
path where every derived number is derived from `550`. The redundant source for that
error is the *source document* — the purchase order's open quantity, the work order's
remaining quantity, the pick list — and it is an application-level tolerance check, not
a ledger invariant. It is deliberately not a ledger invariant because the ledger must be
able to record an over-receipt, which is a real thing that happens. See §9.

---

## 2. Question 1 — the exact set of balance slices

A posting carries a `measure` discriminator: `QUANTITY` or `VALUE`. Four predicates are
multi-row and therefore live in the deferred constraint trigger. Everything else is an
immediate single-row `CHECK`, which is better: it fails at the offending statement with
the offending row in hand, not at `COMMIT` with a group id.

### P0 — atomicity of the group

**Slice:** the group. **Predicate:**

> All postings in a group, and the group header, were written by one transaction:
> `count(DISTINCT created_xid) = 1` over the group, and equal to the header's.

Closes the hole where a later transaction appends a *balanced pair* to a settled group,
mutating history without ever failing P1 or P2. Without P0, "append-only" means
"append-only per row", not "immutable per fact".

### P1 — quantity conservation

**Slice:** `(group_id, item_id, uom_id)` over rows where `measure = 'QUANTITY'`.
**Predicate:** `SUM(quantity) = 0`.

`uom_id` stays in the slice key even though it is pinned by foreign key to the item's
stock UOM (§5.2). The pin makes a mixed-UOM group impossible to write; the slice key is
what survives if the pin is ever relaxed, and it is what makes the failure legible when
an item's stock UOM is migrated. Redundant defenses at the two ends of the same wire are
cheap here and I am keeping both.

### P2 — value conservation

**Slice A, universal:** `(group_id, currency_id)` over `measure = 'VALUE'`.
**Predicate:** `SUM(amount) = 0`.

**Slice B, MOVEMENT only:** `(group_id, currency_id, cost_element)`.
**Predicate:** `SUM(amount) = 0`.

Slice B is strictly stronger and applies only to `MOVEMENT`. Rationale: a pure movement
of matter must not silently reclassify cost between elements. The error it catches is
real and nasty — a costing bug that launders accumulated labour and burden into material
cost during a put-away, which then inflates the material content of every downstream
assembly and is invisible in every total. `TRANSFORMATION` and `VALUATION` are exempt
from slice B **because element mixing is their entire purpose**: labour absorbed into WIP
becomes part of the produced item's cost, and a revaluation moves value between elements
by definition.

### P3 — quantity/value coupling (the load-bearing one)

**Slice:** each individual `QUANTITY` posting `p` with `p.boundary IS NULL`
(a real, owned, valued location) and `p.quantity < 0`, in a group of kind
`MOVEMENT`, `ADJUSTMENT`, or `TRANSFORMATION`.

**Predicate, both halves:**

> `SUM(consumption.quantity WHERE consuming_posting_id = p) = -p.quantity`
> and
> `SUM(consumption.amount   WHERE consuming_posting_id = p) = -SUM(v.amount WHERE v.values_posting_id = p)`

In words: **matter does not leave a real location unless the costing engine has named
the specific earlier postings it came out of, in amounts that add up to exactly what
left, valued at exactly what the valuation engine posted.**

This is the predicate the whole design exists to support. It is the one with two
independent sources. It is also, as a free consequence, what makes genealogy *total*:
no posting can be an orphan in the lineage graph, because the database refuses the
withdrawal that has no named parents. Genealogy stops being a query you hope works and
becomes a database invariant.

### P4 — exact reversal

**Slice:** for `kind = 'REVERSAL'` with target `g0`, the union `{g, g0}` grouped by the
full dimension tuple `(measure, item_id, uom_id, location_id, boundary, lot_id,
serial_id, account, cost_element, cost_object_id, currency_id)`.
**Predicate:** `SUM(quantity) = 0 AND SUM(amount) = 0` on every tuple, **and** the union
of the two groups' `consumption` edges nets to zero per consumed posting.

A correction is not "some offsetting postings". It is the exact negation of a specific
historical fact, and the historical fact is immutable and sitting right there to be
compared against. This is the most falsifiable predicate in the system: transpose any
digit in a reversal and it fails, unconditionally, with no dependence on how the code
was written.

---

## 3. Question 2 — one posting table, two measures

**Decided: one table, `ledger.posting`, with a `measure` discriminator and two mutually
exclusive measure columns (`quantity numeric(24,8)`, `amount numeric(19,4)`). Not two
tables.**

The argument that settles it is not tidiness, it is detectability:

> A deferred constraint trigger fires per affected row. With one table, **every** group
> necessarily fires the trigger at least once, so "this group has quantity rows and no
> value rows at all" is a detectable state. With two tables, the trigger on
> `value_posting` never fires for a group that wrote no value rows — and *a completion
> that moved 500 screws and booked no cost whatsoever* is precisely the error you most
> need to catch. Splitting the tables makes the worst error the one error that is
> structurally invisible.

Supporting reasons, in descending weight:

1. **P3 spans the measures.** It needs quantity rows, value rows, and consumption edges
   under one index scan on `group_id`. Across two tables it is a cross-table deferred
   predicate that must be installed on both, with the empty-side problem above.
2. **Genealogy and cost layers are the same edges.** `consumption` references
   `posting_id`. One id space, one foreign key, one recursive CTE. Two tables means two
   id spaces and a polymorphic edge, which is the exact "two structures that must agree"
   pattern ADR 0004 exists to forbid.
3. **One grant story, one append-only story, one partitioning story, one audit story.**
   `PLAN.md` §6.3 wants insert and select and nothing else; that is one `GRANT`, not two
   that can drift.
4. **Value postings legitimately carry inventory dimensions.** A WIP charge names the
   item and lot it is charged to. Those columns are wanted on value rows anyway, so the
   "two tables have different shapes" argument is weaker than it looks.

Cost accepted: `NULL`s. A value row has no `location_id`; a quantity row has no
`account`. In PostgreSQL the null bitmap makes this nearly free, and the
`measure_shape` `CHECK` makes it impossible for a row to be half of each.

Rejected alternative — **a single `amount` column with a polymorphic unit** (`unit_id`
meaning UOM or currency depending on `measure`). Rejected because it defeats the type
system at the SQL level: nothing then prevents a currency id appearing in a quantity
slice, and `numeric(24,8)` for feet and `numeric(19,4)` for money have genuinely
different scale requirements that a shared column would have to widen to the union of
both, discarding the money-scale guarantee.

---

## 4. Question 3 — how a transformation is typed, and how the trigger dispatches

### 4.1 The group header

Group kind is **recorded, not inferred**. A `ledger.posting_group` header row exists per
group and carries the kind, the actor, the source document, the reason code, the work
order, and the reversal target.

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

### 4.2 The dispatch rule, so it cannot become unfalsifiable

Two rules, and they are the answer to the brief's sharpest question.

> **Rule D1 — dispatch is monotone. P0, P1, P2-A and P4 apply to every kind without
> exception. A kind may only ADD a predicate. No kind is ever exempted from the base
> law.**
>
> **Rule D2 — what varies by kind is not the predicate, it is the set of boundary
> accounts the kind may touch. The boundary is the constrained resource.**

D2 is the whole trick. `CONSUMED` and `PRODUCED` do make a transformation group balance
by construction — and that is fine, *because no other kind of group is allowed to use
them*. A receipt cannot reach for `CONSUMED` to absorb a shortfall. A cycle count
cannot reach for `PRODUCED` to conjure the bars it cannot find. The seam exists, it is
narrow, it is typed, it is named on every row, and stepping through it costs you the
value predicate.

The boundary matrix:

| kind | may touch | forbidden | adds |
|---|---|---|---|
| `MOVEMENT` | `SUPPLIER`, `CUSTOMER` | `SCRAP`, `ADJUSTMENT`, `ROUNDING`, `CONSUMED`, `PRODUCED` | **P2-B** (per cost element) |
| `ADJUSTMENT` | `SCRAP`, `ADJUSTMENT`, `ROUNDING` | `SUPPLIER`, `CUSTOMER`, `CONSUMED`, `PRODUCED` | `reason_code NOT NULL`; `ROUNDING` rows bounded by the item's dust tolerance |
| `TRANSFORMATION` | `CONSUMED`, `PRODUCED` | all external boundaries | `work_order_id NOT NULL`; every boundary-crossing quantity row needs its value counterpart (**P3**) |
| `VALUATION` | none | all | **no `QUANTITY` rows at all** |
| `REVERSAL` | whatever its target touched | — | **P4** (exact negation, pins everything) |

Because the kind is denormalised onto each posting row and **pinned by the composite
foreign key `(group_id, kind) → posting_group(group_id, kind)`**, the entire matrix is
an *immediate single-row* `CHECK`. It cannot lie, it cannot drift, and it fails at the
offending `INSERT` rather than at `COMMIT`. The deferred trigger is left holding only
the four genuinely multi-row predicates. This is the structural point worth
generalising: **composite foreign keys onto denormalised discriminators convert
cross-entity dispatch into single-row checks**, and this design uses the trick four
times — group kind, item stock UOM and scale, location boundary class, and WIP cost
object.

### 4.3 What separates a transformation from its cost intake

One modelling rule prevents `TRANSFORMATION` from becoming a dumping ground:

> **Value that moves *with* matter is posted in the matter's own group. Value that
> *enters* the system without matter — outside-processing service charges, absorbed
> labour and burden, landed cost, standard cost rolls, variance close — is a separate
> `VALUATION` group.**

This is why P2-B (per cost element) is enforceable on `MOVEMENT` at all: the return leg
of an outside-processing trip is a pure movement of the same cost elements, and the
passivation charge that adds an `OUTSIDE` element is a distinct `VALUATION` fact. It
also means a `VALUATION` group, which by construction has no matter, cannot quietly
alter quantities — `CHECK (kind <> 'VALUATION' OR measure = 'VALUE')`.

---

## 5. Question 4 — the schema and the trigger, as SQL

### 5.1 Supporting types and the kernel item registry

```sql
CREATE TYPE ledger.measure       AS ENUM ('QUANTITY','VALUE');
CREATE TYPE ledger.boundary      AS ENUM
  ('SUPPLIER','CUSTOMER','SCRAP','ADJUSTMENT','ROUNDING','CONSUMED','PRODUCED');
CREATE TYPE ledger.cost_element  AS ENUM ('MATERIAL','LABOR','BURDEN','OUTSIDE');
CREATE TYPE ledger.value_account AS ENUM
  ('INVENTORY','WIP','COGS','SCRAP_EXPENSE','ADJUSTMENT_EXPENSE',
   'AP_ACCRUAL','LABOR_ABSORBED','BURDEN_ABSORBED','PPV','MFG_VARIANCE','ROUNDING');

-- Kernel-owned registry. The items module writes it through a published interface;
-- the ledger never reads a module's tables (PLAN.md §6.6).
CREATE TABLE ledger.stock_item (
  item_id            uuid PRIMARY KEY,
  stock_uom_id       uuid     NOT NULL,
  stock_scale        smallint NOT NULL CHECK (stock_scale BETWEEN 0 AND 8),
  residual_tolerance numeric(24,8) NOT NULL CHECK (residual_tolerance >= 0),
  cost_method        text     NOT NULL CHECK (cost_method IN ('FIFO','MOVING_AVG','STANDARD')),
  UNIQUE (item_id, stock_uom_id, stock_scale, residual_tolerance)   -- FK target
);

CREATE TABLE ledger.location (
  location_id    uuid PRIMARY KEY,
  boundary_class ledger.boundary,          -- NULL = a real, owned, valued place
  UNIQUE (location_id, boundary_class)     -- FK target
);
```

### 5.2 The posting table

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
  amount            numeric(19,4),
  values_posting_id bigint,                 -- the QUANTITY row this money prices

  -- provenance. Recorded, audited, NEVER summed by any invariant.
  entered_quantity  numeric(24,8),
  entered_uom_id    uuid,
  conversion_factor numeric(38,18),
  unit_cost_applied numeric(19,8),

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

Three of those foreign keys do quiet, load-bearing work beyond referential integrity:

- `(item_id, uom_id, stock_scale, residual_tolerance) → stock_item` makes a **mixed-UOM
  posting unwritable** rather than merely detectable, and makes an item's stock UOM,
  scale and dust tolerance **immutable for as long as any posting references them** —
  `UPDATE ledger.stock_item SET stock_scale = 2` fails under `NO ACTION`. That single
  line closes the primer §9 corruption path: you cannot change the ruler mid-measurement.
- `(location_id, boundary) → location` makes the denormalised boundary class honest, so
  the boundary matrix can be an immediate `CHECK` instead of a join.
- `(group_id, cost_object_id) → posting_group(group_id, work_order_id)` stops a group
  charging WIP that belongs to a different work order. `MATCH SIMPLE` means it is simply
  skipped when `cost_object_id` is `NULL`, which is exactly the wanted semantics.

### 5.3 The consumption edge — cost layers and genealogy, one structure

```sql
CREATE TABLE ledger.consumption (
  consuming_posting_id bigint NOT NULL REFERENCES ledger.posting (posting_id),
  consumed_posting_id  bigint NOT NULL REFERENCES ledger.posting (posting_id),
  group_id             uuid   NOT NULL REFERENCES ledger.posting_group (group_id),
  quantity             numeric(24,8) NOT NULL CHECK (quantity <> 0),
  amount               numeric(19,4) NOT NULL,
  PRIMARY KEY (consuming_posting_id, consumed_posting_id),
  CONSTRAINT no_self_consumption CHECK (consuming_posting_id <> consumed_posting_id)
);
CREATE INDEX consumption_forward_idx ON ledger.consumption (consumed_posting_id);
CREATE INDEX consumption_group_idx   ON ledger.consumption (group_id);
```

Signed quantity, not positive-only, so a `REVERSAL` restores a layer with negative edges
rather than needing a second mechanism. A layer's remaining quantity is **derived**:
`posting.quantity - COALESCE(SUM(consumption.quantity WHERE consumed = posting), 0)`.
Nothing is stored. See §6.

**Genealogy.** Backward traversal is a recursive CTE down `consumed_posting_id`; forward
traversal is the same CTE up the reverse index. Lot and serial ride on the posting rows,
so a lot trace is a traversal filtered by `lot_id`. Because P3 makes an edge mandatory
for every withdrawal, **the graph is total** — no posting is an orphan.

**Cost methods, all three over this one table.** FIFO chooses the oldest un-exhausted
receipt postings. Moving average allocates pro-rata across all open layers in the
valuation pool, which makes the unit cost the pool average by construction. Standard
cost allocates at layer quantity but at standard `amount`, and posts the difference to
`PPV` in the same group — P2-A still holds. The method is a per-item property in
`stock_item`; the invariant does not know or care which was used, which is the point.

### 5.4 The deferred constraint trigger

```sql
CREATE OR REPLACE FUNCTION ledger.enforce_group_invariants()
RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = ledger, pg_catalog
AS $fn$
DECLARE
  gid uuid := COALESCE(NEW.group_id, OLD.group_id);
  g   ledger.posting_group%ROWTYPE;
  bad record;
BEGIN
  SELECT * INTO g FROM ledger.posting_group WHERE group_id = gid;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'ledger: group % has postings but no header', gid
      USING ERRCODE = 'ZL000';
  END IF;

  ------------------------------------------------------------------ P0
  PERFORM 1 FROM ledger.posting
    WHERE group_id = gid AND created_xid <> g.created_xid LIMIT 1;
  IF FOUND THEN
    RAISE EXCEPTION 'ledger: group % was extended by a later transaction', gid
      USING ERRCODE = 'ZL001',
            HINT = 'a posting group is one atomic fact; post a new group instead';
  END IF;

  ------------------------------------------------------------------ P1
  SELECT p.item_id, p.uom_id, sum(p.quantity) AS net INTO bad
    FROM ledger.posting p
   WHERE p.group_id = gid AND p.measure = 'QUANTITY'
   GROUP BY p.item_id, p.uom_id
  HAVING sum(p.quantity) <> 0
   LIMIT 1;
  IF FOUND THEN
    RAISE EXCEPTION
      'ledger: quantity not conserved in % group %: item % uom % nets %',
      g.kind, gid, bad.item_id, bad.uom_id, bad.net
      USING ERRCODE = 'ZL002';
  END IF;

  ------------------------------------------------------------------ P2-A
  SELECT p.currency_id, sum(p.amount) AS net INTO bad
    FROM ledger.posting p
   WHERE p.group_id = gid AND p.measure = 'VALUE'
   GROUP BY p.currency_id
  HAVING sum(p.amount) <> 0
   LIMIT 1;
  IF FOUND THEN
    RAISE EXCEPTION
      'ledger: value not conserved in % group %: currency % nets %',
      g.kind, gid, bad.currency_id, bad.net
      USING ERRCODE = 'ZL003';
  END IF;

  ------------------------------------------------------------------ P2-B (MOVEMENT only)
  IF g.kind = 'MOVEMENT' THEN
    SELECT p.currency_id, p.cost_element, sum(p.amount) AS net INTO bad
      FROM ledger.posting p
     WHERE p.group_id = gid AND p.measure = 'VALUE'
     GROUP BY p.currency_id, p.cost_element
    HAVING sum(p.amount) <> 0
     LIMIT 1;
    IF FOUND THEN
      RAISE EXCEPTION
        'ledger: MOVEMENT group % reclassifies cost element % by % (currency %)',
        gid, bad.cost_element, bad.net, bad.currency_id
        USING ERRCODE = 'ZL004',
              HINT = 'a movement carries cost, it does not re-elementise it; '
                     'post the intake as a separate VALUATION group';
    END IF;
  END IF;

  ------------------------------------------------------------------ P3
  IF g.kind IN ('MOVEMENT','ADJUSTMENT','TRANSFORMATION') THEN
    SELECT p.posting_id, p.quantity, e.qty, e.amt, v.amt AS vamt INTO bad
      FROM ledger.posting p
      CROSS JOIN LATERAL (
        SELECT COALESCE(sum(c.quantity), 0) AS qty,
               COALESCE(sum(c.amount),   0) AS amt
          FROM ledger.consumption c
         WHERE c.consuming_posting_id = p.posting_id
      ) e
      CROSS JOIN LATERAL (
        SELECT COALESCE(sum(x.amount), 0) AS amt
          FROM ledger.posting x
         WHERE x.group_id = gid
           AND x.measure  = 'VALUE'
           AND x.values_posting_id = p.posting_id
      ) v
     WHERE p.group_id = gid
       AND p.measure  = 'QUANTITY'
       AND p.boundary IS NULL          -- left a real, owned, valued location
       AND p.quantity < 0
       AND (e.qty <> -p.quantity OR e.amt <> -v.amt)
     LIMIT 1;
    IF FOUND THEN
      RAISE EXCEPTION
        'ledger: posting % in group % withdraws %, but cost layers account for % '
        'quantity and % value against % posted',
        bad.posting_id, gid, bad.quantity, bad.qty, bad.amt, bad.vamt
        USING ERRCODE = 'ZL005',
              HINT = 'every withdrawal must name the postings it came out of';
    END IF;
  END IF;

  ------------------------------------------------------------------ P4
  IF g.kind = 'REVERSAL' THEN
    SELECT * INTO bad FROM (
      SELECT measure, item_id, uom_id, location_id, boundary, lot_id, serial_id,
             account, cost_element, cost_object_id, currency_id,
             COALESCE(sum(quantity), 0) AS q,
             COALESCE(sum(amount),   0) AS a
        FROM ledger.posting
       WHERE group_id IN (gid, g.reverses_group_id)
       GROUP BY 1,2,3,4,5,6,7,8,9,10,11
      HAVING COALESCE(sum(quantity), 0) <> 0 OR COALESCE(sum(amount), 0) <> 0
    ) t LIMIT 1;
    IF FOUND THEN
      RAISE EXCEPTION
        'ledger: reversal % is not the exact negation of %: residual qty % value %',
        gid, g.reverses_group_id, bad.q, bad.a
        USING ERRCODE = 'ZL006';
    END IF;

    PERFORM 1 FROM (
      SELECT c.consumed_posting_id
        FROM ledger.consumption c
       WHERE c.group_id IN (gid, g.reverses_group_id)
       GROUP BY c.consumed_posting_id
      HAVING sum(c.quantity) <> 0 OR sum(c.amount) <> 0
    ) t;
    IF FOUND THEN
      RAISE EXCEPTION
        'ledger: reversal % does not restore the cost layers consumed by %',
        gid, g.reverses_group_id
        USING ERRCODE = 'ZL007';
    END IF;
  END IF;

  RETURN NULL;
END;
$fn$;

CREATE CONSTRAINT TRIGGER posting_group_invariants
  AFTER INSERT OR UPDATE OR DELETE ON ledger.posting
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW
  EXECUTE FUNCTION ledger.enforce_group_invariants();

CREATE CONSTRAINT TRIGGER consumption_group_invariants
  AFTER INSERT OR UPDATE OR DELETE ON ledger.consumption
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW
  EXECUTE FUNCTION ledger.enforce_group_invariants();
```

Points I have reasoned about and want on the record:

- **`UPDATE OR DELETE` is included even though the grant forbids both.** `PLAN.md` §6.3
  denies the application role those verbs; the trigger covers the migration and superuser
  paths, where the guarantee matters most and supervision is weakest.
- **The second trigger, on `consumption`, is not optional.** Without it, a transaction
  that inserts only edges and no new postings would never fire the check — and P3 is
  exactly the predicate that edges can break.
- **Constraint triggers cannot use transition tables.** The sweep is right: PostgreSQL
  permits `REFERENCING NEW TABLE` only on `AFTER` triggers that are *not* constraint
  triggers. So this is row semantics and the aggregate runs once per affected row. At
  2–20 rows per group over an index on `group_id`, that is tens of microseconds.
  **Accepted for v1.** The memoisation trick — a transaction-local GUC holding
  already-validated group ids via `set_config(..., is_local => true)` — is documented as
  an optimisation to be applied only if `spike-ledger-constraint` measures it as
  material, because it introduces a path on which the check is *skipped*, and a
  constraint with a skip path is a constraint with a bug waiting in it.
- **`SECURITY DEFINER` with a pinned `search_path`**, so the function reads the real
  tables regardless of the caller's role or schema path.
- **Custom SQLSTATEs in class `ZL`.** Class letters `I` through `Z` are available for
  implementation-defined conditions, so these cannot collide with a PostgreSQL code.
  `wicket-ledger` maps `ZL000`–`ZL007` to distinct typed errors. SQLx surfaces them on
  `tx.commit()`, which means **every multi-posting write must use an explicit
  transaction** — the autocommit path can swallow a deferred failure. That is a mandated
  integration test, not a code-review item.
- **Concurrency.** Two transactions writing the same `group_id` would each see a partial
  group. This is prevented upstream: `group_id` is minted per atomic business transaction
  by a single writer, and P0 turns any violation into a hard failure rather than a silent
  one.
- **Partitioning.** When time partitioning arrives (ADR 0004's 50M-row revisit), all
  postings of a group must land in one partition. `created_xid` is set once per group and
  is the natural key; `posted_at` must not be used, because `clock_timestamp()` can
  straddle a boundary mid-transaction.
- **PostgreSQL 15–17 compatibility.** Everything above is available in 15: `xid8` and
  `pg_current_xact_id()` since 13, composite and partial-unique foreign-key targets,
  `LATERAL`, `round(numeric, int)` as an immutable function usable in `CHECK`, deferrable
  constraint triggers, and `RAISE ... USING ERRCODE`. No feature introduced after 13 is
  used, and no feature deprecated by 17 is used.

---

## 6. Question 5 — does "no stored balances" survive?

**Yes, intact, with no exception and no carve-out. `PLAN.md` §6.1 stands as written.**

The audit, item by item, because this is the invariant most likely to be eroded by
accident:

| Candidate | Is it a stored balance? | Verdict |
|---|---|---|
| `consumption.quantity` / `.amount` | No. It records *an allocation event* — this withdrawal took this much from that layer. It is never updated. A layer's remaining quantity is `layer.quantity - SUM(edges)`, computed. | Allowed |
| `posting.unit_cost_applied` | No. It is the price that *was* applied at that instant — provenance, the same category as `entered_quantity`, re-derivable from the group's own value rows. A fact about a past event, not a running total. | Allowed |
| `posting.values_posting_id` | A link, not a number. | Allowed |
| `posting.stock_scale`, `residual_tolerance`, `boundary`, `kind` | Denormalised keys, every one pinned by a composite FK to its source of truth. They cannot drift, by construction. | Allowed |
| Balance projections | Explicitly caches, per ADR 0004. Rebuildable, reconciled by a scheduled job, ledger wins on disagreement. | Allowed, already decided |
| A `layer.remaining_qty` column | **Yes.** Rejected. This is the exact place ERPs put mutable state and the exact place it rots. | **Forbidden** |
| A `work_order.wip_value` column | **Yes.** Rejected — see below. | **Forbidden** |
| An `item.moving_average_cost` column | **Yes.** Rejected. The current average is `SUM(amount)/SUM(quantity)` over the valuation pool and belongs in the projection cache, nowhere else. | **Forbidden** |

**Work-in-process value specifically, since the brief asks:** it needs **no stored
element**. WIP value for work order *W* is

```sql
SELECT sum(amount)
  FROM ledger.posting
 WHERE measure = 'VALUE' AND account = 'WIP' AND cost_object_id = W;
```

and the variance at close is that residual, computed and then posted explicitly by a
`VALUATION` group that drives it to zero. The thing that makes this work without stored
state is `cost_object_id` — a *dimension*, not a balance — and the composite FK that
stops it naming the wrong work order.

The distinction to teach in the module authoring guide is exactly this one: **a
dimension on an immutable row is free; a total on a mutable row is forbidden. The
question is never "is this a number", it is "does anything ever UPDATE it".** Nothing
above is ever updated.

---

## 7. Question 6 — rounding, precision, and where the residual goes

The primer calls this the thing that "corrupts inventory quietly for months", and the
plan says nothing about it. Here is the rule, in seven parts.

### R1 — one canonical measure, declared per item, exact on every row

Every item declares `stock_uom_id`, `stock_scale` (0–8) and `residual_tolerance` in
`ledger.stock_item`. Storage is `numeric(24,8)`; **the posted quantity must be exactly
representable at `stock_scale`**, enforced by the immediate single-row
`quantity_exact_at_scale` `CHECK`. A bar stocked in feet at scale 4 cannot carry a
posting of `0.58333333 FT`. Ever. On any code path.

### R2 — conversion happens exactly once, before any posting exists

`wicket-uom` converts the entered value to canonical units at the API boundary, with
**round-half-even** at `stock_scale`. What the operator typed is preserved as
`entered_quantity`, `entered_uom_id` and `conversion_factor` — audited provenance, never
summed by any invariant. The kernel owns this, not a module, per
`docs/02-architecture.md` §2.

### R3 — the same canonical number appears on both sides, always

Within a group, the converted value is used for the posting *and* its counterpart.
**Rounding therefore can never cause a group to fail P1.** This is deliberate, and it is
the one place I am arguing *against* strictness: an invariant that fails intermittently
on a rounding edge is an invariant that gets a `SET CONSTRAINTS ... IMMEDIATE` escape
hatch bolted on within six months, and then it protects nothing. The constraint must be
absolute or it will be negotiated away.

### R4 — so where does the residual actually go?

**Nowhere inside a group. It lives in the balance, and that is the honest answer.**

Issue 7 inches from a bar stocked in feet at scale 4: `7/12 = 0.58333…`, posted as
`0.5833 FT`. The group is perfectly balanced; the *physical bar* is 0.0000333 FT shorter
than the ledger believes. Do it 171 times and the ledger believes in 0.0057 FT of bar
that does not exist. Every group was valid. The balance is wrong.

No group-level constraint can catch this, because the error is not in any group — it is
in the *sum* of correct groups. Any design claiming otherwise is lying. What the database
**can** do is guarantee the residual is small, bounded, and cannot be used to hide
something real:

- The `ROUNDING` boundary is reachable **only** from an `ADJUSTMENT` group
  (`boundary_permitted`), which **must** carry a reason code (`reason_required`), and
  `wicket-ledger` restricts that reason code to `UOM_CONVERSION_RESIDUAL`.
- `rounding_is_dust` caps `abs(quantity)` on any `ROUNDING` row at the item's
  `residual_tolerance`. **Dust must actually be dust.** Writing off three whole bars as a
  rounding residual is rejected by the database, and whoever tried is forced to name a
  real reason code — which is precisely the moment a real problem becomes visible instead
  of disappearing.
- Because `residual_tolerance` is part of the composite FK to `stock_item`, it cannot be
  quietly raised to cover an accumulating problem while postings reference the old value.

### R5 — what stops the residual from being *systematic*

Drift that averages out is a nuisance. Drift that is always in one direction is
corruption, and it is caused by the receipt and the issue using *different conversion
factors*. Two rules, both enforceable:

- **Item stock UOM, scale and tolerance are immutable while postings exist**, by the
  composite FK. You cannot change the ruler mid-measurement.
- **Item- and lot-scoped conversion factors are pinned at receipt.** Where a factor
  depends on the physical lot — pounds per foot for a bar heat lot, the primer's own
  example — the factor is captured on the lot at receipt and recorded as
  `conversion_factor` on every posting for that lot. Issue and receipt cannot disagree,
  because there is only one factor and it is written down. Factors are versioned with
  effectivity, and a version change cannot apply retroactively to an existing lot.

### R6 — value rounding is the opposite case, and is enforced

Money is `numeric(19,4)`. Allocating a $47.20 receipt across three lots gives $15.7333
each, which does not sum to the whole. **Value residuals are in-group and therefore the
database does catch them.** The allocator uses largest-remainder so the parts sum exactly;
where a residual of one minor unit survives, it is posted to the `ROUNDING` value account
**inside the same group**, and P2-A fails if it is dropped.

This asymmetry is the crux of the section and belongs verbatim in the authoring guide:

> **A value residual is a discrepancy between postings, so the database enforces it. A
> quantity residual is a discrepancy between the postings and the physical world, so the
> database bounds it and a cycle count finds it.**

### R7 — detection, honestly labelled

A scheduled job reports every item, location and lot whose derived balance is non-zero,
below `residual_tolerance`, and unmoved for N days. That is the dust report. It is a
**report, not a constraint**, and it is named as such so nobody mistakes it for a
guarantee. Its output drives an `ADJUSTMENT` group with reason
`UOM_CONVERSION_RESIDUAL`, which is the only thing permitted to touch `ROUNDING`.

---

## 8. The acceptance cases (a) through (l)

Worked example. `BAR`: stocked in `FT`, scale 4, tolerance 0.0100 FT, FIFO, $2.36/FT;
purchased as `BAR` at 20 FT each, an exact conversion. `SCREW`: stocked in `EA`, scale 0,
tolerance 0, standard cost $0.25. `Q` = quarantine, `A` = available, `FG` = finished
goods. Signs are on `quantity` for Q rows and on `amount` for V rows.

| # | Case | kind | Postings (Q = quantity, V = value) | consumption edges | Proved by | Transposed digit → |
|---|---|---|---|---|---|---|
| **a** | Receive 100 bars against a PO, into quarantine | `MOVEMENT` | Q: −2000.0000 FT BAR @`SUPPLIER`; +2000.0000 FT BAR @Q · V: +4720.00 `INVENTORY`/MAT; −4720.00 `AP_ACCRUAL`/MAT | none — a receipt creates a layer, it consumes none | P1, P2-A, P2-B | `+0200.0000` on the Q row → P1 nets −1800 → **ZL002** |
| **b** | Release quarantine → available after inspection passes | `MOVEMENT` | Q: −2000.0000 FT @Q; +2000.0000 FT @A · V: −4720.00 `INVENTORY`/MAT (values the Q row); +4720.00 `INVENTORY`/MAT (values the A row) | −2000.0000 FT / −$4720.00 consuming the receipt layer; the A row becomes the new layer | P1, P2-B, **P3** | drop the `INVENTORY` credit → P2-A nets +4720 → **ZL003**; state 2000 but allocate layers for 200 → **ZL005** |
| **c** | Issue one bar to WO-1041 | `MOVEMENT` | Q: −20.0000 FT @A; +20.0000 FT @`WIP-1041` · V: −47.20 `INVENTORY`/MAT; +47.20 `WIP`/MAT, `cost_object_id`=WO-1041 | 20.0000 FT / $47.20 from the oldest open BAR layer | P1, P2-B, **P3**, WIP cost-object FK | `−20.0000` against `+02.0000` → **ZL002**; layers allocated for 2 FT while the row says 20 → **ZL005**; charging WO-1042's WIP → composite FK rejects at the INSERT |
| **d** | Complete 500 screws from WO-1041 | `TRANSFORMATION` | Q: −20.0000 FT BAR @`WIP-1041`; +20.0000 FT BAR @`CONSUMED` · Q: +500 EA SCREW @FG; −500 EA SCREW @`PRODUCED` · V: −47.20 `WIP`/MAT (values the BAR row); −60.00 `WIP`/LABOR; −30.00 `WIP`/BURDEN; +125.00 `INVENTORY`/MAT (values the SCREW row); +12.20 `WIP`/MAT — the variance that stays behind | 20.0000 FT / $47.20 against the WIP-1041 bar layer | P1 on both slices via the seam, **P2-A**, **P3**, boundary matrix | `+125.00` typed as `+152.00` → P2-A nets +27 → **ZL003**. This is the case the seam is built for: quantity is *permitted* to change identity, value is not, so the value predicate carries the whole load and still catches the digit |
| **e** | Scrap 12 screws at an operation, reason coded | `ADJUSTMENT` | Q: −12 EA @FG; +12 EA @`SCRAP` · V: −3.00 `INVENTORY`/MAT (values the FG row); +3.00 `SCRAP_EXPENSE`/MAT · header `reason_code='SCRAP_AT_OP_30'` | 12 EA / $3.00 against the completion layer | P1, P2-A, **P3**, `reason_required` | omit the reason → header CHECK rejects immediately; `−12` against `+21` → **ZL002**; scrap 12 but expense $30.00 while the layers say $3.00 → **ZL005** |
| **f** | Cycle count finds three fewer bars than the system believes | `ADJUSTMENT` | Q: −60.0000 FT @A; +60.0000 FT @`ADJUSTMENT` · V: −141.60 `INVENTORY`/MAT; +141.60 `ADJUSTMENT_EXPENSE`/MAT · `reason_code='CYCLE_COUNT_SHORT'` | 60.0000 FT / $141.60 across open A layers, FIFO | P1, P2-A, **P3**, `reason_required` | `−60.0000` against `+06.0000` → **ZL002**; posting it as a `MOVEMENT` to dodge the reason code → `boundary_permitted` rejects `ADJUSTMENT` for kind `MOVEMENT` |
| **g** | Ship 400 screws to a customer | `MOVEMENT` | Q: −400 EA @FG; +400 EA @`CUSTOMER` · V: −100.00 `INVENTORY`/MAT; +100.00 `COGS`/MAT, `cost_object_id`=SO | 400 EA / $100.00 against the completion layer | P1, P2-A, P2-B, **P3** | `−400` against `+040` → **ZL002**; ship 400 but relieve layers for 40 → **ZL005** |
| **h** | Customer returns 10 screws, into quarantine | `MOVEMENT` | Q: −10 EA @`CUSTOMER`; +10 EA @Q · V: −2.50 `COGS`/MAT; +2.50 `INVENTORY`/MAT (values the Q row) | +10 EA / +$2.50, a signed edge restoring the shipped layer | P1, P2-A, P2-B | booking the return at $25.00 against a $2.50 relief → P2-A nets 22.50 → **ZL003** |
| **i** | Rework recovers 8 of the 12 scrapped screws | `ADJUSTMENT` **+** `VALUATION` | Q: −8 EA @`SCRAP`; +8 EA @`WIP-1041` · V: −2.00 `SCRAP_EXPENSE`/MAT; +2.00 `WIP`/MAT, `cost_object_id`=WO-1041 · `reason_code='REWORK_RECOVERY'`, `source_id` = the scrap group · separate `VALUATION` for the rework labour: +6.00 `WIP`/LABOR; −6.00 `LABOR_ABSORBED`/LABOR | −8 EA / −$2.00, un-consuming part of the scrap allocation | P1, P2-A, `reason_required`, `valuation_moves_no_matter` | recover 8 but credit `SCRAP_EXPENSE` for 12 → P2-A nets −1.00 → **ZL003**. Recovering *more than was scrapped* is a balance error, not a group error — see §9 |
| **j** | Outside processing: 500 screws leave for passivation and return | **two** `MOVEMENT` **+ one** `VALUATION` | out — Q: −500 EA @FG; +500 EA @`OSP-VENDOR-7`, a **real owned location** with `boundary_class` NULL, because the goods are still yours · V: −125.00 / +125.00 `INVENTORY`/MAT · back — the mirror · charge — `VALUATION`: +150.00 `INVENTORY`/OUTSIDE; −150.00 `AP_ACCRUAL`/OUTSIDE | full edges on each leg; both are withdrawals from real locations | P1, **P2-B**, **P3** | folding the $150 passivation charge into the return movement → P2-B sees `OUTSIDE` net +150 inside a `MOVEMENT` → **ZL004**. This case is why P2-B exists: it forces the service cost to be its own recorded fact instead of vanishing into the material cost of the screws |
| **k** | Buy bar by the foot, issue by the inch, residual does not divide evenly | `MOVEMENT`, later `ADJUSTMENT` | issue — Q: −0.5833 FT @A; +0.5833 FT @`WIP-1041`, with `entered_quantity=7`, `entered_uom_id=IN`, `conversion_factor=0.083333333333333333` · V: ∓1.38 · later, dust flush — `ADJUSTMENT`: Q: −0.0057 FT @A; +0.0057 FT @`ROUNDING`, `reason_code='UOM_CONVERSION_RESIDUAL'` | 0.5833 FT / $1.38 on the issue; 0.0057 FT on the flush | P1, **R1**, **R4** | posting `0.58333333 FT` → `quantity_exact_at_scale` rejects **at the INSERT**, not at commit; flushing 60.0000 FT (three bars) as "rounding" → `rounding_is_dust` rejects, because 60 > 0.0100, forcing a real reason code |
| **l** | Entry (c) was wrong and must be reversed without deleting anything | `REVERSAL` | the exact negation of (c): Q: +20.0000 FT @A; −20.0000 FT @`WIP-1041` · V: +47.20 `INVENTORY`/MAT; −47.20 `WIP`/MAT, `cost_object_id`=WO-1041 · header `reverses_group_id` = (c) | −20.0000 FT / −$47.20, restoring the exact layer (c) consumed | **P4** | any digit anywhere → the union of the two groups fails to net zero on that dimension tuple → **ZL006**; restoring a *different* layer → **ZL007**; reversing (c) twice → the partial unique index rejects; reversing the reversal → `no_reversal_of_reversal` rejects |

Note what case (d) demonstrates, since it is the one the brief was really asking about.
Quantity conservation there *is* satisfied by construction through the `CONSUMED` and
`PRODUCED` seam, exactly as predicted — and it does not matter, because the seam costs
you P2-A and P3 in exchange. The bar cannot enter `CONSUMED` without its $47.20 leaving
WIP (P3), the screws cannot leave `PRODUCED` without $125.00 entering inventory (P3), and
the $12.20 that was neither consumed nor produced must be named as WIP variance or the
group is rejected (P2-A). **The seam is not free; it is priced in money.**

---

## 9. What this does not catch, stated so nobody is surprised later

Honesty here is worth more than a stronger-sounding claim.

1. **A consistently wrong magnitude.** An operator scans 550 where 500 is physical, and
   every derived number in the group derives from 550. No group invariant catches this,
   because there is no disagreement to detect. The redundant source is the *source
   document*, and over-receipt and over-completion tolerances are application checks
   against the PO's or WO's open quantity. Deliberately **not** a ledger invariant: the
   ledger must be able to record an over-receipt, because over-receipts happen.
2. **P3's teeth depend on an architectural obligation, not on SQL.** The predicate is
   only load-bearing while the movement quantity and the layer allocation come from two
   computations. **Binding on `wicket-ledger`:** the movement request states the quantity;
   the allocator independently resolves it against open layers and returns *its own*
   total; the posting builder never derives one from the other. If a future refactor
   makes the allocator echo the request, P3 degenerates to a checksum. Tag the test
   `p3_independent_sources` and make it a deep-audit item.
3. **Cross-group balance errors.** A negative on-hand, a rework recovering more than was
   scrapped, dust accumulation — all are properties of a *sum of groups*. They are caught
   by projections, allocation checks and the cycle count, and they are reports and
   policies, not constraints. §7 R4 says this plainly and I am not going to dress it up.
4. **Concurrent writers on one `group_id`.** Prevented by construction upstream: one
   group per atomic business transaction, one writer. P0 converts a violation into a hard
   failure rather than a silent partial group, which is the most the database can do.

---

## 10. Consequences for downstream lanes

- **`wicket-uom`** owns conversion, half-even rounding at `stock_scale`, lot-pinned
  factors, and effectivity-versioned factor history. It is already on `wicket-ledger`'s
  dependency edge (`PLAN.md` §5). No new edges.
- **`wicket-core`** takes the `sweep-plan-typed-qty` recommendation as-is: phantom
  **dimension**, runtime `UnitId`, `Money` as its own type. Nothing here needs
  `Quantity<Foot>`. `PLAN.md` §5's sentence should be amended by that lane's decision,
  not by this one.
- **`wicket-ledger`** owns `posting_group`, `posting`, `consumption`, `stock_item`,
  `location`, the trigger, and the `ZL000`–`ZL007` mapping to typed errors. Every
  multi-posting write uses an explicit SQLx transaction and asserts on `commit()`.
- **Property tests (`PLAN.md` §7)** generate per **slice**, never a group-wide scalar.
  Required shards: `p0_atomicity`, `p1_quantity`, `p2_value`, `p2b_cost_element`,
  `p3_coupling`, `p3_independent_sources`, `p4_reversal`, `boundary_matrix`,
  `scale_exactness`, `dust_bounded`. The generator must produce *deliberately corrupted*
  groups — one transposed digit, one dropped row, one wrong boundary — and assert the
  exact SQLSTATE. A property suite that only generates valid groups proves nothing.
- **`spike-ledger-constraint`** measures: duplicate-firing cost at 20 rows per group on
  PG 17; P3's lateral plan at 10M postings; SQLx deferred-failure surfacing on
  `commit()`; the GUC memo, *only* if duplicate firings measure as material.
- **`docs/05-data-model.md`** must land the slice definitions, the boundary matrix and §7
  verbatim before any `wicket-ledger` migration is written.
- **`docs/03-module-system.md` authoring guide** teaches three rules: a dimension on an
  immutable row is free and a total on a mutable row is forbidden; value that moves with
  matter goes in matter's group while value that enters the system gets its own; and the
  seam is priced in money.

---

## 11. Decision record

| Question | Decision |
|---|---|
| 1. Slices | P0 group atomicity; P1 `(group, item, uom)` quantity = 0; P2-A `(group, currency)` value = 0; P2-B `(group, currency, cost_element)` = 0 for `MOVEMENT`; P3 per-withdrawal quantity/value/layer coupling; P4 exact reversal |
| 2. One table or two | **One** `ledger.posting`, two measures, discriminated — because with two tables "the value side is entirely absent" is the one error that is structurally undetectable |
| 3. Transformation typing | `posting_group.kind` recorded and constrained, pinned onto every posting by composite FK. Dispatch is **monotone** — kinds only add predicates — and varies by **which boundary accounts a kind may touch**, not by which law applies |
| 4. The trigger | `CREATE CONSTRAINT TRIGGER … DEFERRABLE INITIALLY DEFERRED FOR EACH ROW` on `posting` **and** on `consumption`; §5.4 |
| 5. No stored balances | **Survives intact.** WIP value needs no stored element: `SUM(amount) WHERE account='WIP' AND cost_object_id = W`. A layer's remaining quantity is derived from an append-only edge table |
| 6. Rounding | Canonical UOM per item, exact at `stock_scale` by immediate CHECK; conversion once at the boundary, half-even, provenance recorded; **the quantity residual lives in the balance, bounded by `rounding_is_dust`, reachable only from a reason-coded `ADJUSTMENT`, and found by cycle count**; **the value residual lives in the group and is enforced by P2-A** |
