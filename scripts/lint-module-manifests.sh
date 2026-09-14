#!/usr/bin/env bash
# T-27: each first-party module's crate name, identifier, schema, canonical
# order, and both profiles agree.
set -eu

ROOT="${REPO_ROOT:-$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)}"
ORDER="$ROOT/crates/wicket-module/src/order.rs"
REG="$ROOT/profiles/regulated-device.toml"
PLAIN="$ROOT/profiles/plain-shop.toml"

fail=0
count=0

if [ ! -f "$ORDER" ] || [ ! -f "$REG" ] || [ ! -f "$PLAIN" ]; then
  echo "lint-module-manifests: missing order.rs or profiles" >&2
  exit 1
fi

for dir in "$ROOT"/modules/*/; do
  [ -f "$dir/Cargo.toml" ] || continue
  [ -f "$dir/module.toml" ] || {
    echo "lint-module-manifests: missing module.toml in $dir" >&2
    fail=1
    continue
  }
  count=$((count + 1))
  base="$(basename "$dir")"
  crate="$(awk -F'"' '/^name = / { print $2; exit }' "$dir/Cargo.toml")"
  mid="$(awk -F'"' '/^id = / { print $2; exit }' "$dir/module.toml")"
  schema="$(
    awk '
      /CREATE SCHEMA IF NOT EXISTS / && $0 !~ /_transient/ {
        for (i = 1; i <= NF; i++) {
          if ($i == "EXISTS") {
            print $(i + 1)
            exit
          }
        }
      }
    ' "$dir"/migrations/*.up.sql 2>/dev/null | head -1
  )"

  if [ -z "$crate" ]; then
    echo "lint-module-manifests: $base: no package name in Cargo.toml" >&2
    fail=1
    continue
  fi
  if [ -z "$mid" ]; then
    echo "lint-module-manifests: $base: no [module] id in module.toml" >&2
    fail=1
    continue
  fi
  if [ -z "$schema" ]; then
    echo "lint-module-manifests: $base: no CREATE SCHEMA in migrations" >&2
    fail=1
  fi

  # mod-production-min -> wicket-mod-production-min; directory production_min
  expected_crate="wicket-${mid}"
  expected_dir="$(printf '%s' "${mid#mod-}" | tr '-' '_')"
  expected_schema="$(printf '%s' "${mid#mod-}" | tr '-' '_')"

  if [ "$crate" != "$expected_crate" ]; then
    echo "lint-module-manifests: $base: crate '$crate' != '$expected_crate' (from id $mid)" >&2
    fail=1
  fi
  if [ "$base" != "$expected_dir" ]; then
    echo "lint-module-manifests: $base: directory != '$expected_dir' (from id $mid)" >&2
    fail=1
  fi
  if [ -n "$schema" ] && [ "$schema" != "$expected_schema" ]; then
    echo "lint-module-manifests: $base: schema '$schema' != '$expected_schema' (from id $mid)" >&2
    fail=1
  fi
  if ! grep -q "\"$crate\"" "$ORDER"; then
    echo "lint-module-manifests: $base: '$crate' missing from CANONICAL_ORDER" >&2
    fail=1
  fi
  if ! grep -q "id = \"$mid\"" "$REG"; then
    echo "lint-module-manifests: $base: '$mid' missing from regulated-device profile" >&2
    fail=1
  fi
  if ! grep -q "id = \"$mid\"" "$PLAIN"; then
    echo "lint-module-manifests: $base: '$mid' missing from plain-shop profile" >&2
    fail=1
  fi
done

if [ "$count" -eq 0 ]; then
  echo "lint-module-manifests: found 0 modules" >&2
  exit 1
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi

echo "lint-module-manifests: $count first-party modules agree on crate/id/schema/order/profiles"
