#!/usr/bin/env bash
# check_numeric_matrix_full_allowlist.sh — fail if full numeric matrix residuals are re-allowlisted.
# Issue #9849: milestone 62 reached a zero-residual full matrix allowlist, so
# adding a new row must be treated as a regression and handled in a dedicated
# Issue instead of silently repopulating the allowlist.

set -euo pipefail
cd "$(dirname "$0")/.."

ALLOWLIST="${ALLOWLIST:-docs/vm/NUMERIC_MATRIX_FULL_ALLOWLIST.tsv}"
EXPECTED_HEADER=$'family\tclassification\tissue\texpected_count\treason'

if [ ! -f "$ALLOWLIST" ]; then
  echo "ERROR: numeric matrix full allowlist not found: $ALLOWLIST" >&2
  exit 1
fi

header="$(sed -n '1p' "$ALLOWLIST")"
if [ "$header" != "$EXPECTED_HEADER" ]; then
  echo "ERROR: $ALLOWLIST has an unexpected header" >&2
  echo "expected: $EXPECTED_HEADER" >&2
  echo "actual:   $header" >&2
  exit 1
fi

residual_rows="$(
  awk '
    NR == 1 { next }
    /^[[:space:]]*$/ { next }
    /^#/ { next }
    { print NR "\t" $0 }
  ' "$ALLOWLIST"
)"

if [ -n "$residual_rows" ]; then
  echo "ERROR: numeric matrix full allowlist has non-header rows (Issue #9849)." >&2
  echo "The milestone 62 full numeric matrix state is zero residuals; fix the regression or file/link a new Issue before changing this ratchet." >&2
  printf '%s\n' "$residual_rows" >&2
  exit 1
fi

echo "OK: numeric matrix full allowlist is header-only"
