#!/usr/bin/env bash
# Read-only guarded-premerge owner for the STATUS.md / DONE.md archive budget
# (Issue #11263). Keep the mutating archive command explicit and opt-in.

set -euo pipefail

if [ "$#" -ne 0 ]; then
  echo "usage: $0" >&2
  echo "ERROR: this guarded check enforces the fixed 3000-line invariant; use archive_status_done.sh directly for maintenance options." >&2
  exit 2
fi

cd "$(dirname "$0")/.."
exec bash scripts/archive_status_done.sh --check
