# Plan-audit slice 4 — Wave 1 bottleneck and stub contract

Adversarial review of PLAN.md §§3, 5, 7, 9 against docs/02-architecture.md (stack), ADR 0002 (Rust/Axum/SQLx), ADR 0003 (Postgres-only), ADR 0004 (ledger), ADR 0005 (audit), ADR 0006 (license still Open).

Date: 2026-09-11. Reviewer: grok-4.6. Product files not modified.

---

## Verdict

The stub approach is the **right bottleneck shape** and the **wrong level of specification**. One Wave 1 workspace lane that freezes names, edges, error types, and lints before a 13-wide fan-out is how isolated git worktrees stay mergeable. PLAN §9 is correct about *why*. PLAN is silent about *what a stub is*, and that silence will make Wave 2 fail its own acceptance criteria.

A Wave 2 worker in an isolated worktree branched from merged Wave 1 sees **only** what Wave 1 committed. If Wave 1 crates are `todo!()` empty libs, `wicket-ledger` cannot name `wicket_uom::Quantity` / `wicket_uom::Error` / `wicket_audit::AuditCtx` in signatures or in a property-test generator. `cargo test -p wicket-ledger` will not compile, let alone pass. PLAN §7 then becomes unsatisfiable in Wave 2.

**PLAN implies (a) for cross-crate tests and (b) for the ledger crate itself, and does not notice the contradiction.** Isolated worktrees cannot see each other's unmerged implementations, so Wave 2 tests that need a *real* ledger *and* a *real* UoM engine cannot pass until merge. The ledger property tests, however, live *inside* `wicket-ledger`. They can run in that lane alone if (and only if) dependency stubs export **constructable types and traits**, and the ledger lane writes its own in-memory engine / TestDb fixtures. That is enough for "property tests on `wicket-ledger`" as a Wave 2 acceptance criterion. It is **not** enough for workspace-level integration tests. Those are Wave 2.5 / phase-end gate, and PLAN does not say so.

**Do not start the workspace lane until an opus DECISION writes the stub SPEC (this document, plus the Quantity decision from slice 5).** Typing 15 crates against an unfrozen `Quantity` is how you pay the "most expensive crate to get wrong" cost twice.

---

## 1. What PLAN actually says (and does not)

| Claim | Source | Reality |
|---|---|---|
| One workspace lane owns the skeleton **and** a compiling stub for every crate in §5 | PLAN §3 | Correct bottleneck. Underspecified payload. |
| Stubs "fix names, dependency edges, error types, and lint configuration" | PLAN §9 | Necessary, not sufficient. Missing: toolchain pin, constructable types, sqlx offline policy, TestDb crate, migration placeholders, `Cargo.lock` / `.sqlx` ownership, `.gitignore` seam with `doc-repo`. |
| Wave 2: 13 lanes, each owns exactly one crate directory, all branch from merged Wave 1 | PLAN §3 | Sound **if** Wave 2 never edits workspace `Cargo.toml` members, `[workspace.dependencies]`, `rust-toolchain.toml`, `.github/`, `justfile`, `.cargo/`, or a workspace-root `.sqlx/`. PLAN never forbids those edits. |
| Property tests on `wicket-ledger` are a per-lane Wave 2 obligation | PLAN §7 | Compiles only against **rich** uom/audit/core/db stubs. Runs only if the ledger lane owns an in-memory (or TestDb) engine. Cross-crate "real uom + real ledger" tests cannot pass in isolated worktrees. |
| `rust-toolchain.toml` is owned by workspace | PLAN §3 table | File is named. **Channel, edition, MSRV, components are not.** Gap. |
| License via workspace package metadata | implied by any real `Cargo.toml` | ADR 0006 is **Open**. Wave 1 cannot honestly set `license.workspace`. |
| Thirteen Wave 2 lanes vs 15 crates in §5 | PLAN §§3, 5 | Off-by-one is slice 3's job. Wave 1 must still **list and stub every member**, including `wicket-module` and `wicket-server`, so Wave 2/3 never touch `[workspace].members`. |

Kernel crates in §5 (15): `core`, `db`, `audit`, `identity`, `esign`, `uom`, `numbering`, `events`, `jobs`, `ledger`, `statemachine`, `documents`, `customfields`, `module`, `server`. Wave 2 "13" is almost certainly the 13 that are not `module` and not `server`. Wave 1 stubs **all 15** plus a 16th test crate (below).

---

## 2. Worktree mechanics — exclusive dirs work, shared files do not

Isolated worktrees share git objects and have separate working trees. They **cannot** see another lane's unmerged tree. Consequences:

1. **Crate-dir exclusivity is sufficient for `src/` merge** provided each Wave 2 lane rewrites only `crates/<its-crate>/**`. Replacing a stub `lib.rs` with a real one is a single-parent merge against main, not a 13-way collision.
2. **Workspace-root files are a 13-way collision.** If any Wave 2 lane adds a member, a workspace dep, a CI job, a `just` recipe, or a query cache file at repo root, the Wave 2 merge is hell. **Normative rule:** Wave 2 may not edit anything outside its crate directory except `Cargo.lock` (see §6.5). New workspace deps or new members are an escalation, same rule as PLAN §5 new edges.
3. **`cargo test --workspace` in a Wave 2 worktree compiles stub implementations of every *other* crate.** Ledger tests that call `wicket_uom::convert()` and expect a real conversion table will fail or skip until uom merges. Design tests to the stub contract, not to the sibling's future body.
4. **Shared `CARGO_TARGET_DIR` across 13 concurrent worktrees will lock-fight.** Wave 1 justfile / CI must use the per-worktree default `target/` (or a per-lane dir). Do not set a workspace-global `build.target-dir` in `.cargo/config.toml`.
5. **rust-analyzer in a worktree** loads the workspace. Stub crates that `deny(warnings)` and then warn will paint the whole workspace red and fail CI. See §8.

---

## 3. Can Wave 2 property tests actually run?

**Short answer: yes, in the ledger lane, against an in-memory engine the ledger lane writes. No, as a workspace integration test, until merge. PLAN does not distinguish these. That is a PLAN amendment.**

PLAN §9 lists what stubs freeze (names, edges, errors, lints). PLAN §7 requires property tests that generate arbitrary transaction sequences and assert groups balance and projections equal the ledger sum. Those assertions are **ledger-internal**. They do not require a production UoM conversion engine or a production audit interceptor.

What they **do** require from Wave 1 stubs:

