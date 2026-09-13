#!/usr/bin/env bash
# Migration SQL checks for just lint-sql (POSIX bash 3.2; requires rg).
# Invoked with REPO_ROOT set to the workspace root.
set -eu

REPO_ROOT="${REPO_ROOT:?REPO_ROOT is required}"

command -v rg >/dev/null 2>&1 || {
  echo 'lint-sql: ripgrep (rg) is required' >&2
  exit 1
}

cd "$REPO_ROOT" || exit 1

# schema:owning-crate-basename (modules use directory name; owner is Cargo package name).
SCHEMA_OWNERS='identity:datum-identity uom:datum-uom ledger:datum-ledger sm:datum-statemachine jobs:datum-jobs events:datum-events numbering:datum-numbering audit:datum-audit items:datum-mod-items locations:datum-mod-locations lots:datum-mod-lots inventory:datum-mod-inventory genealogy:datum-mod-genealogy production_min:datum-mod-production-min server:datum-server datum:datum-db'

# Parse rg -n "file:line:text", including an optional Windows drive letter (C:).
# Sets HIT_FILE, HIT_LINE, HIT_TEXT. Returns 0 on success.
# Portable awk: no match(array), gensub, or interval expressions.
parse_rg_hit() {
  hit="$1"
  HIT_FILE=""
  HIT_LINE=""
  HIT_TEXT=""
  parsed="$(printf '%s\n' "$hit" | awk '
    {
      s = $0
      drive = ""
      if (s ~ /^[A-Za-z]:/) {
        drive = substr(s, 1, 2)
        s = substr(s, 3)
      }
      c1 = index(s, ":")
      if (c1 < 1) exit 1
      file = drive substr(s, 1, c1 - 1)
      rest = substr(s, c1 + 1)
      c2 = index(rest, ":")
      if (c2 < 1) exit 1
      line = substr(rest, 1, c2 - 1)
      if (line !~ /^[0-9]+$/) exit 1
      text = substr(rest, c2 + 1)
      printf "%s\036%s\036%s\n", file, line, text
    }
  ')" || return 1
  [ -n "$parsed" ] || return 1
  IFS="$(printf '\036')" read -r HIT_FILE HIT_LINE HIT_TEXT <<EOF
$parsed
EOF
  [ -n "$HIT_FILE" ] && [ -n "$HIT_LINE" ]
}

# After cd "$REPO_ROOT", open files via relative paths. Convert backslashes and
# strip a drive-letter / REPO_ROOT prefix so a Windows-shaped hit still maps.
localize_hit_file() {
  f="$1"
  f="$(printf '%s' "$f" | tr '\\' '/')"
  if [ -f "$f" ]; then
    printf '%s' "$f"
    return 0
  fi
  root_n="$(printf '%s' "${REPO_ROOT:-}" | tr '\\' '/')"
  root_n="${root_n%/}"
  if [ -n "$root_n" ]; then
    case "$f" in
      "$root_n"/*)
        f="${f#"$root_n"/}"
        if [ -f "$f" ]; then
          printf '%s' "$f"
          return 0
        fi
        ;;
    esac
  fi
  case "$f" in
    [A-Za-z]:*)
      f="${f#?:}"
      ;;
  esac
  case "$f" in
    */crates/*)
      f="crates/${f#*/crates/}"
      ;;
    */modules/*)
      f="modules/${f#*/modules/}"
      ;;
  esac
  printf '%s' "$f"
}

