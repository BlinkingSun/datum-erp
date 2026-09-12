# Plan audit slice 5 — Typed quantities in `datum-core`

**Role:** adversarial researcher  
**Date:** 2026-09-11  
**Verdict:** PLAN.md §5 sentence is **false as written**. UoM cannot be a Rust type parameter for customer-defined units. ADR 0002 remains valid if amended to **dimension** (kind) at compile time, **unit identity** at runtime.

---

## 1. Adversarial read of PLAN.md §5

> `Quantity` carrying its unit of measure as a type parameter

This conflates three distinct things:

| Concept | Examples | Can be a Rust type param? |
|--------|----------|---------------------------|
| **Dimension (kind)** | Length, Mass, Count, Time | Yes — closed kernel set (~5–8) |
| **Unit (identity)** | `FT`, `IN`, customer `"linear metre (shop A)"` | **No** — unbounded, DB-backed |
| **Conversion graph** | 12 in = 1 ft (universal); lb/ft for bar heat lot (item/lot) | **No** — data + context |

If `Quantity<Foot>` and `Quantity<Inch>` are different type parameters, you still cannot express “this item’s stocking UoM is `UnitId(42)`” without either exploding the type system (infinite unit types) or lying (everything is `Quantity<Unknown>`).

**crates `uom` / `measurements` (option C):** Useful for **closed** SI/imperial kernels and compile-time rational conversions. They do **not** survive customer-defined units or item-specific factors without reimplementing your own layer on top. Persistence is always `{ numeric, unit_code }`; the phantom type is erased at the boundary. **Reject as the foundation**; optional internal helper for universal length/mass constants only, behind `datum-uom`.

**Option B (fully runtime):** Honest for DB and serde, but throws away the one Rust advantage ADR 0002 cites unless you reintroduce dimension markers elsewhere.

**Option E (tagged `Decimal` + `convert()` only):** Same as B with nicer API; dimension mistakes become runtime bugs — unacceptable for “silent corruption for months.”

**Winner: A + D hybrid** — phantom **dimension** (not UoM), runtime `UnitId`, item-scoped `StockQuantity` / `ConversionContext` in `datum-uom`.

---

## 2. Recommended `datum-core` signatures (sketches)

`datum-core` has **no `sqlx`**. It defines newtypes, dimensions, quantities, money, errors. DB mapping lives in `datum-db` / `datum-ledger`.

```rust
// --- Identifiers (opaque, Copy, Hash, Eq) ---
pub struct UnitId(pub u64);      // DB assigns; core does not interpret catalog
pub struct ItemId(pub u64);
pub struct LotId(pub u64);

// --- Closed dimension set (compile-time) ---
pub trait Dimension: private::Sealed {
    const KIND: DimensionKind;
}
pub enum DimensionKind { Count, Length, Mass, Time, Volume, Area }

pub struct CountDim;
pub struct LengthDim;
pub struct MassDim;
// impl Dimension for each...

/// Signed quantity: dimension checked at compile time, unit at runtime.
pub struct Quantity<D: Dimension> {
    amount: rust_decimal::Decimal,
    unit: UnitId,
}

impl<D: Dimension> Quantity<D> {
    /// Same `unit` only; no implicit conversion.
    pub fn try_add(self, rhs: Self) -> Result<Self, QuantityError>;
    pub fn try_sub(self, rhs: Self) -> Result<Self, QuantityError>;
    pub fn negate(self) -> Self;
}

/// Item-tagged quantity for inventory semantics (stocking / issue / purchase are data on item).
pub struct StockQuantity {
    item: ItemId,
    qty: Quantity<???>,  // see note below
}
```

**Note on `StockQuantity`:** Do not put `ItemId` inside a generic `Quantity<D>` for every ledger line — ledger dimensions already carry `item`. Use:

```rust
pub struct InventoryPostingQty {
    item: ItemId,
    amount: rust_decimal::Decimal,
    unit: UnitId,
    dimension: DimensionKind, // redundant with unit master; used for fast guards
}
```

Kernel guard: `dimension` must match the dimension implied by `unit` when validated through `UnitRegistry` trait (implemented in `datum-uom`, trait **defined** in core as interface).

**Conversion is not on `Quantity` in core:**

```rust
// datum-core — trait only, no DB
pub struct ConversionContext {
    item: ItemId,
    lot: Option<LotId>,
}

pub trait UnitConverter {
    fn dimension_of(&self, unit: UnitId) -> Result<DimensionKind, QuantityError>;
    fn convert<D: Dimension>(
        &self,
        qty: Quantity<D>,
        to_unit: UnitId,
        ctx: &ConversionContext,
    ) -> Result<Quantity<D>, QuantityError>;
}
```

**Money — separate type, not `Quantity<MoneyDim>`:**