| Need | Must be in Wave 1 stub? | Who implements the body? |
|---|---|---|
| `Identifier`, `Money`, `Quantity`, `Actor`, `wicket_core::Error` / `Result` | **Yes, real types** | Prefer **complete** `wicket-core` in Wave 1, not a stub (see §5.1) |
| `UnitId` / `Unit` newtype, constructable in tests (`UnitId::each()`, `UnitId::inch()`, or `UnitId::from_uuid`) | **Yes** | `wicket-uom` stub constructors; conversion tables are Wave 2 uom |
| `Quantity` arithmetic in **one** unit (add/sub signed amounts, no convert) | **Yes if Quantity lives in core**; uom stub re-exports | core |
| `convert(qty, to) -> Result<Quantity>` | Trait in the uom stub; **body may return `Err(Error::Unimplemented)`** | Wave 2 uom. Ledger tests must not depend on success. Use one unit, or a test fake **inside the ledger crate**. |
| Audit write / actor context | Trait + `AuditCtx` type in audit stub | Wave 2 audit. Ledger tests use a recording fake **inside the ledger crate**, or skip audit on the in-memory path. |
| Postgres deferred zero-sum constraint | Ledger's own migrations, Wave 2 | Ledger lane. Needs working `TestDb` infra from Wave 1 (not a `todo!()`). |
| In-memory ledger | **Must NOT be in the Wave 1 ledger stub** | Wave 2 ledger lane. Putting a fake in the stub guarantees a merge conflict on the same functions. |

**Implied PLAN policy, made explicit:**

- **(a)** Wave 2 crate tests that cross into another kernel crate's *behavior* are unit-only against stubs. Cross-crate integration is Wave 2.5 / FINDINGS-0 whole-program, after merge.
- **(b)** Property tests on `wicket-ledger` run in the ledger lane against **(b1)** an in-memory engine (no Postgres, no sibling crates beyond types) and optionally **(b2)** Postgres via `TestDb` + *that crate's* migrations. (b1) is the Wave 2 done-gate. (b2) is the same lane if TestDb works; it still must not join tables that only exist in unmerged sibling crates.

(b1) is enough for PLAN §7 as written. It is **not** enough for ADR 0004's "enforced by the database, not by application convention." That half of the invariant is a Postgres test in the ledger crate, which needs TestDb, not a real `wicket-uom`.

**PLAN amendment text (paste):**

> Wave 2 acceptance for `wicket-ledger` property tests is: generated sequences against the in-memory engine, plus (if `TestDb` is available) the same invariants against Postgres using only `wicket-ledger` migrations and stub types from dependencies. Tests that require a non-stub `wicket-uom` conversion engine or a non-stub audit interceptor are phase-end integration tests and are not a Wave 2 lane done-gate.

---

## 4. Normative stub SPEC (this should become the workspace lane SPEC)

Frozen by Wave 1. Wave 2 may fill bodies, add private modules, add migrations *after* the placeholder, and add crate-local deps only as allowed in §6. Wave 2 may **not** rename packages, rename public types listed here, change the dependency graph, or remove features listed here.

### 4.1 Repository files the workspace lane owns

```
rust-toolchain.toml
Cargo.toml                          # virtual workspace
Cargo.lock
rustfmt.toml
clippy.toml                         # optional; prefer [workspace.lints]
.cargo/config.toml
justfile
.devcontainer/ or dev/compose.yml   # Postgres for humans and CI
.github/workflows/ci.yml
.env.example                        # DATABASE_URL + SQLX_OFFLINE=true
crates/<each>/Cargo.toml
crates/<each>/src/lib.rs            # and only the files listed per crate below
crates/<each>/migrations/           # db-backed crates
crates/<each>/build.rs              # db-backed crates (sqlx migrate build-script)
crates/wicket-test/                  # NEW — not in PLAN §5
```

Collision with `doc-repo` (README, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, **`.gitignore`**): **Wave 1 workspace must own the Rust/sqlx bits of `.gitignore`** (`/target`, `**/*.rs.bk`, `.env`, `*.pdb`) and must **not** ignore `.sqlx/` or `migrations/`. If `doc-repo` also writes `.gitignore`, the PLAN ownership table is wrong; give workspace a `gitignore-rust` fragment or make `doc-repo` land first with a stub `.gitignore` that workspace is allowed to append. Do not let two lanes write the same file.

ADR 0006 is Open. Workspace sets `license = "UNLICENSED"` **or** omits license until the ADR closes. Do not silently stamp AGPL.

### 4.2 `rust-toolchain.toml` (PLAN never pins — gap)

`channel = "stable"` **is forbidden**. Isolated worktrees and CI must compile the same rustc. Pin the current stable as of Wave 1 start (as of this review: **1.98.1**, released 2026-09-03). Bump is a dedicated change, not a surprise.

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.98.1"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

Do not add `rust-analyzer` / `rust-src` to the pin (CI image bloat). Developers install those locally.

**Edition:** `2024` (stable since 1.85). **MSRV:** same as the pin, written as `rust-version` in `[workspace.package]`. **Resolver:** `"3"` (edition 2024 workspace default). PLAN is silent; this is a Wave 1 decision, not a Wave 2 debate.

### 4.3 Root `Cargo.toml`

```toml
[workspace]
resolver = "3"
members = [
  "crates/wicket-core",
  "crates/wicket-db",
  "crates/wicket-audit",
  "crates/wicket-identity",
  "crates/wicket-esign",
  "crates/wicket-uom",
  "crates/wicket-numbering",
  "crates/wicket-events",
  "crates/wicket-jobs",
  "crates/wicket-ledger",
  "crates/wicket-statemachine",
  "crates/wicket-documents",
  "crates/wicket-customfields",
  "crates/wicket-module",
  "crates/wicket-server",
  "crates/wicket-test",          # NEW; Wave 2 does not own this crate
]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.98.1"
license = "UNLICENSED"         # until ADR 0006 closes
publish = false

[workspace.dependencies]
# path crates — versions inherit workspace.package
wicket-core          = { path = "crates/wicket-core" }
wicket-db            = { path = "crates/wicket-db" }
wicket-audit         = { path = "crates/wicket-audit" }
wicket-identity      = { path = "crates/wicket-identity" }
wicket-esign         = { path = "crates/wicket-esign" }
wicket-uom           = { path = "crates/wicket-uom" }
wicket-numbering     = { path = "crates/wicket-numbering" }
wicket-events        = { path = "crates/wicket-events" }
wicket-jobs          = { path = "crates/wicket-jobs" }
wicket-ledger        = { path = "crates/wicket-ledger" }
wicket-statemachine  = { path = "crates/wicket-statemachine" }
wicket-customfields  = { path = "crates/wicket-customfields" }
wicket-documents     = { path = "crates/wicket-documents" }
wicket-module        = { path = "crates/wicket-module" }
wicket-test          = { path = "crates/wicket-test" }

# closed allow-list of third-party crates. New entries = escalation.
# pin exact versions in Wave 1 so Cargo.lock is complete for Wave 2.
sqlx          = { version = "0.8", default-features = false, features = [
                  "runtime-tokio-rustls", "postgres", "macros", "migrate",
                  "chrono", "uuid", "rust_decimal", "json",
                ] }
tokio         = { version = "1", features = ["macros", "rt-multi-thread", "sync", "time"] }
serde         = { version = "1", features = ["derive"] }
serde_json    = "1"
thiserror     = "2"
uuid          = { version = "1", features = ["v7", "serde"] }
chrono        = { version = "0.4", default-features = false, features = ["clock", "serde"] }
rust_decimal  = { version = "1", features = ["serde-str"] }
tracing       = "0.1"
proptest      = "1"
anyhow        = "1"            # binaries / tests only, not kernel libs
axum          = "0.8"          # wicket-server stub only
tower         = "0.5"
tower-http    = { version = "0.6", features = ["trace"] }
clap          = { version = "4", features = ["derive", "env"] }

[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"          # NOT deny in Wave 1 — stubs will fail deny
dead_code = "warn"             # public stub items are used via the API
unused_must_use = "deny"
unused_crate_dependencies = "warn"

[workspace.lints.clippy]
todo = "deny"                  # this is why stubs must not use todo!()
unimplemented = "deny"
panic = "warn"
unwrap_used = "warn"
expect_used = "warn"
```

