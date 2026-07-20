#!/usr/bin/env bash
# fixture_timing_report.sh — Issue #9671 Phase 0 (measurement baseline)
#
# Produce a timing baseline for the fixture / integration test suite so the
# Issue #9671 compaction phases can report before/after numbers on real data:
#
#   (a) slowest-N individual tests (binary::test, with wall-clock seconds)
#   (b) per-binary total test wall-clock (which binaries dominate)
#   (c) per-category fixture totals (from the fixture journal, if present)
#
# It parses the human-readable `cargo nextest run` output, where each finished
# test prints `<STATUS> [  <secs>s] (<i>/<n>) <binary> <test>`. Point it at a
# saved run, or let it run one:
#
#   # Parse an existing captured run:
#   bash scripts/fixture_timing_report.sh --log /tmp/nextest.out
#
#   # Run the fixture suite (release) and report, also recording the journal:
#   bash scripts/fixture_timing_report.sh --run
#
#   # Options: --top N (default 25); --journal <file> to correlate chunk→fixtures
#             (defaults to $SJULIA_FIXTURE_JOURNAL or a temp file when --run).
#
# Exit code: 0 on success; non-zero if neither --log nor --run yields output.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

TOP=25
LOG=""
DO_RUN=0
JOURNAL="${SJULIA_FIXTURE_JOURNAL:-}"

while [ $# -gt 0 ]; do
  case "$1" in
    --top) TOP="$2"; shift 2 ;;
    --log) LOG="$2"; shift 2 ;;
    --run) DO_RUN=1; shift ;;
    --journal) JOURNAL="$2"; shift 2 ;;
    -h|--help) sed -n '2,26p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [ "$DO_RUN" -eq 1 ]; then
  LOG="$(mktemp)"
  if [ -z "$JOURNAL" ]; then JOURNAL="$(mktemp)"; fi
  echo "Running fixture suite (release); log=$LOG journal=$JOURNAL ..." >&2
  SJULIA_FIXTURE_JOURNAL="$JOURNAL" \
    timeout 1800 cargo nextest run --release --test fixture_tests 2>&1 | tee "$LOG" >&2 || true
fi

if [ -z "$LOG" ] || [ ! -s "$LOG" ]; then
  echo "ERROR: no timing input. Pass --log <file> or --run." >&2
  exit 1
fi

# Extract `<secs>\t<binary>\t<test>` from nextest status lines.
# Line shape: "        PASS [  11.066s] ( 3/15) <binary> <test>"
parsed="$(mktemp)"
trap 'rm -f "$parsed"' EXIT
awk '
  # Only per-test status lines: they carry BOTH a [<secs>s] bracket and an
  # "(i/n)" counter. The Summary line has the bracket but no counter.
  /\([[:space:]]*[0-9]+\/[0-9]+\)/ && match($0, /\[[[:space:]]*[0-9]+\.[0-9]+s\]/) {
    secs = substr($0, RSTART, RLENGTH); gsub(/[^0-9.]/, "", secs)
    rest = $0; sub(/.*\([[:space:]]*[0-9]+\/[0-9]+\)[[:space:]]+/, "", rest)
    n = split(rest, a, /[[:space:]]+/)
    if (n >= 2) print secs "\t" a[1] "\t" a[2]
  }
' "$LOG" > "$parsed"

if [ ! -s "$parsed" ]; then
  echo "ERROR: no timing lines parsed from $LOG (is it nextest output?)." >&2
  exit 1
fi

total_tests="$(wc -l < "$parsed" | tr -d ' ')"
echo "=== Timing report (Issue #9671 Phase 0) ==="
echo "parsed $total_tests timed tests from $LOG"
echo ""

echo "=== (a) slowest $TOP tests (seconds) ==="
sort -t"$(printf '\t')" -k1 -gr "$parsed" | head -n "$TOP" | \
  awk -F'\t' '{ printf "  %8.3fs  %s %s\n", $1, $2, $3 }'
echo ""

echo "=== (b) per-binary total wall-clock (seconds) ==="
awk -F'\t' '{ sum[$2]+=$1; cnt[$2]++ } END {
  for (b in sum) printf "%.3f\t%d\t%s\n", sum[b], cnt[b], b
}' "$parsed" | sort -gr | \
  awk -F'\t' '{ printf "  %9.3fs  %5d tests  %s\n", $1, $2, $3 }'
echo ""

if [ -n "$JOURNAL" ] && [ -s "$JOURNAL" ]; then
  echo "=== (c) per-category fixture counts (from journal $JOURNAL) ==="
  # Journal lines contain executed fixture paths .../fixtures/<category>/<file>.jl
  grep -oE 'fixtures/[^/]+/' "$JOURNAL" 2>/dev/null | sed 's#fixtures/##; s#/##' | \
    sort | uniq -c | sort -rn | head -n "$TOP" | \
    awk '{ printf "  %6d fixtures  %s\n", $1, $2 }'
else
  echo "=== (c) per-category fixture counts ==="
  echo "  (no fixture journal; set SJULIA_FIXTURE_JOURNAL or pass --journal / --run)"
fi
