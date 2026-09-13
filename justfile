set ignore-comments := true
set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

root := justfile_directory()

# Format every crate.
fmt:
    cargo fmt --all --manifest-path "{{root}}/Cargo.toml"

# Check formatting; CI uses this.
fmt-check:
    cargo fmt --all --manifest-path "{{root}}/Cargo.toml" -- --check

# Workspace clippy with warnings denied.
clippy:
    cargo clippy --manifest-path "{{root}}/Cargo.toml" --workspace --all-targets --all-features -- -D warnings

# String-level raw-SQL fence (CONTRACT §5a / §5a.1). Fails on any hit outside datum-db / datum-audit / datum-test.
lint-sql:
    command -v rg >/dev/null 2>&1 || { echo 'lint-sql: ripgrep (rg) is required' >&2; exit 1; }
    if rg -n --glob '*.rs' --glob '!**/datum-db/**' --glob '!**/datum-audit/**' --glob '!**/datum-test/**' \
        -e 'QueryBuilder' -e 'raw_sql' -e 'copy_in_raw' -e 'set_config' -e 'current_setting' \
        "{{root}}/crates"; then \
      echo "lint-sql: session-protocol SQL token outside crates/datum-db, crates/datum-audit, and crates/datum-test" >&2; \
      exit 1; \
    fi
    if rg -n -U --multiline-dotall --glob '*.rs' --glob '!**/datum-db/**' --glob '!**/datum-audit/**' --glob '!**/datum-test/**' \
        -e 'allow\(.{0,400}?clippy::disallowed_' \
        "{{root}}/crates"; then \
      echo "lint-sql: clippy disallowed allow outside crates/datum-db, crates/datum-audit, and crates/datum-test" >&2; \
      exit 1; \
    fi
    if rg -n --glob 'build.rs' --glob '!**/datum-db/**' --glob '!**/datum-audit/**' --glob '!**/datum-test/**' \
        -e 'sqlx' \
        "{{root}}/crates"; then \
      echo "lint-sql: sqlx token in build.rs outside crates/datum-db, crates/datum-audit, and crates/datum-test" >&2; \
      exit 1; \
    fi
    if rg -n --glob '*.rs' --glob '!**/datum-db/**' --glob '!**/datum-audit/**' --glob '!**/datum-test/**' \
        -e 'GRANT ' -e 'CREATE DATABASE' \
        "{{root}}/crates"; then \
      echo "lint-sql: GRANT or CREATE DATABASE outside crates/datum-db, crates/datum-audit, and crates/datum-test" >&2; \
      exit 1; \
    fi
    # PLAN section 6 invariant 6: no crate's src reads another crate's schema-qualified tables.
    # Allow-list: owning crate, datum-module (composition root), datum-test (harness).
    # Production src only - kernel tests may probe audit.event / seed uom.item_stock.
    # Scan per-crate src/ (no path-separator globs): negative **/owner/** fails on Windows paths.
    fail=0; \
    for pair in identity:datum-identity uom:datum-uom ledger:datum-ledger sm:datum-statemachine jobs:datum-jobs events:datum-events numbering:datum-numbering audit:datum-audit items:datum-mod-items locations:datum-mod-locations lots:datum-mod-lots inventory:datum-mod-inventory production_min:datum-mod-production-min genealogy:datum-mod-genealogy; do \
      schema="${pair%%:*}"; \
      owner="${pair##*:}"; \
      for tree in "{{root}}/crates" "{{root}}/modules"; do \
        if [ ! -d "$tree" ]; then continue; fi; \
        for crate_dir in "$tree"/*; do \
          if [ ! -d "$crate_dir" ]; then continue; fi; \
          crate="$(basename "$crate_dir")"; \
          case "$crate" in "$owner"|datum-module|datum-test) continue ;; esac; \
          if [ "$crate" = "$schema" ]; then continue; fi; \
          src_dir="$crate_dir/src"; \
          if [ ! -d "$src_dir" ]; then continue; fi; \
          if rg -n -i --glob '*.rs' \
              -e "(FROM|JOIN|INTO|UPDATE|TABLE)[[:space:]]+(ONLY[[:space:]]+)?${schema}\\." \
              "$src_dir"; then \
            echo "lint-sql: cross-module table read of ${schema}.* outside ${owner}, datum-module, and datum-test" >&2; \
            fail=1; \
          fi; \
        done; \
      done; \
    done; \
    if [ "$fail" -ne 0 ]; then exit 1; fi
    # R-2s-3: production src SQL must not DML-reference ledger.* / transient.*
    # outside the explicit owner exemption set (see scripts/lint-sql-r2s3.sh).
    REPO_ROOT="{{root}}" bash "{{root}}/scripts/lint-sql-r2s3.sh"
    REPO_ROOT="{{root}}" bash "{{root}}/scripts/lint-sql-migrations.sh"

# Plant bad migrations in a throwaway tree (never the real repo) and assert the lints fail.
lint-sql-selftest:
    command -v rg >/dev/null 2>&1 || { echo 'lint-sql-selftest: ripgrep (rg) is required' >&2; exit 1; }
    root="{{root}}"; \
    tmp="$(mktemp -d "${TMPDIR:-/tmp}/lint-sql-selftest.XXXXXX")"; \
    cleanup() { rm -rf "$tmp"; }; \
    trap cleanup EXIT; \
    trap 'cleanup; exit 130' INT TERM; \
    mkdir -p \
      "$tmp/crates/datum-server/migrations" \
      "$tmp/crates/datum-uom/migrations" \
      "$tmp/modules/items/migrations" \
      "$tmp/modules/items/src" \
      "$tmp/modules/lots/src" \
      "$tmp/modules/locations/src"; \
    for f in "$root/crates/datum-uom/migrations/"*.up.sql; do \
      if [ -f "$f" ]; then cp "$f" "$tmp/crates/datum-uom/migrations/"; fi; \
    done; \
    : > "$tmp/modules/locations/src/store.rs"; \
    plant_cross="$tmp/crates/datum-server/migrations/99999999999999_lint_sql_selftest_cross.up.sql"; \
    plant_session="$tmp/modules/items/migrations/99999999999999_lint_sql_selftest_session.up.sql"; \
    plant_create="$tmp/crates/datum-server/migrations/99999999999998_lint_sql_selftest_definer_create.up.sql"; \
    plant_drop="$tmp/crates/datum-server/migrations/99999999999999_lint_sql_selftest_definer_drop.up.sql"; \
    plant_orphan="$tmp/crates/datum-server/migrations/99999999999999_lint_sql_selftest_definer_orphan.up.sql"; \
    plant_r2s3="$tmp/modules/items/src/_lint_sql_r2s3_selftest.rs"; \
    plant_r2s3_ok="$tmp/modules/lots/src/_lint_sql_r2s3_ok.rs"; \
    if ! REPO_ROOT="$tmp" bash "$root/scripts/lint-sql-migrations.sh" --selftest-hits; then \
      echo 'lint-sql-selftest: Windows-shaped hit parser/neutralization failed' >&2; \
      exit 1; \
    fi; \
    if ! REPO_ROOT="$tmp" bash "$root/scripts/lint-sql-r2s3.sh" --selftest-hits; then \
      echo 'lint-sql-selftest: Windows-shaped R-2s-3 hit parser failed' >&2; \
      exit 1; \
    fi; \
    run_lint() { REPO_ROOT="$tmp" bash "$root/scripts/lint-sql-migrations.sh" 2>/dev/null; }; \
    printf '%s\n' '-- lint-sql-selftest: must be rejected (cross-schema DML)' \
      'UPDATE sm.machine SET name = name WHERE false;' > "$plant_cross"; \
    if run_lint; then \
      echo 'lint-sql-selftest: expected migration lint to fail on planted cross-schema SQL' >&2; \
      exit 1; \
    fi; \
    echo 'lint-sql-selftest: planted cross-schema migration correctly rejected'; \
    rm -f "$plant_cross"; \
    printf '%s\n' '-- lint-sql-selftest: must be rejected (session-protocol bypass)' \
      "SELECT set_config('datum.actor', 'lint-selftest', true);" > "$plant_session"; \
    if run_lint; then \
      echo 'lint-sql-selftest: expected migration lint to fail on planted set_config(datum.*)' >&2; \
      exit 1; \
    fi; \
    echo 'lint-sql-selftest: planted session-protocol migration correctly rejected'; \
    rm -f "$plant_session"; \
    printf '%s\n' '-- lint-sql-selftest: paired SECURITY DEFINER must pass net-effect' \
      'CREATE FUNCTION server.lint_sql_selftest_definer_pair() RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog AS $$ BEGIN NULL; END $$;' > "$plant_create"; \
    printf '%s\n' 'DROP FUNCTION IF EXISTS server.lint_sql_selftest_definer_pair();' > "$plant_drop"; \
    if ! run_lint; then \
      echo 'lint-sql-selftest: expected create-then-drop SECURITY DEFINER to pass' >&2; \
      exit 1; \
    fi; \
    echo 'lint-sql-selftest: create-then-drop SECURITY DEFINER correctly allowed'; \
    rm -f "$plant_create" "$plant_drop"; \
    printf '%s\n' '-- lint-sql-selftest: unpaired SECURITY DEFINER must fail' \
      'CREATE FUNCTION server.lint_sql_selftest_definer_orphan() RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog AS $$ BEGIN NULL; END $$;' > "$plant_orphan"; \
    if run_lint; then \
      echo 'lint-sql-selftest: expected migration lint to fail on unpaired SECURITY DEFINER' >&2; \
      exit 1; \
    fi; \
    echo 'lint-sql-selftest: unpaired SECURITY DEFINER correctly rejected'; \
    printf '%s\n' \
      'fn _lint_sql_r2s3_selftest() { let _ = "SELECT 1 FROM ledger.posting WHERE false"; }' \
      > "$plant_r2s3"; \
    printf '%s\n' \
      '/// comment FROM ledger.posting and transient.balance_projection must not trip the rule' \
      'fn _lint_sql_r2s3_ok() {' \
      '    let _ = "SELECT 1 FROM inventory_transient.idempotency";' \
      '    let _ = "VALUES ($1::ledger.boundary)";' \
      '    let _ = "SELECT ledger.has_postings($1)";' \
      '}' \
      > "$plant_r2s3_ok"; \
    r2s3_out="$(REPO_ROOT="$tmp" bash "$root/scripts/lint-sql-r2s3.sh" 2>&1 || true)"; \
    if ! printf '%s\n' "$r2s3_out" | grep -F 'modules/items/src/_lint_sql_r2s3_selftest.rs' >/dev/null; then \
      echo 'lint-sql-selftest: expected R-2s-3 lint to report planted ledger.posting SQL' >&2; \
      printf '%s\n' "$r2s3_out" >&2; \
      exit 1; \
    fi; \
    if printf '%s\n' "$r2s3_out" | grep -F 'modules/lots/src/_lint_sql_r2s3_ok.rs' >/dev/null; then \
      echo 'lint-sql-selftest: R-2s-3 lint false-positive on comment / type-cast / *_transient / published function' >&2; \
      printf '%s\n' "$r2s3_out" >&2; \
      exit 1; \
    fi; \
    echo 'lint-sql-selftest: planted R-2s-3 ledger.* SQL correctly rejected'; \
    echo 'lint-sql-selftest: R-2s-3 negatives (comment, ::ledger.boundary, inventory_transient, has_postings) correctly allowed'

# All tests, including integration.
test:
    cargo test --manifest-path "{{root}}/Cargo.toml" --workspace --all-features

# Library tests only; must pass with no DATUM_*_URL.
test-lib:
    cargo test --manifest-path "{{root}}/Cargo.toml" --workspace --lib --all-features

# Database tests; missing Postgres is a failure.
# Resolves DATUM_TEST_TEMPLATE / DATUM_TEST_DB (defaults match public CI).
test-db:
    . "{{root}}/scripts/datum-db-env.sh"; \
    DATUM_REQUIRE_PG=1 cargo test --manifest-path "{{root}}/Cargo.toml" --workspace --all-features

# Bring Postgres up. Docker when present; otherwise pg_isready, fail closed.
db-up:
    if command -v docker >/dev/null 2>&1; then \
      docker compose -f "{{root}}/dev/compose.yml" up -d; \
    else \
      prefix="$(brew --prefix postgresql@17 2>/dev/null)/bin"; \
      if [ ! -x "${prefix}/pg_isready" ]; then prefix="$(dirname "$(command -v pg_isready)")"; fi; \
      if ! "${prefix}/pg_isready" -h 127.0.0.1 -p 5432 >/dev/null 2>&1; then \
        echo "postgres is not accepting connections on 127.0.0.1:5432" >&2; \
        echo "brew services start postgresql@17" >&2; \
        exit 1; \
      fi; \
    fi

# Bring Postgres down. Docker when present; otherwise print the Homebrew stop command.
db-down:
    if command -v docker >/dev/null 2>&1; then \
      docker compose -f "{{root}}/dev/compose.yml" down; \
    else \
      echo "brew services stop postgresql@17"; \
    fi

# Apply dev/sql/*.sql in lexical order against DATUM_BOOTSTRAP_URL.
# Names: DATUM_TEST_TEMPLATE (default datum_test_template) and DATUM_TEST_DB
# (default: database in DATUM_DATABASE_URL, else datum_test). Roles unchanged.
db-reset:
    url="${DATUM_BOOTSTRAP_URL:?DATUM_BOOTSTRAP_URL is required}"; \
    . "{{root}}/scripts/datum-db-env.sh"; \
    if command -v brew >/dev/null 2>&1 && [ -x "$(brew --prefix postgresql@17)/bin/psql" ]; then \
      psql="$(brew --prefix postgresql@17)/bin/psql"; \
    else \
      psql="$(command -v psql)"; \
    fi; \
    shopt -s nullglob; \
    files=("{{root}}/dev/sql/"*.sql); \
    if [ ${#files[@]} -eq 0 ]; then \
      echo "db-reset: no files under {{root}}/dev/sql (harness lane owns them)"; \
      exit 1; \
    fi; \
    for f in "${files[@]}"; do \
      case "$(basename "$f")" in \
        *-gc.sql) continue ;; \
      esac; \
      "$psql" "$url" -v ON_ERROR_STOP=1 -v template="${DATUM_TEST_TEMPLATE}" -v dbname="${DATUM_TEST_DB}" -f "$f"; \
    done

# Drop stale ephemeral test databases (datum_t_*) older than DATUM_DB_GC_MIN minutes (default 60).
# Excludes the standing pair from DATUM_TEST_TEMPLATE / DATUM_TEST_DB.
db-gc:
    url="${DATUM_BOOTSTRAP_URL:?DATUM_BOOTSTRAP_URL is required}"; \
    gc_min="${DATUM_DB_GC_MIN:-60}"; \
    . "{{root}}/scripts/datum-db-env.sh"; \
    if command -v brew >/dev/null 2>&1 && [ -x "$(brew --prefix postgresql@17)/bin/psql" ]; then \
      psql="$(brew --prefix postgresql@17)/bin/psql"; \
    else \
      psql="$(command -v psql)"; \
    fi; \
    "$psql" "$url" -v ON_ERROR_STOP=1 -v gc_minutes="${gc_min}" -v template="${DATUM_TEST_TEMPLATE}" -v dbname="${DATUM_TEST_DB}" -f "{{root}}/dev/sql/90-gc.sql"

# Run sqlx migrate for one crate. Usage: just migrate datum-db
migrate crate:
    DATABASE_URL="${DATUM_MIGRATE_DATABASE_URL:?DATUM_MIGRATE_DATABASE_URL is required}" \
      sqlx migrate run --source "{{root}}/crates/{{crate}}/migrations"

# Prepare sqlx offline cache for one crate. Usage: just sqlx-prepare datum-db
sqlx-prepare crate:
    DATABASE_URL="${DATUM_MIGRATE_DATABASE_URL:?DATUM_MIGRATE_DATABASE_URL is required}" \
      cargo sqlx prepare --manifest-path "{{root}}/crates/{{crate}}/Cargo.toml" -- --all-targets --all-features

# Offline CI: format, clippy, SQL fence, lib tests.
ci: fmt-check clippy lint-sql test-lib

# CI plus database tests (expected RED until harness lands).
ci-db: ci test-db