migration_unit_owns_schema() {
  unit="$1"
  schema="$2"
  if [ "$unit" = "$schema" ]; then
    return 0
  fi
  for pair in $SCHEMA_OWNERS; do
    sch="${pair%%:*}"
    own="${pair##*:}"
    if [ "$sch" != "$schema" ]; then
      continue
    fi
    case "$unit" in
      "$own") return 0 ;;
    esac
    case "$own" in
      datum-mod-*)
        mod="${own#datum-mod-}"
        mod="${mod//-/_}"
        if [ "$unit" = "$mod" ]; then
          return 0
        fi
        ;;
    esac
  done
  case "$unit" in
    datum-ledger)
      [ "$schema" = ledger ] || [ "$schema" = transient ] && return 0
      ;;
    datum-jobs | datum-events)
      [ "$schema" = app ] || [ "$schema" = transient ] && return 0
      ;;
    datum-identity)
      [ "$schema" = identity ] || [ "$schema" = transient ] && return 0
      ;;
    datum-statemachine)
      [ "$schema" = sm ] && return 0
      ;;
    datum-module)
      [ "$schema" = module ] && return 0
      ;;
    datum-db)
      [ "$schema" = datum ] && return 0
      ;;
    datum-audit)
      [ "$schema" = audit ] && return 0
      ;;
    datum-server)
      [ "$schema" = server ] && return 0
      ;;
    inventory)
      [ "$schema" = inventory ] || [ "$schema" = inventory_transient ] && return 0
      ;;
    genealogy)
      [ "$schema" = genealogy ] || [ "$schema" = genealogy_transient ] && return 0
      ;;
    production_min)
      [ "$schema" = production_min ] && return 0
      ;;
  esac
  return 1
}

# Drop registry/catalog lines every migrator may touch.
filter_migration_hits() {
  schema="$1"
  while IFS= read -r line; do
    case "$line" in
      *audit.attach*)
        continue
        ;;
    esac
    if [ "$schema" = datum ]; then
      case "$line" in
        *INSERT\ INTO\ datum.schema_class* | *DELETE\ FROM\ datum.schema_class*)
          continue
          ;;
      esac
    fi
    printf '%s\n' "$line"
  done
}

# Normalize DROP FUNCTION target to schema.name for pairing (strip arg list, IF EXISTS).
drop_func_key() {
  line="$1"
  key="${line#*DROP FUNCTION }"
  key="${key#IF EXISTS }"
  key="${key%%(*}"
  key="${key%;}"
  key="${key%"${key##*[![:space:]]}"}"
  printf '%s' "$key"
}

# CREATE FUNCTION schema.name on a line (first line of declaration).
create_func_key_from_line() {
  line="$1"
  key="$(printf '%s' "$line" | sed -nE \
    's/.*CREATE[[:space:]]+FUNCTION[[:space:]]+([[:alnum:]_]+\.[[:alnum:]_]+).*/\1/p' \
    | head -1)"
  printf '%s' "$key"
}

# Collect DROP FUNCTION keys from *.up.sql in lexical order after a given basename.
later_drop_func_keys() {
  mig_dir="$1"
  after_base="$2"
  for sql in "$mig_dir"/*.up.sql; do
    if [ ! -f "$sql" ]; then
      continue
    fi
    base="$(basename "$sql")"
    if [[ "$base" < "$after_base" ]] || [[ "$base" == "$after_base" ]]; then
      continue
    fi
    while IFS= read -r line; do
      case "$line" in
        *DROP\ FUNCTION*)
          drop_func_key "$line"
          printf '\n'
          ;;
      esac
    done < "$sql"
  done
}

# True if $2 is a later-up.sql DROP of $1 (schema.func).
func_dropped_later() {
  mig_dir="$1"
  after_base="$2"
  func_key="$3"
  later="$(later_drop_func_keys "$mig_dir" "$after_base")"
  printf '%s\n' "$later" | grep -Fxq "$func_key"
}

# Enclosing CREATE FUNCTION schema.name for line number in file (best-effort).
enclosing_create_func() {
  file="$1"
  line_no="$2"
  if [ ! -f "$file" ]; then
    return 0
  fi
  awk -v target="$line_no" '
    /^[[:space:]]*CREATE[[:space:]]+FUNCTION[[:space:]]+[[:alnum:]_]+\.[[:alnum:]_]+/ {
      line = $0
      sub(/.*CREATE[[:space:]]+FUNCTION[[:space:]]+/, "", line)
      sub(/\(.*/, "", line)
      gsub(/[[:space:]]+$/, "", line)
      name = line
      start = NR
    }
    name != "" && NR >= start && NR <= target { found = name }
    END { if (found != "") print found }
  ' "$file"
}

