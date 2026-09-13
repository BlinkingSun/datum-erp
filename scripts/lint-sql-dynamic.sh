#!/usr/bin/env bash
# Dynamic cross-schema SQL construction (invariant 6 + R-2s-3).
# Invoked with REPO_ROOT set. POSIX bash 3.2; requires rg.
# Run from the repo root over relative paths. Never splits rg hits on the
# first colon: awk walks files we already listed and prints relpath:line:text.
#
# Flags, inside SQL strings under modules/*/src, crates/*/src, and
# crates/*/migrations (modules/*/migrations too), outside each schema's own
# crate and the explicit exemption lists:
#   (a) format(...) / EXECUTE format(...) with a quoted schema literal
#   (b) quote_ident('<schema>')
#   (c) string concatenation producing '<schema>.'  ('schema.' / 'schema' ||)
#
# SQL format( is not Rust format!( and not method .format(.
set -eu

REPO_ROOT="${REPO_ROOT:?REPO_ROOT is required}"

command -v rg >/dev/null 2>&1 || {
  echo 'lint-sql: ripgrep (rg) is required' >&2
  exit 1
}

cd "$REPO_ROOT" || exit 1

# Same pair list as just lint-sql invariant 6 (Wave 2b crates included).
SCHEMA_OWNERS='identity:datum-identity uom:datum-uom ledger:datum-ledger sm:datum-statemachine jobs:datum-jobs events:datum-events numbering:datum-numbering audit:datum-audit items:datum-mod-items locations:datum-mod-locations lots:datum-mod-lots inventory:datum-mod-inventory production_min:datum-mod-production-min genealogy:datum-mod-genealogy documents:datum-documents print:datum-print esign:datum-esign customfields:datum-customfields'

# Explicit invariant-6 exemptions (owning crate is per-pair, not listed here).
CROSS_EXEMPT='datum-module datum-test'

# Explicit R-2s-3 exemptions (ledger.* / transient.*).
R2S3_EXEMPT='datum-ledger datum-db datum-audit datum-test datum-jobs datum-events datum-identity'
R2S3_SCHEMAS='ledger transient'

