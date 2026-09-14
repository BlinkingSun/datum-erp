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

# String-level raw-SQL fence (CONTRACT §5a / §5a.1). Fails on any hit outside wicket-db / wicket-audit / wicket-test.
# Session-protocol scans *.rs and src *.sql (FINDINGS-1 #9: stamp_esign.sql evaded *.rs).
# First scan covers crates/ and modules/ (T-58). Migrations have their own plant.
lint-sql:
    command -v rg >/dev/null 2>&1 || { echo 'lint-sql: ripgrep (rg) is required' >&2; exit 1; }
    if rg -n --glob '*.rs' --glob '*.sql' --glob '!**/migrations/**' \
        --glob '!**/wicket-db/**' --glob '!**/wicket-audit/**' --glob '!**/wicket-test/**' \
        -e 'QueryBuilder' -e 'raw_sql' -e 'copy_in_raw' -e 'set_config' -e 'current_setting' \
        "{{root}}/crates" "{{root}}/modules"; then \
      echo "lint-sql: session-protocol SQL token outside crates/wicket-db, crates/wicket-audit, and crates/wicket-test" >&2; \
      exit 1; \
    fi
    if rg -n -U --multiline-dotall --glob '*.rs' --glob '!**/wicket-db/**' --glob '!**/wicket-audit/**' --glob '!**/wicket-test/**' \
        -e 'allow\(.{0,400}?clippy::disallowed_' \
        "{{root}}/crates" "{{root}}/modules"; then \
      echo "lint-sql: clippy disallowed allow outside crates/wicket-db, crates/wicket-audit, and crates/wicket-test" >&2; \
      exit 1; \
    fi
    if rg -n --glob 'build.rs' --glob '!**/wicket-db/**' --glob '!**/wicket-audit/**' --glob '!**/wicket-test/**' \
        -e 'sqlx' \
        "{{root}}/crates" "{{root}}/modules"; then \
      echo "lint-sql: sqlx token in build.rs outside crates/wicket-db, crates/wicket-audit, and crates/wicket-test" >&2; \
      exit 1; \
    fi
    if rg -n --glob '*.rs' --glob '!**/wicket-db/**' --glob '!**/wicket-audit/**' --glob '!**/wicket-test/**' \
        -e 'GRANT ' -e 'CREATE DATABASE' \
        "{{root}}/crates" "{{root}}/modules"; then \
      echo "lint-sql: GRANT or CREATE DATABASE outside crates/wicket-db, crates/wicket-audit, and crates/wicket-test" >&2; \
      exit 1; \
    fi
    # PLAN section 6 invariant 6: no crate's src reads another crate's schema-qualified tables.
    # Allow-list: owning crate, wicket-module (composition root), wicket-test (harness).
    # Production src only - kernel tests may probe audit.event / seed uom.item_stock.
    # Scan per-crate src/ *.rs and *.sql (include_str!/query_file! includes).
    # No path-separator globs; relative paths from repo root (lintmig-portable).
    # Dynamic construction (format/quote_ident/'schema.' concat) is lint-sql-dynamic.sh.
    REPO_ROOT="{{root}}" bash "{{root}}/scripts/lint-sql-cross.sh"
    # R-2s-3: production src SQL must not DML-reference ledger.* / transient.*
    # outside the explicit owner exemption set (see scripts/lint-sql-r2s3.sh).
    REPO_ROOT="{{root}}" bash "{{root}}/scripts/lint-sql-r2s3.sh"
    REPO_ROOT="{{root}}" bash "{{root}}/scripts/lint-sql-migrations.sh"
    # Dynamic schema construction: format()/quote_ident/'schema.' concat
    # (the documents live_machine_state evasion). Same exemption lists.
    REPO_ROOT="{{root}}" bash "{{root}}/scripts/lint-sql-dynamic.sh"

