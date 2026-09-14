#!/usr/bin/env bash
# T-24: the capability table is the only route source.
# - http.rs must not contain a string-literal .route("...") mount
# - every capability id in capabilities.rs has a match arm in http.rs
# POSIX bash 3.2; portable awk.
set -eu

ROOT="${REPO_ROOT:-$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)}"
HTTP="$ROOT/crates/wicket-server/src/http.rs"
CAPS="$ROOT/crates/wicket-server/src/capabilities.rs"

if [ ! -f "$HTTP" ]; then
  echo "lint-mounts: missing router source: $HTTP" >&2
  exit 1
fi
if [ ! -f "$CAPS" ]; then
  echo "lint-mounts: missing capability table: $CAPS" >&2
  exit 1
fi

# Ignore comments and string contents that mention the ban.
if grep -n -E '^[^/]*\.route\("' "$HTTP" >/dev/null 2>&1; then
  echo "lint-mounts: hand-written string-literal route mount in http.rs:" >&2
  grep -n -E '^[^/]*\.route\("' "$HTTP" >&2
  exit 1
fi

extract_ids() {
  awk '
    /kernel\(|module\(/ { want = 1 }
    want && /"/ {
      n = split($0, parts, "\"")
      if (n >= 2 && parts[2] != "") {
        print parts[2]
        found++
      }
      want = 0
    }
    END {
      if (found + 0 == 0) {
        print "lint-mounts: extracted 0 capability ids" > "/dev/stderr"
        exit 1
      }
    }
  ' "$CAPS"
}

extract_arms() {
  awk '
    /"[A-Za-z][A-Za-z0-9_]*" =>/ {
      n = split($0, parts, "\"")
      if (n >= 2 && parts[2] != "") {
        print parts[2]
        found++
      }
    }
    END {
      if (found + 0 == 0) {
        print "lint-mounts: extracted 0 handler arms from http.rs" > "/dev/stderr"
        exit 1
      }
    }
  ' "$HTTP"
}

ids="$(extract_ids | sort -u)"
arms="$(extract_arms | sort -u)"

id_n="$(printf '%s\n' "$ids" | grep -c . || true)"
arm_n="$(printf '%s\n' "$arms" | grep -c . || true)"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/lint-mounts.XXXXXX")"
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT
trap 'cleanup; exit 130' INT TERM

printf '%s\n' "$ids" > "$tmp/ids"
printf '%s\n' "$arms" > "$tmp/arms"

extra_ids="$(comm -23 "$tmp/ids" "$tmp/arms" || true)"
extra_arms="$(comm -13 "$tmp/ids" "$tmp/arms" || true)"

fail=0
if [ -n "$extra_ids" ]; then
  echo "lint-mounts: capability ids with no handler arm:" >&2
  printf '%s\n' "$extra_ids" | sed 's/^/  /' >&2
  fail=1
fi
if [ -n "$extra_arms" ]; then
  echo "lint-mounts: handler arms with no capability id:" >&2
  printf '%s\n' "$extra_arms" | sed 's/^/  /' >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "lint-mounts: $id_n capability ids, $arm_n handler arms" >&2
  exit 1
fi

echo "lint-mounts: $id_n capability ids bound; no string-literal mounts"
