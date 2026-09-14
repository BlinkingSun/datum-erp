#!/usr/bin/env bash
# Fail a pull-request body that omits ## Goal 1..4 (TODO.md T-06).
# Body source: file arg, else GITHUB_EVENT_PATH pull_request.body, else PR_BODY.
# Empty / null body fails. Push events have no PR body; the workflow skips this.
set -euo pipefail

body=""

extract_event_body() {
  python3 -c '
import json, os, sys
path = os.environ["GITHUB_EVENT_PATH"]
with open(path, encoding="utf-8") as f:
    ev = json.load(f)
b = (ev.get("pull_request") or {}).get("body")
sys.stdout.write("" if b is None else b)
'
}

if [ "${1:-}" != "" ] && [ -f "$1" ]; then
  body="$(cat "$1")"
elif [ -n "${GITHUB_EVENT_PATH:-}" ] && [ -f "${GITHUB_EVENT_PATH}" ]; then
  if command -v python3 >/dev/null 2>&1; then
    body="$(extract_event_body)"
  elif [ "${PR_BODY+x}" = "x" ]; then
    body="$PR_BODY"
  else
    echo "check-pr-goals: cannot read github.event.pull_request.body (python3 missing)" >&2
    exit 1
  fi
elif [ "${PR_BODY+x}" = "x" ]; then
  body="$PR_BODY"
else
  echo "check-pr-goals: no pull-request body (set PR_BODY, pass a file, or run in GitHub Actions)" >&2
  exit 1
fi

body="$(printf '%s' "$body" | tr -d '\r')"

if [ -z "$body" ]; then
  echo "check-pr-goals: pull-request body is empty; ## Goal 1, ## Goal 2, ## Goal 3, ## Goal 4 are required" >&2
  exit 1
fi

missing=0
for n in 1 2 3 4; do
  heading="## Goal ${n}"
  if ! printf '%s\n' "$body" | grep -Eq "^## Goal ${n}([[:space:]]|$)"; then
    echo "check-pr-goals: missing heading: ${heading}" >&2
    missing=1
  fi
done

if [ "$missing" -ne 0 ]; then
  echo "check-pr-goals: pull-request body must contain ## Goal 1, ## Goal 2, ## Goal 3, and ## Goal 4" >&2
  exit 1
fi

echo "check-pr-goals: all four goal headings are present"
exit 0
