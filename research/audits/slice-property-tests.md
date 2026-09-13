# Plan-audit slice 6: Property tests on the ledger

**Role:** adversarial researcher  
**Date:** 2026-09-11  
**Scope:** PLAN.md §7, docs/02-architecture.md §9, ADR 0004, primer bone-screw walk, catalog Phase 1. Cross-read: slice 1 (`sweep-plan-ledger.md`), slice 5 (`sweep-plan-typed-qty.md`), ADR 0007, docs/03 §3.2 (hooks contribute postings).  
**Product files:** not edited.

---

## Executive verdict

PLAN §7 names “property tests on `wicket-ledger`” as the highest-value test in the project and then **stops**. Architecture §9 repeats the slogan (“groups sum to zero, projections equal the ledger sum”) and defers “specifics to a later document.” That later document does not exist. **This is a missing acceptance criterion, not a testing-style preference.** A Wave 2 lane that ships `proptest` over `Vec<i32>` and asserts `group.iter().map(|p| p.qty).sum() == 0` will be green, will not catch a BOM completion, and will not be the test the architecture is betting the company on.

What a property test on this ledger **actually** asserts is a **state-machine lockstep** against a pure model:

1. Every **committed** group balances under the **sliced** invariant (slice 1 + slice 5), not a scalar `SUM(quantity)` over `group_id`.
2. After every prefix of a generated sequence, the live projection equals `SUM(postings)` on the same grain.
3. Rebuild-from-empty equals the live projection.
4. A **reversing group** (append-only; never a `DELETE`) restores the pre-group projection on the affected grain.
5. The posting table is physically append-only (grants + trigger).
6. `posted_at` is server-monotonic per session and never taken from the client.
7. Genealogy: every unit sitting in a **physical** location traces to `SUPPLIER` or `PRODUCTION`; the posting graph has no cycles except those introduced by reverse groups.
8. Concurrent interleaving on overlapping items still commits only balanced groups and never shows a torn projection. **This is not a proptest.** It is a Postgres isolation test (and, for the in-memory engine, a loom/shuttle test of projection locks).

A naive `Arbitrary` over `Vec<Op>` misses the domain. The generator that is worth writing is a **preconditioned state machine** whose shrinker is **accounting-valid** (drop a suffix, or replace a committed group with `Reverse(group_id)` — never delete a prefix posting while keeping its dependents).

**Opus DECISION required** (with slice 1, before Wave 1 freezes the stub API): the balance **slice keys**, whether inventory is normalized to stocking UoM before insert, and whether lot/serial are identity dimensions (they must **not** be conservation keys, or `SplitLot` / `SerializeLot` are illegal). This report assumes the slice-1 narrowing and states it as the SPEC default.

---

## 1. The real invariant (coordinate with slice 1 / 5)

Slice 1’s forcing case: one `group_id`, one `SUM(quantity)` across a WO completion (`−4` screws, `−1` housing, `+1` assembly) **cannot be zero**. Virtual locations fix *appearance from outside the shop*; they do not make unlike items add.

**Conservation keys (must sum to 0 inside a committed group):**

| Ledger | Slice | Measure |
|--------|--------|---------|
| `inventory` | `(group_id, item_id, uom_id)` | `SUM(quantity) = 0` |
| `cost` | `(group_id, ledger=cost, currency_id)` — tighten to `cost_element` if opus says so | `SUM(amount) = 0` |
| `labor` | `(group_id, ledger=labor)` — tighten to `(work_order_id)` if opus says so | `SUM(hours_or_amount) = 0` |

**Not conservation keys:** `location_id` (that is the transfer), `lot_id`, `serial_id` (identity / genealogy). If the SQL trigger groups by lot or serial, `SplitLot` and `SerializeLot` are rejected at COMMIT and the Phase 1 catalog is a lie.

**Locations close the world.** Every inventory group is a set of signed rows whose per-`(item, uom)` net is zero because goods move between locations, including virtual:

| Virtual location | Closes |
|------------------|--------|
| `SUPPLIER` | Receipt (goods enter) |
| `CUSTOMER` | Ship (goods leave) |
| `SCRAP` | Scrap / yield / kerf |
| `ADJUSTMENT` | Cycle count / adjust; **reason required** |
| `WIP` (per work order) | Issue to job; residual after close is variance, moved to `VARIANCE` or `SCRAP` |
| `PRODUCTION` | Finished-good completion (the manufactured analog of `SUPPLIER`) |

**Canonical UoM (slice 1 + 5):** convert in `wicket-uom` **before** insert. The SQL trigger never adds feet to inches. Mixed-UoM rows in one `(group, item)` must fail (either at the engine or as two unbalanced slices at COMMIT).

**PLAN amendment (one sentence to paste into §6.2 / ADR 0004):**

> Every committed posting group balances to zero **per balance slice**: inventory per `(group_id, item_id, uom_id)`; cost and labor per `(group_id, ledger)` (finer keys TBD). Lot and serial are identity dimensions, not conservation keys. Corrections are reversing groups, never updates or deletes.

Until that sentence is in the ADR, the “highest-value test” has nothing true to assert.

---

## 2. Acceptance criteria (paste into `wicket-ledger` SPEC)

Copy from here through the end of §2. Numbered so an audit can fail a lane by AC id.

### AC-L0 — Suite shape