**Why `todo = "deny"`:** empty crates that compile with `todo!()` will fail CI the moment clippy runs `-D warnings` or this lint. PLAN §9 says stubs fix lint configuration; therefore the stub *body* must satisfy that configuration.

**CI clippy:** `cargo clippy --workspace --all-targets --all-features -- -D warnings`. Stubs must be warning-clean under that command. Do not enable `missing_docs = "deny"` until Wave 2 fills docs.

**`unused_crate_dependencies`:** every dependency listed in a crate `Cargo.toml` must appear in a signature, a re-export, or a `use` that is not cfg-gated away. Otherwise Wave 1 CI fails.

### 4.4 Per-crate `Cargo.toml` (template — `wicket-uom` shown)

```toml
[package]
name = "wicket-uom"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true
description = "Kernel units of measure (stub API; Wave 2 fills the engine)."

[lints]
workspace = true

[dependencies]
wicket-core.workspace = true
wicket-db.workspace = true
wicket-audit.workspace = true
sqlx.workspace = true          # used: migrate! and (later) query!
serde.workspace = true
thiserror.workspace = true
uuid.workspace = true
rust_decimal.workspace = true

[features]
default = []
test-utils = ["wicket-test"]    # never default; Wave 2 tests opt in

[dependencies.wicket-test]
workspace = true
optional = true

[dev-dependencies]
wicket-test.workspace = true
proptest.workspace = true
tokio.workspace = true
```

Rules:

- Package `name` **is** the crate name in PLAN §5. No `wicket_uom` vs `wicket-uom` games; Cargo maps hyphens to underscores in Rust, keep the hyphen in toml.
- `version` / `edition` / `rust-version` / `license` **always** `.workspace = true`.
- `[lints] workspace = true` on **every** member. Forgetting this is how workspace clippy config silently does nothing (well-known Cargo 1.74+ footgun).
- Features: `default = []`. `test-utils` is the only extra feature Wave 1 is allowed to add, and it may only enable `wicket-test` plus extra test constructors. Wave 2 may add features inside the crate without renaming `test-utils`.
- Dependencies **exactly** the PLAN §5 graph. No extra kernel edges. Third-party crates only from the workspace allow-list.
- `wicket-server` is the only stub allowed to depend on `axum` / `clap`.
- `wicket-core` depends on **nothing** in the graph. Third-party: `thiserror`, `serde`, `uuid`, `rust_decimal` (and `chrono` if `Actor` timestamps live here — prefer not; time is a server concern per ADR 0005).

`wicket-ledger` crate template is the same shape with deps `wicket-core`, `wicket-db`, `wicket-audit`, `wicket-uom` plus the shared third-party set.

### 4.5 `.cargo/config.toml`

```toml
[build]
# do NOT set target-dir

[env]
SQLX_OFFLINE = { value = "true", relative = false, force = false }

[alias]
xtask = "run --package xtask --"   # only if you add xtask; otherwise omit
```

Force-offline in CI via env, not via a committed `.env` that also contains `DATABASE_URL` (sqlx 0.8.5 bug: `SQLX_OFFLINE=true` in `.env` breaks `cargo sqlx prepare --workspace`; see §7). Commit `.env.example`, gitignore `.env`.

### 4.6 `justfile` (minimum recipes)

```
fmt            cargo fmt --all
clippy         cargo clippy --workspace --all-targets --all-features -- -D warnings
test           cargo test --workspace --all-features
test-lib       cargo test --workspace --lib --all-features
db-up          docker compose -f dev/compose.yml up -d
db-down        docker compose -f dev/compose.yml down
migrate        # runs each crate migrator in graph order against DATABASE_URL
sqlx-prepare   cargo sqlx prepare -- --all-targets --all-features
                 # per crate, NOT --workspace (see §7)
ci             just fmt-check && just clippy && just test-lib
```

Wave 1 CI must call these. A stub that cannot `just clippy` and `just test-lib` is not a compiling stub.

---

## 5. `lib.rs` public API — real types, stub functions

**Law:** anything another crate may name in a type position must be a real type (struct/enum/newtype + derives). Function **bodies** that perform I/O or algorithms return `Err(Error::Unimplemented)`. Never `todo!()`, never `unimplemented!()`, never `panic!("stub")`. Those fail clippy under the workspace lints above and they panic in tests that accidentally call them.

Every public error enum is `#[non_exhaustive]` so Wave 2 can add variants without a metadata dance. Downstream crates must use `?` / `match` with a wildcard.

### 5.1 `wicket-core` — implement for real, do not stub

PLAN §5: primitives with no database dependency; "the crate most expensive to get wrong." Empty newtypes are the whole crate. Wave 1 should **ship a complete `wicket-core`**, not a stub, **after** the Quantity decision (slice 5) lands. Pulling core out of the Wave 2 fan-out is the single highest-leverage PLAN change in this slice.

Until slice 5 decides, Wave 1 **must not freeze** `Quantity<U>` vs a runtime `UnitId`. A wrong freeze contaminates every Wave 2 signature.

