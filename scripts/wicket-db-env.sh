#!/usr/bin/env bash
# Resolve WICKET_TEST_TEMPLATE and WICKET_TEST_DB for just db-reset / db-gc / test-db.
# Source from a just recipe (Git bash on the shop PC; macOS bash 3.2).
# Roles stay cluster-wide; this script only names databases.
#
#   WICKET_TEST_TEMPLATE  default wicket_test_template
#   WICKET_TEST_DB        default: database path in WICKET_DATABASE_URL, else wicket_test

_wicket_db_ident_ok() {
  case "$1" in
    ''|*[!A-Za-z0-9_]*) return 1 ;;
  esac
  case "$1" in
    [A-Za-z_]*) return 0 ;;
    *) return 1 ;;
  esac
}

if [ -z "${WICKET_TEST_TEMPLATE:-}" ]; then
  WICKET_TEST_TEMPLATE=wicket_test_template
fi

if [ -z "${WICKET_TEST_DB:-}" ]; then
  _wicket_url="${WICKET_DATABASE_URL:-}"
  _wicket_noqs="${_wicket_url%%\?*}"
  _wicket_from_url=""
  case "$_wicket_noqs" in
    */*) _wicket_from_url="${_wicket_noqs##*/}" ;;
  esac
  if [ -n "$_wicket_from_url" ] && _wicket_db_ident_ok "$_wicket_from_url"; then
    WICKET_TEST_DB="$_wicket_from_url"
  else
    WICKET_TEST_DB=wicket_test
  fi
  unset _wicket_url _wicket_noqs _wicket_from_url
fi

if ! _wicket_db_ident_ok "$WICKET_TEST_TEMPLATE"; then
  echo "wicket-db-env: WICKET_TEST_TEMPLATE is not a simple identifier: ${WICKET_TEST_TEMPLATE}" >&2
  return 1 2>/dev/null || exit 1
fi
if ! _wicket_db_ident_ok "$WICKET_TEST_DB"; then
  echo "wicket-db-env: WICKET_TEST_DB is not a simple identifier: ${WICKET_TEST_DB}" >&2
  return 1 2>/dev/null || exit 1
fi

export WICKET_TEST_TEMPLATE WICKET_TEST_DB

if [ "${BASH_SOURCE[0]-}" = "$0" ]; then
  printf 'WICKET_TEST_TEMPLATE=%s\n' "$WICKET_TEST_TEMPLATE"
  printf 'WICKET_TEST_DB=%s\n' "$WICKET_TEST_DB"
fi
