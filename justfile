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
    for pair in identity:datum-identity uom:datum-uom ledger:datum-ledger sm:datum-statemachine jobs:datum-jobs events:datum-events numbering:datum-numbering audit:datum-audit items:datum-mod-items locations:datum-mod-locations lots:datum-mod-lots; do \
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

# All tests, including integration.
test:
    cargo test --manifest-path "{{root}}/Cargo.toml" --workspace --all-features

# Library tests only; must pass with no DATUM_*_URL.
test-lib:
    cargo test --manifest-path "{{root}}/Cargo.toml" --workspace --lib --all-features

# Database tests; missing Postgres is a failure.
test-db:
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
db-reset:
    url="${DATUM_BOOTSTRAP_URL:?DATUM_BOOTSTRAP_URL is required}"; \
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
      "$psql" "$url" -v ON_ERROR_STOP=1 -f "$f"; \
    done

# Drop stale ephemeral test databases (datum_t_*) older than DATUM_DB_GC_MIN minutes (default 60).
db-gc:
    url="${DATUM_BOOTSTRAP_URL:?DATUM_BOOTSTRAP_URL is required}"; \
    gc_min="${DATUM_DB_GC_MIN:-60}"; \
    if command -v brew >/dev/null 2>&1 && [ -x "$(brew --prefix postgresql@17)/bin/psql" ]; then \
      psql="$(brew --prefix postgresql@17)/bin/psql"; \
    else \
      psql="$(command -v psql)"; \
    fi; \
    "$psql" "$url" -v ON_ERROR_STOP=1 -v gc_minutes="${gc_min}" -f "{{root}}/dev/sql/90-gc.sql"

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