- The suite is a **state-machine property test** (recommended: `proptest` + `proptest-state-machine`, or equivalent `proptest-stateful`) plus a **small set of explicit scenarios** (bone-screw walk, empty group, single-sided posting).
- Two profiles, both required: `WellFormed` (every op is a balanced group; on-hand may go negative) and `Physical` (preconditions: never issue/move/ship/scrap more than on-hand at the source grain).
- Golden-file costing and MRP **are forbidden** in this crate. They belong to later modules.

### AC-P1 — Balance (rank 1)

- For every committed group `g` in a generated sequence:
  - inventory: `∀ (item, uom): SUM(quantity) = 0`
  - cost/labor: `∀ ledger ∈ {cost, labor}: SUM(amount) = 0` (or the opus-tightened key)
- An **unbalanced** group is rejected at **`COMMIT`**, not at the first `INSERT`. SQLx maps this to `LedgerBalanceError { group_id }`. No projection row from that group is visible after rollback.
- Multi-item WO completion **commits** (per-item slices balance; group-wide scalar sum may be ≠ 0).
- Mixed UoM for one item in one group **does not commit**.

### AC-P2 — Projection lockstep at every prefix (rank 2)

- Grain: `(item_id, location_id, lot_id, serial_id)` (and `uom_id` if a location is allowed to hold two units of the same item — default: **no**, stocking UoM only).
- After each op `i` in the sequence, `projection(grain) == SUM(postings on grain)` over postings with `posted_at` ≤ op `i`’s group time (i.e. the prefix).
- This is checked **on the prefix**, not only at the end. A bug that heals by the last op is still a fail.

### AC-P3 — Rebuild equals live (rank 3)

- `rebuild_from_empty() == live_projection` at the end of every sequence, and at a random subset of prefixes (full every-prefix rebuild is a slow shard — see §7).
- If they disagree, the ledger is truth; the test fails (the scheduled “page someone” job is not a substitute for this test).

### AC-P4 — Reverse restores pre-state (rank 4)

- `Reverse(g)` inserts a new group whose postings are the negation of `g` (new ids, new `posted_at`, same actor-or-service-principal, reason `reversal_of = g`).
- After `Reverse(g)`, projection on grains touched by `g` equals the projection as of the prefix **before** `g`, **provided no later non-reverse group touched those grains**. If later groups exist, the test either (a) only reverses a **suffix** group, or (b) asserts the weaker “net of `g` + `reverse(g)` is zero on those grains.”
- `Reverse(Reverse(g))` is a new group, not a delete. Double reverse returns to post-`g` state.
- Reversing an already-reversed group without a second reverse is rejected (idempotency of “this group is reversed” is a boolean on the original group, or a uniqueness on `reversal_of`).

### AC-P5 — Append-only (rank 5)

- Application role: `INSERT` + `SELECT` on `posting`; **no** `UPDATE` or `DELETE`.
- Direct `UPDATE` / `DELETE` as the app role fails (permission). A trigger that `RAISE`s on `UPDATE`/`DELETE` is extra defense for the owner role and is tested as the **migration** role as well (expect: owner can still DDL; owner `UPDATE` of a posting is either forbidden by event trigger or documented as out of threat model — coordinate slice 2).
- The generator never emits “edit posting” or “delete posting.” Those are **negative tests**, not ops.

### AC-P6 — Time (rank 6)

- `posted_at` is assigned by the database (`DEFAULT clock_timestamp()` or kernel `now()` inside the transaction). A client-supplied timestamp in the insert payload is ignored or rejected.
- Within one session/connection, `posted_at` is **monotonic non-decreasing** across committed groups. (`clock_timestamp()` not `CURRENT_TIMESTAMP`/`now()` — the latter is transaction-start and would collapse a whole group, which is OK per group, but two groups in one transaction would share a time. **Decision:** one group per transaction. Then `now()` is acceptable. Property: `posted_at` equal within a group, strictly increasing across groups on a connection.)
- Property tests pass a client timestamp of `1999-01-01` and of `now()+1 year` and assert the stored value is server time within a test-clock skew bound.

### AC-P7 — Genealogy (rank 7) — `Physical` profile only

- Every positive quantity at a **physical** location (not `SUPPLIER`/`CUSTOMER`/`SCRAP`/`ADJUSTMENT`/`PRODUCTION`/`VARIANCE`) for a lot or serial has a path on the posting graph to a `SUPPLIER` or `PRODUCTION` source.
- No cycles in the **origin** graph except those created by reverse groups (A→B then reverse is not an origin cycle).
- Serials: a serial is in at most one physical location at any prefix (cardinality 0 or 1). Qty of a serialized grain is `0` or `1`.
- Forward and backward walk from a finished serial (bone-screw) reaches the supplier lot of the bar. This is an explicit scenario, not only a random property.

### AC-P8 — Concurrency (rank 8) — **not proptest**

- Two sessions interleave well-formed groups on **overlapping** `(item, location)` grains.
- Invariants that must hold: AC-P1 on every committed group; AC-P2 after both commit (no torn projection); uncommitted work of A is not visible to B’s projection under the chosen isolation level.
- Isolation level is an opus decision. Floor scans need **read-your-writes** in the same session and a consistent projection after commit (ADR 0004 rejected CQRS eventual reads). Test both `READ COMMITTED` (default) and `SERIALIZABLE` / `REPEATABLE READ` if projection rows are updated in-place.
- Implementation: two `sqlx` connections + a barrier, **or** `pg_isolation_tester`-style; **not** loom on Postgres. Loom/shuttle is allowed **only** on the in-memory engine’s projection lock.

