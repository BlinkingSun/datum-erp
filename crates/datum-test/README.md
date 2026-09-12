# datum-test

Commit-mode PostgreSQL test harness. Each case clones an ephemeral database from
`DATUM_TEST_TEMPLATE` and drops it afterwards so deferred constraint triggers fire
at commit (a rolled-back wrapper would never arm them). CONTRACT §5a exemption:
this crate may use session-protocol SQL; it never ships in the binary.

## Public API (`src/lib.rs`)

- `TestDb` — per-test database (`migrate_pool`, `app_pool`, `bootstrap_pool`, `database`)
- `TestDb::case` — `CREATE DATABASE datum_t_<name>_<random> OWNER datum_owner TEMPLATE …`
- `TestDb::migrate` — run given migrators as `datum_migrate` (order is the caller's)
- `TestDb::begin` — transaction that the test must commit or roll back (never auto-rollback)
- `TestDb::finish` — `DROP DATABASE … WITH (FORCE)`
- `Error` — `Unavailable` / `Sqlx` / `Migration` / `Env`
- `postgres_available` — `Err(reason)` if migrate URL unset or server silent for 2 s
- `require_postgres` — panics with that reason (`DATUM_REQUIRE_PG=1`)
- `db_case!` — skip or panic according to `DATUM_REQUIRE_PG`, then `TestDb::case`

## Migrations

None. The harness does not own schema. Template grants come from `dev/sql`.

## Tests (`src/lib.rs`; no `tests/` directory)

- `connect_reports_unavailable_without_url`
- `clone_retries_while_template_is_in_use` — CREATE DATABASE 55006 retry
- `case_database_is_owned_by_datum_owner`
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
exists so module crates can call `datum_audit::install_privileged` without
opening a superuser connection themselves (CONTRACT §5a.1). `just db-gc` reads
the `datum.harness_created_at` comment this crate stamps.
