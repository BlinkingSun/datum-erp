# DECISION D1 — The `datum-core` quantity, unit, and money contract

**Decider:** Opus (decision authority, team task `erp`, decision D1)
**Date:** 2026-09-11
**Status:** DECIDED. This is the frozen public contract for `datum-core`. Wave 1 builds to it.
**Inputs:** `_team/reports/sweep-plan-typed-qty.md`, `_team/reports/plan-audit.md` (§0.2, R2, G7, D1),
`_team/reports/sweep-plan-ledger.md`, `_team/reports/sweep-plan-proptests.md` §§387–394,
`PLAN.md` §5, `docs/02-architecture.md` §2, `docs/adr/0002-backend-language.md`, `docs/adr/0004-append-only-ledger.md`.
**Amends:** `PLAN.md` §5, `docs/adr/0002-backend-language.md`.

---

## 0. Ruling in one paragraph

PLAN §5 and ADR 0002 are wrong and are corrected in place. Unit of measure is **not** a type
parameter; it cannot be, because units are customer-defined rows in a table and Rust types are
not. What *is* a type parameter is **dimension** — a sealed kernel set of six kinds. Unit identity
is a runtime `UnitId` that can only enter a `Quantity<D>` through a checked witness. Money is a
separate primitive that shares no arithmetic with quantity. The numeric type is
`rust_decimal::Decimal` for both, at different bounded scales. `datum-core` performs **no rounding
at all**; it makes rounding un-ignorable by returning conversion and extension results in a type
whose value cannot be read without either proving exactness or naming a destination for the
residual. That last type, not the phantom dimension, is the guard that addresses the failure mode
ADR 0002 cites — and it addresses only half of it, which §7 and §9 state plainly.

---

## 1. Reconciliation with the ledger decision lane

`_team/reports/DECISION-ledger-invariant.md` **does not exist** as of this writing. I checked.
This decision therefore states its dependencies as explicit assumptions. Reconcile before Wave 1
freezes, or this contract needs a named amendment.

| # | Assumption about D2 (ledger) | If D2 decides otherwise |
|---|---|---|
| **A1** | Inventory conservation slice is `(group_id, item_id, unit_id)`. `unit_id` is a partition key of the balance bucket. | **This is the load-bearing one.** If the slice does not partition by `unit_id`, cross-dimension sums become expressible in SQL, the database stops being a backstop, and `Quantity<D>`'s phantom parameter goes from belt-and-braces to sole defence in the posting path. §7's answer changes and §3's verdict gets *stronger*, not weaker. |
| **A2** | Every inventory row in a group is normalized to one unit per item **before** `INSERT`. Conversion never happens in SQL. | If SQL may convert, `datum-db` needs the conversion graph, `datum-uom` loses its monopoly on rounding, and §4.3 is unenforceable. Refuse this. |
| **A3** | Cost and labor slices are `(group_id, ledger)` plus finer cost dimensions, and money never shares a numeric column with quantity (separate `amount` / `quantity` columns, per `sweep-plan-ledger.md` §2). | If one numeric column carries both, `Money` and `Quantity` become indistinguishable at the persistence boundary and acceptance criterion (d) survives only in Rust, not in the database. |
| **A4** | `lot_id` and `serial_id` are dimensions but **not** conservation keys. | Affects catch-weight (§5) only; catch-weight's eventual linkage wants lot/serial as a correlation key, not a balance key. No change to core types. |
| **A5** | Residual and loss destinations are ordinary postings to virtual locations inside the same group, and `ADJUSTMENT` requires a reason code (ADR 0004 already says this). | If residuals cannot be posted, §4.3 collapses to "refuse the transaction," which is a worse product and must then be a stated product decision rather than an accident of the type system. |

Nothing in §2's type sketches changes under any of A1–A5. That is deliberate: the core contract is
chosen so the ledger lane can still move without reopening `datum-core`.

---

## 2. The frozen contract

`datum-core` has **no** `sqlx` dependency, no database dependency, and no async. Dependencies are
`rust_decimal` (with `serde-with-str`), `serde`, and `thiserror`. Everything below is `pub`.

### 2.1 Identifiers and dimension

```rust
//! datum-core::units

/// Opaque catalog identifier. Core never interprets the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UnitId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CurrencyId(pub i32);   // ISO 4217 numeric; catalog row lives in the database

/// The sealed kernel dimension set. Adding a variant is a kernel change and a
/// migration, never a customization. Modules cannot extend it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DimensionKind { Count, Length, Mass, Time, Volume, Area }

mod private { pub trait Sealed {} }

/// Compile-time witness for exactly one `DimensionKind`.
pub trait Dimension: private::Sealed + Copy + core::fmt::Debug + 'static {
    const KIND: DimensionKind;
}

macro_rules! dimension { ($t:ident => $k:ident) => {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct $t;
    impl private::Sealed for $t {}
    impl Dimension for $t { const KIND: DimensionKind = DimensionKind::$k; }
}}
dimension!(CountDim  => Count);
dimension!(LengthDim => Length);
dimension!(MassDim   => Mass);
dimension!(TimeDim   => Time);
dimension!(VolumeDim => Volume);
dimension!(AreaDim   => Area);
```