```rust
pub struct CurrencyId(pub u16); // ISO numeric or internal enum; catalog in DB

/// Fixed-scale money; no float.
pub struct Money {
    /// Amount in minor units (e.g. cents) OR use Decimal + scale — pick one in DECISION.
    minor: i64,
    currency: CurrencyId,
}
```

ADR 0007 defers GL; **cost ledger** still posts `Money`. Currency is runtime like `UnitId`. Compile-time guarantee: **do not add `Money` to `Quantity<Length>`** — achieved by separate types, not a shared generic.

---

## 3. Worked example (receive / issue / scrap)

**Setup:** Item `BAR-6061`, stocking UoM = foot (`U_FT`), purchase UoM = foot, issue UoM = inch (`U_IN`). Chips scrap tracked as mass on a **scrap reason** line in pounds (`U_LB`) via item-specific factor or separate scrap item — here: same item allows mass scrap with lot context.

| Step | Business | Kernel behavior |
|------|----------|-----------------|
| Receive 10 ft | PO receipt | Group: `+10 @ U_FT` → `RECEIVING`, `-10 @ U_FT` → `SUPPLIER`. All lines same `item`, same `unit_id`. |
| Issue 36 in to WO | Backflush / issue | `convert(36 @ U_IN → U_FT, ctx{item})` → `3 @ U_FT`. Group: `-3 @ U_FT` from `STOCK`, `+3 @ U_FT` to `WIP` (locations). |
| Scrap 0.2 lb chips | By-product loss | **Different dimension** (Mass). Not added to foot balance. Either: (a) post to scrap item `CHIPS` in `U_LB`, or (b) `datum-uom` item rule: `length_remaining → mass` with lot density. Group balances **per (item, unit_id)** — scrap group uses `U_LB` only. |

Compile-time: `Quantity<LengthDim> + Quantity<MassDim>` does not compile.  
Runtime: `10 FT + 36 IN` without `convert` returns `UnitMismatch` / `DimensionMismatch`.

---

## 4. Must-answer

### 4.1 Serde / SQLx and the “type parameter”

- **Rust type parameters are not stored.** They exist only in the compiler.
- Persist and serde: **`{ amount, unit_id }`** (+ `item_id` / dimensions on posting row).
- Optional serde helper in core:

```rust
#[derive(Serialize, Deserialize)]
pub struct QuantityWire {
    pub amount: Decimal,
    pub unit: UnitId,
    pub dimension: DimensionKind, // denormalized for API safety; must match catalog
}
```

`sqlx::FromRow` maps to `QuantityWire` or ledger struct in `datum-ledger`, then `try_into_quantity<D>()` using `UnitConverter::dimension_of`.

### 4.2 Generic ledger posting table, mixed items

One table, many items per `group_id` (e.g. multi-line receipt). Each row:

- `ledger` (inventory | cost | labor)
- `group_id`
- `dimensions` (item, location, lot, …)
- `signed_amount` **numeric**
- `unit_id` **not null**

**Zero-sum rule (coordinate with ADR 0004):** Deferrable constraint on **`SUM(signed_amount) GROUP BY group_id, ledger, item_id, unit_id`** (and same dimension keys that define the balance bucket). Mixed items in one group: each `(item_id, unit_id)` bucket must sum to zero. **Never** sum across different `unit_id` in SQL — conversion is application/`datum-uom` duty **before** insert.

Cost ledger rows use `Money` columns (`amount_minor`, `currency_id`), same `group_id`, separate bucket — **do not mix quantity and money in one numeric column**.

### 4.3 SQL zero-sum when units differ

If a developer posts `+10 FT` and `-3 FT` and one stray `-36 IN` in the same `(group, item)`:

- With per-`(item, unit_id)` constraint: **commit fails** (IN bucket ≠ 0, FT bucket ≠ 0) — correct failure.
- **Forbidden:** single global `SUM(quantity)` per group without unit partition — would be meaningless.

**Invariant text for ledger slice:** “Posting groups balance in every `(ledger, item_id, unit_id, dimension slice)` bucket; inventory postings for an item are normalized to stocking `unit_id` unless explicitly modeling a multi-UoM item with separate buckets (rare, document in item master).”

### 4.4 Money

**Separate from `Quantity`.** Same *pattern*: runtime currency id, explicit scale, no cross-currency add without `FxContext` (future). Costing posts `Money`; inventory posts `Quantity<D>`. ADR 0002 justification: **type system separates Money from Quantity<Length>**, not “every unit is a type.”

### 4.5 Precision and rounding (docs/02 kernel UoM)

| Layer | Owns |
|-------|------|
| `datum-core` | `Decimal` usage policy, `QuantityError`, dimension seals, **no rounding rules** |
| `datum-uom` | Unit catalog, conversion paths, **rounding mode per operation** (issue vs receive vs count), item/lot factors, property tests |
| `datum-ledger` | “post only in allowed unit for this movement type” orchestration |

