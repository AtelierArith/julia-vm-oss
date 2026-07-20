#!/usr/bin/env bash
# check_fixture_categories.sh — Issue #9671 Phase 2
#
# Prevent fixture category drift. Historically near-duplicate categories
# proliferated (`array`/`arrays`/`array_utils`/`global_arrays`, `macro`/`macros`,
# `meta`/`metaprogramming`, `function`/`functions`, `int_ops`/`intfuncs`,
# `number`/`numeric`, …), which scattered related fixtures and made
# category-targeted `nextest run --test fixture_tests <cat>::` unreliable.
#
# This audit enforces a CANONICAL allowlist: every directory under
# subset_julia_vm/tests/fixtures/ must be named in docs/vm/FIXTURE_CATEGORIES.tsv.
# Adding a new category therefore requires an allowlist edit that surfaces in
# review — the reviewer can ask "is this a near-synonym of an existing category?"
#
# Usage (from the repository root):
#   bash scripts/check_fixture_categories.sh
#
# Overrides for testing:
#   SJULIA_FIXTURES_DIR=<dir> SJULIA_FIXTURE_CATEGORIES=<file> bash scripts/check_fixture_categories.sh
#
# Exit code: 0 = every category directory is allowlisted; 1 = an unlisted
#            category directory exists.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

FIXTURES_DIR="${SJULIA_FIXTURES_DIR:-$REPO_ROOT/subset_julia_vm/tests/fixtures}"
ALLOWLIST="${SJULIA_FIXTURE_CATEGORIES:-$REPO_ROOT/docs/vm/FIXTURE_CATEGORIES.tsv}"

if [ ! -f "$ALLOWLIST" ]; then
  echo "FAIL: fixture category allowlist not found: $ALLOWLIST (Issue #9671)"
  exit 1
fi

allow_tmp="$(mktemp)"
present_tmp="$(mktemp)"
trap 'rm -f "$allow_tmp" "$present_tmp"' EXIT

grep -v '^[[:space:]]*#' "$ALLOWLIST" | awk 'NF { print $1 }' | sort -u > "$allow_tmp"

if [ -d "$FIXTURES_DIR" ]; then
  for d in "$FIXTURES_DIR"/*/; do
    [ -d "$d" ] || continue
    base="$(basename "$d")"
    printf '%s\n' "$base"
  done | sort -u > "$present_tmp"
else
  : > "$present_tmp"
fi

present_count="$(wc -l < "$present_tmp" | tr -d ' ')"
unlisted="$(comm -23 "$present_tmp" "$allow_tmp")"
stale="$(comm -13 "$present_tmp" "$allow_tmp")"

status=0
if [ -n "$unlisted" ]; then
  status=1
  echo "FAIL: unapproved fixture category directory not in the allowlist (Issue #9671):"
  printf '%s\n' "$unlisted" | sed 's#^#  - subset_julia_vm/tests/fixtures/#; s#$#/#'
  echo ""
  echo "  Prefer an EXISTING category over a near-synonym (e.g. use \`array\`, not"
  echo "  \`arrays\`; \`macros\`, not \`macro\`). If a genuinely new category is needed,"
  echo "  add its name to docs/vm/FIXTURE_CATEGORIES.tsv in the same PR."
fi

if [ -n "$stale" ]; then
  echo "NOTE: allowlist entries with no matching fixtures/ directory (safe to remove):"
  printf '%s\n' "$stale" | sed 's/^/  - /'
fi

if [ "$status" -eq 0 ]; then
  echo "OK: $present_count fixture categories, all allowlisted (Issue #9671)."
fi
exit "$status"