Six dimensions, sealed. A customer unit that fits none of them is a modelling error and the item
uses `Count`; there is deliberately no `Other` variant, because an escape hatch reopens exactly
the hole this decision closes. Revisit trigger: a real customer unit that is genuinely a rate
(pieces per hour, parts per million) — those are not quantities and belong in their own type.

### 2.2 `UnitRef<D>` — the unforgeable bridge from runtime to compile time

This is the piece the sweep report did not have, and it is what makes acceptance criterion (f)
real rather than aspirational.

```rust
/// A `UnitId` that has been proven to belong to dimension `D`.
///
/// The only constructor is `checked`. A `Quantity<D>` cannot be built from a bare
/// `UnitId`, so "unit does not belong to its dimension" is unrepresentable once
/// construction has succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitRef<D: Dimension> {
    id: UnitId,                                  // private
    _d: core::marker::PhantomData<D>,
}

impl<D: Dimension> UnitRef<D> {
    /// `catalog_kind` is what the caller read from the unit master. Core performs the
    /// comparison; a caller cannot skip it, only lie about its own data.
    pub fn checked(id: UnitId, catalog_kind: DimensionKind) -> Result<Self, QuantityError> {
        if catalog_kind == D::KIND {
            Ok(Self { id, _d: core::marker::PhantomData })
        } else {
            Err(QuantityError::DimensionMismatch {
                unit: id, expected: D::KIND, actual: catalog_kind,
            })
        }
    }
    pub fn id(self) -> UnitId { self.id }
    pub fn kind(self) -> DimensionKind { D::KIND }
}
```

No `unsafe`, no `new_unchecked`, no sealed-token ceremony — and `datum-uom`, in a different crate,
can still mint one. PLAN invariant 7 is respected.

### 2.3 `Quantity<D>`

```rust
/// Maximum decimal places any quantity may carry. Matches the `numeric(24,8)` posting
/// column. A quantity with more precision is an error, never a truncation.
pub const QUANTITY_MAX_SCALE: u32 = 8;

/// A signed decimal amount in a specific unit of a specific dimension.
/// Signed because postings are signed. Non-negativity is a business rule, not a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Quantity<D: Dimension> {
    amount: Decimal,               // private; invariant: scale <= QUANTITY_MAX_SCALE
    unit: UnitRef<D>,              // private
}

impl<D: Dimension> Quantity<D> {
    /// Rejects scale > QUANTITY_MAX_SCALE and overflowing input.
    /// Never rounds. Never truncates.
    pub fn new(amount: Decimal, unit: UnitRef<D>) -> Result<Self, QuantityError>;
    pub fn zero(unit: UnitRef<D>) -> Self;

    pub fn amount(self) -> Decimal;
    pub fn unit(self) -> UnitRef<D>;
    pub fn unit_id(self) -> UnitId;
    pub fn kind(self) -> DimensionKind { D::KIND }
    pub fn is_zero(self) -> bool;
    pub fn signum(self) -> i8;

    // --- arithmetic: fallible, same-unit only, no operator traits ---
    pub fn try_add(self, rhs: Self) -> Result<Self, QuantityError>;
    pub fn try_sub(self, rhs: Self) -> Result<Self, QuantityError>;
    pub fn try_add_assign(&mut self, rhs: Self) -> Result<(), QuantityError>;
    pub fn negate(self) -> Self;
    pub fn abs(self) -> Self;

    /// Ordering must be fallible: `10 ft > 36 in` compared by `amount` is a bug that
    /// would otherwise compile and return a wrong `bool`.
    pub fn try_cmp(self, rhs: Self) -> Result<core::cmp::Ordering, QuantityError>;

    /// Scalar scaling (BOM multiplier, yield factor). Exact or error.
    pub fn try_scale_exact(self, factor: Decimal) -> Result<Self, QuantityError>;
    /// Scalar scaling that hands you the residual. `Scaled<D>` is un-ignorable (§2.6).
    pub fn scale(self, factor: Decimal, to_scale: u32) -> Scaled<D>;

    /// Dimensionless ratio. There is deliberately no `Div` impl on `Quantity`.
    pub fn try_ratio(self, rhs: Self) -> Result<Decimal, QuantityError>;

    /// Summation. `None` for an empty iterator, because a zero has no unit.
    pub fn try_sum<I: IntoIterator<Item = Self>>(items: I)
        -> Result<Option<Self>, QuantityError>;
}
```

Deliberate omissions, each load-bearing:

- **No `impl Add`, `Sub`, `AddAssign`, `Sum`, `Neg`.** An infallible operator would have to either
  panic or silently pick a unit on mismatch. `try_add` is the only door.
- **No `impl Div`, no `Mul<Decimal>`.** Division is where residuals are born.
- **No `PartialOrd`, no `Ord`.** See `try_cmp`.
- **No `Default`.** A default quantity has no unit.
- **No `Serialize`/`Deserialize`.** A type parameter cannot round-trip through JSON; serializing
  `Quantity<D>` would either drop `D` or fabricate it on read. See §6.

### 2.4 Erasure for heterogeneous collections, the wire, and the database

