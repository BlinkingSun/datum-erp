#!/usr/bin/env bash
# R-2s-3: production src SQL must not DML-reference ledger.* or transient.*
# except the crates that own those schemas. Invoked with REPO_ROOT set.
# POSIX bash 3.2; requires rg. Run from the repo root over relative paths.
# Globs: *.rs and *.sql (include_str!/query_file! includes under src/).
set -eu

REPO_ROOT="${REPO_ROOT:?REPO_ROOT is required}"

command -v rg >/dev/null 2>&1 || {
  echo 'lint-sql: ripgrep (rg) is required' >&2
  exit 1
}

cd "$REPO_ROOT" || exit 1

# Explicit, minimal exemption set. wicket-module is not listed: KERNEL_AUDIT_RELS
# names ledger tables as strings, which is not SQL DML (FROM/JOIN/INTO/UPDATE/TABLE).
# wicket-uom's SELECT ledger.has_postings($1) is the published function, not a table read.
# Dynamic construction of ledger./transient. (format/quote_ident/'schema.' concat)
# is scripts/lint-sql-dynamic.sh, same exemption list.
R2S3_EXEMPT='wicket-ledger wicket-db wicket-audit wicket-test wicket-jobs wicket-events wicket-identity'

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

crate_is_r2s3_exempt() {
  unit="$1"
  for ex in $R2S3_EXEMPT; do
    if [ "$unit" = "$ex" ]; then
      return 0
    fi
  done
  return 1
}

# Comment lines are not SQL strings (//, ///, //!, --).
hit_is_comment() {
  text="$1"
  trimmed="${text#"${text%%[![:space:]]*}"}"
  case "$trimmed" in
    //*) return 0 ;;
    --*) return 0 ;;
  esac
  return 1
}

# Parser fixtures for Windows-shaped rg hits (just lint-sql-selftest).
run_hit_selftest() {
  fail=0
  win_hit='C:\ci\wicket-erp\modules\locations\src\store.rs:403:                   SELECT 1 FROM transient.balance_projection'
  rel_hit='modules/locations/src/store.rs:403:                   SELECT 1 FROM transient.balance_projection'
  win_file='C:\ci\wicket-erp\modules\locations\src\store.rs'
  rel_file='modules/locations/src/store.rs'
  expect_text='                   SELECT 1 FROM transient.balance_projection'

  if ! parse_rg_hit "$win_hit"; then
    echo "lint-sql-selftest: Windows-shaped R-2s-3 hit failed to parse: $win_hit" >&2
    fail=1
  elif [ "$HIT_FILE" != "$win_file" ] || [ "$HIT_LINE" != "403" ] || [ "$HIT_TEXT" != "$expect_text" ]; then
    echo "lint-sql-selftest: Windows-shaped R-2s-3 hit parsed file='$HIT_FILE' line='$HIT_LINE' text='$HIT_TEXT'" >&2
    fail=1
  else
    echo "lint-sql-selftest: Windows-shaped R-2s-3 hit parsed file='$HIT_FILE' line=$HIT_LINE"
  fi

  if ! parse_rg_hit "$rel_hit"; then
    echo "lint-sql-selftest: relative R-2s-3 hit failed to parse: $rel_hit" >&2
    fail=1
  elif [ "$HIT_FILE" != "$rel_file" ] || [ "$HIT_LINE" != "403" ] || [ "$HIT_TEXT" != "$expect_text" ]; then
    echo "lint-sql-selftest: relative R-2s-3 hit parsed file='$HIT_FILE' line='$HIT_LINE' text='$HIT_TEXT'" >&2
    fail=1
  else
    echo "lint-sql-selftest: relative R-2s-3 hit parsed file='$HIT_FILE' line=$HIT_LINE"
  fi

  if [ -f "$rel_file" ]; then
    loc="$(localize_hit_file "$win_file")"
    if [ "$loc" != "$rel_file" ]; then
      echo "lint-sql-selftest: localize_hit_file Windows path -> '$loc' (expected '$rel_file')" >&2
      fail=1
    else
      echo "lint-sql-selftest: Windows-shaped R-2s-3 path localized to $loc"
    fi
  fi

  comment_hit='modules/items/src/store.rs:56:/// Insert `new`, sync `ledger.stock_item`, spawn the draft instance.'
  if ! parse_rg_hit "$comment_hit"; then
    echo "lint-sql-selftest: comment hit failed to parse" >&2
    fail=1
  elif ! hit_is_comment "$HIT_TEXT"; then
    echo "lint-sql-selftest: doc-comment line should be classified as a comment" >&2
    fail=1
  else
    echo "lint-sql-selftest: doc-comment line correctly ignored"
  fi

  sql_hit='modules/locations/src/store.rs:403:                   SELECT 1 FROM transient.balance_projection'
  if ! parse_rg_hit "$sql_hit"; then
    echo "lint-sql-selftest: SQL hit failed to parse" >&2
    fail=1
  elif hit_is_comment "$HIT_TEXT"; then
    echo "lint-sql-selftest: SQL DML line must not be classified as a comment" >&2
    fail=1
  else
    echo "lint-sql-selftest: SQL DML line correctly kept"
  fi

  return "$fail"
}

if [ "${1:-}" = "--selftest-hits" ]; then
  run_hit_selftest
  exit $?
fi

lint_fail=0
hit_count=0

# Table DML only: FROM/JOIN/INTO/UPDATE/TABLE + ledger. or transient.
# Does not match ::ledger.boundary (type cast), SELECT ledger.has_postings
# (published function), inventory_transient.* / genealogy_transient.* /
# server_transient.* (different schemas), or KERNEL_AUDIT_RELS name lists.
for tree in modules crates; do
  if [ ! -d "$tree" ]; then
    continue
  fi
  for unit_dir in "$tree"/*; do
    if [ ! -d "$unit_dir" ]; then
      continue
    fi
    unit="$(basename "$unit_dir")"
    if [ "$tree" = crates ] && crate_is_r2s3_exempt "$unit"; then
      continue
    fi
    src_dir="$unit_dir/src"
    if [ ! -d "$src_dir" ]; then
      continue
    fi
    hits="$(rg -n --glob '*.rs' --glob '*.sql' \
      -e '(FROM|JOIN|INTO|UPDATE|TABLE)[[:space:]]+(ONLY[[:space:]]+)?(ledger|transient)\.' \
      "$src_dir" 2>/dev/null || true)"
    if [ -z "$hits" ]; then
      continue
    fi
    while IFS= read -r hit; do
      if [ -z "$hit" ]; then
        continue
      fi
      if parse_rg_hit "$hit"; then
        if hit_is_comment "$HIT_TEXT"; then
          continue
        fi
        file="$(localize_hit_file "$HIT_FILE")"
      else
        file="$hit"
      fi
      printf '%s\n' "$hit"
      echo "lint-sql: R-2s-3: SQL table token ledger.* or transient.* in ${file} (use wicket_ledger::has_postings / has_quantity_at; modules must not read those schemas directly)" >&2
      hit_count=$((hit_count + 1))
      lint_fail=1
    done <<EOF
$hits
EOF
  done
done

if [ "$lint_fail" -ne 0 ]; then
  echo "lint-sql: R-2s-3: ${hit_count} hit(s) in production src" >&2
  exit 1
fi
