# Wicket

<p align="center"><img src="design/icon-wicket-512.png" width="160" alt="Wicket ERP icon"></p>

Conforms to: [ADR 0003](docs/adr/0003-database.md) (Accepted, as amended), [ADR 0006](docs/adr/0006-license.md) (Accepted), [ADR 0010](docs/adr/0010-one-registry.md) (Proposed).

Audience: operator. Status: partial.

## 1. What this is

Wicket is an open source ERP for discrete manufacturing. The wedge is a 21 CFR Part 11 electronic signature — no surveyed open source ERP has one (`research/background/competitive-landscape.md` §0.1) — and an append-only ledger of quantity and value, both as kernel properties that a module cannot turn off. Everything else named in the vision (quality workflows, a Device History Record, CAD-native estimating) is a roadmap, not a present product (`docs/01-vision-and-scope.md`).

## 2. Status

Pre-alpha. There is no release. The kernel is being built. See [docs/07-roadmap.md](docs/07-roadmap.md).

The measure of progress is the Wave 2s slice acceptance (`crates/wicket-server/tests/slice.rs`) under both profiles, run by `just ci-db`. Stars, crate count and commit count are not the measure (`docs/07-roadmap.md` §5.4). The public build runs `just ci-db` (`.github/workflows/ci.yml` job `ci-db`) and is green. `just ci` is not that run: it calls `test-lib`, which passes `--lib` and excludes every integration test.

## 3. Who it is for

The beachhead is a 10-to-100-person regulated device shop — machined implants, instruments, single-use disposables — that today splits production records and quality records across two systems (`docs/01-vision-and-scope.md` §3).

A plain shop — a job shop that is not regulated — runs the same binary with regulated modules disabled and never sees a quality screen (`PLAN.md` §1a).

## 4. How to read the docs

A first small change must start from [GOALS.md](GOALS.md), [CONTRIBUTING.md](CONTRIBUTING.md), and [docs/00-erp-primer.md](docs/00-erp-primer.md). GOALS.md is binding. CONTRIBUTING.md is the DCO, the change classes, and what a pull request must answer. The primer is what an ERP does, taught by following one order of titanium bone screws from a customer request through to a recall query.

[AGENTS.md](AGENTS.md) is the standing orders for anyone pointing an AI agent at this repository. [TODO.md](TODO.md) is the sequenced backlog.

Before touching the kernel, the ledger, or the module contract, half a day of reading. There is no shortcut.

1. **[GOALS.md](GOALS.md)** — the four things that must be true of every capability this project grows: it is legible to an AI agent, every function has an API, a shop can migrate onto it from an incumbent ERP, and both installing it and improving it are followable. Short, and binding. Section 6 states what is true today, which is mostly "not yet".
2. **[docs/00-erp-primer.md](docs/00-erp-primer.md)** — what an ERP does, taught by following one order of titanium bone screws from a customer request through to a recall query eighteen months later. Written for someone who has never worked inside one. Start here even if you think you know.
3. **[docs/01-vision-and-scope.md](docs/01-vision-and-scope.md)** — who this is for and what it refuses to do.
4. **[docs/02-architecture.md](docs/02-architecture.md)** — the kernel and module split, and the ledger.
5. **[docs/adr/](docs/adr/)** — ten decisions, each with its costs stated. `README.md` indexes them.
6. **[PLAN.md](PLAN.md)** — the three-wave build, seventeen invariants, and the crate contract.
7. **[research/README.md](research/README.md)** — where the evidence lives.
8. **[DESIGN.md](DESIGN.md)** — binding interface rules.

## 5. Building

Rust **1.98.1** is pinned in `rust-toolchain.toml` (`PLAN.md` §11). PostgreSQL **17** is installed and lifecycle-managed by the operating system; Wicket never installs or manages PostgreSQL (`docs/adr/0003-database.md` as amended). You also need `just` and `sqlx-cli` 0.9.0 (`PLAN.md` §11).

`just ci` is the offline lint and library-test gate: `fmt-check`, `clippy`, `lint-sql`, and `test-lib` (`PLAN.md` §11). `test-lib` passes `--lib` and excludes every integration test under `crates/*/tests/`, including the Wave 2s slice suite. Green `just ci` is not slice acceptance.

`just ci-db` is the slice and acceptance gate. It runs `crates/wicket-server/tests/slice.rs` under both profiles. Hosted CI on the public repository is on (`.github/workflows/ci.yml`); the `ci-db` job is green.

Machines with a container runtime can bring up Postgres from `dev/compose.yml` (image `postgres:17`). Machines without one use the OS-managed server on `127.0.0.1:5432`. Wicket never installs that server. Run `just db-gc` to drop orphaned `wicket_t_*` test databases left behind by killed test runs (default age threshold 60 minutes, override with `WICKET_DB_GC_MIN`).

## 6. License

Wicket is licensed under the GNU Affero General Public License v3.0 or later; see LICENSE. Contributions are accepted under the Developer Certificate of Origin; see CONTRIBUTING.md. Reasoning: [docs/adr/0006-license.md](docs/adr/0006-license.md).

## 7. Contributing

Read [GOALS.md](GOALS.md) and then [CONTRIBUTING.md](CONTRIBUTING.md) before you open a pull request.
Every pull request answers the four goals. The CI check that enforces this is ABSENT (promised: `TODO.md` T-06); until it lands the maintainer enforces it by hand.
[TODO.md](TODO.md) is the sequenced backlog, and its Stage 0 is startable today.
[AGENTS.md](AGENTS.md) is the standing orders for anyone pointing an AI agent at this repository.
Use `git commit -s` so every commit carries a Developer Certificate of Origin sign-off.
The public repository is https://github.com/BlinkingSun/wicket-erp.