```rust
/// The boundary type. What crosses HTTP. What `datum-db` maps to a row. The only
/// serde-bearing quantity representation in the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnyQuantity {
    pub amount: Decimal,           // serialized as a JSON string, never a float
    pub unit: UnitId,
    pub dimension: DimensionKind,  // denormalized so the boundary can check itself
}

impl<D: Dimension> From<Quantity<D>> for AnyQuantity { /* infallible */ }

impl AnyQuantity {
    /// Runtime -> compile time. The one place a dimension error can occur on read.
    pub fn downcast<D: Dimension>(self) -> Result<Quantity<D>, QuantityError>;
    /// Same-unit arithmetic on erased values, for generic ledger code that must not
    /// know the dimension. Requires equal `unit` AND equal `dimension`.
    pub fn try_add(self, rhs: Self) -> Result<Self, QuantityError>;
    pub fn try_sum<I: IntoIterator<Item = Self>>(i: I)
        -> Result<Option<Self>, QuantityError>;
}
```

`AnyQuantity` is a first-class peer of `Quantity<D>`, not an escape hatch. Generic ledger,
serialization, and reporting code is written against the erased type; domain computation is
written against `Quantity<D>`. That division of labour is what keeps the phantom parameter from
infecting the whole workspace (§3). The rule for contributors is one sentence: **domain math is
`Quantity<D>`, plumbing is `AnyQuantity`.**

### 2.5 Money and unit cost

```rust
//! datum-core::money

/// Money carries up to 6 decimal places: enough for 4 sub-minor digits on a 2-minor
/// currency, which is what extended amounts and landed-cost proration actually need.
/// Storage column: numeric(24,6).
pub const MONEY_MAX_SCALE: u32 = 6;
/// Unit costs and rates carry 8. Storage column: numeric(24,8).
pub const RATE_MAX_SCALE: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Money {
    amount: Decimal,               // private; invariant: scale <= MONEY_MAX_SCALE
    currency: CurrencyId,          // private
}

impl Money {
    pub fn new(amount: Decimal, currency: CurrencyId) -> Result<Self, MoneyError>;
    pub fn zero(currency: CurrencyId) -> Self;
    pub fn amount(self) -> Decimal;
    pub fn currency(self) -> CurrencyId;

    pub fn try_add(self, rhs: Self) -> Result<Self, MoneyError>;
    pub fn try_sub(self, rhs: Self) -> Result<Self, MoneyError>;
    pub fn negate(self) -> Self;
    pub fn try_cmp(self, rhs: Self) -> Result<core::cmp::Ordering, MoneyError>;
    pub fn try_sum<I: IntoIterator<Item = Self>>(i: I)
        -> Result<Option<Self>, MoneyError>;

    /// Bring an amount to a currency's minor-unit scale. Returns value AND residual;
    /// the caller decides where the residual goes. `Settled` is un-ignorable.
    pub fn settle(self, minor_exponent: u32, rule: Rounding) -> Settled;

    /// Exact split by integer weights, largest-remainder. The sum of the output equals
    /// `self` exactly. No residual exists, therefore none can be lost. This is the
    /// ergonomic answer for landed cost, freight, and overhead proration.
    pub fn allocate(self, weights: &[u64], scale: u32) -> Result<Vec<Money>, MoneyError>;
}

/// Price or cost per one unit of a dimensioned quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitCost<D: Dimension> {
    amount: Decimal,               // private; invariant: scale <= RATE_MAX_SCALE
    currency: CurrencyId,
    per: UnitRef<D>,               // the unit the cost is quoted in
}

impl<D: Dimension> UnitCost<D> {
    pub fn new(amount: Decimal, currency: CurrencyId, per: UnitRef<D>)
        -> Result<Self, MoneyError>;
    /// cost x quantity. Requires `qty.unit() == self.per` (typed error otherwise) and
    /// yields an `Extended`, carrying the residual the multiplication created.
    pub fn extend(self, qty: Quantity<D>, to_scale: u32) -> Result<Extended, MoneyError>;
}
```

`Money` and `Quantity<D>` share no trait, no operator, and no conversion. There is no
`From<Quantity<D>> for Money`, no `MoneyDim`, and no shared `Measure` supertrait. Criterion (d)
holds because the two types are simply unrelated — the cheapest possible mechanism.

`UnitCost<D>` is included in the freeze, rather than deferred to a costing crate, because it is
the one place money and quantity legitimately meet and its signature has to be settled before
thirteen crates import `datum-core`.

### 2.6 The residual types — the actual guard

```rust
//! datum-core::rounding

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Rounding {
    HalfUp,          // default for money settlement
    HalfEven,        // bankers'; cost proration across many lines, avoids drift
    TowardZero,      // only where a physical count cannot exceed what exists
    AwayFromZero,
}

/// The result of an operation that may not divide evenly.
///
/// Private fields, no `Deref`, no `Into`, no getter that yields the value alone. The
/// only two exits are `into_exact` (typed error if a residual exists) and `split`
/// (hands you both halves). `#[must_use]` so discarding the whole value is a lint fail.
#[must_use = "a conversion residual must be proven zero or given a destination"]
#[derive(Debug, Clone, Copy)]
pub struct Converted<D: Dimension> {
    value: Decimal, residual: Decimal, unit: UnitRef<D>, scale: u32,   // all private
}