# Plant bad migrations in a throwaway tree (never the real repo) and assert the lints fail.
lint-sql-selftest:
    command -v rg >/dev/null 2>&1 || { echo 'lint-sql-selftest: ripgrep (rg) is required' >&2; exit 1; }
    root="{{root}}"; \
    tmp="$(mktemp -d "${TMPDIR:-/tmp}/lint-sql-selftest.XXXXXX")"; \
    cleanup() { rm -rf "$tmp"; }; \
    trap cleanup EXIT; \
    trap 'cleanup; exit 130' INT TERM; \
    mkdir -p \
      "$tmp/crates/wicket-server/migrations" \
      "$tmp/crates/wicket-server/src" \
      "$tmp/crates/wicket-db/src" \
      "$tmp/crates/wicket-esign/src" \
      "$tmp/crates/wicket-uom/migrations" \
      "$tmp/crates/wicket-documents/migrations" \
      "$tmp/crates/wicket-documents/src" \
      "$tmp/crates/wicket-module/src" \
      "$tmp/crates/wicket-ledger/src" \
      "$tmp/modules/items/migrations" \
      "$tmp/modules/items/src" \
      "$tmp/modules/lots/src" \
      "$tmp/modules/locations/src"; \
    for f in "$root/crates/wicket-uom/migrations/"*.up.sql; do \
      if [ -f "$f" ]; then cp "$f" "$tmp/crates/wicket-uom/migrations/"; fi; \
    done; \
    : > "$tmp/modules/locations/src/store.rs"; \
    plant_cross="$tmp/crates/wicket-server/migrations/99999999999999_lint_sql_selftest_cross.up.sql"; \
    plant_session="$tmp/modules/items/migrations/99999999999999_lint_sql_selftest_session.up.sql"; \
    plant_create="$tmp/crates/wicket-server/migrations/99999999999998_lint_sql_selftest_definer_create.up.sql"; \
    plant_drop="$tmp/crates/wicket-server/migrations/99999999999999_lint_sql_selftest_definer_drop.up.sql"; \
    plant_orphan="$tmp/crates/wicket-server/migrations/99999999999999_lint_sql_selftest_definer_orphan.up.sql"; \
    plant_r2s3="$tmp/modules/items/src/_lint_sql_r2s3_selftest.rs"; \
    plant_r2s3_ok="$tmp/modules/lots/src/_lint_sql_r2s3_ok.rs"; \
    plant_r2s3_sql="$tmp/crates/wicket-server/src/_lint_sql_r2s3_include.sql"; \
    plant_r2s3_sql_ok="$tmp/modules/lots/src/_lint_sql_r2s3_ok.sql"; \
    plant_include_cross="$tmp/crates/wicket-server/src/_lint_sql_include_cross.sql"; \
    plant_include_own="$tmp/crates/wicket-esign/src/_lint_sql_include_own.sql"; \
    plant_include_exempt="$tmp/crates/wicket-module/src/_lint_sql_include_exempt.sql"; \
    plant_session_sql="$tmp/crates/wicket-server/src/_lint_sql_session_include.sql"; \
    plant_session_sql_ok="$tmp/crates/wicket-db/src/_lint_sql_session_ok.sql"; \
    if ! REPO_ROOT="$tmp" bash "$root/scripts/lint-sql-migrations.sh" --selftest-hits; then \
      echo 'lint-sql-selftest: Windows-shaped hit parser/neutralization failed' >&2; \
      exit 1; \
    fi; \
    if ! REPO_ROOT="$tmp" bash "$root/scripts/lint-sql-r2s3.sh" --selftest-hits; then \
      echo 'lint-sql-selftest: Windows-shaped R-2s-3 hit parser failed' >&2; \
      exit 1; \
    fi; \
    if ! REPO_ROOT="$tmp" bash "$root/scripts/lint-sql-dynamic.sh" --selftest-hits; then \
      echo 'lint-sql-selftest: dynamic SQL fixture parser failed' >&2; \
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
      "SELECT set_config('wicket.actor', 'lint-selftest', true);" > "$plant_session"; \
    if run_lint; then \
      echo 'lint-sql-selftest: expected migration lint to fail on planted set_config(wicket.*)' >&2; \
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
    printf '%s\n' 'SELECT 1 FROM ledger.posting WHERE false;' > "$plant_r2s3_sql"; \
    printf '%s\n' '-- comment FROM ledger.posting must not trip the rule' > "$plant_r2s3_sql_ok"; \
    r2s3_out="$(REPO_ROOT="$tmp" bash "$root/scripts/lint-sql-r2s3.sh" 2>&1 || true)"; \
    if ! printf '%s\n' "$r2s3_out" | grep -F 'modules/items/src/_lint_sql_r2s3_selftest.rs' >/dev/null; then \
      echo 'lint-sql-selftest: expected R-2s-3 lint to report planted ledger.posting SQL' >&2; \
      printf '%s\n' "$r2s3_out" >&2; \
      exit 1; \
    fi; \
    if ! printf '%s\n' "$r2s3_out" | grep -F 'crates/wicket-server/src/_lint_sql_r2s3_include.sql' >/dev/null; then \
      echo 'lint-sql-selftest: expected R-2s-3 lint to report planted *.sql include reading ledger.*' >&2; \
      printf '%s\n' "$r2s3_out" >&2; \
      exit 1; \
    fi; \
    if printf '%s\n' "$r2s3_out" | grep -E 'modules/lots/src/_lint_sql_r2s3_ok\.rs|modules/lots/src/_lint_sql_r2s3_ok\.sql' >/dev/null; then \
      echo 'lint-sql-selftest: R-2s-3 lint false-positive on comment / type-cast / *_transient / published function' >&2; \
      printf '%s\n' "$r2s3_out" >&2; \
      exit 1; \
    fi; \
    echo 'lint-sql-selftest: planted R-2s-3 ledger.* SQL correctly rejected'; \
    echo 'lint-sql-selftest: planted R-2s-3 *.sql include reading ledger.* correctly rejected'; \
    echo 'lint-sql-selftest: R-2s-3 negatives (comment, ::ledger.boundary, inventory_transient, has_postings, SQL -- comment) correctly allowed'; \
    printf '%s\n' 'SELECT consumed_at FROM esign.signature WHERE false;' > "$plant_include_cross"; \
    printf '%s\n' 'SELECT consumed_at FROM esign.signature WHERE false;' > "$plant_include_own"; \
    printf '%s\n' 'SELECT consumed_at FROM esign.signature WHERE false;' > "$plant_include_exempt"; \
    cross_out="$(REPO_ROOT="$tmp" bash "$root/scripts/lint-sql-cross.sh" 2>&1 || true)"; \
    if ! printf '%s\n' "$cross_out" | grep -F 'crates/wicket-server/src/_lint_sql_include_cross.sql' >/dev/null; then \
      echo 'lint-sql-selftest: expected invariant-6 lint to report planted *.sql include reading esign.*' >&2; \
      printf '%s\n' "$cross_out" >&2; \
      exit 1; \
    fi; \
    if printf '%s\n' "$cross_out" | grep -E 'crates/wicket-esign/src/_lint_sql_include_own\.sql|crates/wicket-module/src/_lint_sql_include_exempt\.sql' >/dev/null; then \
      echo 'lint-sql-selftest: invariant-6 lint false-positive on owning crate or wicket-module *.sql include' >&2; \
      printf '%s\n' "$cross_out" >&2; \
      exit 1; \
    fi; \
    echo 'lint-sql-selftest: planted *.sql include reading esign.* correctly rejected'; \
    echo 'lint-sql-selftest: invariant-6 negatives (owning crate *.sql, wicket-module *.sql) correctly allowed'; \
    if [ -f "$root/crates/wicket-module/src/stamp_esign.sql" ]; then \
      echo 'lint-sql-selftest: stamp_esign.sql must be deleted (FINDINGS-1 #9)' >&2; \
      exit 1; \
    fi; \
    echo 'lint-sql-selftest: stamp_esign.sql is gone'; \
    plant_session_sql_mod="$tmp/modules/items/src/_lint_sql_session_mod.sql"; \
    printf '%s\n' "SELECT pg_catalog.set_config('wicket.esign_id', 'lint-selftest', true);" > "$plant_session_sql"; \
    printf '%s\n' "SELECT pg_catalog.set_config('wicket.esign_id', 'lint-selftest', true);" > "$plant_session_sql_ok"; \
    printf '%s\n' "SELECT pg_catalog.set_config('wicket.esign_id', 'lint-selftest', true);" > "$plant_session_sql_mod"; \
    session_out="$(rg -n --glob '*.rs' --glob '*.sql' --glob '!**/migrations/**' \
      --glob '!**/wicket-db/**' --glob '!**/wicket-audit/**' --glob '!**/wicket-test/**' \
      -e 'QueryBuilder' -e 'raw_sql' -e 'copy_in_raw' -e 'set_config' -e 'current_setting' \
      "$tmp/crates" "$tmp/modules" 2>&1 || true)"; \
    if ! printf '%s\n' "$session_out" | grep -F 'crates/wicket-server/src/_lint_sql_session_include.sql' >/dev/null; then \
      echo 'lint-sql-selftest: expected session-protocol lint to report planted src *.sql set_config' >&2; \
      printf '%s\n' "$session_out" >&2; \
      exit 1; \
    fi; \
    if ! printf '%s\n' "$session_out" | grep -F 'modules/items/src/_lint_sql_session_mod.sql' >/dev/null; then \
      echo 'lint-sql-selftest: expected session-protocol lint to report planted modules/ src *.sql set_config' >&2; \
      printf '%s\n' "$session_out" >&2; \
      exit 1; \
    fi; \
    if printf '%s\n' "$session_out" | grep -F 'crates/wicket-db/src/_lint_sql_session_ok.sql' >/dev/null; then \
      echo 'lint-sql-selftest: session-protocol lint false-positive on wicket-db src *.sql' >&2; \
      printf '%s\n' "$session_out" >&2; \
      exit 1; \
    fi; \
    echo 'lint-sql-selftest: planted src *.sql set_config correctly rejected'; \
    echo 'lint-sql-selftest: planted modules/ src *.sql set_config correctly rejected'; \
    echo 'lint-sql-selftest: wicket-db src *.sql set_config correctly allowed'; \
    rm -f "$plant_session_sql" "$plant_session_sql_ok" "$plant_session_sql_mod"; \
    plant_dyn_doc="$tmp/crates/wicket-documents/migrations/99999999999999_lint_sql_dyn_documents.up.sql"; \
    plant_dyn_qid="$tmp/modules/items/src/_lint_sql_dyn_quote_ident.rs"; \
    plant_dyn_concat="$tmp/modules/items/src/_lint_sql_dyn_concat.rs"; \
    plant_dyn_ok="$tmp/modules/lots/src/_lint_sql_dyn_ok.rs"; \
    plant_dyn_exempt="$tmp/crates/wicket-module/src/_lint_sql_dyn_exempt.rs"; \
    plant_dyn_own="$tmp/crates/wicket-documents/src/_lint_sql_dyn_own.rs"; \
    plant_dyn_r2s3_ex="$tmp/crates/wicket-ledger/src/_lint_sql_dyn_r2s3_exempt.rs"; \
    plant_dyn_sql_include="$tmp/crates/wicket-server/src/_lint_sql_dyn_include.sql"; \
    plant_dyn_sql_ok="$tmp/modules/lots/src/_lint_sql_dyn_ok.sql"; \
    printf '%s\n' \
      '-- lint-sql-selftest: documents live_machine_state evasion' \
      'CREATE FUNCTION documents.live_machine_state(p_doc_id uuid) RETURNS text' \
      'LANGUAGE plpgsql VOLATILE SET search_path = pg_catalog, pg_temp AS $fn$' \
      'DECLARE' \
      '  live_state text;' \
      'BEGIN' \
      '  EXECUTE format(' \
      "    'SELECT state FROM %I.%I WHERE doc_type = \$1 AND doc_id = \$2'," \
      "    'sm'," \
      "    'instance'" \
      '  )' \
      '  INTO live_state' \
      "  USING 'document', p_doc_id;" \
      '  RETURN live_state;' \
      'END' \
      '$fn$;' \
      > "$plant_dyn_doc"; \
    printf '%s\n' \
      'fn _lint_sql_dyn_quote_ident() { let _ = "SELECT state FROM " + quote_ident("sm"); }' \
      > "$plant_dyn_qid"; \
    printf '%s\n' \
      'fn _lint_sql_dyn_concat() { let _ = "SELECT 1 FROM " + "ledger." + "posting"; }' \
      > "$plant_dyn_concat"; \
    printf '%s\n' \
      '/// comment format('\''sm'\'') and quote_ident('\''ledger'\'') must not trip the rule' \
      'fn _lint_sql_dyn_ok() {' \
      '    let _ = format!("status {sm}");' \
      '    let _ = ts.format("%Y-%m-%d");' \
      '}' \
      > "$plant_dyn_ok"; \
    printf '%s\n' \
      'fn _lint_sql_dyn_exempt() { let _ = "EXECUTE format('\''%I.%I'\'', '\''sm'\'', '\''instance'\'')"; }' \
      > "$plant_dyn_exempt"; \
    printf '%s\n' \
      'fn _lint_sql_dyn_own() { let _ = "EXECUTE format('\''%I.%I'\'', '\''documents'\'', '\''document'\'')"; }' \
      > "$plant_dyn_own"; \
    printf '%s\n' \
      'fn _lint_sql_dyn_r2s3_exempt() { let _ = "EXECUTE format('\''%I.%I'\'', '\''ledger'\'', '\''posting'\'')"; }' \
      > "$plant_dyn_r2s3_ex"; \
    printf '%s\n' \
      '-- lint-sql-selftest: *.sql include dynamic construction' \
      "EXECUTE format('%I.%I', 'sm', 'instance');" \
      > "$plant_dyn_sql_include"; \
    printf '%s\n' "-- comment format('%I.%I', 'sm', 'instance') must not trip the rule" \
      > "$plant_dyn_sql_ok"; \
    dyn_out="$(REPO_ROOT="$tmp" bash "$root/scripts/lint-sql-dynamic.sh" 2>&1 || true)"; \
    if ! printf '%s\n' "$dyn_out" | grep -F 'crates/wicket-documents/migrations/99999999999999_lint_sql_dyn_documents.up.sql' >/dev/null; then \
      echo 'lint-sql-selftest: expected dynamic lint to report planted documents format('\''sm'\'') evasion' >&2; \
      printf '%s\n' "$dyn_out" >&2; \
      exit 1; \
    fi; \
    if ! printf '%s\n' "$dyn_out" | grep -F 'modules/items/src/_lint_sql_dyn_quote_ident.rs' >/dev/null; then \
      echo 'lint-sql-selftest: expected dynamic lint to report planted quote_ident("sm")' >&2; \
      printf '%s\n' "$dyn_out" >&2; \
      exit 1; \
    fi; \
    if ! printf '%s\n' "$dyn_out" | grep -F 'modules/items/src/_lint_sql_dyn_concat.rs' >/dev/null; then \
      echo 'lint-sql-selftest: expected dynamic lint to report planted concatenation '\''ledger.'\''' >&2; \
      printf '%s\n' "$dyn_out" >&2; \
      exit 1; \
    fi; \
    if ! printf '%s\n' "$dyn_out" | grep -F 'crates/wicket-server/src/_lint_sql_dyn_include.sql' >/dev/null; then \
      echo 'lint-sql-selftest: expected dynamic lint to report planted *.sql include format('\''sm'\'')' >&2; \
      printf '%s\n' "$dyn_out" >&2; \
      exit 1; \
    fi; \
    if printf '%s\n' "$dyn_out" | grep -E 'modules/lots/src/_lint_sql_dyn_ok\.rs|modules/lots/src/_lint_sql_dyn_ok\.sql|crates/wicket-module/src/_lint_sql_dyn_exempt.rs|crates/wicket-documents/src/_lint_sql_dyn_own.rs|crates/wicket-ledger/src/_lint_sql_dyn_r2s3_exempt.rs' >/dev/null; then \
      echo 'lint-sql-selftest: dynamic lint false-positive on comment / format! / .format / exempt crate / own schema / SQL -- comment' >&2; \
      printf '%s\n' "$dyn_out" >&2; \
      exit 1; \
    fi; \
    echo 'lint-sql-selftest: planted documents EXECUTE format('\''%I.%I'\'','\''sm'\'',...) evasion correctly rejected'; \
    echo 'lint-sql-selftest: planted quote_ident and '\''schema.'\'' concatenation correctly rejected'; \
    echo 'lint-sql-selftest: planted *.sql include format('\''sm'\'') correctly rejected'; \
    echo 'lint-sql-selftest: dynamic SQL negatives (comment, format!, .format, wicket-module, own schema, R-2s-3 exempt, SQL -- comment) correctly allowed'

