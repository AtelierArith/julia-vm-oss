#!/usr/bin/env bash
# check_generator_trait_matrix.sh
#
# Ratchet the generator/iterator trait matrix added for Issue #9566:
#   - the oracle/fixture must be regenerated from the upstream-only generator;
#   - every skiplist id must exist in the oracle and link to an Issue; and
#   - the skiplist must not grow beyond the current residual count.

set -euo pipefail
cd "$(dirname "$0")/.."

ORACLE="${ORACLE:-subset_julia_vm/tests/fixtures/generator/generator_trait_matrix_9566.tsv}"
FIXTURE="${FIXTURE:-subset_julia_vm/tests/fixtures/generator/generator_trait_matrix_9566.jl}"
SKIPLIST="${SKIPLIST:-docs/vm/GENERATOR_TRAIT_MATRIX_SKIPLIST.tsv}"
GENERATOR="${GENERATOR:-scripts/gen_generator_trait_matrix_fixture.jl}"

EXPECTED_SKIPLIST_HEADER=$'id\tissue\tclassification\treason'
EXPECTED_ORACLE_HEADER=$'id\tcategory\ttransform\tbase\tconsumer\texpr\tstatus\tresult_type\tresult_repr\texception_type\tissue_refs'
MIN_MATRIX_ROWS=152
MAX_SKIPLIST_ROWS=0

for path in "$ORACLE" "$FIXTURE" "$SKIPLIST" "$GENERATOR"; do
  if [ ! -f "$path" ]; then
    echo "ERROR: generator trait matrix file not found: $path" >&2
    exit 1
  fi
done

skip_header="$(sed -n '1p' "$SKIPLIST")"
if [ "$skip_header" != "$EXPECTED_SKIPLIST_HEADER" ]; then
  echo "ERROR: $SKIPLIST has an unexpected header" >&2
  echo "expected: $EXPECTED_SKIPLIST_HEADER" >&2
  echo "actual:   $skip_header" >&2
  exit 1
fi

oracle_header="$(sed -n '1p' "$ORACLE")"
if [ "$oracle_header" != "$EXPECTED_ORACLE_HEADER" ]; then
  echo "ERROR: $ORACLE has an unexpected header" >&2
  echo "expected: $EXPECTED_ORACLE_HEADER" >&2
  echo "actual:   $oracle_header" >&2
  exit 1
fi

matrix_rows="$(
  awk '
    NR == 1 { next }
    /^[[:space:]]*$/ { next }
    /^#/ { next }
    { count += 1 }
    END { print count + 0 }
  ' "$ORACLE"
)"

if [ "$matrix_rows" -lt "$MIN_MATRIX_ROWS" ]; then
  echo "ERROR: generator trait matrix row count shrank: $matrix_rows < $MIN_MATRIX_ROWS (Issue #9566)." >&2
  echo "Do not remove coverage cells without replacing them with equal or broader matrix coverage." >&2
  exit 1
fi

skip_rows="$(
  awk '
    NR == 1 { next }
    /^[[:space:]]*$/ { next }
    /^#/ { next }
    { count += 1 }
    END { print count + 0 }
  ' "$SKIPLIST"
)"

if [ "$skip_rows" -gt "$MAX_SKIPLIST_ROWS" ]; then
  echo "ERROR: generator trait matrix skiplist grew: $skip_rows > $MAX_SKIPLIST_ROWS (Issue #9566)." >&2
  echo "Fix the regression, link a new Issue and intentionally update this ratchet, or remove newly-fixed rows before regenerating." >&2
  exit 1
fi

awk -F '\t' '
  NR == FNR {
    if (FNR > 1 && $1 != "") {
      known[$1] = 1
    }
    next
  }
  FNR == 1 { next }
  /^[[:space:]]*$/ { next }
  /^#/ { next }
  NF < 4 {
    print "ERROR: malformed generator trait matrix skiplist row " FNR ": expected 4 tab-separated columns" > "/dev/stderr"
    errors += 1
    next
  }
  !($1 in known) {
    print "ERROR: generator trait matrix skiplist references unknown id " $1 > "/dev/stderr"
    errors += 1
  }
  seen[$1]++ {
    print "ERROR: duplicate generator trait matrix skiplist id " $1 > "/dev/stderr"
    errors += 1
  }
  $2 !~ /^[0-9]+$/ {
    print "ERROR: generator trait matrix skiplist id " $1 " has non-numeric issue " $2 > "/dev/stderr"
    errors += 1
  }
  END { exit errors ? 1 : 0 }
' "$ORACLE" "$SKIPLIST"

if ! command -v julia >/dev/null 2>&1; then
  echo "ERROR: julia not found; cannot regenerate generator trait matrix oracle (Issue #9566)." >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
cleanup() { rm -rf "$tmp_dir"; }
trap cleanup EXIT

tmp_oracle="$tmp_dir/generator_trait_matrix_9566.tsv"
tmp_fixture="$tmp_dir/generator_trait_matrix_9566.jl"

julia --startup-file=no "$GENERATOR" \
  --out-tsv "$tmp_oracle" \
  --out-fixture "$tmp_fixture" \
  --skiplist "$SKIPLIST" >/dev/null

if ! cmp -s "$ORACLE" "$tmp_oracle"; then
  echo "ERROR: generator trait matrix oracle is stale (Issue #9566)." >&2
  echo "Regenerate with: julia --startup-file=no $GENERATOR" >&2
  diff -u "$ORACLE" "$tmp_oracle" >&2 || true
  exit 1
fi

if ! cmp -s "$FIXTURE" "$tmp_fixture"; then
  echo "ERROR: generator trait matrix fixture is stale (Issue #9566)." >&2
  echo "Regenerate with: julia --startup-file=no $GENERATOR" >&2
  diff -u "$FIXTURE" "$tmp_fixture" >&2 || true
  exit 1
fi

asserted_rows=$((matrix_rows - skip_rows))
echo "OK: generator trait matrix is current ($matrix_rows cells, $skip_rows skiplisted, $asserted_rows asserted)."