impl<D: Dimension> Converted<D> {
    /// Succeeds only when the operation divided evenly at the target scale.
    pub fn into_exact(self) -> Result<Quantity<D>, ResidualError>;
    /// Value at the target scale under `rule`, plus the residual as a quantity in the
    /// same unit. `value + residual` reconstructs the pre-rounding amount exactly.
    pub fn split(self, rule: Rounding) -> (Quantity<D>, Quantity<D>);
    pub fn has_residual(self) -> bool;
    /// Inspection only: reveals the residual without releasing the value.
    pub fn peek_residual(self) -> Decimal;
}

/// Same shape, same discipline, for the three other places a residual is born.
#[must_use] pub struct Scaled<D: Dimension>;  // -> (Quantity<D>, Quantity<D>)
#[must_use] pub struct Settled;               // -> (Money, Money)
#[must_use] pub struct Extended;              // -> (Money, Money)

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ResidualError {
    #[error("operation left residual {residual} in unit {unit:?}; use split() and post \
             the residual, or reject the transaction")]
    NotExact { residual: Decimal, unit: UnitId },
}
```

You cannot write `converter.convert(..)?.value` — the field is private. You cannot write
`converter.convert(..)?.into()` — no `Into` impl exists. The compiler forces the author to type
either `into_exact()?` or `split(rule)` and bind two names. Binding the second name and dropping
it is possible — that *is* the residual being discarded — and it is caught by the workspace's
`unused_variables` deny, plus the `datum-ledger` property test asserting every slice sums to zero.
Belt, braces, and a database constraint.

### 2.7 The conversion interface (trait in core, implementation in `datum-uom`)

```rust
/// Item and lot context, because a conversion factor in this domain is item data and
/// sometimes lot data (bar stock lb-per-ft varies by heat lot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversionContext { pub item: ItemId, pub lot: Option<LotId> }

pub trait UnitCatalog {
    fn dimension_of(&self, unit: UnitId) -> Result<DimensionKind, QuantityError>;
    /// Decimal places this unit is tracked to for this item. Drives `Converted::split`.
    fn scale_of(&self, unit: UnitId, ctx: &ConversionContext) -> Result<u32, QuantityError>;

    /// Resolve a raw id into a dimension-proven reference. Blanket-provided.
    fn resolve<D: Dimension>(&self, unit: UnitId) -> Result<UnitRef<D>, QuantityError> {
        UnitRef::<D>::checked(unit, self.dimension_of(unit)?)
    }
}

pub trait UnitConverter: UnitCatalog {
    /// Within a dimension only — the signature makes cross-dimension conversion
    /// unwritable. Returns `Converted`, never a bare `Quantity`.
    fn convert<D: Dimension>(
        &self,
        qty: Quantity<D>,
        to: UnitRef<D>,
        ctx: &ConversionContext,
    ) -> Result<Converted<D>, QuantityError>;
}
```

`convert` returning `Converted<D>` rather than `Quantity<D>` is the single most important line in
this document.

### 2.8 Errors

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum QuantityError {
    #[error("unit {unit:?} is dimension {actual:?}, expected {expected:?}")]
    DimensionMismatch { unit: UnitId, expected: DimensionKind, actual: DimensionKind },
    #[error("cannot combine quantities in units {left:?} and {right:?} without conversion")]
    UnitMismatch { left: UnitId, right: UnitId },
    #[error("scale {found} exceeds maximum {max}")]
    ScaleExceeded { found: u32, max: u32 },
    #[error("arithmetic overflow")]                                  Overflow,
    #[error("division by zero")]                                     DivideByZero,
    #[error("operation would not be exact")]                         Inexact,
    #[error("no conversion path from {from:?} to {to:?} for item {item:?}")]
    NoConversionPath { from: UnitId, to: UnitId, item: ItemId },
    #[error("unknown unit {0:?}")]                                   UnknownUnit(UnitId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MoneyError {
    #[error("cannot combine currency {left:?} with {right:?}")]
    CurrencyMismatch { left: CurrencyId, right: CurrencyId },
    #[error("cost is quoted per {cost_unit:?} but quantity is in {qty_unit:?}")]
    RateUnitMismatch { cost_unit: UnitId, qty_unit: UnitId },
    #[error("scale {found} exceeds maximum {max}")]
    ScaleExceeded { found: u32, max: u32 },
    #[error("arithmetic overflow")]                Overflow,
    #[error("allocation weights sum to zero")]     EmptyAllocation,
}
```

---

## 3. Does the phantom dimension earn its keep? Honestly.

I argued this against myself and it came out closer than the recommendation implies.

**The case against keeping it, at full strength.**

1. The dimension of a unit is *data*. It arrives as `unit_id = 42` from a row. At that boundary you
   must do a runtime check no matter what you do. So the compile-time parameter only protects code
   *downstream* of a check that already happened at runtime. It buys earlier detection inside a
   function body, not detection where the mistake originates.
