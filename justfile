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
    if rg -n --glob '*.rs' --glob '!**/datum-db/**' --glob '!**/datum-audit/**' --glob '!**/datum-test/**' \
        -e 'QueryBuilder' -e 'raw_sql' -e 'copy_in_raw' -e 'set_config' -e 'current_setting' \
        "{{root}}/crates"; then \
      echo "lint-sql: session-protocol SQL token outside crates/datum-db, crates/datum-audit, and crates/datum-test" >&2; \
      exit 1; \
    fi
    if rg -n --glob '*.rs' --glob '!**/datum-db/**' --glob '!**/datum-audit/**' --glob '!**/datum-test/**' \
        -e 'allow\(clippy::disallowed_' \
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
      "$psql" "$url" -v ON_ERROR_STOP=1 -f "$f"; \
    done

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