### AC-X — Explicit rejects (negative tests, not “the generator happens to avoid them”)

Each of the following is a **named test** that expects `Err`, not a silent skip:

| Id | Input | Expected |
|----|--------|----------|
| X1 | quantity `0` | reject |
| X2 | empty group (commit with no postings) | reject |
| X3 | single-sided posting (one row, no virtual/other location) | reject at COMMIT (unbalanced slice) |
| X4 | `Adjust` without `reason` | reject before COMMIT |
| X5 | serial grain with `quantity != 1` (and != 0) | reject |
| X6 | mixed UoM in one `(group, item)` without conversion | reject |
| X7 | client-supplied `posted_at` | ignore or reject (AC-P6) |
| X8 | `UPDATE`/`DELETE` as app role | permission error |
| X9 | `Reverse` of unknown `group_id` | reject |
| X10 | inventory `amount` set / cost `quantity` set | reject (slice 1 measure CHECK) |

### AC-N — Explicitly **not** this suite

- FIFO / average / standard **costing** golden files.
- MRP netting golden files.
- Period-close / inventory freeze (ADR 0007: no GL, no period close in this build). Do not generate `FreezePeriod`. If a future inventory-freeze module exists, it is a hook that vetoes postings, tested there.
- Catch-weight dual-unit as a first-class conservation law (see §5 — treat as reject or as a second attribute, opus).
- Customer IQ/OQ pack (docs/02 §9) — later `validation-pack` module may **re-export** this suite; the ledger lane does not format it.

---

## 3. Domain model of a generated op

Universe (shrunk independently of the op log):

```text
items:       2..=6 SKUs, each with stocking_uom, optional issue_uom, type ∈ {buy, make, phantom}
locations:   STOCK, RECEIVING, QUARANTINE, plus virtual SUPPLIER, CUSTOMER, SCRAP,
             ADJUSTMENT, PRODUCTION, VARIANCE, and WIP(wo_id) allocated on demand
lots:        0..=4 per item (lot-tracked items only)
serials:     allocated on SerializeLot / Receipt of serialized items
uoms:        EA, FT, IN, LB  (conversions: 12 IN = 1 FT; LB only on scrap item or mass dimension)
actors:      one human + one service principal
work orders: 0..=3, each with a tiny BOM (parent make-item, 1..=3 components, optional phantom)
```

**Status** (available / quarantined / rejected / hold) is a **location** (or a bin under a warehouse), not a fourth conservation key. A receipt into quarantine is `SUPPLIER → QUARANTINE`. Release is `QUARANTINE → STOCK`. This keeps every op a transfer.

```rust
/// One atomic commit = one group_id = one Op.
enum Op {
    Receipt {
        item: ItemId,
        dest: LocId,          // RECEIVING or QUARANTINE
        lot: Option<LotId>,   // generated or supplier_lot cross-ref
        serials: Vec<SerialId>, // empty unless item is serialized
        qty: Decimal,         // > 0, stocking uom after conversion
        from_uom: UnitId,     // purchase uom; engine converts
    },
    Issue { item, from: LocId, lot, serial, qty, to: LocId }, // to is not WIP
    Move { item, from, to, lot, serial, qty },
    Adjust { item, loc, lot, serial, delta, reason: ReasonCode }, // reason required
    Scrap { item, from, lot, serial, qty, reason },
    CycleCount { item, loc, lot, counted: Decimal, reason }, // posts delta vs projection
    SplitLot { item, loc, parent: LotId, children: Vec<(LotId, Decimal)> },
    SerializeLot { item, loc, lot: LotId, serials: Vec<SerialId> }, // |serials| == qty
    Reverse { group: GroupId },
    IssueToWO { wo: WoId, component: ItemId, from, lot, serial, qty },
    CompleteWO {
        wo: WoId,
        output: ItemId,
        dest: LocId,
        qty: Decimal,           // FG from PRODUCTION → dest
        backflush: bool,        // if true, explode BOM in THE SAME group
    },
    CloseWO { wo: WoId },      // residual WIP → VARIANCE/SCRAP (variance is leftover qty)
    Ship { item, from, lot, serial, qty }, // → CUSTOMER
    // Kernel-visible, not a costing golden:
    CostPair { debit: Money, credit: Money, element: CostElement },
    // docs/03 §3.2: a hook may add a balanced pair to the current group
    HookContribute { extra: TransferPair },
}

struct TransferPair {
    item: ItemId,
    uom: UnitId,
    qty: Decimal,       // > 0
    from: LocId,
    to: LocId,
    lot: Option<LotId>,
    serial: Option<SerialId>,
}
```

**Compilation of each `Op` → postings (engine, not the test):** every op becomes ≥ 2 inventory rows, same `group_id`, per-slice net zero.

| Op | Postings (inventory) |
|----|----------------------|
| Receipt | `−qty SUPPLIER`, `+qty dest` |
| Issue / Move / IssueToWO | `−qty from`, `+qty to` (`to` = `WIP(wo)` for IssueToWO) |
| Adjust / CycleCount | `±delta loc`, `∓delta ADJUSTMENT`; reason on both or on the group |
| Scrap | `−qty from`, `+qty SCRAP` |
| SplitLot | `−sum parent lot`, `+qi` each child lot; **same loc, same item, same uom** |
| SerializeLot | `−n` unserialized lot grain, `+1` each serial grain |
| Reverse | negate every posting of `g` (new ids) |
| CompleteWO | FG: `−qty PRODUCTION`, `+qty dest`; if backflush, plus IssueToWO pairs for each BOM line in **this** group |
| CloseWO | for each leftover `(item,uom)` at `WIP(wo)`: move to `VARIANCE` (or `SCRAP`) so that WO WIP is zero |
| Ship | `−qty from`, `+qty CUSTOMER` |
| CostPair | two `ledger=cost` rows, amounts negate |
| HookContribute | additional transfer pair **in the same group** as the triggering op (generator: attach to next non-reverse op) |