# Portable awk: no match(array), gensub, or interval expressions.
# \047 is a single quote so the program can live in a bash single-quoted string.
# mode=stdin: SQL on stdin, jobs is "kind:schema:owner" (selftest).
# mode=list: relative paths on stdin, one awk walk per unit.
run_dynamic_awk() {
  mode="$1"
  jobs="$2"
  relpath="${3:-}"
  awk -v mode="$mode" -v jobs="$jobs" -v relpath="$relpath" '
    function is_sql_format(line,    lower, rest, off, i, c) {
      lower = tolower(line)
      off = 1
      rest = lower
      while (match(rest, /format[ \t]*\(/)) {
        i = off + RSTART - 1
        if (i == 1) return 1
        c = substr(lower, i - 1, 1)
        if (c !~ /[a-z0-9_!.]/) return 1
        off = i + RLENGTH
        rest = substr(lower, off)
      }
      return 0
    }
    function has_quoted_schema(lower, sch,    pat) {
      pat = "[\047\"]" sch "[\047\"]"
      return match(lower, pat)
    }
    function is_comment(line,    t) {
      t = line
      sub(/^[ \t]+/, "", t)
      return (index(t, "//") == 1 || index(t, "--") == 1)
    }
    function outpath() {
      if (mode == "stdin") return relpath
      return curfile
    }
    function emit(j, nr, text) {
      if (mode == "stdin") {
        printf "%s:%d:%s\n", outpath(), nr, text
      } else {
        printf "%s\036%s\036%s\036%s:%d:%s\n", kinds[j], schemas[j], owners[j], outpath(), nr, text
      }
    }
    function reset_file() {
      linenr = 0
      win = 0
      delete fmt_hit
    }
    function process_line(raw,    lower, cmt, j, sch, qid, dot, pipe) {
      sub(/\r$/, "", raw)
      linenr++
      lower = tolower(raw)
      cmt = is_comment(raw)
      if (is_sql_format(raw)) {
        win = 12
        fmt_nr = linenr
        fmt_raw = raw
        delete fmt_hit
      }
      for (j = 1; j <= nj; j++) {
        sch = schemas[j]
        qid = "quote_ident[ \\t]*\\([ \\t]*[\047\"]" sch "[\047\"]"
        dot = "[\047\"]" sch "\\.[\047\"]"
        pipe = "[\047\"]" sch "[\047\"][ \\t]*\\|\\|"
        if (win > 0 && !cmt && !fmt_hit[j] && has_quoted_schema(lower, sch)) {
          emit(j, fmt_nr, fmt_raw)
          fmt_hit[j] = 1
        } else if (!cmt) {
          if (match(lower, qid) || match(lower, dot) || match(lower, pipe)) {
            emit(j, linenr, raw)
          }
        }
      }
      if (win > 0) win--
    }
    BEGIN {
      nj = split(jobs, job, " ")
      for (i = 1; i <= nj; i++) {
        split(job[i], p, ":")
        kinds[i] = p[1]
        schemas[i] = tolower(p[2])
        owners[i] = p[3]
      }
      if (mode == "stdin") reset_file()
    }
    mode == "stdin" { process_line($0); next }
    {
      curfile = $0
      gsub(/\\/, "/", curfile)
      reset_file()
      while ((getline raw < curfile) > 0) {
        process_line(raw)
      }
      close(curfile)
    }
  '
}

dynamic_awk() {
  schema="$1"
  relpath="$2"
  run_dynamic_awk stdin "cross:${schema}:x" "$relpath"
}

list_scan_files() {
  dir="$1"
  if [ ! -d "$dir" ]; then
    return 0
  fi
  rg --files --glob '*.rs' --glob '*.sql' "$dir" 2>/dev/null || true
}

in_word_list() {
  needle="$1"
  hay="$2"
  for w in $hay; do
    if [ "$w" = "$needle" ]; then
      return 0
    fi
  done
  return 1
}

cross_unit_skips_schema() {
  unit="$1"
  schema="$2"
  owner="$3"
  if in_word_list "$unit" "$CROSS_EXEMPT"; then
    return 0
  fi
  if [ "$unit" = "$owner" ] || [ "$unit" = "$schema" ]; then
    return 0
  fi
  return 1
}

expect_hit() {
  label="$1"
  needle="$2"
  hits="$3"
  if printf '%s\n' "$hits" | grep -F "$needle" >/dev/null; then
    echo "lint-sql-selftest: $label"
    return 0
  fi
  echo "lint-sql-selftest: $label FAILED: [$hits]" >&2
  return 1
}

expect_clean() {
  label="$1"
  hits="$2"
  if [ -z "$hits" ]; then
    echo "lint-sql-selftest: $label"
    return 0
  fi
  echo "lint-sql-selftest: $label FAILED: [$hits]" >&2
  return 1
}

run_hit_selftest() {
  fail=0

  sample="$(printf '%s\n' \
    'EXECUTE format(' \
    "  'SELECT state FROM %I.%I WHERE doc_type = \$1 AND doc_id = \$2'," \
    "  'sm'," \
    "  'instance'" \
    ')')"
  hits="$(printf '%s\n' "$sample" | dynamic_awk sm plant.sql)"
  expect_hit "documents EXECUTE format('%I.%I','sm',...) fixture caught at plant.sql:1" \
    'plant.sql:1:EXECUTE format(' "$hits" || fail=1

  crlf="$(printf 'EXECUTE format(\r\n  '\''sm'\'',\r\n  '\''instance'\''\r\n)')"
  hits="$(printf '%s\n' "$crlf" | dynamic_awk sm plant_crlf.sql)"
  expect_hit "CRLF documents format fixture caught at plant_crlf.sql:1" \
    'plant_crlf.sql:1:EXECUTE format(' "$hits" || fail=1

  hits="$(printf '%s\n' 'SELECT state FROM ' "  quote_ident('sm') || '.instance'" | dynamic_awk sm plant_qid.sql)"
  expect_hit "quote_ident('sm') fixture caught" \
    "quote_ident('sm')" "$hits" || fail=1

  hits="$(printf '%s\n' "EXECUTE 'SELECT 1 FROM ' || 'ledger.' || 'posting'" | dynamic_awk ledger plant_dot.sql)"
  expect_hit "concatenation 'ledger.' fixture caught" \
    "'ledger.'" "$hits" || fail=1

  hits="$(printf '%s\n' "let s = a || 'sm' || b;" | dynamic_awk sm plant_pipe.sql)"
  expect_hit "concatenation 'sm' || fixture caught" \
    "'sm' ||" "$hits" || fail=1

  hits="$(printf '%s\n' 'out.push(format!("bad {sm}"));' | dynamic_awk sm plant_rust.rs)"
  expect_clean "Rust format! correctly ignored" "$hits" || fail=1

  hits="$(printf '%s\n' 'let _ = ts.format("%Y-%m-%d");' | dynamic_awk sm plant_method.rs)"
  expect_clean "method .format( correctly ignored" "$hits" || fail=1

  hits="$(printf '%s\n' "-- used format('%I.%I','sm','instance') — fail-class" | dynamic_awk sm plant_cmt.sql)"
  expect_clean "SQL comment correctly ignored" "$hits" || fail=1

  hits="$(printf '%s\n' "/// comment format('sm') in docs" | dynamic_awk sm plant_rs_cmt.rs)"
  expect_clean "Rust comment correctly ignored" "$hits" || fail=1

  hits="$(printf '%s\n' "EXECUTE format('%I.%I', 'sm', 'instance');" | dynamic_awk sm plant_one.sql)"
  expect_hit "one-line format('%I.%I','sm',...) fixture caught" \
    "plant_one.sql:1:EXECUTE format('%I.%I', 'sm', 'instance');" "$hits" || fail=1

  return "$fail"
}

if [ "${1:-}" = "--selftest-hits" ]; then
  run_hit_selftest
  exit $?
fi

lint_fail=0
hit_count=0

unit_files() {
  unit_dir="$1"
  for sub in src migrations; do
    list_scan_files "$unit_dir/$sub"
  done
}

unit_jobs() {
  unit="$1"
  jobs=""
  for pair in $SCHEMA_OWNERS; do
    schema="${pair%%:*}"
    owner="${pair##*:}"
    if cross_unit_skips_schema "$unit" "$schema" "$owner"; then
      continue
    fi
    if [ -n "$jobs" ]; then
      jobs="$jobs cross:${schema}:${owner}"
    else
      jobs="cross:${schema}:${owner}"
    fi
  done
  if ! in_word_list "$unit" "$R2S3_EXEMPT"; then
    for schema in $R2S3_SCHEMAS; do
      if [ -n "$jobs" ]; then
        jobs="$jobs r2s3:${schema}:${schema}"
      else
        jobs="r2s3:${schema}:${schema}"
      fi
    done
  fi
  printf '%s' "$jobs"
}

for tree in modules crates; do
  if [ ! -d "$tree" ]; then
    continue
  fi
  for unit_dir in "$tree"/*; do
    if [ ! -d "$unit_dir" ]; then
      continue
    fi
    unit="$(basename "$unit_dir")"
    jobs="$(unit_jobs "$unit")"
    if [ -z "$jobs" ]; then
      continue
    fi
    files="$(unit_files "$unit_dir")"
    if [ -z "$files" ]; then
      continue
    fi
    hits="$(printf '%s\n' "$files" | tr '\\' '/' | run_dynamic_awk list "$jobs" || true)"
    if [ -z "$hits" ]; then
      continue
    fi
    while IFS="$(printf '\036')" read -r kind schema owner rec; do
      if [ -z "$kind" ] || [ -z "$rec" ]; then
        continue
      fi
      printf '%s\n' "$rec"
      file="${rec%%:*}"
      case "$kind" in
        r2s3)
          echo "lint-sql: R-2s-3: dynamic SQL schema ${schema} in ${file} (format/quote_ident/concat; use datum_ledger::has_postings / has_quantity_at; modules must not read those schemas directly)" >&2
          ;;
        *)
          echo "lint-sql: dynamic cross-module schema ${schema} in ${file} (format/quote_ident/concat; owner ${owner})" >&2
          ;;
      esac
      hit_count=$((hit_count + 1))
      lint_fail=1
    done <<EOF
$hits
EOF
  done
done

if [ "$lint_fail" -ne 0 ]; then
  echo "lint-sql: dynamic SQL: ${hit_count} hit(s)" >&2
  exit 1
fi
