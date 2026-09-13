# Wicket

<p align="center"><img src="design/icon-wicket-512.png" width="160" alt="Wicket ERP icon"></p>

Conforms to: [ADR 0003](docs/adr/0003-database.md) (Accepted, as amended), [ADR 0006](docs/adr/0006-license.md) (Accepted).

## 1. What this is

Wicket is an open source ERP for discrete manufacturing. The wedge is a 21 CFR Part 11 electronic signature — no surveyed open source ERP has one (`research/background/competitive-landscape.md` §0.1) — and an append-only ledger of quantity and value, both as kernel properties that a module cannot turn off. Everything else named in the vision (quality workflows, a Device History Record, CAD-native estimating) is a roadmap, not a present product (`docs/01-vision-and-scope.md`).

## 2. Status

Pre-alpha. There is no release. The kernel is being built. See [docs/07-roadmap.md](docs/07-roadmap.md).

## 3. Who it is for

The beachhead is a 10-to-100-person regulated device shop — machined implants, instruments, single-use disposables — that today splits production records and quality records across two systems (`docs/01-vision-and-scope.md` §3).

A plain shop — a job shop that is not regulated — runs the same binary with regulated modules disabled and never sees a quality screen (`PLAN.md` §1a).

## 4. How to read the docs

Half a day of reading. There is no shortcut.

1. **[docs/00-erp-primer.md](docs/00-erp-primer.md)** — what an ERP does, taught by following one order of titanium bone screws from a customer request through to a recall query eighteen months later. Written for someone who has never worked inside one. Start here even if you think you know.
2. **[docs/01-vision-and-scope.md](docs/01-vision-and-scope.md)** — who this is for and what it refuses to do.
3. **[docs/02-architecture.md](docs/02-architecture.md)** — the kernel and module split, and the ledger.
4. **[docs/adr/](docs/adr/)** — nine decisions, each with its costs stated. `README.md` indexes them.
5. **[PLAN.md](PLAN.md)** — the three-wave build, seventeen invariants, and the crate contract.
6. **[research/README.md](research/README.md)** — where the evidence lives.
7. **[DESIGN.md](DESIGN.md)** — binding interface rules.

## 5. Building

Rust **1.98.1** is pinned in `rust-toolchain.toml` (`PLAN.md` §11). PostgreSQL **17** is installed and lifecycle-managed by the operating system; Wicket never installs or manages PostgreSQL (`docs/adr/0003-database.md` as amended). You also need `just` and `sqlx-cli` 0.9.0 (`PLAN.md` §11).

`just ci` is the local gate: `fmt-check`, `clippy`, `lint-sql`, and `test-lib` (`PLAN.md` §11). Machines with a container runtime can bring up Postgres from `dev/compose.yml` (image `postgres:17`). Machines without one use the OS-managed server on `127.0.0.1:5432`. Wicket never installs that server. Run `just db-gc` to drop orphaned `wicket_t_*` test databases left behind by killed test runs (default age threshold 60 minutes, override with `WICKET_DB_GC_MIN`).

## 6. License

Wicket is licensed under the GNU Affero General Public License v3.0 or later; see LICENSE. Contributions are accepted under the Developer Certificate of Origin; see CONTRIBUTING.md. Reasoning: [docs/adr/0006-license.md](docs/adr/0006-license.md).

## 7. Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before you open a pull request.
Use `git commit -s` so every commit carries a Developer Certificate of Origin sign-off.
The public repository is https://github.com/BlinkingSun/wicket-erp.
