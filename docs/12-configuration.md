# Configuration

Audience: operator. Status: partial (the procedures T-71+ are ABSENT).

Conforms to: [ADR 0003](adr/0003-database.md) (Accepted, as amended); `PLAN.md` §1a;
`docs/09-workspace-contract.md` §8; D3 (`research/decisions/audit-persistence.md` §1.1).

This is the canon table of every environment variable the source reads. `.env.example`
is a strict subset of this table. Loading a `.env` file into the process is ABSENT; the
operator must export the variables (or pass equivalent CLI / TOML values where this
table says they exist).

Provisioning, profile choice, first boot, backup, and hardening procedures are ABSENT
(promised: `TODO.md` T-71 through T-80).

---

## 1. Facts that are widely misread

- **`WICKET_REQUIRE_PG` is test-harness only.** No production path reads it. The
  variable is read by `crates/wicket-test/src/lib.rs` (`db_case!`, `require_postgres`)
  and by test `common` modules that copy that skip/fail rule. It is not a production
  boot requirement. Setting it does not start the server, and leaving it unset does
  not prevent a production boot.
- **`WICKET_BLOB_ROOT` is the only hard production boot failure.** `App::boot` refuses
  both `plain-shop` and `regulated-device` when it is missing, empty, or not a writable
  directory (`crates/wicket-server/src/boot.rs`). There is no TOML key, no CLI flag, no
  default, and no temp-directory fallback. The process does not create the directory.
- **`WICKET_SETTINGS` is not an environment variable.** It is a Rust constant of
  PostgreSQL `wicket.*` GUC names (`crates/wicket-db/src/lib.rs`). The OS environment
  has no `WICKET_SETTINGS`.

---

## 2. Classes

| Class | Meaning |
|---|---|
| required-at-boot | `wicket serve` / `App::boot` / `Config::load` fails without a value from env or the named fallback. |
| optional-at-runtime | Read when present; a default or a skip path exists. |
| test-only | Read by the test harness, `just` database recipes, or test child processes. Not a production boot input. |
| compile-time | Baked in at build (`option_env!` / `cargo:rustc-env`). Setting it in the process environment at run time does nothing. |
| not-an-env-var | A name that appears next to the `WICKET_` prefix in source and is not an OS environment variable. |

---

## 3. Canon table

Primary read site is the production or harness path. Tests that copy the same name are
not listed.

| Name | Class | Source path that reads it |
|---|---|---|
| `WICKET_BLOB_ROOT` | required-at-boot | `crates/wicket-documents/src/blob.rs` (`FsBlobStore::from_env`); `crates/wicket-server/src/boot.rs` (`compose_blob_store` / `App::boot`) |
| `WICKET_DATABASE_URL` | required-at-boot | `crates/wicket-server/src/config.rs` (`Config::load`); `crates/wicket-server/src/cli.rs` (`wicket db check`) |
| `WICKET_MIGRATE_DATABASE_URL` | required-at-boot | `crates/wicket-server/src/config.rs` (`Config::load`) |
| `WICKET_BOOTSTRAP_URL` | required-at-boot | `crates/wicket-server/src/config.rs` (`Config::load`) |
| `WICKET_PROFILE` | required-at-boot | `crates/wicket-server/src/config.rs` (`Config::load`; also `--profile`, TOML `profile`) |
| `WICKET_BIND` | optional-at-runtime | `crates/wicket-server/src/config.rs` (default `0.0.0.0:8080`; also `--bind`) |
| `WICKET_CONFIG` | optional-at-runtime | `crates/wicket-server/src/config.rs` (TOML path when `--config` is omitted) |
| `WICKET_GENEALOGY_INLINE_MAX` | optional-at-runtime | `modules/genealogy/src/store.rs` (`inline_max_postings`; default 32 from `modules/genealogy/src/domain.rs`) |
| `PGUSER` | optional-at-runtime | `crates/wicket-server/src/config.rs` (`with_os_userinfo`); `crates/wicket-test/src/lib.rs` (`os_username`) |
| `USER` | optional-at-runtime | same as `PGUSER` (fallback) |
| `LOGNAME` | optional-at-runtime | same as `PGUSER` (fallback) |
| `WICKET_REQUIRE_PG` | test-only | `crates/wicket-test/src/lib.rs` (`db_case!`, harness tests). **Not a production boot requirement.** |
| `WICKET_TEST_TEMPLATE` | test-only | `crates/wicket-test/src/lib.rs` (`TestDb::case`); `scripts/wicket-db-env.sh` |
| `WICKET_TEST_DB` | test-only | `scripts/wicket-db-env.sh` (default: database path in `WICKET_DATABASE_URL`, else `wicket_test`) |
| `WICKET_TEST_CHILD` | test-only | `crates/wicket-test/src/lib.rs`; `crates/wicket-server/src/boot.rs` (blob-root child tests) |
| `WICKET_DB_GC_MIN` | test-only | `justfile` (`db-gc`; default 60) |
| `WICKET_GIT_DESCRIBE` | compile-time | `crates/wicket-db/build.rs` writes `cargo:rustc-env`; `crates/wicket-db/src/lib.rs` (`app_version`, `option_env!`) |
| `WICKET_SETTINGS` | not-an-env-var | `crates/wicket-db/src/lib.rs` (constant of `wicket.*` GUC names bound by `Tx::begin`) |

