# DECISION — `PostingSink`, `SignatureGate`, installation profiles

**Authority:** opus (decision lane, sole writer of this file) · **Date:** 2026-09-12
**Inputs:** `CONTRACT-workspace.md` §6.1–§6.3 (as amended today) · `sweep-p2c-traits.md` (findings 1–12 + proposed text) · `sweep-p2c-profiles.md` · `plan-audit.md` §1 (R2/R3/R4), §3, §6 · D2 `ledger-invariant.md` §2, §4.1, §5.3, §9.2, §9.4 · D3 `audit-persistence.md` §9, §10 d/e · `docs/03-module-system.md` §3.2, §5, §6 · `PLAN.md` §1a, §3 Wave 2s.
**Status:** binding on the `core-r1`/`core-r2` race, CONTRACT §6.2–§6.3, SPEC-core, later SPEC-ledger / SPEC-statemachine / SPEC-profiles.

---

## Q1 — `PostingSink`: sufficient for Wave 2s, with four bindings and four error variants

**Verdict: the amended §6.2 *types* are sufficient; the amended §6.2 *rule set* is not, by one hole.**

Costs first. Every field added to §6.2 is frozen for the life of the major version and is copied
by hand into a race that is already running; every rule added is a test somebody must write in
`datum-ledger` or `datum-statemachine` before Wave 2s can close. So I add one type, four error
variants and four normative bullets, and I refuse everything else the slice proposed.

Against the four postings of the slice: a **receipt** is `+qty` at a bin plus `−qty` at
`Boundary::Supplier` plus `Inventory`/`ApAccrual` value rows — P1, P2-A, P2-B satisfied, P3 not
engaged (D2 §2 P3 slices only rows with `boundary IS NULL AND quantity < 0`). An **issue to a
work order** engages P3 on exactly the bin withdrawal; the seam row is `Consumed` and therefore
exempt. A **completion** posts the finished lot `+qty` at a real location (positive → exempt) and
a `Produced` seam row (bounded → exempt). All four express themselves in `QuantityPosting`,
`ValuePosting` and `ConsumptionPosting` with no shape missing. P0 and §9.4 are carried by one
sink per transaction + single `finalize` + header/postings/consumption in that transaction, which
is what makes `created_xid` identical across the group and the header (D2 §4.1, §2 P0).

**The hole: the produced lot's lineage is not implied by P3.** D2 §5.3's "the graph is total"
follows from P3, and P3 constrains *withdrawals only*. The finished-lot posting is positive and
the `Produced` seam is bounded, so nothing in the database forces an edge from the finished lot
back to the material it came out of — a `TRANSFORMATION` can satisfy P0–P4 and still leave Wave 2s
acceptance 8 (forward trace from the heat = backward trace from the finished lot) returning two
different trees. The types can express it (a `Consumption` intent whose `consuming` is the
finished-lot handle); no rule requires it. Bullet 6 below requires it, and `LineageRequired`
names the failure.

### (a) `consumed_posting_id`: typed newtype, not bare `i64`

`pub struct PostingId(pub i64)` in §6.2. Cost: three lines in the frozen surface and a `.0` at
the SQL boundary. Benefit: §6.1 already ships `UnitId(pub i64)`, so a bare `i64` field invites
`unit.0` or a widened `PostingHandle` to compile; and the database cannot save us here — a wrong
but *existing* `posting_id` passes the `ledger.consumption` foreign key (D2 §5.3) and passes P3,
which only checks sums against the consuming posting. The result is a valid group with corrupted
genealogy, which is the one class of error this system exists to prevent. The newtype does not
catch a wrong-item layer either; bullet 5 does.

### (b) `finalize` belongs on the trait

On the trait, with the object-safe by-value receiver. The transition executor lives in
`datum-statemachine`, which **must not** depend on `datum-ledger` — that edge is the cycle R5
broke and CONTRACT §4 forbids. If `finalize` existed only on the ledger's concrete builder, the
only callers left are the ledger (which cannot know when the hooks are done) or the composition
root (which puts the P0 obligation outside the transaction), i.e. exactly the "remember to call
it" failure class ADR 0005 rejects for audit. Hooks receive `&mut dyn PostingSink` and therefore
cannot finalize; the executor holds the `Box` and therefore must.