Phantom BOM line: **no posting for the phantom item**. Backflush explodes through it to children. A generator that posts the phantom is a bug in the engine, caught by AC-P7 (phantom has no SUPPLIER receipt and should never sit in STOCK) and by an explicit scenario.

---

## 4. Generator sketch (Rust-ish)

Do **not** `impl Arbitrary for Vec<Op>`. Use a state machine so preconditions and shrinking stay valid.

```rust
use proptest::prelude::*;
use proptest_state_machine::{ReferenceStateMachine, Sequential};

struct Universe { /* items, uoms, boms, conversions — shrunk slowly */ }

struct Model {
    uni: Universe,
    // grain -> qty  (stocking uom)
    on_hand: BTreeMap<Grain, Decimal>,
    groups: Vec<CommittedGroup>,          // in order
    reversed: BTreeSet<GroupId>,
    next_wo: u32,
    open_wos: BTreeMap<WoId, WoState>,    // issued components, completed FG
    serial_at: BTreeMap<SerialId, Grain>, // physical location or none
}

enum Profile { WellFormed, Physical }

impl ReferenceStateMachine for Model {
    type State = Model;
    type Transition = Op;

    fn init_state() -> BoxedStrategy<Model> { /* small universe */ }

    fn transitions(state: &Model) -> BoxedStrategy<Op> {
        // Weighted union. Weights are the test design:
        //   Receipt 5, Move 4, Issue 3, IssueToWO 3, CompleteWO 2, Ship 2,
        //   Adjust 2, Scrap 2, CycleCount 1, SplitLot 1, SerializeLot 1,
        //   Reverse 3, CloseWO 1, CostPair 1, HookContribute 1
        //
        // Physical: filter via prop_filter / generate from on_hand keys.
        // WellFormed: any dest/from; qty from nonzero_qty().
        todo!()
    }

    fn apply(state: Model, op: &Op) -> Model {
        // Pure: apply the compiled posting pairs to on_hand;
        // record group; if Reverse, negate that group's postings.
        todo!()
    }

    fn preconditions(state: &Model, op: &Op) -> bool {
        match op {
            Op::Reverse { group } =>
                state.groups.iter().any(|g| g.id == *group)
                && !state.reversed.contains(group),
            Op::Issue { qty, .. } | Op::Ship { qty, .. } | Op::Scrap { qty, .. }
                if matches!(profile(), Profile::Physical) =>
                    on_hand(source_grain(op)) >= *qty,
            Op::SerializeLot { lot, serials, .. } =>
                on_hand(unserialized(lot)) >= Decimal::from(serials.len())
                && serials.len() >= 1,
            Op::SplitLot { parent, children, .. } => {
                let s: Decimal = children.iter().map(|(_, q)| *q).sum();
                s > Decimal::ZERO && on_hand(parent_grain(parent)) >= s
            }
            Op::CompleteWO { wo, qty, .. } => {
                state.open_wos.contains_key(wo) && *qty > Decimal::ZERO
            }
            Op::Adjust { reason, delta, .. } =>
                !reason.is_empty() && *delta != Decimal::ZERO,
            Op::Receipt { qty, .. } => *qty > Decimal::ZERO,
            _ => true,
        }
    }
}

fn nonzero_qty() -> impl Strategy<Value = Decimal> {
    prop_oneof![
        10 => (1i64..=20).prop_map(Decimal::from),                 // typical
        3  => (1i64..=500).prop_map(Decimal::from),                // bone-screw scale
        1  => Just(Decimal::new(1, 8)),                            // 1e-8
        1  => Just(Decimal::new(1, 0) / Decimal::from(3)),         // 1/3 — conversion loss bait
        // NOT rust_decimal::MAX in the default strategy — see shard "extremal"
    ]
}

// Lockstep SUT: either FakeLedger (in-memory) or PgLedger (sqlx).
fn assert_prefix(model: &Model, sut: &Sut) {
    // AC-P1: every group in sut.committed balanced under REAL slice
    // AC-P2: sut.project(grain) == model.on_hand[grain] == sum(postings)
}
```

**Bone-screw walk (required explicit sequence, not a golden cost file):**

```text
seed: item SCREW (make, EA, serialized), BAR (buy, FT, lot), PASSIVATE (service — no stock)
      BOM: 1 BAR blank per SCREW (issue converted to IN or EA blanks — pick one and freeze)
      existing stock: 120 blanks @ STOCK lot L0 (or 10 FT bar)
1. Receipt 180 blanks (or 15 FT) SUPPLIER → QUARANTINE lot L1
2. Move QUARANTINE → STOCK
3. Release WO-500 qty 500
4. IssueToWO BAR/blanks for 500
5. Scrap 12 at mill (STOCK or WIP → SCRAP) reason mill-breakage
6. CompleteWO 488 SCREW PRODUCTION → STOCK, serials S001..S488
7. CloseWO — leftover blank qty in WIP → VARIANCE (expect 0 if issue was exact)
8. Ship 10 serials → CUSTOMER
9. Genealogy(S042): serial → WO-500 → lot L1 → SUPPLIER
10. Reverse(step 8 group): those 10 serials back at STOCK
```

