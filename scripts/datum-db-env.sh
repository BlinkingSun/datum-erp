#!/usr/bin/env bash
# Resolve DATUM_TEST_TEMPLATE and DATUM_TEST_DB for just db-reset / db-gc / test-db.
# Source from a just recipe (Git bash on the shop PC; macOS bash 3.2).
# Roles stay cluster-wide; this script only names databases.
#
#   DATUM_TEST_TEMPLATE  default datum_test_template
#   DATUM_TEST_DB        default: database path in DATUM_DATABASE_URL, else datum_test

_datum_db_ident_ok() {
  case "$1" in
    ''|*[!A-Za-z0-9_]*) return 1 ;;
  esac
  case "$1" in
    [A-Za-z_]*) return 0 ;;
    *) return 1 ;;
  esac
}

if [ -z "${DATUM_TEST_TEMPLATE:-}" ]; then
  DATUM_TEST_TEMPLATE=datum_test_template
fi

if [ -z "${DATUM_TEST_DB:-}" ]; then
  _datum_url="${DATUM_DATABASE_URL:-}"
  _datum_noqs="${_datum_url%%\?*}"
  _datum_from_url=""
  case "$_datum_noqs" in
    */*) _datum_from_url="${_datum_noqs##*/}" ;;
  esac
  if [ -n "$_datum_from_url" ] && _datum_db_ident_ok "$_datum_from_url"; then
    DATUM_TEST_DB="$_datum_from_url"
  else
    DATUM_TEST_DB=datum_test
  fi
  unset _datum_url _datum_noqs _datum_from_url
fi

if ! _datum_db_ident_ok "$DATUM_TEST_TEMPLATE"; then
  echo "datum-db-env: DATUM_TEST_TEMPLATE is not a simple identifier: ${DATUM_TEST_TEMPLATE}" >&2
  return 1 2>/dev/null || exit 1
fi
if ! _datum_db_ident_ok "$DATUM_TEST_DB"; then
  echo "datum-db-env: DATUM_TEST_DB is not a simple identifier: ${DATUM_TEST_DB}" >&2
  return 1 2>/dev/null || exit 1
fi

export DATUM_TEST_TEMPLATE DATUM_TEST_DB

if [ "${BASH_SOURCE[0]-}" = "$0" ]; then
  printf 'DATUM_TEST_TEMPLATE=%s\n' "$DATUM_TEST_TEMPLATE"
  printf 'DATUM_TEST_DB=%s\n' "$DATUM_TEST_DB"
fi