### (c) the automatic allocator satisfies §9.2 for money, not for quantity — say so

Ratified as necessary: requiring hooks to name layers pushes FIFO/average/standard costing into
modules, which D2 §5.3 puts in the ledger. But the independence claim must be priced honestly.
A FIFO loop that consumes layers *until the requested quantity is met* produces a quantity total
that equals the request **by construction**; for the quantity half of P3 the automatic allocator
is a checksum, which is the degeneration D2 §9.2 warns about. It is genuinely independent on the
**money** half: each edge's `amount` comes from the consumed layer's own stored quantity and
amount, while `ValuePosting.amount` comes from the valuation engine, and P3's second predicate
compares them. Therefore: `p3_independent_sources` is re-scoped in SPEC-ledger to assert the
money half (corrupt one layer's unit cost → P3 fails) plus "the allocator never reads the
consuming posting's value rows"; quantity-side redundancy exists only when a hook contributes an
**explicit** pick, so `mod-inventory`'s issue path contributes explicit edges whenever the
operator named lots — the canonical example set does, so Wave 2s acceptance 5 gets one honestly
two-source P3 case. The allocator must re-derive remaining layer quantity from the ledger
(`posting.quantity − SUM(consumption)`, D2 §5.3/§6) at `finalize`, and must fail rather than clamp.

### (d) missing error variants: four

`AllocationMismatch` (explicit edges that do not reproduce the withdrawal — otherwise the only
report is an opaque ZL005 at `COMMIT`, *after* `finalize` returned `Ok`), `IneligibleLayer` (the
consumed posting is a different item, an incompatible lot, or an exhausted layer — the case
neither the FK nor P3 catches), `LineageRequired` (the hole above), and `Unfinalized` — a sink
that took contributions and was dropped without `finalize` writes nothing at all while the
transition's own rows commit, which is the empty-group hole wearing a different hat; the ledger
poisons its `Tx` on such a drop and `commit` returns this variant.

### Exact final text for §6.2

Derives, uniformly: `Debug + Clone` on every struct and enum below; additionally
`Copy + PartialEq + Eq + PartialOrd + Ord + Hash + Serialize + Deserialize` on `PostingHandle`
and `PostingId`; `Copy + PartialEq + Eq + Hash + Serialize + Deserialize` on the four unit enums;
`PartialEq + Eq` on `PostingError`, whose `Display`/`Error` impls come by the same mechanism §6.1's
`Error` uses.

```rust
#[non_exhaustive] pub enum GroupKind { Movement, Adjustment, Transformation, Valuation, Reversal }
#[non_exhaustive] pub enum Boundary { Supplier, Customer, Scrap, Adjustment, Rounding, Consumed, Produced }
#[non_exhaustive] pub enum CostElement { Material, Labor, Burden, Outside }
#[non_exhaustive] pub enum ValueAccount { Inventory, Wip, Cogs, ScrapExpense, AdjustmentExpense, ApAccrual, LaborAbsorbed, BurdenAbsorbed, Ppv, MfgVariance, Rounding }

/// Group-local index of a contributed intent; lets a VALUE intent price a QUANTITY intent and a
/// CONSUMPTION intent name the withdrawal it allocates. Stable for the life of the sink. Only a
/// handle returned for `PostingIntent::Quantity` may appear in `values` or `consuming`; any other
/// handle there is `UnknownHandle`.
pub struct PostingHandle(pub u32);

/// The immutable `ledger.posting.posting_id` (D2 §5.2, `bigint`) of a posting that already exists
/// in the database. Never a `PostingHandle`, never an ordinal, never derived by arithmetic.
pub struct PostingId(pub i64);

/// Metadata for `ledger.posting_group` (D2 §4.1). Set once at sink construction, never per intent.
/// `group_id`, `actor_id`, `posted_at`, `created_xid` and `reverses_kind` are deliberately absent:
/// the ledger stamps the first four from the transaction and its context (D3 §2.1) and resolves
/// `reverses_kind` from the target group row.
pub struct PostingGroupHeader {
    pub source_kind: String, pub source_id: Option<Identifier>, pub work_order_id: Option<Identifier>,
    pub reason_code: Option<String>, pub reverses_group_id: Option<Identifier>,
}
pub struct QuantityPosting {
    pub item: ItemId, pub quantity: AnyQuantity, pub location: LocationId,
    pub boundary: Option<Boundary>, pub lot: Option<LotId>, pub serial: Option<SerialId>,
    pub entered: Option<AnyQuantity>,       // provenance only, never summed
}
pub struct ValuePosting {
    pub account: ValueAccount, pub cost_element: CostElement, pub cost_object: Option<Identifier>,
    pub amount: Money, pub values: Option<PostingHandle>,
}
/// Allocation of a withdrawal against an existing ledger posting (a layer), and the edge the
/// genealogy graph is traversed over (D2 §5.3). The consumed posting is named by its immutable
/// `posting_id`, never by a handle. Signed quantity: a REVERSAL restores a layer with negative edges.
pub struct ConsumptionPosting {
    pub consuming: PostingHandle, pub consumed_posting_id: PostingId,
    pub quantity: AnyQuantity, pub amount: Money,
}
#[non_exhaustive] pub enum PostingIntent { Quantity(QuantityPosting), Value(ValuePosting), Consumption(ConsumptionPosting) }

#[non_exhaustive] pub enum PostingError {
    NoSink, Shape(String), UnknownHandle(PostingHandle), AfterFinalize, EmptyGroup,
    /// No open layer can cover this withdrawal.
    AllocationRequired(PostingHandle),
    /// Explicit `Consumption` edges do not reproduce the withdrawal's quantity or its money (D2 P3).
    AllocationMismatch(PostingHandle),
    /// The consumed posting is a different item, an incompatible lot or serial, or an exhausted layer.
    IneligibleLayer { consuming: PostingHandle, consumed: PostingId },
    /// A TRANSFORMATION's produced quantity has no incoming consumption edge (D2 §5.3 genealogy).
    LineageRequired(PostingHandle),
    /// The sink took contributions and was dropped without `finalize`; the ledger refuses the commit.
    Unfinalized,
    Unimplemented,
}

pub trait PostingSink {
    fn kind(&self) -> GroupKind;
    fn header(&self) -> &PostingGroupHeader;
    /// Contribute one intent to this group. Order within the group is contribution order.
    fn contribute(&mut self, intent: PostingIntent) -> core::result::Result<PostingHandle, PostingError>;
    /// Exactly once per database transaction, by the transition executor, after every hook has
    /// run. A later `contribute` on the same sink is `AfterFinalize`. A group with zero posting
    /// rows is `EmptyGroup`: an empty group never reaches the database (D2 empty-group hole).
    /// By-value and object-safe: hooks hold `&mut dyn PostingSink` and cannot call this.
    fn finalize(self: Box<Self>) -> core::result::Result<(), PostingError>;
}

/// Core's own implementation for tests and for transitions that must not post: refuses everything.
pub struct NoPostings;   // contribute -> Err(NoSink); finalize -> Err(NoSink)
```

Normative rules (`datum-ledger` and `datum-statemachine` enforce them and are audited on them):

1. **One sink per transaction**, `&mut dyn PostingSink` to every hook in the hook order below,
   then one `finalize`. Two sinks in one transaction is a defect (D2 §9.4). A sink that received
   contributions and is dropped unfinalized poisons the transaction: `commit` → `Unfinalized`.
2. **The actor is never contributed.** `posting_group.actor_id` is read from the transaction-local
   context the audit trigger reads (D3 §2.1, §10 c/d). No hook input can name a different actor,
   so ledger attribution and audit attribution are the same fact by construction; a ledger test
   asserts the group header's actor equals the group's audit rows' actor.
3. **Withdrawals are allocated, explicitly or automatically, never left unallocated.** A
   `boundary == None`, negative quantity intent must, by `finalize`, be covered by `Consumption`
   intents whose quantity and money sums reproduce it (D2 P3). Explicit picks come from hooks;
   otherwise `datum-ledger`'s allocator produces them at `finalize` from the item's cost method
   and layers re-derived from the ledger — never from the withdrawal's own numbers (D2 §9.2), and
   never from the consuming posting's value rows. Short layers → `AllocationRequired`; explicit
   edges that do not add up → `AllocationMismatch`; no clamping, no invented layer.
4. **Insert order at finalize** is header, postings, consumption, in the one transaction; the
   deferred trigger judges the whole group at commit.
5. **Layer eligibility.** Before insert the ledger checks each edge's consumed posting: same
   `item_id`, lot/serial compatible under invariants 10–11, layer not already exhausted; else
   `IneligibleLayer`. It also checks that a `Wip` value row's `cost_object` is the header's
   `work_order_id` (D2 §4.1 pins it by FK).
6. **Produced lineage is contributed, not implied.** In a `Transformation` group, every positive
   quantity intent at a real location must be the `consuming` side of at least one `Consumption`
   edge naming the postings it was made from; else `LineageRequired`. P3 makes *withdrawals*
   total; only this rule makes the forward and backward traces of Wave 2s acceptance 8 agree.
7. **Enum bijection.** `datum-ledger` owns the Rust↔SQL enum mapping (D2 §5.1) and carries an
   exhaustive round-trip test over every variant.
8. **Hook order** is dependency-topological over the registering modules, ties broken by module
   id; `datum-statemachine` documents and tests it (`docs/03` §3.2). A value intent may only price
   a handle contributed earlier in that order.
9. Boundary-matrix violations (D2 §4.2) fail at insert, i.e. at `finalize`; an implementation may
   reject earlier, never later.

---

## Q2 — `SignatureGate`: types ratified unchanged, three rules tightened

### (a) token shape: keep `[u8; 32]`; no nonce, no `minted_at`

`[u8; 32]` over hex `String`: fixed width *is* the "SHA-256 of the exact record version" claim of
D3 §9, with no encoding, case or prefix drift to litigate at two ends of a wire. A `minted_at` on
the token would be a caller-carried copy of a server-stamped column: the verifier already holds
the row, so the field is either ignored (a dead field that looks load-bearing — the worst kind) or
compared against the row, which makes the row the source anyway. D3 §10 d and e are the pattern —
authority is server-side state (`set_config` context discarded at `COMMIT`, seals chained
off-box), never data the caller hands in. A nonce is worse: a nonce means nothing unless the
verifier remembers issued nonces, and the thing that remembers is the signature row keyed by
`SignatureId` — the uuid v7 `SignatureId` **is** the nonce, and single use is the `consumed` claim
on that row, not a token field. `minted_at`, the evidence that both identification components were
presented, the permission snapshot and `consumed_at` live on the esign row only. `signer` stays on
the token: it lets the executor reject a token belonging to another principal without a round trip,
which is where 11.200(a)(3) is enforced; disagreement with the row is `Invalid`.

### (b) single use is **per signature**

Per action class is rejected twice over: "action class" is a taxonomy nobody has specified, and it
*permits* one signature to authorise two different mutations — a second signing event with no
signing, against 11.200/11.70's record-to-signature link. Per record version is the same defect
and is already implied by the hash check. So: `datum-esign` claims the row with
`UPDATE … SET consumed_at = now() WHERE signature_id = $1 AND consumed_at IS NULL RETURNING`
**inside the transition's own transaction**, so a rollback releases the claim and a legitimate
retry still works; a claim that returns no row is `Consumed`. Named cost: a workflow that wants one
signature to authorise two transitions must mint two signatures. That is what 11.200 means. Delete
"of the same action class" from §6.3's prose. `verify` keeps `&self` — the claim is SQL, not Rust
mutation, so the gate stays shareable across hooks.

### (c) `Some(requirement)` + executor-calls-verify is **not** sufficient

It closes unsigned-by-omission-of-the-*call* and leaves unsigned-by-omission-of-the-*declaration*
open, which is plan-audit R3 and the precise failure ADR 0005 rejected for audit: a manifest a
validator can read is a human remembering, one review away from a WO release that was never
signed. So the kernel must make omission unrepresentable where it matters: `datum-statemachine`'s
edge metadata carries a **total** declaration, not an `Option` —
`enum SignatureDeclaration { Required(SignatureRequirement), NotRequired { reason: &'static str } }`
with no `Default` — and every edge of a module whose manifest says `regulated = true` must supply
one, so an omission is a compile error in that module's registration. Non-regulated modules keep
absence-means-none, because charging a bracket-shop module author for an annotation buys nothing.
The type lives in `datum-statemachine`, not core: core needs `SignatureRequirement` and the gate,
nothing more, and freezing the edge shape in core would also freeze it as *one* requirement per
edge — dual signature (review then approve) stays a Wave 2b statemachine change with no core
surface change. The executor calls `verify` before the mutation, aborts the transaction on any
`Err`, and may not catch and continue; with `NoSignatures` wired every `Required` edge fails closed
with `NoProvider`; and a release build whose enabled module set contains a `Required` edge while
the bound gate is `NoSignatures` fails at **startup**, not at the edge (Q3 key 4).

### (d) version 6 transition, version 5 token → `RecordMismatch`

`RecordMismatch` is a *reference* failure: the token's `record` does not identify what is being
mutated — different table, different id, or different `version`. `HashMismatch` is an *integrity*
failure: the references agree and the bytes do not. Version is part of `RecordRef`, so the v5 token
is a reference failure, decidable without hashing, and it tells the operator the truth ("the record
changed since you signed; sign again") instead of implying tamper evidence failed. Check order is
fixed so the error is deterministic and testable: row loaded (`NoProvider`/`Invalid`) → meaning
(`MeaningMismatch`) → record reference including version (`RecordMismatch`) → stored hash vs
`token.record_content_hash` vs the live record at that version (`HashMismatch`) → permission
snapshot (`SignerNotPermitted`) → single-use claim (`Consumed`); first failure wins. Same version
with differing hash is tamper or a mutation that skipped version stamping, and additionally logs a
security event on its own connection (D3 §10 b/e).

### Exact final text for §6.3

Byte-identical to the amended §6.3 except the doc comments below; the race has nothing to redo.
Derives: `Debug + Clone` throughout, plus `PartialEq + Eq + Hash + Serialize + Deserialize` on
`SignatureMeaning`, `PermissionKey`, `RecordRef`, `SignatureRequirement` and `SignatureToken`, and
`PartialEq + Eq` on `SignatureError`.

```rust
pub struct SignatureMeaning(pub String);            // "Approved", "Reviewed", "Released"
pub struct PermissionKey(pub String);               // "calibration.approve"
pub struct RecordRef { pub table: String, pub id: Identifier, pub version: i64 }

pub struct SignatureRequirement { pub meaning: SignatureMeaning, pub permission: PermissionKey }

/// A reference to a signature that already exists, plus the bytes the signer committed to.
/// Nothing here is secret and nothing here is authority: `minted_at`, the two-component
/// authentication evidence, the permission snapshot and `consumed_at` live only on the esign row.
pub struct SignatureToken {
    pub signature: SignatureId, pub signer: Actor, pub meaning: SignatureMeaning, pub record: RecordRef,
    /// SHA-256 of the canonical record bytes at `record.version`, as stored on the signature row (D3 §9).
    pub record_content_hash: [u8; 32],
}

#[non_exhaustive] pub enum SignatureError {
    NoProvider, MeaningMismatch, RecordMismatch, HashMismatch, SignerNotPermitted, Consumed,
    Invalid(String), Unimplemented,
}

pub trait SignatureGate {
    /// `record` is the live reference the executor is about to mutate, read in the same
    /// transaction. Checked in order: row, meaning, reference (incl. version), hashes, permission
    /// snapshot, single-use claim; the first failure is returned.
    fn verify(&self, token: &SignatureToken, required: &SignatureRequirement, record: &RecordRef)
        -> core::result::Result<(), SignatureError>;
}
pub struct NoSignatures;   // verify -> Err(SignatureError::NoProvider)
```

Normative rules: as amended, with three edits — (i) `verify` is authoritative only in
`datum-esign` (Wave 2b), which loads the row by `token.signature` and confirms both identification
components at mint, the stored hash, the live record at `record.version`, the **permission
snapshot taken at mint** (a live RBAC read would need a database in core and is rejected), the
meaning, and the single-use claim; (ii) single use is per signature, claimed in the transition's
transaction — the phrase "of the same action class" is struck; (iii) the statemachine obligation is
the total `SignatureDeclaration` of (c) above, and the configuration manifest (`docs/03` §8) lists
both the `Required` edges and the `NotRequired` ones with their reasons, in both profiles.

---

## Where the slice and the amended CONTRACT disagree

**`finalize(self)` vs `finalize(self: Box<Self>)` — CONTRACT wins.** The slice's replacement text
takes `self` by value, which makes the method require `Self: Sized` and the trait no longer
object-safe for a by-value call; hooks are handed `&mut dyn PostingSink`, so the trait must be a
trait object and the receiver must be `Box<Self>`. The slice's version would not compile at the
call site the design depends on.

**Consumption in core vs a ledger-private `GroupBuilder` — CONTRACT wins.** The slice's
"pragmatic split" keeps `ConsumptionPosting` out of core and admits in the same sentence that
"then hooks cannot do issues". That is the whole Wave 2s slice: `mod-inventory` issuing operator-
picked lots is the only path that gives P3 two genuinely independent quantity sources (Q1c), and
the alternative routes allocation through a ledger type that `datum-statemachine` would have to
name — the dependency edge R5 removed. Cost of keeping it in core is one struct and one enum arm.

**`BoundaryRejected` / a core-side boundary matrix — CONTRACT wins, the slice's optional variant
is dropped.** D2 §4.2's matrix is an immediate `CHECK`; a second copy in core is a second source of
truth that drifts silently, and the slice itself labels early rejection ergonomics, "not a
compliance hole". Implementations may reject earlier, but the matrix has exactly one home.

---

## Q3 — Profiles: PLAN §1a ratified, with one amendment and one retagging

Ratified as written: runtime enablement of compiled-in modules plus declarative configuration
(`docs/03` §5 Phase 1, §6 enable/disable flips a flag and never drops records), one binary, one
license, one schema, no `plain-shop` Cargo feature ever (ADR 0001; CONTRACT §2 has none), and the
kernel record properties always on, listed as cost, paid in storage — which is ADR 0005's accepted
price and closes plan-audit R4.

**Amendment (load-bearing).** "declares no signature requirement" must not read as a profile
authority over declarations. Signature requirements are **module-owned**; a profile may not add,
remove or downgrade one. `plain-shop` has no requirement because it enables no `regulated = true`
module, not because it overrode anything. Enablement is the only lever a profile has.

**Retagging, against the profiles slice.** That slice proposes tagging Wave 2s `mod-genealogy`
`regulated = true`. Rejected: `regulated = true` marks a module whose workflow and navigation exist
only because of a regulation the shop may not be subject to — CAPA, complaint handling,
calibration and training gates, e-signature workflows, validation/IQ navigation. Lot traceability
is a business capability every shop that has ever had a recall wants, the consumption edges it
reads exist in both profiles because P3 requires them, and tagging it regulated would fork the
Wave 2s acceptance script, which must pass unchanged under both profiles (PLAN §3, item 11).

### Keys `SPEC-profiles.md` must freeze, before Wave 2.6

1. **`profile`** — `id` ∈ {`regulated-device`, `plain-shop`}, display name, spec version; the
   effective profile is recorded as an audited configuration record at first boot, and a change is
   an audited config event, not a silent restart.
2. **`modules[]`** — for every compiled-in module: `id`, `version`, `regulated: bool` (by the
   criterion above), `installed: true` in both profiles (all first-party migrations run, so the
   schema is identical across profiles and a later enable is a flag flip, not a migration in a
   validated instance), `enabled: bool`; plus the closure rule — enabling a module enables its
   dependencies, and disabling a depended-on module is refused (`docs/03` §6).
3. **`signature_edges[]`** — generated from the registry, never hand-written: every edge of every
   enabled module as `Required { meaning, permission }` or `NotRequired { reason }`. Totality for
   `regulated = true` modules. `plain-shop`'s `Required` set is empty **and asserted empty**, not
   assumed. A profile may not rewrite either side.
4. **`signature_gate_binding`** — which `SignatureGate` the composition root binds per profile and
   build: `NoSignatures` pre-Wave-2b and in tests only, `datum-esign` from 2b; plus the startup
   guard — any `Required` edge in the enabled set with `NoSignatures` bound is a release-build
   startup failure, and a CI check that no release profile binds `NoSignatures`.
5. **`validation_manifest`** — always generated in both profiles (`docs/03` §8), with its content
   hash recorded; freeze its route/CLI, the permission key that reads it, and the fact that the
   only thing a profile hides is *navigation*: `datum iq`, `datum audit export` and
   `/api/v1/audit` stay available as admin tools, permission-gated, in both profiles (they are
   kernel SELECTs, not module routes — plan-audit R4).
6. **`navigation`** — the per-profile visible nav set, as the single place profile-driven hiding is
   expressed; `plain-shop` hides Validation/IQ and every `regulated = true` module's nav.
7. **`numbering`** — per regulated document type and per profile: format, prefix, scope, and
   `gap_free` where D3 §8 requires it, plus the reset policy. The lot and serial identifier
   charset and ≤20 length (invariant 9) are **kernel, not a profile key**; only a generator
   template is configurable.
8. **`kernel_always_on[]`** — the frozen, non-configurable list, named as cost: audit trigger,
   hash chain and seals, server time, actor required on every write, no hard deletes outside
   `transient` (D-W1-2), version stamping, the constrained lot/serial identifier, audit export. A
   future key may not move anything out of this list.
9. **`anchor_sink`** — off-box anchor configuration is a per-instance key, not a profile switch;
   SPEC-profiles states which profile's IQ suite treats a missing sink as a failure and which
   merely nags (D3 §10 e).
10. **`seeded_permissions` / defaults** — the role bundles seeded per profile (signature
    `permission` keys must resolve to something), base currency, stock UOM system, display
    timezone (stored time is UTC, D3 §4).
11. **`acceptance`** — the both-profile matrix: which Wave 2s assertions differ by profile (items 7
    and 11 only), and a diff test that dumps both profiles' effective configuration and asserts the
    delta is a subset of keys 2, 3, 4, 6, 7 and 10. Anything else differing is a defect.

DECISION D-W1-3: `PostingSink` is ratified as amended with `PostingId` replacing the bare `i64`, `finalize` staying on the trait as `self: Box<Self>`, automatic allocation permitted but independent only on P3's money half (quantity-side redundancy requires an explicit pick), and four added error variants — `AllocationMismatch`, `IneligibleLayer`, `LineageRequired`, `Unfinalized` — with produced-lot lineage now a contributed obligation rather than a P3 consequence.
DECISION D-W1-4: `SignatureGate`'s types are ratified byte-unchanged — `[u8; 32]` hash, no nonce or `minted_at` on the token — with single use redefined as per signature and claimed inside the transition's transaction, a v5 token at v6 ruled `RecordMismatch` under a fixed check order, and the executor obligation strengthened from `Option<requirement>` to a total `SignatureDeclaration` on every edge of a `regulated = true` module plus a startup failure when a `Required` edge meets `NoSignatures`.
DECISION D-W1-5: PLAN §1a is ratified — one binary, runtime enablement of compiled-in modules, identical always-on kernel record properties priced as cost — amended so that signature requirements are module-owned and a profile's only lever is enablement, `mod-genealogy` is retagged `regulated = false`, and `SPEC-profiles.md` must freeze the eleven keys listed above before Wave 2.6.
