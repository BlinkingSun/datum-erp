# wicket-test

Commit-mode PostgreSQL test harness. Each case clones an ephemeral database from
`WICKET_TEST_TEMPLATE` and drops it afterwards so deferred constraint triggers fire
at commit (a rolled-back wrapper would never arm them). CONTRACT §5a exemption:
this crate may use session-protocol SQL; it never ships in the binary.

## Public API (`src/lib.rs`)

- `TestDb` — per-test database (`migrate_pool`, `app_pool`, `bootstrap_pool`, `database`)
- `TestDb::case` — `CREATE DATABASE wicket_t_<name>_<random> OWNER wicket_owner TEMPLATE …`
- `TestDb::migrate` — run given migrators as `wicket_migrate` (order is the caller's)
- `TestDb::begin` — transaction that the test must commit or roll back (never auto-rollback)
- `TestDb::finish` — `DROP DATABASE … WITH (FORCE)`
- `Error` — `Unavailable` / `Sqlx` / `Migration` / `Env`
- `postgres_available` — `Err(reason)` if migrate URL unset or server silent for 2 s
- `require_postgres` — panics with that reason (`WICKET_REQUIRE_PG=1`)
- `db_case!` — skip or panic according to `WICKET_REQUIRE_PG`, then `TestDb::case`

## Migrations

None. The harness does not own schema. Template grants come from `dev/sql`.

## Tests (`src/lib.rs`; no `tests/` directory)

- `connect_reports_unavailable_without_url`
- `clone_retries_while_template_is_in_use` — CREATE DATABASE 55006 retry
- `case_database_is_owned_by_wicket_owner`
- `bootstrap_pool_connects_as_superuser`
- `migrate_role_can_create_schema_in_case_database`
- `case_databases_are_isolated`
- `begin_does_not_auto_rollback`
- `canary_deferred_constraint_fires_at_commit`
- `grants_pattern_holds` — app SELECT/INSERT/UPDATE; no app DELETE outside transient; audit SELECT-only; no TRUNCATE
- `pool_hooks_reset_state`
- `require_pg_hard_fails`
- `error_unavailable_formats`

## Frozen / seams

Frozen: case-clone protocol and the three URL names (CONTRACT §8). `bootstrap_pool`
exists so module crates can call `wicket_audit::install_privileged` without
opening a superuser connection themselves (CONTRACT §5a.1). `just db-gc` reads
the `wicket.harness_created_at` comment this crate stamps.

## Per-worktree isolation

`TestDb::case` clones from `WICKET_TEST_TEMPLATE` (required at runtime; `just test-db`
defaults it to `wicket_test_template`). The standing template and case database that
`just db-reset` creates are named by `WICKET_TEST_TEMPLATE` and `WICKET_TEST_DB`
(CONTRACT §8). Roles stay cluster-wide (`wicket_app` / `wicket_migrate` / `wicket_owner`);
only the database names isolate concurrent worktrees on one Postgres.

Example (a gate worktree sharing the Homebrew cluster with other lanes):

```
WICKET_TEST_TEMPLATE=wicket_tpl_gate
WICKET_TEST_DB=wicket_gate
WICKET_DATABASE_URL=postgres://wicket_app:wicket@127.0.0.1:5432/wicket_gate?sslmode=disable
WICKET_MIGRATE_DATABASE_URL=postgres://wicket_migrate:wicket@127.0.0.1:5432/wicket_gate?sslmode=disable
WICKET_BOOTSTRAP_URL=postgres://127.0.0.1:5432/postgres?sslmode=disable
```

Defaults stay `wicket_test_template` / `wicket_test` so the three CI nodes and public CI
are unchanged.
