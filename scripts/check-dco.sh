#!/usr/bin/env bash
# DCO 1.1 sign-off check (ADR 0006 / TODO.md T-04). Text trailer only; not GPG.
# Usage: check-dco.sh <git-range>
#   pull_request: origin/<base>..HEAD
#   push:         <before>..<sha>
#   before zeros: HEAD (that commit only, not its ancestors)
set -euo pipefail

usage() {
  echo "usage: check-dco.sh <git-range>" >&2
  echo "  pull_request: origin/<base>..HEAD" >&2
  echo "  push:         <before>..<sha>  (HEAD if before is zeros)" >&2
  exit 2
}

if [ "${1:-}" = "" ] || [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  usage
fi

RANGE="$1"
ZEROS="0000000000000000000000000000000000000000"

case "$RANGE" in
  "$ZEROS"..*)
    RANGE="HEAD"
    ;;
  "$ZEROS")
    RANGE="HEAD"
    ;;
esac

list_commits() {
  case "$RANGE" in
    *..*)
      git rev-list "$RANGE"
      ;;
    *)
      git rev-list -1 "$RANGE"
      ;;
  esac
}

if ! commits="$(list_commits)"; then
  echo "check-dco: failed to list commits for range: ${RANGE}" >&2
  exit 1
fi

if [ -z "$commits" ]; then
  echo "check-dco: no commits in range ${RANGE} (ok)"
  exit 0
fi

fail=0
while IFS= read -r sha; do
  [ -n "$sha" ] || continue
  body="$(git log -1 --format='%B' "$sha")"
  if ! printf '%s\n' "$body" | grep -q '^Signed-off-by: '; then
    subject="$(git log -1 --format='%s' "$sha")"
    echo "check-dco: missing Signed-off-by: on ${sha} ${subject}" >&2
    fail=1
  fi
done <<EOF
$commits
EOF

if [ "$fail" -ne 0 ]; then
  echo "check-dco: every commit must carry a Signed-off-by: line (ADR 0006; git commit -s)" >&2
  exit 1
fi

echo "check-dco: all commits in ${RANGE} are signed off"
exit 0