# All tests, including integration.
test:
    cargo test --manifest-path "{{root}}/Cargo.toml" --workspace --all-features

# Library tests only (`--lib`). Excludes every integration test under crates/*/tests/,
# including Wave 2s slice acceptance (crates/wicket-server/tests/slice.rs).
# Must pass with no WICKET_*_URL. Not a full test run; that is `test` or `ci-db`.
test-lib:
    cargo test --manifest-path "{{root}}/Cargo.toml" --workspace --lib --all-features

# Database tests; missing Postgres is a failure.
# Resolves WICKET_TEST_TEMPLATE / WICKET_TEST_DB (defaults match public CI).
test-db:
    . "{{root}}/scripts/wicket-db-env.sh"; \
    WICKET_REQUIRE_PG=1 cargo test --manifest-path "{{root}}/Cargo.toml" --workspace --all-features

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

# Apply dev/sql/*.sql in lexical order against WICKET_BOOTSTRAP_URL.
# Names: WICKET_TEST_TEMPLATE (default wicket_test_template) and WICKET_TEST_DB
# (default: database in WICKET_DATABASE_URL, else wicket_test). Roles unchanged.
db-reset:
    url="${WICKET_BOOTSTRAP_URL:?WICKET_BOOTSTRAP_URL is required}"; \
    . "{{root}}/scripts/wicket-db-env.sh"; \
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
      "$psql" "$url" -v ON_ERROR_STOP=1 -v template="${WICKET_TEST_TEMPLATE}" -v dbname="${WICKET_TEST_DB}" -f "$f"; \
    done