If the engine cannot express this walk, Phase 1 catalog is not exercised and the property suite is a toy.

---

## 5. Shrinking (the part PLAN is silent on, and where suites die)

`proptest-state-machine` default shrink is:

1. Delete transitions **from the back** (keep a prefix) until `min_size`.
2. Shrink individual transitions **from the front**.
3. Shrink the initial universe.

**Prefix drop is accounting-valid.** A shorter prefix is a legal history. Use it.

**Front-shrink of a `Receipt` qty while a later `Issue` remains is not valid** in the `Physical` profile (precondition saves you if it is wired; `WellFormed` will just go negative and hide bugs). **Mid-sequence deletion is never valid** (delete the receipt, keep the issue: genealogy and conservation of origin break).

**Required custom shrinker (AC for the test crate):**

| Move | Valid? | Use |
|------|--------|-----|
| Drop suffix ops | Yes | Default, first |
| Replace suffix group `g` with nothing (drop) | Yes | same as suffix |
| Replace any committed `g` that has **no later dependent grain touch** with nothing | Yes | like deleting an independent receipt |
| Replace `g` … later ops with `g` + `Reverse(g)` + later ops | Yes | **preferred “delete”** — history stays append-only |
| Negate-shrink: cut all qtys in a transfer pair toward 1 | Yes if Physical preconditions re-checked | second |
| Shrink universe (fewer items/lots) | Yes if ops still well-typed | last |
| Delete an interior op | **No** | forbid |
| Drop one side of a pair | **No** | that is X3 |

Document this in the test module. If the team uses `proptest-state-machine` without a custom `Tree`, they **must** rely on preconditions rejecting invalid front-shrinks and accept that minimal counterexamples may still contain long prefixes. A one-day spike on the shrinker is cheaper than debugging 40-op failures in Wave 2.

**Reversals, not deletions** is also an **engine** law (AC-P4/P5). The shrinker imitates the engine so a failure printout is a legal posting tape an auditor could read.

---

## 6. What a naive generator misses (and what to do)

| Miss | Why it bites | Handle |
|------|----------------|--------|
| **Zero qty** | Silent no-op groups, empty-looking commits | X1 named reject; `nonzero_qty()` never 0 |
| **`Decimal` max / high scale** | `numeric(24,8)` overflow; `SUM` ≠ 0 by rounding; rust_decimal vs PG `numeric` | Extremal **shard**; default strategy stays in `1..=500` plus `1e-8` and `1/3` |
| **UoM conversion loss** | `10 FT` received, `36 IN` issued, `convert` rounds; `A→B→A` not identity (primer §9.2) | Engine posts **stocking UoM only**. Property: leftover vs `convert(issue)` is either 0 or posted to a documented remainder policy. `1/3` in the qty strategy. Conversion tests **live in `wicket-uom`**, not as golden costing here; ledger only asserts “no mixed uom in a slice” |
| **Dual-unit catch-weight** | Serial qty=1 each **and** a unique kg; one `quantity` column cannot conserve both | **Out of kernel v1 unless opus says otherwise.** X: posting with two measures in one row rejected. Optional weight is an attribute, not a conservation key (slice 1: designable). Do not generate catch-weight as a balanced pair of unlike dimensions |
| **Serial qty ≠ 1** | Breaks “a serial is a unit” and genealogy cardinality | X5; generator for serialized grains only emits `1` |
| **Empty group** | `COMMIT` with zero rows: trigger never fires (FOR EACH ROW), **unbalanced-empty succeeds** — this is a real hole in a row-level constraint trigger | X2: engine refuses empty commit **in application code**; add a deferred **statement-level** guard or a group header row that the trigger checks. **PLAN gap:** slice 1’s trigger does not catch empty groups. Property test must include this or the hole ships |
| **Single-sided posting** | The bug the whole ADR exists to prevent | X3; also `WellFormed` never emits it, so the named test is mandatory |
| **Reasonless adjustment** | 21 CFR 11.10(e) “why”; ADR 0004 “reason required” | X4; `Adjust`/`CycleCount` constructors take `ReasonCode` newtype (`std::convert::TryFrom<&str>` fails on empty) |
| **WIP residual as variance** | Completion of FG does **not** empty component WIP. Variance is leftover **qty at WIP(wo)**, moved on `CloseWO` | Generator includes `CompleteWO` without `CloseWO` (WIP residual remains — AC-P2) and with `CloseWO` (WIP(wo) all zero — AC). Do not “balance” leftover by cooking FG qty |
| **Backflush of phantoms** | Phantom is not stocked; explosion must skip it | Explicit BOM with one phantom between FG and a buy item; assert **no posting row with the phantom `item_id`** |
| **Period-close freeze** | ERPNext freezes stock by date; Wicket has **no GL** (ADR 0007) | **Out of scope.** Do not generate. Note in SPEC as a future hook, not a ledger invariant |
| **Hook extra postings** | docs/03 §3.2: hook may contribute to the **same** group | `HookContribute` attached to an op; group still per-slice zero; hook-unbalanced pair fails the whole business transaction |
| **Multi-ledger one group** | WO complete posts inventory + cost together | `CompleteWO` may include a `CostPair` in the same `group_id`; each ledger slices independently (slice 1: fatal if one scalar) |
| **Negative on-hand** | Not a group-balance bug (slice 1) | `WellFormed` allows; `Physical` does not. Policy flag, not a constraint trigger |
| **Quarantine as a boolean** | If status is a mutable column on a balance row, you have invented stored state | Status = location. Generator never “sets status” |
| **Client clocks / equal `posted_at`** | `now()` vs `clock_timestamp()`; two groups one txn | One group per transaction (AC-P6) |
| **TOAST / wide JSON dimensions** | If slice keys live only in JSONB, the trigger cannot index them | Slice keys are **columns** (slice 1 schema). Generator does not stuff item/uom into JSON only |