Runtime UoM means rounding lives in **`datum-uom`** with rules keyed by `(item, operation, unit)` from DB seed/migrations — not in const generics.

---

## 5. Options scorecard

| Option | Compile-time safety | Customer units | Ledger/SQL | Verdict |
|--------|---------------------|----------------|------------|---------|
| A (dimension + UnitId) | Kind yes, unit runtime | Yes | Clean | **Adopt** |
| B (full runtime) | Weak | Yes | Clean | Reject alone |
| C (uom crate) | Strong for closed set | No | Poor fit | Reject as base |
| D (Dimensional + Stock + Context) | Strong | Yes | Clean | **Adopt** (with A) |
| E (Decimal tag) | Weak | Yes | Clean | Reject |

---

## 6. What is NOT a type parameter

- `UnitId` (runtime catalog)
- Item, lot, location ids
- Conversion factors (item/lot/universal tables)
- Rounding policy
- CurrencyId
- Stocking vs purchase vs issue UoM **roles** (data on item master)

**IS** compile-time: dimension kind (`Length` vs `Mass` vs `Count`); optionally sealed APIs on ledger writers that only accept `Quantity<LengthDim>` for length-tracked items (module uses item metadata to pick API).

---

## 7. PLAN amendment (proposed text)

Replace PLAN.md §5 bullet:

> `Quantity` carrying its unit of measure as a type parameter

With:

> `Quantity<D: Dimension>` — a signed decimal amount with a runtime `UnitId`, where `D` is a **closed kernel dimension** (Count, Length, Mass, Time, …) so incompatible kinds do not compile. **Units of measure are not type parameters**; customer-defined and item-specific units are `UnitId` values interpreted by `datum-uom` via `ConversionContext`. `Money` is a separate primitive with `CurrencyId` and explicit scale. Ledger persistence stores amount + unit id (+ dimension kind denormalized where needed); SQL zero-sum is enforced per `(group, ledger, item, unit)` bucket.

**ADR 0002 tweak (non-blocking but honest):** Change “quantity in inches and millimetres can be different types” to “quantities of different **dimensions** are different types; **within** a dimension, unit compatibility is enforced at runtime via `UnitId` and conversion.”

---

## 8. EXECUTOR / process

| Step | Owner | Action |
|------|-------|--------|
| 1 | **Opus DECISION** | Ratify dimension-vs-unit split, money split, ledger bucket constraint — **before Wave 1 workspace freeze** |
| 2 | Grok/Cursor **workspace** | Stub `datum-core` public types per decision (not `Quantity<U: Unit>`) |
| 3 | Grok/Cursor **datum-uom** | `UnitConverter`, rounding, item UoM roles |
| 4 | Grok/Cursor **datum-ledger** | Posting row shape + property tests with normalized stocking UoM |

**Yes — this is an Opus DECISION that must land before the workspace lane freezes public types.** Thirteen crates will import `Quantity`; fixing dimension later is a workspace-wide rewrite.

---

## 9. SPLIT (crate boundaries)

| Crate | Responsibility |
|-------|----------------|
| `datum-core` | `UnitId`, `CurrencyId`, `Dimension`/`DimensionKind`, `Quantity<D>`, `Money`, `ConversionContext` (struct), `UnitConverter` (trait), errors |
| `datum-uom` | DB-backed unit master, universal + item + lot conversions, rounding, impl `UnitConverter` |
| `datum-ledger` | Posting types, group builder, normalization before insert, SQL mapping |
| `datum-db` | Migrations: `unit`, `item_uom`, `conversion_rule`, posting columns |

---

## 10. TIER: **deep**

`datum-core` + `datum-uom` + ledger bucket invariant warrant deep audit (per PLAN already for core; extend uom/ledger coupling).

---

## 11. Tests: core vs uom

| `datum-core` | `datum-uom` |
|--------------|-------------|
| `try_add` same/different `UnitId` | Conversion chains, rounding boundaries |
| Compile tests (or separate `tests/compile_fail/`) dimension mismatch | Property: convert → convert⁻¹ within epsilon policy |
| `Money` add same currency / reject mixed | Item-specific factor overrides universal |
| Serde round-trip `QuantityWire` | Fuzz arbitrary unit graph, no silent loss |
| — | Integration: issue UoM ≠ stocking UoM → ledger rows all stocking |

**`datum-ledger` property tests (PLAN §7):** generate posting sequences; assert per-bucket zero-sum and projection = sum(ledger).

---

## 12. Bottom line

`Quantity<U: Unit>` as PLAN states is **unworkable** and **misstates** what Rust gives you. The fix is **not** abandoning ADR 0002 — it is **parameterizing dimension, not unit**, and pushing all ERP-specific conversion/rounding to `datum-uom` with explicit context. PLAN and ADR 0002 prose must change **before Wave 1** stubs encode the wrong public API.
