#!/usr/bin/env bash
# Reverse-diff: axum `.route(` table vs MOUNTED (ADR 0010 interim / T-23).
# Extra or missing METHOD+path pairs fail. POSIX bash 3.2; portable awk.
set -eu

ROOT="${REPO_ROOT:-$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)}"
HTTP="$ROOT/crates/wicket-server/src/http.rs"
OPENAPI="$ROOT/crates/wicket-server/src/openapi.rs"

if [ ! -f "$HTTP" ]; then
  echo "lint-mounts: missing router source: $HTTP" >&2
  exit 1
fi
if [ ! -f "$OPENAPI" ]; then
  echo "lint-mounts: missing OpenAPI table: $OPENAPI" >&2
  exit 1
fi

extract_router() {
  awk '
    {
      src = src $0 "\n"
    }
    END {
      n = length(src)
      i = 1
      found = 0
      while (i <= n) {
        rest = substr(src, i)
        p = index(rest, ".route(")
        if (p == 0) break
        i = i + p + 5
        depth = 1
        start = i + 1
        i++
        while (i <= n && depth > 0) {
          c = substr(src, i, 1)
          if (c == "(") depth++
          else if (c == ")") depth--
          i++
        }
        body = substr(src, start, i - start - 1)
        q1 = index(body, "\"")
        if (q1 == 0) continue
        after = substr(body, q1 + 1)
        q2 = index(after, "\"")
        if (q2 == 0) continue
        path = substr(after, 1, q2 - 1)
        nm = split("get post put patch delete", meths, " ")
        for (mi = 1; mi <= nm; mi++) {
          needle = meths[mi] "("
          off = 1
          while (1) {
            chunk = substr(body, off)
            at = index(chunk, needle)
            if (at == 0) break
            abs = off + at - 1
            ok = 1
            if (abs > 1) {
              prev = substr(body, abs - 1, 1)
              if (prev ~ /[A-Za-z0-9_]/) ok = 0
            }
            if (ok) {
              print toupper(meths[mi]) " " path
              found++
            }
            off = abs + length(needle)
          }
        }
      }
      if (found == 0) {
        print "lint-mounts: extracted 0 routes from http.rs" > "/dev/stderr"
        exit 1
      }
    }
  ' "$HTTP"
}

extract_mounted() {
  awk '
    /method: "/ {
      n = split($0, parts, "\"")
      method = ""
      if (n >= 2) method = parts[2]
    }
    /path: "/ {
      n = split($0, parts, "\"")
      path = ""
      if (n >= 2) path = parts[2]
      if (method != "" && path != "") {
        print method " " path
        found++
      }
      method = ""
    }
    END {
      if (found + 0 == 0) {
        print "lint-mounts: extracted 0 entries from MOUNTED" > "/dev/stderr"
        exit 1
      }
    }
  ' "$OPENAPI"
}

router_ops="$(extract_router | sort -u)"
mounted_ops="$(extract_mounted | sort -u)"

router_n="$(printf '%s\n' "$router_ops" | grep -c . || true)"
mounted_n="$(printf '%s\n' "$mounted_ops" | grep -c . || true)"

if [ "$router_n" -eq 0 ]; then
  echo "lint-mounts: extracted 0 routes from http.rs" >&2
  exit 1
fi
if [ "$mounted_n" -eq 0 ]; then
  echo "lint-mounts: extracted 0 entries from MOUNTED" >&2
  exit 1
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/lint-mounts.XXXXXX")"
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT
trap 'cleanup; exit 130' INT TERM

printf '%s\n' "$router_ops" > "$tmp/router"
printf '%s\n' "$mounted_ops" > "$tmp/mounted"

extra_router="$(comm -23 "$tmp/router" "$tmp/mounted" || true)"
extra_mounted="$(comm -13 "$tmp/router" "$tmp/mounted" || true)"

fail=0
if [ -n "$extra_router" ]; then
  echo "lint-mounts: extra in router (missing from MOUNTED):" >&2
  printf '%s\n' "$extra_router" | sed 's/^/  /' >&2
  fail=1
fi
if [ -n "$extra_mounted" ]; then
  echo "lint-mounts: extra in MOUNTED (missing from router):" >&2
  printf '%s\n' "$extra_mounted" | sed 's/^/  /' >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "lint-mounts: $router_n router ops, $mounted_n MOUNTED ops" >&2
  exit 1
fi

echo "lint-mounts: $router_n method+path pairs match"