# Drop stale ephemeral test databases (wicket_t_*) older than WICKET_DB_GC_MIN minutes (default 60).
# Excludes the standing pair from WICKET_TEST_TEMPLATE / WICKET_TEST_DB.
db-gc:
    url="${WICKET_BOOTSTRAP_URL:?WICKET_BOOTSTRAP_URL is required}"; \
    gc_min="${WICKET_DB_GC_MIN:-60}"; \
    . "{{root}}/scripts/wicket-db-env.sh"; \
    if command -v brew >/dev/null 2>&1 && [ -x "$(brew --prefix postgresql@17)/bin/psql" ]; then \
      psql="$(brew --prefix postgresql@17)/bin/psql"; \
    else \
      psql="$(command -v psql)"; \
    fi; \
    "$psql" "$url" -v ON_ERROR_STOP=1 -v gc_minutes="${gc_min}" -v template="${WICKET_TEST_TEMPLATE}" -v dbname="${WICKET_TEST_DB}" -f "{{root}}/dev/sql/90-gc.sql"

# Run sqlx migrate for one crate. Usage: just migrate wicket-db
migrate crate:
    DATABASE_URL="${WICKET_MIGRATE_DATABASE_URL:?WICKET_MIGRATE_DATABASE_URL is required}" \
      sqlx migrate run --source "{{root}}/crates/{{crate}}/migrations"

# Prepare sqlx offline cache for one crate. Usage: just sqlx-prepare wicket-db
sqlx-prepare crate:
    DATABASE_URL="${WICKET_MIGRATE_DATABASE_URL:?WICKET_MIGRATE_DATABASE_URL is required}" \
      cargo sqlx prepare --manifest-path "{{root}}/crates/{{crate}}/Cargo.toml" -- --all-targets --all-features

# Reverse-diff axum `.route(` mounts vs MOUNTED (ADR 0010 / T-23).
lint-mounts:
    bash "{{root}}/scripts/lint-mounts.sh"

# Offline CI: format, clippy, SQL fence, mount lint, lib tests.
ci: fmt-check clippy lint-sql lint-mounts test-lib

# CI plus database tests (`ci` then `test-db`). This recipe, not `ci`, runs the
# integration tests under crates/*/tests/, including Wave 2s slice acceptance,
# because `test-lib` passes `--lib`.
ci-db: ci test-db