# True if a later *.up.sql revokes the same foreign-table grant string fragment.
foreign_grant_neutralized() {
  mig_dir="$1"
  after_base="$2"
  fragment="$3"
  for sql in "$mig_dir"/*.up.sql; do
    if [ ! -f "$sql" ]; then
      continue
    fi
    base="$(basename "$sql")"
    if [[ "$base" < "$after_base" ]] || [[ "$base" == "$after_base" ]]; then
      continue
    fi
    if grep -qF "$fragment" "$sql" 2>/dev/null; then
      if grep -qi 'REVOKE' "$sql" 2>/dev/null; then
        return 0
      fi
    fi
  done
  return 1
}

cross_schema_hit_neutralized() {
  mig_dir="$1"
  hit_line="$2"
  if ! parse_rg_hit "$hit_line"; then
    return 1
  fi
  file="$(localize_hit_file "$HIT_FILE")"
  line_no="$HIT_LINE"
  after_base="$(basename "$file")"
  case "$hit_line" in
    *REVOKE*|*revoke*)
      return 0
      ;;
  esac
  func_key="$(enclosing_create_func "$file" "$line_no")"
  if [ -n "$func_key" ] && func_dropped_later "$mig_dir" "$after_base" "$func_key"; then
    return 0
  fi
  case "$hit_line" in
    *ledger.posting*)
      foreign_grant_neutralized "$mig_dir" "$after_base" 'ledger.posting' && return 0
      ;;
  esac
  return 1
}

# List schema.func for each CREATE FUNCTION … SECURITY DEFINER block in one .up.sql file.
security_definer_func_keys() {
  file="$1"
  if ! rg -q 'SECURITY[[:space:]]+DEFINER' "$file" 2>/dev/null; then
    return 0
  fi
  awk '
    /^[[:space:]]*CREATE[[:space:]]+FUNCTION[[:space:]]+[[:alnum:]_]+\.[[:alnum:]_]+/ {
      name = $0
      sub(/.*CREATE[[:space:]]+FUNCTION[[:space:]]+/, "", name)
      sub(/\(.*/, "", name)
      gsub(/[ \t\r]+$/, "", name)
      def = 0
      if ($0 ~ /SECURITY[[:space:]]+DEFINER/) {
        def = 1
      } else {
        for (i = 0; i < 12; i++) {
          if (getline ln <= 0) break
          if (ln ~ /SECURITY[[:space:]]+DEFINER/) { def = 1; break }
          if (ln ~ /^[[:space:]]*CREATE[[:space:]]+FUNCTION/) { i--; break }
        }
      }
      if (def) print name
    }
  ' "$file"
}

