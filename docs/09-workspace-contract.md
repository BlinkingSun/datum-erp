# CONTRACT — the Datum workspace (Wave 1, frozen)

This file is the integration contract for every code lane. Where it and
`research/audits/slice-wave1-stubs.md` disagree, **this file wins** (it carries the v2
deltas). Where it is silent, that audit's §4, §5 and §7 are normative.

Repository root (main tree): `/Users/jroberts/Desktop/Internal Development/Tools/ERP`.
Lanes work in worktrees under `/Users/jroberts/Desktop/Internal Development/Tools/ERP-wt/`.

## 1. Toolchain

`rust-toolchain.toml` (byte-exact):

```toml
[toolchain]
channel = "1.98.1"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

Edition `2024`. Resolver `"3"`. MSRV = the pin. The named toolchain `1.98.1` with `rustfmt`
and `clippy` is installed on the build node once by the orchestrator (a worktree must never
trigger a rustup download). PostgreSQL **17** everywhere (`dev/compose.yml` image
`postgres:17`; the MacBook runs Homebrew `postgresql@17` 17.11, keg-only but linked; the
canonical client path is `$(brew --prefix postgresql@17)/bin`, and `/opt/homebrew/bin/psql`
also resolves today). Server on `127.0.0.1:5432` — **never `localhost`** — with `trust`
authentication on loopback for the login user, which is why the bootstrap URL carries no
credentials **on this MacBook only**; the NUC and the shop PC need an explicit superuser in
`DATUM_BOOTSTRAP_URL`. The Homebrew cluster is a LaunchAgent: it dies at logout, which is fine
for a development laptop and is not the production story (D5).

`rustfmt.toml`: `edition = "2024"`, `max_width = 100`, `use_field_init_shorthand = true`.

## 2. Root `Cargo.toml` (canonical; `ws-skeleton` is its only writer)

```toml
[workspace]
resolver = "3"
members = [
  "crates/datum-core",
  "crates/datum-test",
  "crates/datum-db",
  "crates/datum-audit",
  "crates/datum-identity",
  "crates/datum-numbering",
  "crates/datum-uom",
  "crates/datum-events",
  "crates/datum-jobs",
  "crates/datum-ledger",
  "crates/datum-statemachine",
  "crates/datum-esign",
  "crates/datum-customfields",
  "crates/datum-documents",
  "crates/datum-print",
  "crates/datum-module",
  "crates/datum-server",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.98.1"
license = "AGPL-3.0-or-later"
publish = false
repository = "https://github.com/BlinkingSun/datum-erp"

[workspace.dependencies]
datum-core          = { path = "crates/datum-core" }
datum-test          = { path = "crates/datum-test" }
datum-db            = { path = "crates/datum-db" }
datum-audit         = { path = "crates/datum-audit" }
datum-identity      = { path = "crates/datum-identity" }
datum-numbering     = { path = "crates/datum-numbering" }
datum-uom           = { path = "crates/datum-uom" }
datum-events        = { path = "crates/datum-events" }
datum-jobs          = { path = "crates/datum-jobs" }
datum-ledger        = { path = "crates/datum-ledger" }
datum-statemachine  = { path = "crates/datum-statemachine" }
datum-esign         = { path = "crates/datum-esign" }
datum-customfields  = { path = "crates/datum-customfields" }
datum-documents     = { path = "crates/datum-documents" }
datum-print         = { path = "crates/datum-print" }
datum-module        = { path = "crates/datum-module" }

# Closed allow-list. A new third-party crate is an escalation, not a commit.
sqlx          = { version = "0.9", default-features = false, features = [
                  "runtime-tokio", "tls-rustls", "postgres", "macros", "migrate",
                  "chrono", "uuid", "rust_decimal", "json" ] }   # 0.9.0 (2026-05-21); sqlx-cli 0.9.0 on the build node — MAJOR MUST MATCH
tokio         = { version = "1", features = ["macros", "rt-multi-thread", "sync", "time"] }
serde         = { version = "1", features = ["derive"] }
serde_json    = "1"
thiserror     = "2"
uuid          = { version = "1", features = ["v7", "serde"] }
chrono        = { version = "0.4", default-features = false, features = ["clock", "serde"] }
rust_decimal  = { version = "1", default-features = false, features = ["std", "serde-with-str"] }
tracing       = "0.1"
proptest      = "1"
trybuild      = "1"
anyhow        = "1"
axum          = "0.8"
tower         = "0.5"
tower-http    = { version = "0.6", features = ["trace"] }
clap          = { version = "4", features = ["derive", "env"] }

[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"
dead_code = "warn"
unused_must_use = "deny"
unused_crate_dependencies = "warn"

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
unwrap_used = "warn"
```

`sqlx` is pinned to the **0.9** line because the build node's `sqlx-cli` is 0.9.0 and the offline
query cache format must agree with the crate; the stub audit's 0.8 advice predates 0.9.0. The
feature names above are 0.9's. Nothing else in this block changes without an escalation.
`Cargo.lock` is committed and complete after Wave 1: every allow-list crate is pulled by
at least one member so Wave 2 lockfile diffs stay small.

## 3. Per-crate manifest template

```toml
[package]
name = "datum-<name>"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true
description = "<one line>"

[lints]
workspace = true

[features]
default = []
test-utils = []

[dependencies]
# exactly the edges in §4, via `.workspace = true`; third-party only from the allow-list

[dev-dependencies]
datum-test.workspace = true     # db-backed crates only
tokio.workspace = true
proptest.workspace = true
```

`[lints] workspace = true` on every member, without exception. Package `name` is the
crate name below, hyphenated.

## 4. Dependency graph (v2; acyclic; load-bearing)

| Crate | Kernel deps | Notes |
|---|---|---|
| `datum-core` | — | thiserror, serde, uuid, rust_decimal. **No sqlx, no async, no chrono.** |
| `datum-test` | — | sqlx, tokio, thiserror, tracing. Harness only; Wave 1 owns it for the life of the build. |
| `datum-db` | core | **Wave 1 stub has REAL parts** (D3 §11): `connect`, the `after_connect` / `after_release` pool hooks exactly as `research/decisions/audit-persistence.md` §2.2, and `Tx::begin` implementing §2.1–§2.3 (transaction-local actor, transaction-id check, fail closed). Only DDL and the audit trigger remain `Unimplemented`. |
| `datum-audit` | core db | |
| `datum-identity` | core db audit | |
| `datum-numbering` | core db | |
| `datum-uom` | core db audit | implements `core::UnitCatalog` + `core::UnitConverter` |
| `datum-events` | core db | |
| `datum-jobs` | core db events | |
| `datum-ledger` | core db audit uom | implements `core::PostingSink` |
| `datum-statemachine` | core db audit identity | uses `core::PostingSink` + `core::SignatureGate`; **never** ledger or esign |
| `datum-esign` | core db audit identity | implements `core::SignatureGate` (Wave 2b) |
| `datum-customfields` | core db audit | (Wave 2b) |
| `datum-documents` | core db audit identity numbering statemachine | (Wave 2b) |
| `datum-print` | core db audit documents esign | (Wave 2b) |
| `datum-module` | all of the above | composition root |
| `datum-server` | everything | only crate allowed `axum`, `tower*`, `clap` |

## 5. Stub rules (every crate except `datum-core` and `datum-test`)

- `src/lib.rs` exports the **real** public type and trait names from
  `research/audits/slice-wave1-stubs.md` §5 for that crate, amended by §4 above.
- Fallible bodies return `Error::Unimplemented`; **zero** `todo!()` / `unimplemented!()`.
- Each crate defines its own `Error` (`thiserror`, `#[non_exhaustive]`) with at least
  `Unimplemented`, and `From<datum_core::Error>`; db-backed crates also `From<sqlx::Error>`
  via `datum_db::Error`.
- Db-backed crates carry `migrations/00000000000000_placeholder.up.sql` and `.down.sql`
  (both a comment-only no-op), `build.rs` with `println!("cargo:rerun-if-changed=migrations")`,
  and `pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");`.
- **Zero** `sqlx::query!` macros in Wave 1. **No** workspace-root `.sqlx/`. `SQLX_OFFLINE`
  set in `.cargo/config.toml` `[env]`, never in `.env`.
- One `#[cfg(test)]` smoke test per stub: `Error::Unimplemented` formats; `MIGRATOR`
  has at least one migration. Nothing in a Wave 1 **lib** test needs Postgres.
- Stubs may name only this subset of `datum-core`: `Error`, `Result`, `Actor`,
  `ActorKind`, `Identifier`, `ItemId`, `LotId`, `SerialId`, `LocationId`, `UnitId`,
  `CurrencyId`, `Money`, `AnyQuantity`, `DimensionKind`, `PostingSink`,
  `PostingIntent`, `SignatureGate`, `SignatureRequirement`, `SignatureToken`,
  `RecordRef`. Anything else from core is a Wave 2 concern.
- Re-export nothing from other datum crates.
- **`datum-db` exception:** the items marked REAL in §4 are implemented and tested in Wave 1 (against a scratch database the lane creates from `DATUM_BOOTSTRAP_URL` and drops), because thirteen Wave 2 lanes would otherwise each invent the session protocol.

### 5a. The raw-SQL fence (D3 §11; owned by `ws-skeleton`)

`clippy.toml` at the root carries a `disallowed-macros` / `disallowed-methods` list for `sqlx::query`, `sqlx::query_as`, `sqlx::query_scalar`, `sqlx::query!`-family macros, `sqlx::QueryBuilder`, `sqlx::raw_sql`, `copy_in_raw`, `set_config`, `current_setting`; `datum-db`, `datum-audit` and **`datum-test`** opt out of that list with a crate-level `#![allow(clippy::disallowed_methods, clippy::disallowed_macros)]` and a one-line justification (`datum-test` is the test harness: it creates and drops databases and probes sessions with raw SQL, and it never ships in the binary — exemption ruled 2026-09-12 at Wave 1 integration, where the fence and the harness met for the first time). The `justfile` recipe `lint-sql` runs `rg` for the same tokens outside `crates/datum-db`, `crates/datum-audit` and `crates/datum-test` and fails on any hit; CI runs it.

## 6. `datum-core` public surface (frozen)

The complete contract is `research/decisions/core-quantity.md` §2 (types, verbatim),
§4 (numeric representation), §6 (serialization boundaries), §8 (acceptance mechanisms).
This section adds the identifier family, the actor, the shared error, and the two
inversion traits. All names below are frozen.

### 6.1 Identifiers and actor

```rust
/// Opaque uuid v7 newtype. `Copy`, `Eq`, `Hash`, `Ord`, `Serialize`, `Deserialize`, `Display`.
pub struct Identifier(uuid::Uuid);
impl Identifier { pub fn generate() -> Self; pub fn from_uuid(u: uuid::Uuid) -> Self; pub fn as_uuid(self) -> uuid::Uuid; }

// Typed identifiers, same derives, same three constructors, no cross-conversion.
pub struct ItemId(Identifier);   pub struct LotId(Identifier);   pub struct SerialId(Identifier);
pub struct LocationId(Identifier);  pub struct UserId(Identifier);  pub struct SignatureId(Identifier);

#[non_exhaustive] pub enum ActorKind { User, ServicePrincipal }
pub struct Actor { pub id: Identifier, pub kind: ActorKind }   // Copy, Eq, Hash, Serialize, Deserialize

#[non_exhaustive] pub enum Error { Invariant(String), Overflow, Quantity(QuantityError), Money(MoneyError), Residual(ResidualError), Posting(PostingError), Signature(SignatureError), Unimplemented }
pub type Result<T> = core::result::Result<T, Error>;
```

`UnitId(pub i64)` and `CurrencyId(pub i32)` are as in D1 §2.1.

### 6.2 `PostingSink` — how a hook contributes postings without a ledger dependency

Design: a **synchronous collector with one finalize point**. A hook or transition pushes *intents*
into the sink; `datum-ledger` implements the sink as a group builder and, at `finalize`, inserts the
group header, the postings and the consumption edges in the one database transaction the transition
runs in. Core has no async and no database. Shapes mirror `research/decisions/ledger-invariant.md`
§4.1, §5.1–5.3. **Ratified by DECISION D-W1-3** (`_team/reports/DECISION-traits-profiles.md`);
the text below is that ruling's, verbatim, and the core race builds it verbatim.

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

### 6.3 `SignatureGate` — how a transition demands a signature without an esign dependency

Design: **verify, never mint**. Minting (two identification components, meaning, credential check,
the signature row carrying the record's content hash and a permission snapshot) is `datum-esign`'s
asynchronous job before the transition; the transition receives a `SignatureToken` and asks the
gate synchronously. Core ships `NoSignatures`, which refuses every token. **Ratified by DECISION
D-W1-4**; the text below is that ruling's, verbatim.

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

**Executor obligation (D-W1-4 (c)).** Every transition edge of a module marked `regulated = true`
carries a total `SignatureDeclaration`: `Required { meaning, permission }` or
`NotRequired { reason }`; an edge with neither does not register. `datum-statemachine`, not the
module author, calls `verify` before the mutation on every `Required` edge and fails closed. A
release build whose enabled set contains a `Required` edge while `NoSignatures` is bound **fails at
startup**, and CI asserts no release profile binds `NoSignatures`. The configuration manifest
(`docs/03` §8) lists every edge's declaration, both kinds, in both profiles.

## 7. `justfile` recipe names (fixed; bodies belong to `ws-skeleton`)

`fmt`, `fmt-check`, `clippy`, `lint-sql` (§5a), `test`, `test-lib`, `test-db` (runs with
`DATUM_REQUIRE_PG=1`), `db-up`, `db-down`, `db-reset` (applies `dev/sql/*.sql` in order
against `DATUM_BOOTSTRAP_URL`), `migrate` and `sqlx-prepare` (per crate, never
`--workspace`; both export `DATABASE_URL=$DATUM_MIGRATE_DATABASE_URL` for sqlx-cli),
`ci` (= `fmt-check && clippy && lint-sql && test-lib`), `ci-db` (= `ci && test-db`).

`db-up` uses `docker compose -f dev/compose.yml up -d` when `docker` is on PATH; otherwise it
runs `pg_isready -h 127.0.0.1 -p 5432` and **exits 1 with the Homebrew start command printed**
if the server is not accepting connections — a green `db-up` means a reachable server, never a
no-op. `db-down` mirrors it. Neither installs anything. Every recipe quotes paths (the tree
lives under a directory with a space). `ci.yml`'s `postgres:17` service is the GitHub-shaped
run; **local acceptance is the Homebrew server**, and a lane is never failed for docker being
absent.

## 8. Environment

`.env.example` (owned by `harness`) documents exactly:

```
# names are D3's (research/decisions/audit-persistence.md §1.1); dev passwords only
DATUM_DATABASE_URL=postgres://datum_app:datum@127.0.0.1:5432/datum_test?sslmode=disable
DATUM_MIGRATE_DATABASE_URL=postgres://datum_migrate:datum@127.0.0.1:5432/datum_test?sslmode=disable
DATUM_BOOTSTRAP_URL=postgres://127.0.0.1:5432/postgres?sslmode=disable   # MacBook loopback trust; other nodes add user:password
DATUM_TEST_TEMPLATE=datum_test_template
# DATUM_REQUIRE_PG=1   (CI only: a missing database is a failure, not a skip)
```

There is no bare `DATABASE_URL` in the product; the two `just` recipes that drive sqlx-cli
export it from `DATUM_MIGRATE_DATABASE_URL` for the duration of the command. Tests are
ephemeral per test: a database cloned from `DATUM_TEST_TEMPLATE` and dropped afterwards
(D3 §11), never a shared database with a shared `audit.event`.

`SQLX_OFFLINE=true` lives in `.cargo/config.toml` and nowhere else.

### 8a. The five roles (verbatim from D3 §1.1; `harness` owns the file, every lane may apply it to a scratch database)

```sql
CREATE ROLE datum_owner       NOLOGIN;
CREATE ROLE datum_audit_row   NOLOGIN;
CREATE ROLE datum_audit_event NOLOGIN;
CREATE ROLE datum_migrate     LOGIN;
CREATE ROLE datum_app         LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
GRANT datum_owner TO datum_migrate;
REVOKE SET ON PARAMETER session_replication_role FROM datum_app;  -- PG 15+
```

Three schema classes (DECISION D-W1-2): `app` (records with history: no DELETE), `transient`
(working state with no history: DELETE allowed), `audit` (SELECT only). `datum_app` holds TRUNCATE
nowhere. `ON DELETE CASCADE` is banned in every schema. The grant SQL is in `SPEC-harness.md`.

## 9. Things no lane does

Edit another lane's files. Add a kernel edge. Add a third-party crate outside §2. Write
`todo!()`. Ignore `.sqlx/` or `migrations/`. Push.

License (decided by the owner 2026-09-12, ADR 0006 Accepted): **AGPL-3.0-or-later**, contributions
under the **Developer Certificate of Origin**. `doc-repo` writes `LICENSE`; every crate manifest
inherits `license.workspace = true`.

## 10. Integration of Wave 1 (orchestrator; mechanical)

The placeholder scheme is only safe with these rules, which the specs restate:

1. **Placeholders are worktree-only and untracked.** The `ws-skeleton` branch never contains
   `crates/datum-core/**` or `crates/datum-test/**`. Gate: `git ls-tree -r lane/ws-skeleton
   --name-only | grep -E '^crates/datum-(core|test)/'` prints nothing.
2. **Throwaway roots are untracked.** `core-r*` and `harness` branches contain only
   `crates/<own>/**` (and for `harness` also `dev/sql/**`, `.env.example`). Gate: `git ls-tree -r
   <branch> --name-only` shows nothing else.
3. **Merge order:** doc lanes (any order) → `ws-skeleton` (merge) → winning `core-r*` and
   `harness` by **path checkout**, never by branch merge:
   `git checkout lane/core-rN -- crates/datum-core` and
   `git checkout lane/harness -- crates/datum-test dev/sql .env.example`.
4. **One lockfile writer:** after step 3, `cargo generate-lockfile` once on the integrated
   tree; commit `Cargo.lock` once.
5. **Post-integration gate on the integrated tree:** `head -1 crates/datum-core/src/lib.rs`
   and `crates/datum-test/src/lib.rs` contain no `PLACEHOLDER`; `just ci` green; `just db-reset`
   then `just ci-db` green with `DATUM_REQUIRE_PG=1`; `cargo tree -e normal` matches §4.
6. Nothing in Wave 2 starts until step 5 passes.
