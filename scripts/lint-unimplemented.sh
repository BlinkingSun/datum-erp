#!/usr/bin/env bash
# Fail on todo!()/unimplemented!() and todo(/unimplemented( in tracked Rust
# outside compile-fail fixtures (TODO.md T-14 / CONTRIBUTING.md §6).
# Allowlist: **/compile-fail/** and **/*compile_fail*. Tests are not exempt.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
cd "$root"

# Identifier boundary so write_unimplemented() / a test named todo do not match.
pattern='(^|[^[:alnum:]_])(todo!|unimplemented!|todo|unimplemented)\('

set +e
hits="$(git grep -nE -e "$pattern" -- '*.rs')"
status=$?
set -e

if [ "$status" -gt 1 ]; then
  echo "lint-unimplemented: git grep failed (status ${status})" >&2
  exit 1
fi

fail=0
if [ -n "${hits}" ]; then
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    file="${line%%:*}"
    case "$file" in
      */compile-fail/*|*compile_fail*)
        continue
        ;;
    esac
    printf '%s\n' "$line"
    fail=1
  done <<EOF
$hits
EOF
fi

if [ "$fail" -ne 0 ]; then
  echo "lint-unimplemented: todo!()/unimplemented!() (or todo(/unimplemented() calls) are forbidden outside compile-fail fixtures" >&2
  exit 1
fi

echo "lint-unimplemented: no placeholder macros in tracked Rust"
exit 0
