#!/usr/bin/env bash
# Run the same nextest selection with embedded Base cache and with all sjulia
# caches disabled, then fail explicitly when the two modes disagree.
#
# Usage:
#   scripts/check_cold_cached_nextest.sh --test fixture_tests dispatch::
#
# Arguments are forwarded to scripts/test_with_cache.sh and
# scripts/test_without_cache.sh.
set -euo pipefail

cached_status=0
cold_status=0

echo "== cached run =="
set +e
bash scripts/test_with_cache.sh "$@"
cached_status=$?
set -e

echo "== cold run =="
set +e
bash scripts/test_without_cache.sh "$@"
cold_status=$?
set -e

if [[ "$cached_status" -ne "$cold_status" ]]; then
  cat >&2 <<EOF
ERROR: cached and cold nextest results differ.
  cached exit: $cached_status
  cold exit:   $cold_status

This indicates a cache-transparency regression: the same test selection behaves
different depending on whether Base/prelude caches are used.
EOF
  exit 1
fi

if [[ "$cached_status" -ne 0 ]]; then
  echo "ERROR: cached and cold runs both failed (exit $cached_status)." >&2
  exit "$cached_status"
fi

echo "OK: cached and cold nextest runs agree."