Minimum surface that other crates will name (names are PLAN's; shapes are placeholders pending slice 5):

```rust
//! crates/wicket-core/src/lib.rs
#![forbid(unsafe_code)]

mod actor;
mod error;
mod id;
mod money;
mod quantity;

pub use actor::Actor;
pub use error::{Error, Result};
pub use id::Identifier;
pub use money::Money;
pub use quantity::Quantity;
```

| Type | Must be real | Notes |
|---|---|---|
| `Identifier` | yes | Newtype over `uuid::Uuid` (v7). `Copy`, `Eq`, `Hash`, `Serialize`, `Display`. `Identifier::generate()` / `Identifier::from_uuid`. |
| `Actor` | yes | Newtype id + kind enum `{ User, ServicePrincipal }`. No DB. RBAC is `wicket-identity`. |
| `Money` | yes | Amount + currency code + **explicit scale**. Do not use `f64`. `rust_decimal::Decimal`. No implicit rounding. |
| `Quantity` | **gated on slice 5** | PLAN says type parameter. Slice 5 may kill that. Freeze only after DECISION. |
| `Error` | yes | `#[non_exhaustive]`, `thiserror`. Variants other crates will match: `Invariant`, `Overflow`, `Unimplemented` (core itself should not need Unimplemented if complete). |
| `Result<T>` | yes | `type Result<T> = std::result::Result<T, Error>;` |

Re-exports: core re-exports **nothing** from other wicket crates (it has no deps). Other crates **may** re-export core types from their crate root so callers write `wicket_ledger::Identifier` — **do not do that in stubs.** Re-export sugar is a Wave 2/3 convenience and causes glob-import collisions. Callers use `wicket_core::Identifier`.

Derives on every value type: `Debug, Clone, Copy` (if true), `Eq, PartialEq, Hash, Serialize, Deserialize`. `Copy` on `Money`/`Quantity` only if the inner Decimal is Copy (it is).

### 5.2 `wicket-db` stub

Real types / traits:

| Item | Kind | Why real |
|---|---|---|
| `Pool` | type alias `sqlx::PgPool` | every crate's function signatures |
| `Tx<'a>` | type alias `sqlx::Transaction<'a, sqlx::Postgres>` | posting runs in a transaction |
| `Error` | enum, `From<sqlx::Error>` | `?` in every crate |
| `connect(url) -> impl Future<Result<Pool>>` | fn, **may be real** (thin wrapper) | not domain logic |
| `trait SessionCtx { fn actor(&self) -> Actor; }` | trait | ADR 0005 GUC/session protocol; body of the setter can be Unimplemented |
| `MIGRATOR` | `sqlx::migrate!("./migrations")` | compile-time embed |

Must **not** contain: the sealed Write interceptor (Wave 2 db + audit), real role-creation SQL beyond a placeholder comment, a `todo!()` TestDb.

Migrations: no-op pair, see §5.8.

### 5.3 `wicket-audit` stub — example of a dependency the ledger names

```rust
//! crates/wicket-audit/src/lib.rs
#![forbid(unsafe_code)]

use wicket_core::{Actor, Identifier, Result as CoreResult};

pub use wicket_core::{Actor, Identifier};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("audit stub: not implemented")]
    Unimplemented,
    #[error(transparent)]
    Core(#[from] wicket_core::Error),
    #[error(transparent)]
    Db(#[from] wicket_db::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

/// Context the persistence layer needs to write an audit row.
/// Fields are real so ledger signatures can name them.
#[derive(Debug, Clone)]
pub struct AuditCtx {
    pub actor: Actor,
    pub reason: Option<String>,
    pub source: Option<Identifier>,
}

/// Recorded change. Constructable; persistence is Wave 2.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub id: Identifier,
    pub entity: &'static str, // stubs may use &'static; Wave 2 may change to interned string if the SPEC allows — do not, freeze as String
}

// Freeze as String:
// pub entity: String,

/// The write path other crates call. Body is a stub error, not todo!().
pub async fn record(_tx: &mut wicket_db::Tx<'_>, _ctx: &AuditCtx, _entry: AuditEntry) -> Result<Identifier> {
    Err(Error::Unimplemented)
}

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
```

Freeze `AuditEntry.entity` as `String` (not `&'static str`) so Wave 2 does not have to change the signature. I flag the `&'static` version above as a trap.

Traits other crates will impl or take as generics: if slice 2 decides "sealed Write trait in wicket-db", that trait lives in `wicket-db`, not here. Audit stub only needs the types ledger/identity/esign will name.

`#[cfg(test)]` / `test-utils`: `AuditCtx::test(actor: Actor) -> Self`. No recording fake — recording fakes belong in the **consumer** crate so Wave 2 audit does not merge-conflict on them.

### 5.4 `wicket-uom` stub — the other ledger dependency

Pending slice 5 for `Quantity`. Assuming PLAN-as-written until DECISION:

```rust
//! crates/wicket-uom/src/lib.rs
#![forbid(unsafe_code)]

pub use wicket_core::{Identifier, Quantity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UnitId(Identifier);

impl UnitId {
    pub fn from_id(id: Identifier) -> Self { Self(id) }
    pub fn as_id(self) -> Identifier { self.0 }
}

/// Closed kernel dimensions. Customer-defined units are data (UnitId), not extra enum variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum Dimension { Count, Length, Mass, Time, Volume }

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("uom stub: not implemented")]
    Unimplemented,
    #[error("incompatible units")]
    Incompatible,
    #[error("unknown unit")]
    UnknownUnit,
    #[error(transparent)]
    Core(#[from] wicket_core::Error),
    #[error(transparent)]
    Db(#[from] wicket_db::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

/// Named because ledger and items will call it. Always Err in the stub.
pub fn convert<U>(_qty: Quantity<U>, _to: UnitId) -> Result<Quantity<U>> {
    Err(Error::Unimplemented)
}

pub async fn load_unit(_pool: &wicket_db::Pool, _id: UnitId) -> Result<UnitId> {
    Err(Error::Unimplemented)
}

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
```

**`test-utils` extra (feature-gated, constructable, no conversion tables):**

```rust
// crates/wicket-uom/src/test_utils.rs, compiled only with feature = "test-utils"
impl UnitId {
    pub fn test_count() -> Self { Self::from_id(Identifier::from_uuid(uuid::Uuid::nil())) }
}
```

Ledger property tests use `UnitId::test_count()` and never call `convert()`. If they need conversion, the **ledger crate** defines `struct IdentityUom;` that impls a small trait **defined in the uom stub**:

```rust
pub trait Convert {
    fn convert(&self, qty: /* Quantity */, to: UnitId) -> Result</* Quantity */>;
}
```

Wave 2 uom implements `Convert` for the real engine. Wave 2 ledger tests impl `Convert` for `IdentityUom` locally. The stub does **not** ship `IdentityUom`.

### 5.5 `wicket-ledger` stub — types only, no engine

The Wave 2 ledger lane replaces this file. Keep it small so the replacement is obvious.

| Item | Real in stub? | Reason |
|---|---|---|
| `PostingId`, `GroupId` | yes, newtypes over `Identifier` | named in every module later |
| `LedgerKind` `{ Inventory, Cost, Labor }` | yes, enum `#[non_exhaustive]` | architecture §3 |
| `VirtualLocation` `{ Supplier, Customer, Scrap, Adjustment, Wip }` | yes | architecture §3 |
| `Posting` struct (fields: id, group_id, ledger, posted_at, posted_by, dimensions, quantity, source, reason) | yes, **constructable** | property-test generators in Wave 2 will build these. `posted_at` is `chrono::DateTime<Utc>` **not** supplied by the caller on the *write* path; the struct still has the field for reads. |
| `Dimensions` | yes, typed keys as `struct` of `Option<Identifier>` | freeze names: `item`, `location`, `lot`, `serial`, `work_order`, `cost_element` |
| `Error` | yes | variants: `Unimplemented`, `UnbalancedGroup`, `MissingReason`, `Core`, `Db`, `Uom`, `Audit` |
| `post(tx, posting) -> Result<PostingId>` | signature real, body `Err(Unimplemented)` | Wave 2 fills |
| `rebuild_projections(pool) -> Result<()>` | signature real, body stub err | architecture §3 |
| In-memory engine | **MUST NOT exist in the stub** | Wave 2 exclusive body |
| `query!` SQL | **MUST NOT exist in the stub** | no schema, no `.sqlx` (see §7) |
| Property tests | **MUST NOT exist in the stub** | they would fail against Unimplemented, fail CI, and conflict with Wave 2 tests |

Re-exports: `pub use wicket_core::{Actor, Identifier, Money, Quantity};` — **forbidden in stubs** (see 5.1). Downstream writes `wicket_core::Quantity`.

### 5.6 Other crates (pattern)

Same pattern everywhere:

- Error enum with `Unimplemented` + `#[from]` of dependency errors.
- Public structs/enums named in PLAN/ADRs/architecture, constructable, `#[non_exhaustive]` where Wave 2 will add variants.
- Async fns that would hit Postgres return `Err(Error::Unimplemented)`.
- `pub static MIGRATOR` on every db-backed crate.
- No `query!`. No in-memory fakes. No sample data.

Minimum named types by crate (non-exhaustive; opus SPEC may extend, not shrink):

| Crate | Must-exist types |
|---|---|
| identity | `UserId`, `RoleId`, `Principal`, `Error` |
| esign | `SignatureId`, `SignatureMeaning`, `Error` |
| numbering | `SequenceId`, `Error` |
| events | `Event`, `EventKind`, `Error` |
| jobs | `JobId`, `ServicePrincipal` (or reuse `Actor`), `Error` |
| statemachine | `MachineId`, `State`, `Transition`, `Error` |
| documents | `DocumentId`, `RevisionId`, `Error` |
| customfields | `FieldKey`, `FieldType`, `Error` |
| module | `ModuleId`, `Manifest`, `Error` |
| server | `fn main` in `src/main.rs` that returns, does not bind a port in tests; `lib.rs` empty `pub fn version() -> &'static str` |

`wicket-server` is a binary. Stub it as a library+binary that compiles and whose `main` prints version and exits 0, so `cargo test --workspace` does not start a server.

### 5.7 What MUST NOT be in a stub

- `todo!()`, `unimplemented!()`, `panic!("todo")`, `unreachable!()` in non-divergent paths.
- Algorithm bodies: posting, conversion tables, RBAC checks, numbering allocation, hash-chained audit, state-machine evaluation.
- In-memory fakes that Wave 2 will want under the same names (`Ledger::in_memory`, `Uom::identity`).
- `sqlx::query!` / `query_as!` / `query_scalar!` (requires schema + `.sqlx`).
- Seed SQL, fixture rows, demo tenants.
- Extra dependency edges "to make tests work."
- `#[cfg(test)]` modules that call other crates' stub functions and `assert!` success.
- Comments that narrate Wave 1/2 ("filled in Wave 2") as the public rustdoc. One-line `// stub: returns Error::Unimplemented` is enough.
- `unsafe`.
- Default features that pull `test-utils`.

### 5.8 Migrations: empty folder vs no-op

**Empty `migrations/` directory:** `sqlx::migrate!("./migrations")` compiles to an empty `Migrator` if the directory exists. `sqlx migrate run` against an empty folder historically exits without a useful error (launchbadge/sqlx#1338). The directory must exist or the `migrate!` macro fails at compile time.

**Normative:** ship a **no-op reversible placeholder**, not an empty folder.

```
crates/wicket-uom/migrations/
  00000000000000_placeholder.up.sql      -- SELECT 1;
  00000000000000_placeholder.down.sql    -- SELECT 1;
crates/wicket-uom/build.rs                -- sqlx migrate build-script (picks up new files)
```

Why a no-op, not empty:

- PLAN invariant 8: every migration has a tested reverse. A placeholder with a reverse satisfies the invariant without inventing schema.
- Wave 2 **adds** `YYYYMMDDHHMMSS_create_*.{up,down}.sql`. It does not edit the placeholder. No merge conflict.
- Version `00000000000000` sorts before any timestamp Wave 2 uses.
- Do **not** put real tables in Wave 1. Real tables are the crate lane's work. A Wave 1 `CREATE TABLE postings` **is** the implementation and **will** conflict.

`SELECT 1;` is the whole file. No `CREATE SCHEMA` in the placeholder unless opus decides on one schema per crate in Wave 1 — that would be a real design freeze and belongs in the db SPEC, not a stealth stub. Default: public schema, Wave 2 db lane owns grants/roles.

Aggregator: `wicket-db` exports

```rust
pub fn migrators() -> Vec<&'static sqlx::migrate::Migrator> { /* dependency order */ }
```

listing every crate migrator. Wave 1 can implement this as a **real** function (it is a static list matching PLAN §5). Wave 2 db lane may extend it only if new crates appear (they must not). This is the one "implementation" allowed in the db stub because it is the workspace contract.

### 5.9 Test helpers and `test-utils`

**Do not put `TestDb` in `wicket-db`.** Wave 2 `wicket-db` owns that directory and will rewrite it. A working TestDb written in Wave 1 then rewritten in Wave 2 is a conflict and a period where 12 other lanes have a broken TestDb.

**New crate `wicket-test` (Wave 1, not in the Wave 2 exclusive set, not in PLAN §5 — PLAN amendment):**

```
crates/wicket-test/
  Cargo.toml          # depends on sqlx, tokio, wicket-core, wicket-db
  src/lib.rs          # TestDb, compose/url helpers, skip-if-no-pg
```

Surface:

```rust
pub struct TestDb { /* pool, url, drop-on-drop if ephemeral */ }

impl TestDb {
    /// Connects to DATABASE_URL. Err if unset and no embedded/server.
    pub async fn connect() -> Result<Self, Error>;
    pub fn pool(&self) -> &wicket_db::Pool;
    /// Runs migrators() in graph order.
    pub async fn migrate(&self) -> Result<(), Error>;
}

/// Skip helper: returns Err reason if Postgres is not up, so tests can `return`.
pub fn postgres_available() -> Result<(), String>;
```

Wave 1 implements this **for real** (thin: parse `DATABASE_URL`, `PgPoolOptions::connect`, run migrators). It may use testcontainers or a CI service; pick one in the workspace SPEC and stick to it. Embedded postgres on Windows is hostile (slice 7); **CI service + local docker compose is the Wave 1 choice.** Tests that need Postgres `return` if `postgres_available()` fails, except in CI where `WICKET_REQUIRE_PG=1` makes that a hard fail.

Feature `test-utils` on each kernel crate: enables extra constructors (`UnitId::test_count`, `AuditCtx::test`). Those constructors are **stable API** of the stub and Wave 2 must keep them.

`#[cfg(test)]` inside a stub crate: one smoke test that `Error::Unimplemented` formats and that `MIGRATOR.migrations` is non-empty (the placeholder). Nothing that requires Postgres in Wave 1 unit tests, so `just test-lib` is green without Docker.

---

## 6. Collision matrix (confirm / amend PLAN)

### 6.1 Members list — confirm

**Confirmed:** the workspace lane must list every member up front, including Wave 3's `wicket-server` and `wicket-module`, plus `wicket-test`. Wave 2 lanes own crate dirs exclusively and **must not** edit `[workspace].members`. If they need a new crate, they escalate.

### 6.2 `[workspace.dependencies]` — PLAN hole

If Wave 2 ledger adds `proptest` only in its crate via workspace inheritance, it must edit root `Cargo.toml`. Thirteen lanes cannot all do that.

**Rule:** Wave 1's `[workspace.dependencies]` is a **closed allow-list**. Wave 2 may add a third-party dep **in its own `crates/<name>/Cargo.toml` with a version literal**, not via workspace inheritance. Slightly less DRY; zero members-file collision. Adding a *kernel* path dep is still an escalation (graph change).

Better: Wave 1 already lists `proptest`, `tokio`, `sqlx`, etc., and every crate that will need them already has them in `[dev-dependencies]`. Then Wave 2 does not touch the lockfile for those.

### 6.3 `Cargo.lock`

Even crate-local new deps dirty `Cargo.lock` at the workspace root. 13-way lockfile merges are usually auto-mergeable and sometimes not.

**Mitigation:** Wave 1 pre-seeds the allow-list so the lockfile already contains sqlx/tokio/proptest/axum/…. Wave 2 lockfile diffs stay small. After Wave 2 merge, one serial `cargo generate-lockfile` / `cargo update -w` lane if needed. Do not have 13 lanes "clean up" the lockfile.

### 6.4 `.sqlx/` — see §7. This is the dangerous one.

### 6.5 Files Wave 2 may touch outside its crate

| File | Wave 2? |
|---|---|
| `crates/<own>/**` | yes, exclusive |
| `Cargo.lock` | tolerated, keep diffs minimal |
| `crates/<own>/.sqlx/**` | yes, exclusive (per-crate cache) |
| root `Cargo.toml` | **no** |
| `rust-toolchain.toml` | **no** |
| `.github/**` | **no** |
| `justfile` | **no** |
| `.cargo/**` | **no** |
| `dev/**` | **no** |
| `crates/wicket-test/**` | **no** (Wave 1 owns for the life of the build) |
| workspace-root `.sqlx/**` | **no — must not exist** |

---

## 7. SQLx offline mode — classic workspace footgun

Researched against current sqlx CLI docs (sqlx-cli README, prepare.rs, issues 1223 / 1770 / 2667 / 3644 / 3836 / 3961, discussion 4215) and crates.io 0.8 notes.

### 7.1 How it actually works (0.8)

- `query!` / `query_as!` / `query_scalar!` need **either** a live `DATABASE_URL` whose schema matches **or** cached describe-data in `.sqlx/`.
- In 0.8, offline support is tied to the `macros` feature (`macros` enables `sqlx-core/offline`). There is no separate `offline` feature to forget. **Wave 1 must still enable `macros`.**
- `DATABASE_URL` **wins** over `.sqlx` if set. `SQLX_OFFLINE=true` forces the cache and fails the build on a cache miss: `SQLX_OFFLINE=true but there is no cached data for this query`.
- `cargo sqlx prepare` writes `.sqlx` next to the **current package** manifest.
- `cargo sqlx prepare --workspace` writes **one** `.sqlx` at the **workspace root** and hashes queries from all members into it.
- `prepare --check` (and `--check --workspace`) is the CI freshness gate.
- Queries behind `#[cfg(test)]` or features are invisible to prepare unless you pass `-- --all-targets --all-features`.
- Known bug (sqlx#3836, 0.8.5): `SQLX_OFFLINE=true` **in `.env`** makes `cargo sqlx prepare --workspace` fail with cache-miss errors. Prepare internally sets `SQLX_OFFLINE=false` for its own check, but a dotenv load can fight it. **Do not put `SQLX_OFFLINE=true` in a committed `.env`.** Put it in CI env / `.cargo/config.toml` `[env]` with `force = false`, and in the GHA workflow.
- `SQLX_OFFLINE_DIR` exists to relocate the cache (used by prepare itself; Nix/crane users hit 0.8.6 regressions when renaming `.sqlx` → `sqlx`). Do not rename the directory.
- Macros resolve cache relative to `CARGO_MANIFEST_DIR` and the workspace root. A crate compiled from an isolated worktree only sees **that worktree's** committed `.sqlx`.

### 7.2 Why this explodes in *this* plan

Wave 2 = 13 isolated worktrees, each adding `query!` against tables that crate's migrations create.

If Wave 1 follows the README and uses **workspace-root `.sqlx`**:

1. Thirteen lanes write the same directory. Merge hell, every query file.
2. A ledger worktree compiling `query!("select … from postings")` needs cache produced against a schema that **includes** `postings`. That schema does not exist until the ledger lane's migrations exist. The lane can `prepare` locally, but it needs a live Postgres with **its** migrations applied, and it must commit cache files at a path it owns.
3. A uom worktree's `prepare --workspace` will **wipe and rewrite** the root `.sqlx` from whatever members compile in *that* worktree (stubs of siblings, real uom). Sibling query hashes disappear. The next merge deletes other lanes' cache.
4. CI `prepare --check --workspace` on main after a partial merge is red until every lane has merged.

This is not theoretical. sqlx#1770 / #1223 / #3644 are this class of workspace/cache mismatch.

### 7.3 Normative sqlx policy (PLAN amendment)

1. **No workspace-root `.sqlx/`.** `.gitignore` does not ignore `.sqlx`; each db-backed crate commits **`crates/<name>/.sqlx/`**.
2. Wave 2 lanes run `cargo sqlx prepare -- --all-targets --all-features` **from the crate directory** (no `--workspace`). That writes `crates/<name>/.sqlx/`, which they own.
3. Wave 1 stubs contain **zero** `query!` macros, so Wave 1 commits **no** cache files (or an empty dir with a `.gitkeep` — empty cache is useless and `migrate!` does not use `.sqlx`). Do not add `.gitkeep` in `.sqlx`; it is not a query file and may confuse globbers. Leave the directory absent until the first `query!`.
4. CI: **not** `cargo sqlx prepare --check --workspace`. CI runs per-crate `--check` for crates that have a `.sqlx` directory, or a just recipe that loops members. After Wave 2 merge this can be a shard.
5. `SQLX_OFFLINE=true` in GitHub Actions and in developer docs. `DATABASE_URL` is set only for `prepare`, `migrate`, and integration tests — never for `cargo clippy` / `cargo test --lib` on CI images that should compile offline.
6. Wave 2 `query!` may reference **only** tables created by that crate's migrations plus tables created by **already-merged** dependency crates. In Wave 2 isolation, dependency crates are stubs with placeholder migrations, so **they have no tables**. Therefore Wave 2 `query!` may only hit **that crate's own tables**. Joins to `uom_units` from `wicket-ledger` are illegal until uom has merged to main and ledger rebases. Ledger property tests that need a unit store a `UnitId` newtype; they do not join.
7. Runtime `sqlx::query()` (not the macro) is allowed in Wave 2 if a crate wants to avoid prepare entirely, at the cost of losing compile-time checking. The workspace SPEC should **prefer `query!` + per-crate `.sqlx`** so CI can `prepare --check`. Do not mix styles inside one crate.
8. `dev/compose.yml` Postgres is a Wave 1 deliverable. Without it, Wave 2 cannot `prepare` and cannot run migration tests. PLAN is silent. **This is a missing Wave 1 subtask.**
9. sqlx-cli version is pinned in the justfile / CI (`cargo install sqlx-cli --version 0.8.x --no-default-features --features rustls,postgres`) so prepare hashes stay stable.

### 7.4 Interaction with isolated worktrees

A ledger worktree:

- Sees Wave 1 uom stub (no `uom_units` table).
- Adds `postings` migrations and `query!("select … from postings")`.
- Starts compose Postgres, runs **ledger** migrator (placeholder from deps + real ledger migrations).
- `cargo sqlx prepare` in `crates/wicket-ledger`.
- Commits `crates/wicket-ledger/.sqlx/*.json` and the new migrations.

Compile in CI after merge: `SQLX_OFFLINE=true`, no database, macros read `crates/wicket-ledger/.sqlx`. Sibling crates still have no `query!`. This works.

Compile in the ledger worktree *before* prepare, with `SQLX_OFFLINE=true`: **fails**. Developer docs / just recipe must say: first `query!` in a crate requires `just db-up && just migrate && cargo sqlx prepare -- --all-targets --all-features` from that crate. Wave 1 should encode this in CONTRIBUTING — but CONTRIBUTING is `doc-repo`. Put the rule in `justfile` comments and in the workspace SPEC so both lanes copy it.

---

## 8. Clippy / deny-warnings / rust-analyzer

Empty `cargo new` crates (`pub fn add(left, right)`) are not stubs. Delete that.

Failure modes Wave 1 **will** hit if unspecified:

| Lint | Stub trigger | Fix in SPEC |
|---|---|---|
| `clippy::todo` / `clippy::unimplemented` | `todo!()` bodies | `Err(Error::Unimplemented)` |
| `dead_code` | private stub fns | keep API public; no private dead helpers |
| `unused_crate_dependencies` | sqlx listed but unused | `MIGRATOR` uses sqlx; re-export or type-alias `Pool` |
| `missing_docs` deny | undoc'd public types | Wave 1: `missing_docs = "warn"` only |
| `clippy::empty_structs_with_brackets` | `struct Foo;` vs `struct Foo {}` | use tuple newtypes or documented unit structs as `struct Foo;` |
| `clippy::large_enum_variant` | unlikely in stubs | ignore |
| rustc `warnings` as deny in CI | unused imports in a re-export glob | no glob re-exports |

`forbid(unsafe_code)` at workspace lint level matches PLAN invariant 7.

rust-analyzer: `rust-analyzer.cargo.features = ["test-utils"]` should **not** be forced from committed `.vscode` unless `doc-repo` owns editor config. Optional `rust-analyzer.toml` in Wave 1 is fine and is workspace-owned.

---

## 9. Wave 1 size — split recommendation

The workspace lane as PLAN writes it is: root manifests + toolchain + cargo config + justfile + GHA + `dev/` + ~15 crate skeletons + sqlx policy + TestDb. That is **one contract**, many files. File count is not the problem. **Unspecified public API** is the problem. A single agent can emit 15 nearly-identical stubs from a SPEC; a single agent inventing 15 APIs will poison Wave 2.

### Collisions if split naively

| Split | Collision |
|---|---|
| workspace-skeleton vs stub-APIs | every `crates/*/Cargo.toml` and `src/lib.rs` |
| stub-APIs vs CI | `justfile`, GHA test command, `test-utils` feature names |
| CI vs `wicket-db` TestDb | resolved by moving TestDb to `wicket-test` |
| workspace vs `doc-repo` | `.gitignore`, LICENSE, CONTRIBUTING sqlx instructions |
| workspace vs `doc-adr` | ADR 0006 license string in `Cargo.toml` |

### Recommended split (serial where they collide)

**Keep one `workspace` lane** for anything that shares `Cargo.toml` / crate files. Do **not** parallelize skeleton vs stub-APIs.

Optional serial fast-follow, same agent or a second lane after merge:

1. **`workspace-contract`** (the PLAN `workspace` lane): toolchain, root + per-crate Cargo.toml, stub `lib.rs` APIs per this SPEC, placeholder migrations, build.rs, `wicket-test` **types + real TestDb**, `dev/compose.yml`, rustfmt/clippy, `.cargo/config.toml`, `justfile`, `.env.example`, rust bits of `.gitignore`.
2. **`workspace-ci`** (optional, after 1): GitHub Actions matrix (linux/macos/windows), sqlx-cli install, `SQLX_OFFLINE`, postgres service, shard stubs. Collides with `justfile` recipes if 1 already wrote them — so fold CI **into** lane 1 unless GHA yaml is the only leftover.

**Do not blind-race this.** Four agents inventing crate APIs is four incompatible contracts. Blind-race is for hard algorithms (ledger engine), not for a file tree that must be identical.

**Do extract `wicket-core` as a named, complete implementation inside this lane** (or a 1.5 serial lane immediately after Quantity DECISION, before fan-out). Deep-audit core on its own.

**Gating:** workspace-contract must not start until:

- opus DECISION on Quantity (slice 5) — otherwise core freezes the wrong shape;
- opus accepts this stub SPEC (or a tightened rewrite);
- ADR 0006 either closes or the SPEC's `UNLICENSED` workaround is accepted;
- slice 2's interception choice is known enough to know whether `SessionCtx` lives in `wicket-db` (if unknown, freeze a tiny `SessionCtx` in db and let slice 2 add, `#[non_exhaustive]`).

---

## 10. EXECUTOR / TIER / SHARD

### EXECUTOR for `workspace`

| Question | Answer |
|---|---|
| Blind-race? | **No.** Divergent stubs are worse than a slow single pass. |
| Provider | **grok** (must read ADRs + this SPEC + PLAN graph; this is contract literacy, not volume-only typing). Cursor is acceptable *against a frozen SPEC* if grok is busy; then the SPEC is the whole prompt. |
| opus first? | **Yes.** DECISION: accept this SPEC (Quantity shape, `wicket-test` crate, per-crate `.sqlx`, no-op migrations, `UNLICENSED` until 0006, core-is-real). The executor types; opus does not. |
| Hard/risky? | The **decision** is hard. The **typing** is not. Do not spend `caps.blind_race_attempts` here. |

Wave 1 docs lanes stay as PLAN (cursor/grok 1:1). `workspace` is the grok-shaped code lane in that wave.

### TIER

PLAN says **deep**. **Agree.** This lane is contract-bearing and gates thirteen others. Recommend `audit.double: true` (cross-family second auditor). A miss here is multiplied by 13.

Audit checklist (for the later auditor, not this review): every member listed; every crate has `[lints] workspace = true`; no `todo!()`; clippy `-D warnings` green; `cargo test --workspace --lib` green without Postgres; `migrate!` compiles; no workspace-root `.sqlx`; `SQLX_OFFLINE` documented; TestDb in `wicket-test` not in `wicket-db`; PLAN graph edges match `[dependencies]`; no `query!`.

### SHARD of `cargo test --workspace` after Wave 2

Wave 1 stub tests: **no**, well under 10 minutes (`--lib` should be seconds to ~1 min).

Wave 2 landed, honest estimate:

| Job | Likely time | Shard? |
|---|---|---|
| `cargo clippy --workspace --all-targets -D warnings` | 5–15 min on GHA, worse on Windows (ADR 0002 compile-tax) | yes, own shard; sccache/rust-cache mandatory in Wave 1 CI |
| `cargo test --workspace --lib` (unit, no pg) | 3–10 min compile-dominated | maybe; keep as one shard until measured |
| `cargo test -p wicket-ledger` proptest in-memory | 1–5 min at 256 cases if no IO | keep with ledger; raise cases carefully |
| `cargo test -p wicket-ledger` proptest against Postgres | **will exceed 10 min** at 256 × N ops × roundtrips (slice 6's problem) | **yes**: `gates-shard-ledger-prop` |
| per-crate migration up/down | minutes + Docker | **yes**: `gates-shard-migrate` |
| `cargo sqlx prepare --check` per crate | compile-sized | fold into clippy shard or a sqlx shard |
| Windows + sqlx macros + 15 crates | often 15–25 min cold | matrix OS is already a shard; do not add Linux+Windows serial |

**PLAN amendment:** Wave 1 CI is already sharded (`fmt` ∥ `clippy` ∥ `test-lib`). Wave 2 phase-end `cargo test --workspace` is **not** one job. Caps `gate_shard_max_min` = 10 applies. Pre-declare `gates-shard-unit`, `gates-shard-ledger-prop`, `gates-shard-migrate`. Do not discover this at INTEGRATE.

`WICKET_REQUIRE_PG=1` on CI so skipped tests cannot green-wash a missing service.

---

## 11. PLAN gaps (checklist for plan-audit.md)

1. Stub contents unspecified — **this SPEC is the missing subtask artifact.**
2. `rust-toolchain.toml` named, never pinned.
3. sqlx offline / per-crate `.sqlx` / no `--workspace` prepare — unspecified. Classic footgun.
4. No TestDb / Postgres-for-tests in Wave 1. Cannot run PLAN §7 migration tests or `query!` prepare.
5. Property-test AC does not distinguish in-memory vs Postgres vs cross-crate.
6. Wave 2 file-ownership does not explicitly forbid editing workspace manifests, lockfile policy, or `.sqlx` location.
7. `wicket-test` crate missing from §5.
8. `wicket-core` treated as a stub peer of `wicket-ledger`; it should be complete in Wave 1 (gated on Quantity DECISION).
9. ADR 0006 Open vs `[workspace.package].license`.
10. `.gitignore` dual-owned with `doc-repo`.
11. Wave 2 "13 lanes" vs 15 crates — list members anyway; do not let the off-by-one drop `wicket-module` from the stub set.
12. `missing_docs = deny` / `clippy::todo` will fail empty stubs; lint policy must match stub bodies.
13. No pin of sqlx-cli, sqlx version, or rustc.
14. No rule that Wave 2 `query!` cannot join unmerged sibling tables.
15. Workspace lane too large to invent, not too large to type — needs opus SPEC before dispatch, not a three-way split.

---

## 12. One-page executor card (paste into the workspace SPEC)

**Goal.** A virtual Cargo workspace that `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo test --workspace --lib` pass on a clean checkout **without** Postgres, and from which 13 isolated Wave 2 worktrees can compile crate tests against frozen types.

**Acceptance.**

- [ ] `rust-toolchain.toml` pins `1.98.1` (or the then-current stable, exact version) + rustfmt + clippy.
- [ ] `[workspace].members` lists all 15 PLAN crates plus `wicket-test`.
- [ ] Dependency graph in crate manifests matches PLAN §5 exactly.
- [ ] `[workspace.dependencies]` closed allow-list; `[lints] workspace = true` on every member.
- [ ] `wicket-core` is a complete primitive crate (post-Quantity-decision), not `Unimplemented`.
- [ ] Every other library crate exports the types in §5 of this report; fallible bodies return `Error::Unimplemented`; **zero** `todo!`/`unimplemented!`.
- [ ] Every db-backed crate has `migrate!`, `build.rs`, and a reversible `00000000000000_placeholder` migration; no real tables.
- [ ] Zero `sqlx::query!` macros. Zero workspace-root `.sqlx/`.
- [ ] `crates/wicket-test` implements `TestDb::connect` / `migrate` / `postgres_available`.
- [ ] `dev/compose.yml` Postgres 16/17; `.env.example` documents `DATABASE_URL` and does **not** set `SQLX_OFFLINE`.
- [ ] CI: fmt, clippy `-D warnings`, test-lib, cached toolchain; Postgres service present but not required for test-lib.
- [ ] `justfile` recipes in §4.6.
- [ ] No file owned by `doc-repo` except a coordinated `.gitignore` Rust block.

**Out of scope.** Ledger engine, conversion tables, audit interceptor body, GHA OS matrix polish beyond one Linux job, license text, README.

---

## Sources

- PLAN.md §§3, 5, 7, 9
- docs/02-architecture.md §§2–3, 5, 7, 9
- docs/adr/0002, 0003, 0004, 0005, 0006
- sqlx-cli README (offline mode, `--workspace`, `SQLX_OFFLINE`)
- sqlx issues 1223, 1338, 1770, 2667, 3644, 3836, 3961; discussion 4215
- Clippy/Cargo workspace lints (`[lints] workspace = true` required per member)
- Rust stable 1.98.1 (2026-09-03)
