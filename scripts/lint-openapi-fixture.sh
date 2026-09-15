#!/usr/bin/env bash
# T-44: the capability table's (method, path) set must match a committed fixture.
# Offline; no Postgres. POSIX bash 3.2; portable awk.
# Regeneration is `just openapi-fixture` (this script --write). Never a CI dependency.
set -eu

ROOT="${REPO_ROOT:-$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)}"
CAPS="$ROOT/crates/wicket-server/src/capabilities.rs"
FIXTURE="$ROOT/crates/wicket-server/tests/fixtures/openapi-operations.txt"

WRITE=0
if [ "${1:-}" = "--write" ]; then
  WRITE=1
elif [ "${1:-}" != "" ]; then
  echo "lint-openapi-fixture: unknown argument: $1 (expected --write or none)" >&2
  exit 1
fi

if [ ! -f "$CAPS" ]; then
  echo "lint-openapi-fixture: missing capability table: $CAPS" >&2
  exit 1
fi

# Each kernel()/module() call's quoted strings are id, method, path, ...
extract_ops() {
  awk '
    /kernel\(|module\(/ {
      in_call = 1
      nstr = 0
    }
    in_call {
      s = $0
      while (match(s, /"[^"]*"/)) {
        nstr++
        strs[nstr] = substr(s, RSTART + 1, RLENGTH - 2)
        s = substr(s, RSTART + RLENGTH)
      }
    }
    in_call && /\)/ {
      if (nstr >= 3 && strs[2] != "" && strs[3] != "") {
        print strs[2] " " strs[3]
        found++
      }
      in_call = 0
      nstr = 0
    }
    END {
      if (found + 0 == 0) {
        print "lint-openapi-fixture: extracted 0 operations from capabilities.rs" > "/dev/stderr"
        exit 1
      }
    }
  ' "$CAPS"
}

load_fixture() {
  awk '
    /^[[:space:]]*#/ { next }
    /^[[:space:]]*$/ { next }
    { print }
  ' "$1"
}

tmp="$(mktemp -d "${TMPDIR:-/tmp}/lint-openapi-fixture.XXXXXX")"
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT
trap 'cleanup; exit 130' INT TERM

extract_ops | LC_ALL=C sort > "$tmp/table"
table_n="$(grep -c . "$tmp/table" || true)"
table_u="$(LC_ALL=C sort -u "$tmp/table" | grep -c . || true)"
if [ "$table_n" -ne "$table_u" ]; then
  echo "lint-openapi-fixture: duplicate (method, path) in capability table" >&2
  LC_ALL=C sort "$tmp/table" | uniq -d | sed 's/^/  /' >&2
  exit 1
fi

if [ "$WRITE" -eq 1 ]; then
  mkdir -p "$(dirname "$FIXTURE")"
  {
    echo "# T-44 OpenAPI operation fixture. One METHOD path per line. Regenerate: just openapi-fixture"
    cat "$tmp/table"
  } > "$FIXTURE"
  echo "lint-openapi-fixture: wrote $table_n operations to $FIXTURE"
  exit 0
fi

if [ ! -f "$FIXTURE" ]; then
  echo "lint-openapi-fixture: missing fixture: $FIXTURE" >&2
  echo "lint-openapi-fixture: regenerate with: just openapi-fixture" >&2
  exit 1
fi

load_fixture "$FIXTURE" | LC_ALL=C sort > "$tmp/fixture"
fixture_n="$(grep -c . "$tmp/fixture" || true)"
fixture_u="$(LC_ALL=C sort -u "$tmp/fixture" | grep -c . || true)"
if [ "$fixture_n" -ne "$fixture_u" ]; then
  echo "lint-openapi-fixture: duplicate (method, path) in fixture" >&2
  LC_ALL=C sort "$tmp/fixture" | uniq -d | sed 's/^/  /' >&2
  exit 1
fi

# extra: in fixture, not in table. missing: in table, not in fixture.
extra="$(comm -23 "$tmp/fixture" "$tmp/table" || true)"
missing="$(comm -13 "$tmp/fixture" "$tmp/table" || true)"

fail=0
if [ -n "$extra" ]; then
  echo "lint-openapi-fixture: extra (in fixture, not in table):" >&2
  printf '%s\n' "$extra" | sed 's/^/  /' >&2
  fail=1
fi
if [ -n "$missing" ]; then
  echo "lint-openapi-fixture: missing (in table, not in fixture):" >&2
  printf '%s\n' "$missing" | sed 's/^/  /' >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "lint-openapi-fixture: $fixture_n fixture operations, $table_n table operations" >&2
  echo "lint-openapi-fixture: regenerate with: just openapi-fixture" >&2
  exit 1
fi

echo "lint-openapi-fixture: $table_n operations match the capability table"