2. Under A1, **the database already prevents cross-dimension summation**, because the conservation
   slice partitions by `unit_id`. Feet and pounds are different buckets and each must independently
   sum to zero. A developer who adds a mass row to a length group does not corrupt inventory — the
   commit fails. On the entire posting path, which is where the money is, the phantom parameter is
   redundant with the ledger constraint.
3. Generic infection is a real tax. Every helper that touches a quantity becomes
   `fn f<D: Dimension>(..)`. Mixed collections need erasure. Trait objects need care. Every
   contributor pays it, and ADR 0002 already concedes the contributor pool is the real price of
   Rust.
4. You need `DimensionKind` at runtime anyway (§2.4), so you maintain two representations of one
   fact and must keep them agreeing.

**The case for keeping it.**

1. There is a large body of computation that **never touches the posting table** and therefore has
   no database backstop: MRP netting, BOM rollup and explosion, capacity and routing math, cost
   rollup, yield and scrap factor arithmetic, genealogy aggregation. PLAN §8 explicitly names MRP
   and genealogy traversal as real multi-threaded compute. A dimension error in an MRP
   net-requirements calculation produces a wrong purchase recommendation, never a rejected commit.
   That zone is where a phantom parameter is the only guard, and it is not small.
2. Cost is genuinely zero. `PhantomData` occupies no bytes, `Quantity<D>` is `Copy`, and no runtime
   branch is added.
3. Signatures become self-documenting in a way a `kind` field cannot:
   `fn cut_length(stock: Quantity<LengthDim>, kerf: Quantity<LengthDim>)` cannot be called with
   hours.
4. The runtime alternative returns `Err`, and an `Err` in a `?` chain becomes one variant of a
   generic error enum that a caller can map, log, and continue past. That is precisely the "silent
   for months" shape ADR 0002 is afraid of. A compile error cannot be swallowed.

**Ruling: the phantom dimension survives, on a narrowed justification.** It is kept for the
non-ledger computation zone in point 1, not because it protects the posting path — on the posting
path, under A1, it is redundant belt-and-braces and I will not pretend otherwise. It is kept
affordable by promoting `AnyQuantity` to a first-class peer so plumbing never becomes generic.

**The ceremony I am cutting, because it was aesthetics:** no phantom currency parameter on `Money`
(currencies are catalog rows — `Money<USD>` would be PLAN §5's original mistake at a different
address); no `Measure` supertrait unifying money and quantity; no two-parameter `Quantity<D, U>`;
no const-generic scale.

---

## 4. Numeric representation and rounding

### 4.1 Representation

| Type | In Rust | Max scale | PostgreSQL column |
|---|---|---|---|
| `Quantity<D>` | `rust_decimal::Decimal` | 8 | `numeric(24,8)` |
| `Money` | `rust_decimal::Decimal` | 6 | `numeric(24,6)` |
| `UnitCost<D>` | `rust_decimal::Decimal` | 8 | `numeric(24,8)` |

**`rust_decimal` over a fixed-point `i128`.** `Decimal` is a 96-bit mantissa plus a scale byte:
16 bytes, `Copy`, no heap allocation, exact base-10 arithmetic. Four reasons it beats a
hand-rolled `i128` at scale 8:

1. **Native `numeric` codec.** `rust_decimal`'s `db-postgres` feature encodes and decodes
   PostgreSQL `numeric` directly. A fixed-point integer means every read and write goes through a
   manual scale conversion — new, hand-written code in the one place precision loss must not happen.
2. **Scale is preserved.** `1.50` and `1.5` are distinguishable. An operator who counted to two
   decimal places entered two decimal places, and a regulated system that reprints a record should
   reprint what was entered. Fixed-point erases this.
3. **Exact base-10 semantics** with a documented, tested `round_dp_with_strategy`, which is what
   `Converted::split` delegates to. Rolling that myself is the kind of code that has a half-even
   bug at negative values and nobody notices for a year.
4. It is the mainstream choice with real production mileage. ADR 0002 already laments the thinness
   of Rust business-domain libraries; spending the budget here buys nothing.

**The costs, stated.** `Decimal` division is inexact and its maximum scale is 28, so a chain of
multiplications can exhaust precision before any rounding is requested. Mitigations, all enforced
in core: scale is validated at every construction; `try_scale_exact` errors rather than rounds;
`try_ratio` returns a raw `Decimal` that is not a `Quantity` and therefore cannot be posted;
arithmetic returns `Overflow` rather than saturating. Also: PostgreSQL `numeric` has effectively
unbounded range, so a hand-written `INSERT` of `1e30` would decode as an *error* rather than a
wrong number — and the `numeric(24,8)` column type stops it being written at all.

**Why `24` and `8`.** Eight fractional digits covers troy-ounce precious metal, gram-level mass on
a kilogram base, and ten-thousandths of an inch with four digits to spare — the tightest real
tolerance in the target market. Sixteen integer digits is a quadrillion of anything. Twenty-four
significant digits sits safely inside `Decimal`'s 28. Six for money leaves four sub-minor digits,
which is what proration and unit pricing consume; settlement to two happens via `Money::settle`.

### 4.2 Where rounding happens, and who may do it