---

## 4. Required at boot

`wicket serve --profile <plain-shop|regulated-device>` still needs the blob root and the
three D3 URLs. Profile may come from `--profile`, `WICKET_PROFILE`, or TOML `profile`.
The three URLs may come from the environment or from the TOML file named by `--config` /
`WICKET_CONFIG`. `WICKET_BLOB_ROOT` has no such fallback.

| Name | What it must be |
|---|---|
| `WICKET_BLOB_ROOT` | Existing writable directory. Content-addressed blobs and archived print live here. Missing, empty, or a non-directory fails boot on both profiles. |
| `WICKET_DATABASE_URL` | PostgreSQL URL as role `wicket_app`. |
| `WICKET_MIGRATE_DATABASE_URL` | PostgreSQL URL as role `wicket_migrate`. |
| `WICKET_BOOTSTRAP_URL` | PostgreSQL URL as a role that can create databases and install. On this MacBook loopback trust the URL has no userinfo; other nodes must add user:password. `Config::load` rewrites the database path to the app database so install does not run against `postgres`. |
| `WICKET_PROFILE` | `plain-shop` or `regulated-device`. `wicket serve` requires `--profile` on the command line; migrate / iq / manifest export read this variable when `--profile` is omitted. |

`.env.example` holds development-only placeholders for these names. Production must not
use those passwords or loopback trust.

---

## 5. Optional at runtime

| Name | Default / behaviour when unset |
|---|---|
| `WICKET_BIND` | `0.0.0.0:8080` |
| `WICKET_CONFIG` | No file; CLI flags and the other variables only |
| `WICKET_GENEALOGY_INLINE_MAX` | `32` unique postings before a genealogy trace is enqueued as a job |
| `PGUSER` / `USER` / `LOGNAME` | Used only to fill userinfo on a URL that has none, so sqlx does not connect as `anonymous`. `PGUSER` wins, then `USER`, then `LOGNAME`, then `id -un` |

---

## 6. Test-only

`WICKET_REQUIRE_PG=1` turns a missing Postgres into a test panic instead of a skip. CI
sets it. Production must not.

`WICKET_TEST_TEMPLATE` is the template database `TestDb::case` clones (required when
tests run; `just db-reset` defaults it to `wicket_test_template`). `WICKET_TEST_DB` names
the standing database `just db-reset` / `just db-gc` keep. `WICKET_TEST_CHILD` is an
internal flag for harness child processes. `WICKET_DB_GC_MIN` is the age threshold in
minutes for `just db-gc` (default 60).

---

## 7. Compile-time and names that are not environment variables

`WICKET_GIT_DESCRIBE` is stamped at compile time from `git describe --always --dirty --abbrev=12`
when that command succeeds. It is not read from the process environment at run time.

`WICKET_SETTINGS` is the list of `wicket.*` GUCs `Tx::begin` binds. It is not an OS
environment variable.

Cargo injects `CARGO_PKG_VERSION` and `CARGO_MANIFEST_DIR` at compile / test time. Those
are not operator configuration. There is no product `DATABASE_URL`; `just migrate` and
`just sqlx-prepare` export `DATABASE_URL` from `WICKET_MIGRATE_DATABASE_URL` for sqlx-cli
only.

`SQLX_OFFLINE=true` lives in `.cargo/config.toml` and nowhere else.

---

## 8. Example file

`.env.example` at the repository root is the development example. It is a strict subset
of this table: every name it sets or comments appears above. It includes every
required-at-boot variable. Test-only names are commented.

A first-boot procedure that copies that file, creates the blob directory, applies
`dev/sql/*.sql`, and runs `wicket serve` with an expected result is ABSENT (promised:
`TODO.md` T-76).
