#!/usr/bin/env bash
# check_test_binary_budget.sh — Issue #9671 Phase 1
#
# Bound the number of integration-test binaries under subset_julia_vm/tests/*.rs.
#
# Each tests/<name>.rs is compiled as its own test binary, and each one links the
# full ~370k-line subset_julia_vm rlib. The per-binary link is the dominant term
# in full-suite build time (which is the ONLY merge gate — GitHub Actions is
# disabled). Left unchecked, the historical pattern of "one bug fix ≒ one new
# per-issue test binary" grows this set without bound (3,296 fixtures / 103
# binaries as of 2026-07-08, Issue #9671).
#
# This audit enforces an ALLOWLIST: every tests/*.rs binary must be named in
# docs/vm/TEST_BINARY_ALLOWLIST.tsv. Adding a NEW binary therefore requires an
# allowlist edit, which surfaces in review — the reviewer can ask "why not a
# `mod` inside an existing regression_*_tests.rs binary?" (see TESTING_GUIDE.md).
#
# Usage (from the repository root):
#   bash scripts/check_test_binary_budget.sh
#
# The scanned tests dir and allowlist can be overridden for testing:
#   SJULIA_TEST_BINARY_DIR=<dir> SJULIA_TEST_BINARY_ALLOWLIST=<file> bash scripts/check_test_binary_budget.sh
#
# Exit code: 0 = every present binary is allowlisted; 1 = an unlisted binary
#            exists (a new binary was added without updating the allowlist).

set -uo pipefail

# Resolve repo root from the script location so overrides can point elsewhere.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

TEST_DIR="${SJULIA_TEST_BINARY_DIR:-$REPO_ROOT/subset_julia_vm/tests}"
ALLOWLIST="${SJULIA_TEST_BINARY_ALLOWLIST:-$REPO_ROOT/docs/vm/TEST_BINARY_ALLOWLIST.tsv}"

if [ ! -f "$ALLOWLIST" ]; then
  echo "FAIL: test-binary allowlist not found: $ALLOWLIST (Issue #9671)"
  exit 1
fi

# Allowed basenames = non-comment, non-blank lines (first whitespace-token).
allow_tmp="$(mktemp)"
trap 'rm -f "$allow_tmp"' EXIT
grep -v '^[[:space:]]*#' "$ALLOWLIST" | awk 'NF { print $1 }' | sort -u > "$allow_tmp"

# Present binaries = subset_julia_vm/tests/*.rs (top-level only; subdirectories
# such as tests/common, tests/fixtures are module trees, not test binaries).
present_tmp="$(mktemp)"
trap 'rm -f "$allow_tmp" "$present_tmp"' EXIT
if [ -d "$TEST_DIR" ]; then
  for f in "$TEST_DIR"/*.rs; do
    [ -e "$f" ] || continue
    base="$(basename "$f")"
    printf '%s\n' "${base%.rs}"
  done | sort -u > "$present_tmp"
else
  : > "$present_tmp"
fi

present_count="$(wc -l < "$present_tmp" | tr -d ' ')"

# Unlisted = present but not allowlisted → a new unapproved test binary.
unlisted="$(comm -23 "$present_tmp" "$allow_tmp")"
# Stale = allowlisted but not present → reported (non-fatal) so the list can be
# tidied when binaries are consolidated/removed.
stale="$(comm -13 "$present_tmp" "$allow_tmp")"

status=0
if [ -n "$unlisted" ]; then
  status=1
  echo "FAIL: unapproved test binary not in the allowlist (Issue #9671):"
  printf '%s\n' "$unlisted" | sed 's/^/  - subset_julia_vm\/tests\//; s/$/.rs/'
  echo ""
  echo "  A new tests/*.rs binary links the full VM rlib and grows full-suite"
  echo "  build time. Prefer adding a \`mod\` to an existing consolidated binary"
  echo "  (regression_*_tests.rs, integration_tests.rs). If a separate binary is"
  echo "  genuinely required (process isolation / distinct required-features),"
  echo "  add its name to docs/vm/TEST_BINARY_ALLOWLIST.tsv in the same PR."
fi

if [ -n "$stale" ]; then
  echo "NOTE: allowlist entries with no matching tests/*.rs (safe to remove):"
  printf '%s\n' "$stale" | sed 's/^/  - /'
fi

if [ "$status" -eq 0 ]; then
  echo "OK: $present_count test binaries, all allowlisted (Issue #9671)."
fi
exit "$status"