| Crate | Rounding authority |
|---|---|
| `datum-core` | **None.** Core defines `Rounding` and the residual types. Core never rounds a value. There is no `round()` on `Quantity`, `Money`, or `UnitCost`. |
| `datum-uom` | Owns the *policy lookup*: for `(item, unit, operation)` it supplies the target scale and the `Rounding` rule from the unit master. It calls `Converted::split`; it does not invent arithmetic. |
| `datum-ledger` | Owns *where a residual is posted*. It is the only crate that may turn a residual into a posting row. |
| Modules | May choose to reject rather than round. May never round. |

Defaults: `HalfUp` for money settlement (matches invoice convention and customer expectation),
`HalfEven` for cost proration across many lines (avoids systematic drift), `TowardZero` only where
a physical count cannot exceed what exists.

### 4.3 The residual's one home

**An inventory conversion residual is posted to `ADJUSTMENT` with reason code `UOM_ROUNDING`, in
the same posting group, in the same unit as the rest of its slice.**

`ADJUSTMENT` is chosen over a new `ROUNDING` virtual location because ADR 0004 already names
`ADJUSTMENT`, already requires a reason code on it, and already tells an inspector what it means.
Inventing a second virtual location for the same concept adds a row to the location master and a
paragraph to every audit; the reason code carries the distinction.

**A money residual normally does not exist**, because `Money::allocate` is exact by construction
(largest-remainder: the parts sum to the whole). Where one is genuinely created — `settle` on a
converted foreign amount, `extend` of an 8-scale unit cost against an 8-scale quantity — it posts
to a `ROUNDING` cost element in the same group under A3.

**Why this is one rule and not two.** The ordinary transfer case creates **no residual at all**,
and this is worth stating because it is the case everyone worries about. Issue 100 inches of a bar
stocked in feet: `datum-uom` converts *once*, gets `8.3333` ft plus a residual, and then **both**
the `-` row from `STOCK` and the `+` row to `WIP` carry `8.3333`. The slice sums to zero by
construction. The residual is a *measurement* discrepancy between the physical world and the
record, not a conservation violation, and it surfaces where measurement discrepancies belong: at
cycle count, as an `ADJUSTMENT`. Residuals reach the ledger only when a single side is converted
independently — a partial depletion computed from a remainder, or a cost proration.

The invariant, in one sentence for the ledger lane to adopt verbatim:

> Conversion happens exactly once per posting group, before any row is constructed; every row in a
> conservation slice carries the same unit at the same scale; a residual produced by that single
> conversion is either proven zero, posted to `ADJUSTMENT` with reason `UOM_ROUNDING` in the same
> group, or the transaction is refused.

---

## 5. Catch-weight

**Out of scope for v1.** The titanium billet counted as one piece and consumed by mass is a real
requirement in the target market and it is not a v1 requirement.

**The reason, not the excuse.** Catch-weight is not a quantity-*type* problem. `Quantity<D>` and
the `(group, item, unit)` slice already handle the arithmetic: a catch-weight item posts an `EA`
slice and a `KG` slice in the same group, each balancing independently. That part is free. What is
not free is everything around it, and none of it lives in `datum-core`:

1. **Item master.** "One item has one stocking unit" becomes false. Every screen, API response, and
   report that says "quantity on hand" must answer *in which measure*.
2. **Allocation and availability.** Reserving one piece and reserving 12.4 kg are different
   promises; shortage calculation needs to know which measure is authoritative.
3. **Costing.** Cost must be driven by the mass measure, and the count measure must not
   independently value inventory, or the same billet is valued twice.
4. **MRP.** Netting a mass-consumed item in eaches produces a wrong purchase order.
5. **Genealogy.** A serial with a unique mass means the `EA` and `KG` slices must be correlated by
   `serial_id`, which under A4 is a dimension rather than a conservation key — so the correlation is
   a query, not a constraint, and it can silently go missing.

**What breaks when catch-weight arrives.** Not `datum-core`. `Quantity<D>` is already
dimension-parameterized, `UnitRef<D>` already proves membership, and the slice already partitions
by unit — the §2 contract survives verbatim. What changes is the item master schema, the
availability and allocation APIs, the costing driver, and MRP's unit selection. The honest risk is
point 5: the correlation between an item's two measures has no database guard, so a catch-weight
implementation that forgets to post the paired slice will balance *and be wrong*. That hole is the
reason this is deferred rather than half-built.

**Explicit prohibition for Waves 1 and 2**, matching `sweep-plan-proptests.md` §389: a posting row
carries exactly one measure. No row carries two quantities. A weight recorded alongside a count is
an *attribute* on the lot or serial, never a conservation key, and property-test generators do not
emit catch-weight pairs.

**Revisit trigger:** the first beachhead customer whose stock includes a mass-consumed serialized
item. At that point this becomes an ADR, not a patch.

---

## 6. Serialization boundaries

`datum-core` depends on `serde` and **not** on `sqlx`. Three representations, one per boundary.

