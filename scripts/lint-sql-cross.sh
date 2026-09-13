#!/usr/bin/env bash
# PLAN section 6 invariant 6: no crate's production src reads another crate's
# schema-qualified tables. Invoked with REPO_ROOT set.
# POSIX bash 3.2; requires rg. Run from the repo root over relative paths.
# No path-separator globs (negative **/owner/** fails on Windows paths).
#
# Globs: *.rs and *.sql. SQL includes (include_str! / sqlx::query_file!) used
# to evade the *.rs-only scan (wicket-server esign_manifest.sql / esign_consumed.sql).
# Production src only — kernel tests may probe audit.event / seed uom.item_stock.
set -eu

REPO_ROOT="${REPO_ROOT:?REPO_ROOT is required}"

command -v rg >/dev/null 2>&1 || {
  echo 'lint-sql: ripgrep (rg) is required' >&2
  exit 1
}

cd "$REPO_ROOT" || exit 1

# Same pair list as scripts/lint-sql-dynamic.sh SCHEMA_OWNERS (Wave 2b crates included).
SCHEMA_OWNERS='identity:wicket-identity uom:wicket-uom ledger:wicket-ledger sm:wicket-statemachine jobs:wicket-jobs events:wicket-events numbering:wicket-numbering audit:wicket-audit items:wicket-mod-items locations:wicket-mod-locations lots:wicket-mod-lots inventory:wicket-mod-inventory production_min:wicket-mod-production-min genealogy:wicket-mod-genealogy documents:wicket-documents print:wicket-print esign:wicket-esign customfields:wicket-customfields'

fail=0
for pair in $SCHEMA_OWNERS; do
  schema="${pair%%:*}"
  owner="${pair##*:}"
  for tree in crates modules; do
    if [ ! -d "$tree" ]; then
      continue
    fi
    for crate_dir in "$tree"/*; do
      if [ ! -d "$crate_dir" ]; then
        continue
      fi
      crate="$(basename "$crate_dir")"
      case "$crate" in
        "$owner"|wicket-module|wicket-test) continue ;;
      esac
      if [ "$crate" = "$schema" ]; then
        continue
      fi
      src_dir="$crate_dir/src"
      if [ ! -d "$src_dir" ]; then
        continue
      fi
      hits="$(rg -n -i --glob '*.rs' --glob '*.sql' \
        -e "(FROM|JOIN|INTO|UPDATE|TABLE)[[:space:]]+(ONLY[[:space:]]+)?${schema}\\." \
        "$src_dir" 2>/dev/null || true)"
      if [ -n "$hits" ]; then
        printf '%s\n' "$hits"
        echo "lint-sql: cross-module table read of ${schema}.* outside ${owner}, wicket-module, and wicket-test" >&2
        fail=1
      fi
    done
  done
done

if [ "$fail" -ne 0 ]; then
  exit 1
fi