**Empty group is the stealth bug.** A deferred `FOR EACH ROW` constraint trigger **does not run** if you `COMMIT` with no inserts. The engine must hold a `group` header (insert one row into `ledger.group` that the trigger also validates) **or** refuse empty commits in Rust **and** test that refusal. Recommend a `ledger.posting_group(id, posted_by)` header with a deferred trigger: `EXISTS postings` and all slices zero. That also gives a place to put `reversal_of` and `reason`.

---

## 7. Infrastructure

### 7.1 Two SUTs, not one

| SUT | What it is allowed to prove | What it cannot prove |
|-----|-----------------------------|----------------------|
| **In-memory `FakeLedger`** | AC-P1 (engine assembler), P2, P3, P4, P7 graph, shrinker, generator sanity | Deferred PG trigger, grants, `posted_at` default, isolation, numeric(24,8) vs Decimal |
| **Real PostgreSQL** | All of the above **plus** P1-at-COMMIT, P5, P6, P8 | Nothing — this is the compliance surface |

**A fake that reimplements `SUM == 0` tests the fake.** Because ADR 0004’s entire claim is *database-enforced* balance, the **default `cargo test -p wicket-ledger` must hit Postgres for P1/P5/P6.** In-memory is the fast inner loop and the shrinker playground.

**Do not** use one container per proptest case. Startup would blow the time cap by itself.

### 7.2 Where Postgres comes from (Waves 1–2, before any installer)

Priority order:

1. **`DATABASE_URL`** — CI service (`postgres:16` in GitHub Actions `services:`) and `dev/docker-compose.yml` (workspace lane). This is the missing Wave 1 subtask slice 7 will also flag.
2. **testcontainers-rs**, **one container per test process**, reused across cases, `TRUNCATE posting, posting_group, projection RESTART IDENTITY` (or `DELETE`) per case. Requires Docker. Fine on Linux CI; **hostile on this Windows CNC node** if Docker Desktop is not a given.
3. **`postgresql_embedded` / zonky-style** — fallback for no-Docker Windows unit boxes. Not the thing we ship to customers (ADR 0003). Test-only.

**`sqlx::test`** with a managed pool is the right harness if `DATABASE_URL` is set: it can wrap each test in a transaction and roll back. **Conflict:** deferred constraint triggers run at **COMMIT**. A test that `ROLLBACK`s never fires them. Therefore:

- **Cannot** use “one wrapping transaction per case, roll back” to test AC-P1.
- Use **COMMIT + TRUNCATE** per case, or `COMMIT` into a throwaway schema, or savepoints **around everything except the group commit under test** (savepoint cannot emulate deferred constraint-at-commit of the inner group if the outer txn is still open — actually: deferred constraints fire at the **outer** COMMIT, so a savepoint-release of an unbalanced group will **not** error until the test transaction commits).

**Normative:** each property case is **its own committed transactions** (one per op) plus a final `TRUNCATE` in a cleanup connection. Isolation from other tests: unique schema per test process (`SET search_path TO t_<pid>`) created once per worker.

### 7.3 Time budget vs `caps.gate_shard_max_min` (10 minutes)

Assumptions: shared Postgres, migrate **once** per process, `TRUNCATE` ~2–5 ms, insert 2–12 rows + COMMIT + trigger SUM over ≤12 rows ≈ **1–3 ms**, projection upsert ≈ 1 ms.

| Check | Cases | Ops/case | Extra queries | Est. |
|-------|-------|----------|---------------|------|
| In-memory P1–P4, P7 | 256 | 8–24 | every prefix in RAM | **2–8 s** |
| PG P1 only (commit, no prefix rebuild) | 256 | 16 | 16 commits | **15–40 s** |
| PG P2 every prefix (`SUM(postings)` query) | 256 | 16 | 16 extra aggregates | **30–90 s** |
| PG P3 full rebuild every case (not every prefix) | 256 | — | 1 rebuild | **+20–60 s** |
| PG P3 rebuild **every prefix** | 256 × 16 | heavy | | **4–12 min** — **over cap if combined** |
| PG P5/P6/X-rejects | ~20 scripted | — | | **<10 s** |
| PG P8 two sessions, 32 scenarios | 32 | 20+20 | locks | **1–3 min** |
| Extremal Decimal + 20-row groups | 64 | 20 | | **20–40 s** |
| **Naive worst:** 1024 cases × 50 ops × every-prefix rebuild × migrate-per-case | | | | **>15 min, often >30** |

**Will default `cargo test -p wicket-ledger` exceed 10 minutes?**  
**Yes, if** P2-every-prefix + P3-every-prefix + P8 + 256 PG cases share one command.  
**No, if** the default target is the **fast set** below.

**Default `cargo test -p wicket-ledger` (must stay < 10 min, target < 2 min):**