security_definer_unpaired() {
  mig_dir="$1"
  found=0
  for sql in "$mig_dir"/*.up.sql; do
    if [ ! -f "$sql" ]; then
      continue
    fi
    base="$(basename "$sql")"
    keys="$(security_definer_func_keys "$sql")"
    if [ -z "$keys" ]; then
      continue
    fi
    while IFS= read -r key; do
      if [ -z "$key" ]; then
        continue
      fi
      if ! func_dropped_later "$mig_dir" "$base" "$key"; then
        printf '%s: unpaired SECURITY DEFINER function %s\n' "$sql" "$key"
        found=1
      fi
    done <<EOF
$keys
EOF
  done
  return "$found"
}

# Parser + neutralization fixtures for Windows-shaped rg hits (just lint-sql-selftest).
run_hit_selftest() {
  fail=0
  win_hit='C:\ci\datum-erp\crates\datum-uom\migrations\0001.up.sql:12:FROM ledger.posting'
  rel_hit='crates/datum-uom/migrations/0001.up.sql:12:FROM ledger.posting'
  win_file='C:\ci\datum-erp\crates\datum-uom\migrations\0001.up.sql'
  rel_file='crates/datum-uom/migrations/0001.up.sql'

  if ! parse_rg_hit "$win_hit"; then
    echo "lint-sql-selftest: Windows-shaped hit failed to parse: $win_hit" >&2
    fail=1
  elif [ "$HIT_FILE" != "$win_file" ] || [ "$HIT_LINE" != "12" ] || [ "$HIT_TEXT" != "FROM ledger.posting" ]; then
    echo "lint-sql-selftest: Windows-shaped hit parsed file='$HIT_FILE' line='$HIT_LINE' text='$HIT_TEXT' (expected file='$win_file' line=12 text='FROM ledger.posting')" >&2
    fail=1
  else
    echo "lint-sql-selftest: Windows-shaped hit parsed file='$HIT_FILE' line=$HIT_LINE"
  fi

  if ! parse_rg_hit "$rel_hit"; then
    echo "lint-sql-selftest: relative hit failed to parse: $rel_hit" >&2
    fail=1
  elif [ "$HIT_FILE" != "$rel_file" ] || [ "$HIT_LINE" != "12" ] || [ "$HIT_TEXT" != "FROM ledger.posting" ]; then
    echo "lint-sql-selftest: relative hit parsed file='$HIT_FILE' line='$HIT_LINE' text='$HIT_TEXT' (expected file='$rel_file' line=12 text='FROM ledger.posting')" >&2
    fail=1
  else
    echo "lint-sql-selftest: relative hit parsed file='$HIT_FILE' line=$HIT_LINE"
  fi

  mig_dir='crates/datum-uom/migrations'
  real_rel='crates/datum-uom/migrations/00000000000001_uom.up.sql:89:      SELECT 1 FROM ledger.posting p WHERE p.item_id = p_item_id'
  win_real='C:\ci\datum-erp\crates\datum-uom\migrations\00000000000001_uom.up.sql:89:      SELECT 1 FROM ledger.posting p WHERE p.item_id = p_item_id'

  if ! cross_schema_hit_neutralized "$mig_dir" "$real_rel"; then
    echo 'lint-sql-selftest: relative ledger.posting hit in datum-uom should be neutralized (owner check must not misfire)' >&2
    fail=1
  else
    echo 'lint-sql-selftest: relative hit neutralization correctly allowed'
  fi

  if ! cross_schema_hit_neutralized "$mig_dir" "$win_real"; then
    echo 'lint-sql-selftest: Windows-shaped ledger.posting hit in datum-uom should be neutralized (owner check must not misfire)' >&2
    fail=1
  else
    echo 'lint-sql-selftest: Windows-shaped hit neutralization correctly allowed'
  fi

  return "$fail"
}

if [ "${1:-}" = "--selftest-hits" ]; then
  run_hit_selftest
  exit $?
fi

lint_fail=0

for tree in crates modules; do
  if [ ! -d "$tree" ]; then
    continue
  fi
  for unit_dir in "$tree"/*; do
    if [ ! -d "$unit_dir" ]; then
      continue
    fi
    mig_dir="$unit_dir/migrations"
    if [ ! -d "$mig_dir" ]; then
      continue
    fi
    unit="$(basename "$unit_dir")"
    case "$unit" in datum-module | datum-test) continue ;; esac

    # (a) cross-schema DDL/DML (same table-read law as src/, applied to migrations).
    for pair in $SCHEMA_OWNERS; do
      schema="${pair%%:*}"
      if migration_unit_owns_schema "$unit" "$schema"; then
        continue
      fi
      # Kernel lanes register audit metadata (exempt/redact/event FK), not module reads.
      if [ "$schema" = audit ]; then
        case "$unit" in
          datum-identity | datum-numbering) continue ;;
        esac
      fi
      hits="$(rg -n -i --glob '*.sql' \
        -e "(FROM|JOIN|INTO|UPDATE|TABLE)[[:space:]]+(ONLY[[:space:]]+)?${schema}\\." \
        "$mig_dir" 2>/dev/null || true)"
      if [ -n "$hits" ]; then
        filtered="$(printf '%s\n' "$hits" | filter_migration_hits "$schema")"
        if [ -n "$filtered" ]; then
          while IFS= read -r hit; do
            if [ -z "$hit" ]; then
              continue
            fi
            if cross_schema_hit_neutralized "$mig_dir" "$hit"; then
              continue
            fi
            printf '%s\n' "$hit"
            echo "lint-sql: migration cross-schema reference to ${schema}.* in ${unit} (owner mismatch)" >&2
            lint_fail=1
          done <<EOF
$filtered
EOF
        fi
      fi
    done

    # (b) SECURITY DEFINER functions only in datum-ledger / datum-audit / datum-db / datum-numbering.
    # R-2s-8: modules may not define SECURITY DEFINER; numbering exempt (D3 §8 gap-free counters).
    case "$unit" in
      datum-ledger | datum-audit | datum-db | datum-numbering) ;;
      *)
        if ! security_definer_unpaired "$mig_dir"; then
          echo "lint-sql: CREATE FUNCTION ... SECURITY DEFINER in migrations of ${unit} (allowed only in datum-ledger, datum-audit, datum-db, datum-numbering; R-2s-8 / D3 §8)" >&2
          lint_fail=1
        fi
        ;;
    esac

    # (c) GRANT/REVOKE on foreign schemas outside datum-db bootstrap migration.
    bootstrap="crates/datum-db/migrations/00000000000001_datum_schema.up.sql"
    for sql in "$mig_dir"/*.sql; do
      if [ ! -f "$sql" ]; then
        continue
      fi
      case "$sql" in
        "$bootstrap") continue ;;
      esac
      grant_hits="$(rg -n -i --glob "$(basename "$sql")" \
        -e '^[[:space:]]*(GRANT|REVOKE)[[:space:]]' \
        "$sql" 2>/dev/null || true)"
      if [ -z "$grant_hits" ]; then
        continue
      fi
      for pair in $SCHEMA_OWNERS; do
        schema="${pair%%:*}"
        if migration_unit_owns_schema "$unit" "$schema"; then
          continue
        fi
        schema_hits="$(printf '%s\n' "$grant_hits" | rg "${schema}\\." || true)"
        if [ -n "$schema_hits" ]; then
          printf '%s\n' "$schema_hits"
          echo "lint-sql: GRANT/REVOKE on ${schema}.* in ${sql} (only datum-db bootstrap may grant foreign schemas)" >&2
          lint_fail=1
        fi
      done
    done

    # (d) Session-protocol fence in migrations (CONTRACT §5a / §5a.1 mirror).
    # Wave-1 bootstrap in 0001 migrations is grandfathered; rule (d) targets later bypass (e.g. server-slice wo_start).
    case "$unit" in
      datum-db | datum-audit) ;;
      *)
        session_hits="$(rg -n -i --glob '*.sql' --glob '!00000000000001_*.up.sql' \
          -e "SET[[:space:]]+datum\\." \
          -e "set_config\\('datum\\." \
          -e "current_setting\\('datum\\." \
          "$mig_dir" 2>/dev/null || true)"
        if [ -n "$session_hits" ]; then
          while IFS= read -r hit; do
            if [ -z "$hit" ]; then
              continue
            fi
            case "$hit" in
              *DEFAULT*current_setting*datum.*) continue ;;
            esac
            file="$hit"
            if parse_rg_hit "$hit"; then
              file="$(localize_hit_file "$HIT_FILE")"
            fi
            printf '%s\n' "$hit"
            echo "lint-sql: session-protocol fence bypass in ${file} (datum.* GUC outside datum-db/datum-audit migrations)" >&2
            lint_fail=1
          done <<EOF
$session_hits
EOF
        fi
        ;;
    esac
  done
done

if [ "$lint_fail" -ne 0 ]; then
  exit 1
fi