| Boundary | Type | Encoding |
|---|---|---|
| **In-process domain math** | `Quantity<D>`, `Money`, `UnitCost<D>` | None. These types are deliberately **not** `Serialize`/`Deserialize`: a type parameter cannot round-trip through JSON, so serializing `Quantity<D>` would either drop `D` or fabricate it on read. |
| **HTTP API** | `AnyQuantity`, `MoneyWire` | JSON, with `Decimal` as a **string**, never a JSON number, because `serde_json` numbers are `f64` in most clients: `{"amount":"8.33330000","unit":42,"dimension":"Length"}`. `dimension` is denormalized so a client or middleware can reject a mismatch without a catalog round-trip. |
| **PostgreSQL** | `AnyQuantity`, `MoneyWire` | `datum-db` implements `sqlx::FromRow`/`Encode` over these: `amount -> numeric(24,8)`, `unit -> bigint`, `dimension -> text` or a PG enum. Core's only obligation is that these structs have public fields, which they do. |

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MoneyWire { pub amount: Decimal, pub currency: CurrencyId }
impl From<Money> for MoneyWire { /* infallible */ }
impl TryFrom<MoneyWire> for Money { type Error = MoneyError; /* scale-checked */ }
```

The read path is always the same three steps, and `datum-db` is the only crate that writes them:
row → `AnyQuantity` → `downcast::<D>()`. The dimension check is not skippable, because
`AnyQuantity` has no other exit toward `Quantity<D>`.

Core does **not** own the DDL. `numeric(24,8)` and `numeric(24,6)` are frozen here as the column
types and `datum-db` writes the migration; `QUANTITY_MAX_SCALE` and `MONEY_MAX_SCALE` are the
single source of truth, and `datum-db` carries a test asserting the migration's scale matches them.

---

## 7. The one guard

ADR 0002 justifies Rust partly on: *"a unit conversion bug corrupts inventory silently for
months."* Decompose that failure mode.

| Sub-mode | Prevented? | By what |
|---|---|---|
| **(i) Wrong conversion factor in the unit master** — 12.0 entered as 1.2 | **No. Not by any type system.** | Data governance only: audited and e-signed changes to `conversion_rule`; `datum-uom` round-trip property tests; per-dimension sanity bounds. |
| **(ii) A residual silently dropped, so ledger and physical world diverge a little on every issue** | **Yes — this design prevents it.** | `Converted<D>` / `Scaled<D>` / `Settled` / `Extended`: private fields, no `Deref`, no `Into`, `#[must_use]`, and the only exits are `into_exact() -> Result<_, ResidualError>` and `split(rule) -> (value, residual)`. |
| **(iii) Adding unlike dimensions** | Yes, twice. | Compile error in domain math; rejected commit in the posting path via the `unit_id` slice key (A1). |
| **(iv) A `Quantity` built on a unit from the wrong dimension** | Yes. | `UnitRef::checked` is the only constructor; there is no `new_unchecked` and no `unsafe`. |

**The one guard, named:** `Converted<D>` — a conversion result whose value cannot be read without
either proving the residual is zero or binding it to a name. Everything else in this document is
either redundant with the database or a convenience.

**And the honest limit.** It stops (ii). It does not stop (i), and (i) is the more likely bug in
production, because entering a conversion factor is a data-entry task performed by a shop employee
and reviewed by nobody. **No type system prevents (i).** ADR 0002's central justification, as
written, therefore overstates what Rust delivers, and §9 amends the ADR rather than quietly
strengthening the prose around it.

---

## 8. Acceptance criteria — mechanism for each

| # | Requirement | Mechanism | Kind |
|---|---|---|---|
| **a** | Adding a length to a mass | `try_add(self, rhs: Self)` — `Quantity<LengthDim>` and `Quantity<MassDim>` are distinct types. No `Add` impl, no `Measure` trait, no coercion. Second defence: the `(group, item, unit_id)` slice puts them in different buckets, each of which must independently sum to zero (A1). | **Does not compile** |
| **b** | Adding two lengths in different units without explicit conversion | `try_add` compares `self.unit.id() == rhs.unit.id()` and returns `QuantityError::UnitMismatch { left, right }`. Cannot be compile-time: `UnitId` is a database row. Third defence: `try_cmp` is fallible too, so `10 ft > 36 in` is an error rather than a wrong `bool`. | **Typed error** |
| **c** | Adding money in two currencies | `Money::try_add` returns `MoneyError::CurrencyMismatch { left, right }`. Runtime for the same reason as (b), and deliberately *not* a phantom currency parameter, which would repeat PLAN §5's original mistake at a different address. | **Typed error** |
| **d** | Adding a quantity to a money value | `Money` and `Quantity<D>` share no trait, no operator, and no `From`/`Into`. Unrelated types. Under A3 they also occupy different database columns. | **Does not compile** |
| **e** | Silently truncating a conversion residual | Four locks. (1) `UnitConverter::convert` returns `Converted<D>`, never `Quantity<D>`. (2) `Converted` has private fields, no `Deref`, no `Into`, and is `#[must_use]`. (3) The only exits are `into_exact() -> Result<_, ResidualError>` and `split(rule) -> (Quantity<D>, Quantity<D>)`. (4) `Quantity::new` rejects scale > 8 with `ScaleExceeded` rather than truncating, and there is no `Div` and no `round` on `Quantity`. | **Does not compile** to ignore the residual; **typed error** (`ResidualError::NotExact`) when exactness is demanded and not available |
| **f** | Constructing a quantity whose unit does not belong to its dimension | `Quantity::new` takes `UnitRef<D>`, not `UnitId`. `UnitRef::<D>::checked(id, catalog_kind)` is the sole constructor and compares against `D::KIND`, returning `DimensionMismatch`. No `new_unchecked`, no `unsafe`. Unrepresentable once constructed; a typed error at the one boundary where it can be attempted. | **Does not compile** to bypass; **typed error** at the catalog boundary |
| **g** | Summing a column of postings, same item and unit | `Quantity::try_sum(iter) -> Result<Option<Self>, QuantityError>` — one unit check per element, `None` on empty because a zero has no unit. Plus `Quantity::zero(unit_ref)` and `try_add_assign` for fold-style code, and `AnyQuantity::try_sum` for erased ledger rows. | **Permitted, ergonomic** |
| **h** | Converting feet to inches for an item whose factor is item data | `converter.convert(qty, to_inches, &ConversionContext { item, lot })?` — `ConversionContext` is a core struct carrying exactly the keys a factor may depend on; the factor lookup and the conversion graph live in `datum-uom`. Returns `Converted<D>`, so (e) applies. | **Permitted, ergonomic** |
| **i** | Storing and reloading through PostgreSQL without losing precision | `Decimal` at scale ≤ 8 into `numeric(24,8)` via `rust_decimal`'s native `numeric` codec; 24 significant digits sits inside `Decimal`'s 28. Core exposes `AnyQuantity` with public fields and no `sqlx` dependency; `datum-db` owns `FromRow`. Overlong values are refused by the column type and out-of-range values decode as an error, never a wrong number. `datum-db` carries a round-trip property test and a test asserting migration scale equals `QUANTITY_MAX_SCALE`. | **Permitted, verified by test** |