- All unit tests
- In-memory proptest 256 cases, every-prefix P1–P4, P7
- Postgres: 64 cases, P1 + P2 (prefix **project** vs sum, not full rebuild) + P3 once at end + P5/P6/X
- P8 and every-prefix rebuild: `#[ignore]` or separate binary

**Full gate (phase-end / CI job):** sharded.

### 7.4 SHARD plan (invariant shards, not random case splits)

Invariant shards diagnose; case shards only help wall-clock. **Do both:** invariant shards as separate bins; case shards via `PROPTEST_CASES` / seed ranges if a bin still > 10 min.

| Shard lane | Filters / `--ignored` bins | Est. | Caps |
|------------|----------------------------|------|------|
| `gates-shard-ledger-mem` | in-memory `WellFormed`+`Physical`, P1–P4, P7, shrinker smoke | 1–3 min | under |
| `gates-shard-ledger-pg-balance` | PG P1, X1–X10, P5 grants, P6 time, empty-group hole | 2–4 min | under |
| `gates-shard-ledger-pg-proj` | PG P2 prefixes, P3 rebuild end-of-case, P4 reverse | 3–6 min | under |
| `gates-shard-ledger-pg-rebuild-prefix` | P3 **every prefix** (slow) | 4–12 min | **split to 2 case shards if >10** |
| `gates-shard-ledger-conc` | P8 isolation + in-memory loom on projection locks | 2–5 min | under |
| `gates-shard-ledger-extremal` | max Decimal, 20-row groups, `1/3` UoM, phantom backflush, hook pairs | 1–3 min | under |

Tags (so shards are `cargo test --features pg -- --ignored slice_inventory` etc., matching slice 1’s note):

`slice_inventory`, `slice_cost`, `multi_item_wo`, `mixed_uom_forbidden`, `physical`, `well_formed`, `genealogy`, `reverse`, `grants`, `concurrency`, `extremal`.

**Rebuild-at-10M-rows** is a **benchmark / soak**, not a property case. Architecture §7: 10M rebuild < 10 min. That is a gated bench in `dev/` or `crates/wicket-ledger/benches`, **not** `cargo test`. Putting it in the property suite guarantees a cap violation.

---

## 8. EXECUTOR / SPLIT / TIER

Protocol: *test/gate code always splits off the algorithm lane.* PLAN currently gives **one** crate directory `wicket-ledger` to one Wave 2 lane. That collides with exclusive crate ownership **and** with the split rule.

### Recommended split

| Lane | Owns (exclusive) | Provider | Notes |
|------|------------------|----------|--------|
| `ledger-engine` | `crates/wicket-ledger/src/**`, `migrations/**`, unit tests of the assembler | **cursor** (slice 1) or 1:1 with grok; **blind-race** after the slice keys are frozen (risky: Wave 2 does not close until this passes) | Algorithm, SQL trigger, projection, reverse, group header |
| `ledger-proptest` | `crates/wicket-ledger/tests/**` **or** (cleaner) `crates/wicket-ledger-proptest/` | **the other family** (grok if engine is cursor) | Generator, model, ACs, PG harness, shards |
| `spike-ledger-constraint` | `_team/reports/spike-ledger-constraint.md` + throwaway SQL | cursor | slice 1 already asked; **include empty-group trigger hole** |
| `spike-ledger-proptest` | failing harness + this SPEC copied into `crates/wicket-ledger/SPEC.md` | grok | Wave 1 optional — see §9 |

**Do not** put costing/MRP goldens on either lane.

**If PLAN will not add a crate:** amend Wave 2 law to “crate directory is exclusive **except** `tests/` which is a second lane.” File-level split is enough for worktrees if both branch from Wave 1 stubs and do not touch `Cargo.toml` members. A separate crate is still cleaner (`wicket-ledger-proptest` as a workspace `dev-only` member or `[[test]]` package).

**Serial vs parallel:**  
- Stubs freeze `Ledger::commit`, `project`, `rebuild`, `reverse`, posting types in Wave 1.  
- Then **parallel**: engine implements, proptest writes tests against the stub signatures.  
- Merge risk: API drift. Mitigate by putting the signatures in `wicket-ledger/src/api.rs` **owned by engine**, tests compile against public API only. Proptest lane **may not** change public types; it files an escalation.

**If stubs are `todo!()`:** proptest cannot even compile. That is slice 4’s problem. This slice’s requirement on Wave 1 stubs:

```rust
// must be REAL in the Wave 1 stub, not todo!()
pub struct Posting { /* fields */ }
pub struct GroupId(/* ... */);
pub struct LedgerBalanceError { pub group_id: GroupId }
impl Ledger {
    pub async fn commit(&mut self, postings: Vec<NewPosting>) -> Result<GroupId, LedgerError>;
    pub async fn reverse(&mut self, g: GroupId) -> Result<GroupId, LedgerError>;
    pub async fn project(&self, grain: Grain) -> Result<Decimal, LedgerError>;
    pub async fn rebuild(&mut self) -> Result<(), LedgerError>;
}
```

### TIER: **deep** (and `audit.double: true`)

Integration-critical, contract-bearing, previously identified as the project’s central risk (PLAN §9). A miss here is a false compliance claim. Cross-family double audit on **both** engine and proptest lanes.

### Opus DECISION (before stub freeze)