---

## 9. Effect on ADR 0002

**The Rust decision stands. Its second stated reason does not, as written.**

- "One static binary per platform with no runtime to install" — untouched, and the ADR already
  calls it "the single largest factor." The decision rests on this.
- "No garbage collector pause during a long MRP run" — untouched.
- "A quantity in inches and a quantity in millimetres can be different types that will not add" —
  **false**, and corrected. Inches and millimetres are rows in a table.

What Rust actually delivers here, precisely: dimensions are distinct types; money and quantity are
distinct types; a residual cannot be read without being addressed; state enums are exhaustively
matched; newtypes prevent passing an `ItemId` where a `LotId` belongs. That is a real and useful
list. It is a shorter list than the ADR claimed.

**Does this weaken the case for Rust? Yes, at one specific joint, and the ADR now says so.** The Go
comparison is the casualty. ADR 0002 rejected Go on type-system strength for units of measure. Go
can express distinct dimension types with named types, and it can make a residual awkward to ignore
with a second return value that lint-style tooling polices. What Go cannot do is make ignoring it a
*compile* error rather than a lint finding. That is a narrower gap than "units of measure are things
Rust checks at compile time and Go checks in tests," which is what the ADR asserts. The honest
position: Go loses on the type system by a margin, not by a mile; the decisive reasons are packaging
and no-GC, both of which Go also satisfies — which is exactly why the ADR calls it "the closest
call." The ADR's own revisit trigger, contributor velocity becoming the binding constraint, is
therefore *more* live after this correction, not less.

This is recorded in the ADR rather than smoothed over. A justification quietly patched to match a
design it no longer supports is worth nothing to whoever reads it in two years.

---

## 10. Consequences for other lanes

| Lane | Obligation |
|---|---|
| `workspace` (Wave 1) | Stub `datum-core` exactly per §2. `rust_decimal` with `serde-with-str`; **no** `sqlx`. `Quantity<D>` is not `Serialize`. |
| `datum-core` (Wave 1 — complete, not stubbed, per audit G7) | Full implementation plus compile-fail tests under `tests/compile_fail/` for (a), (d), and reading `Converted`'s value. |
| `datum-uom` (Wave 2) | `impl UnitCatalog + UnitConverter`; owns scale and `Rounding` policy per `(item, unit, operation)`; calls `Converted::split`; owns conversion round-trip property tests. |
| `datum-ledger` (Wave 2) | Sole authority to post a residual. Normalizes to one unit per slice before `INSERT`. Rows carry exactly one measure. |
| `datum-db` (Wave 2) | `numeric(24,8)` / `numeric(24,6)`; `FromRow` for `AnyQuantity` and `MoneyWire`; scale-vs-constant test; round-trip precision test. |
| `doc-datamodel` (Wave 1) | Column types and the three-representation boundary table from §6. |
| `doc-api` (Wave 1) | `Decimal` on the wire is a **string**. `AnyQuantity` is the API contract shape. |
| D2 ledger decision | Adopt §4.3's invariant sentence verbatim, or tell me which of A1–A5 changed. |

---

## 11. Open item handed back

One thing I am deliberately not deciding, because it is not mine: whether `Time` is the right home
for labor hours, or whether labor belongs in its own ledger with its own measure (A3 hints at the
latter). `TimeDim` exists in §2.1 either way, so this does not block the freeze. Flagged so the
cost and labor grain decision does not discover it late.