1. Balance slice keys (this report + slice 1 default: inventory `(group, item, uom)`; lot/serial **not** in the key).
2. Canonical stocking UoM at insert vs multi-uom buckets.
3. Empty-group header row vs Rust-only reject.
4. Cost slice grain (`ledger` vs `cost_element`).
5. Catch-weight: reject vs attribute.
6. Isolation level for projection upserts.

Build lanes do not invent these.

---

## 9. Should a Wave 1 spike land a failing harness?

**Yes, but not in the workspace lane.**

Workspace is already one human-equivalent doing toolchain, CI, compose, and ~15 stubs. A red proptest module that does not compile against `todo!()` stubs is noise. A red module that **does** compile against the frozen API is the cheapest possible specification.

**Sequence:**

1. Opus DECISION on slice keys (with slice 1/5).
2. Wave 1 workspace stubs the four methods in §8 (real signatures, `unimplemented!()` bodies).
3. **`spike-ledger-proptest`** (after stubs exist, still Wave 1 or Wave 1.5): lands `crates/wicket-ledger/tests/proptest_ledger.rs` with:
   - `Model` + `Op` enum
   - `#[ignore]` or `#[should_panic]` / failing `proptest!` that `commit`s one Receipt and asserts P1
   - `sqlx` harness skipped if no `DATABASE_URL`
   - this report’s AC list copied to `crates/wicket-ledger/SPEC.md` (or `_team/specs/wicket-ledger.md` if crate dir is not to be touched yet — **spike may write SPEC under `_team/` only** if product files stay frozen; then Wave 2 engine copies it in)
4. Wave 2 `ledger-proptest` un-ignores and fills the generator; `ledger-engine` makes it green.

A harness that is **red on purpose** is the only way PLAN’s “highest-value test” exists as an acceptance gate rather than a retrospective essay. Without it, Wave 2 will ship a constraint trigger and a handful of hand-written receipts and call the property-test box ticked.

If Wave 1 cannot spare a spike lane: **this file is the SPEC**. Exec-master must paste §2 into the ledger SPEC before dispatch. That is strictly worse (no compile-time API lock) but better than PLAN §7 as written.

---

## 10. PLAN gaps (for the master consolidator)

1. **§7 is not an acceptance criterion.** Replace the one-liner with a pointer to the ledger SPEC ACs (this §2).
2. **§6.2 invariant is false as a scalar.** Use slice 1 / this report’s slice keys.
3. **Wave 2 is one `wicket-ledger` lane.** Split engine vs proptest (protocol + exclusive-ownership clash).
4. **Empty group hole** in a `FOR EACH ROW` deferred trigger — not in PLAN, not in ADR 0004.
5. **Wave 1 CI Postgres** (`dev/docker-compose` + Actions `services`) is required before any ledger test runs; PLAN is silent (shared with slice 7).
6. **`sqlx::test` rollback vs deferred triggers** — must be in the workspace/test-harness SPEC so Wave 2 does not “pass” tests that never COMMIT.
7. **Rebuild 10M** is a bench, not `cargo test`.
8. **Period close** is out of scope (ADR 0007); do not sneak it into ledger tests.
9. **Hook ABI** (docs/03 §3.2 contribute postings) is a kernel contract the generator must see; PLAN has no hook-ABI task (slice 9).
10. **Customer-runnable IQ suite** (docs/02 §9) is not this lane; do not block ledger on it. Later re-export.

---

## 11. Dispatch metadata (for plan-audit.md)

| Field | Value |
|--------|--------|
| **VERDICT on PLAN §7** | **Revise** — missing AC, missing generator, missing shrink law, missing PG harness, missing split |
| **EXECUTOR** | `ledger-engine`: cursor (or blind-race **after** opus freeze). `ledger-proptest`: grok (other family). **Not** the same agent. |
| **Opus DECISION first?** | **Yes** — slice keys, empty-group header, UoM normalize, cost grain, catch-weight, isolation |
| **SPLIT** | engine `src/`+migrations ‖ proptest `tests/` (or `wicket-ledger-proptest` crate) ‖ conc shard ‖ 10M bench. **Serial** only if stubs do not freeze the API |
| **TIER** | **deep** + `audit.double: true` on both ledger lanes |
| **SHARD** | §7.4 — default `cargo test -p wicket-ledger` = fast set (<2 min target). Full gate = 4–6 `gates-shard-ledger-*` bins. 10M rebuild is **not** a test shard |
| **Wave 1 failing harness?** | **Yes**, as `spike-ledger-proptest` after stub API exists; **not** inside the workspace lane |
| **Do not** | Dump costing/MRP goldens on the ledger lane |

---

## References

- PLAN.md §§6–7, 9  
- docs/02-architecture.md §§3, 7, 9  
- docs/adr/0004-append-only-ledger.md  
- docs/adr/0007-defer-general-ledger.md (period close out of scope)  
- docs/00-erp-primer.md §§3 (bone-screw), 4, 9.2 (UoM)  
- docs/04-module-catalog.md Phase 0–1  
- docs/03-module-system.md §3.2 hooks  
- `_team/reports/sweep-plan-ledger.md` (real SQL invariant, empty-group implication)  
- `_team/reports/sweep-plan-typed-qty.md` (bucket = `(group, ledger, item, unit)`)  
- [proptest state machine shrinking](https://proptest-rs.github.io/proptest/proptest/state-machine.html) — delete-from-back then front-shrink transitions  
- [PostgreSQL CREATE TRIGGER](https://www.postgresql.org/docs/17/sql-createtrigger.html) — constraint triggers are row-level; empty insert set does not fire  
)
