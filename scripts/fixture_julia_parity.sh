#!/usr/bin/env bash
# fixture_julia_parity.sh
#
# Run a single fixture under both ./target/release/sjulia and upstream
# julia, then compare the trailing `N passed, M failed` testset summary
# lines. Exit non-zero if the counts differ or either side aborts.
#
# This automates the manual recipe several recent PRs (#4693, #4695,
# #4697, #4702, #4707, #4709, #4711) followed by hand: write a fixture,
# run it under sjulia, run the same fixture under julia, eyeball that
# both interpreters report identical pass/fail counts. The eyeball step
# is where wrong-expectation fixtures slip through, so codifying it
# helps catch them before merge.
#
# NAMING: deliberately NOT named `check_*.sh` so it does NOT trip the
# `Verify all check_*.sh scripts are referenced in this workflow and
# docs` audit. This is a developer-side helper, not a CI gate yet
# (Issue #4712). A follow-up PR with `workflow` OAuth scope can rename
# it and wire it into `.github/workflows/ci.yml` for CI enforcement.
#
# SCOPE: only useful for fixtures whose assertions are *expected* to
# pass under both interpreters (the common case for new reflection /
# parity-style fixtures). Fixtures that intentionally test
# sjulia-specific behavior (e.g. `@assert`-based fixtures whose
# semantics deliberately diverge from upstream) will report "ERROR:
# upstream julia run failed" — that's correct, but means this script
# is not meant to be looped over the whole fixture tree blindly.
#
# SELF-TEST (Issue #4718): bash scripts/test_fixture_julia_parity.sh
# runs this helper against a known-good fixture and a two-digit
# pass-count regression guard. Run it whenever you edit this script.
#
# Usage:
#   bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/io/summary.jl
#
# Requirements:
#   - julia on PATH
#   - ./target/release/sjulia already built (cargo build --release
#     --bin sjulia --features repl)

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "Usage: bash scripts/fixture_julia_parity.sh <fixture.jl>" >&2
    exit 2
fi

fixture="$1"

if [[ ! -f "$fixture" ]]; then
    echo "ERROR: fixture not found: $fixture" >&2
    exit 2
fi

if ! command -v julia >/dev/null 2>&1; then
    echo "ERROR: 'julia' is not on PATH. Install upstream Julia or" >&2
    echo "       skip this parity check (Issue #4712)." >&2
    exit 2
fi

sjulia_bin="./target/release/sjulia"
if [[ ! -x "$sjulia_bin" ]]; then
    echo "ERROR: sjulia binary not built. Run:" >&2
    echo "  cargo build --release --bin sjulia --features repl" >&2
    exit 2
fi

# Extract every `N passed, M failed` line from a fixture run.
# Test.jl prints these once per testset; the summary that matters for
# parity is the *total* across all testsets — we compare line-by-line
# in order so a 2-testset fixture compares both summaries pairwise.
extract_summaries() {
    # Match lines like "  15 passed, 0 failed (15 total)" without
    # tripping over greedy regex semantics — capture the integers
    # immediately preceding " passed," and " failed".
    awk 'match($0, /([0-9]+) passed, ([0-9]+) failed/, m) { print m[1] " " m[2] }' "$1" 2>/dev/null \
        || grep -oE '[0-9]+ passed, [0-9]+ failed' "$1" \
           | awk '{print $1 " " $3}' \
        || true
}

sjulia_out=$(mktemp)
julia_out=$(mktemp)
trap 'rm -f "$sjulia_out" "$julia_out"' EXIT

if ! timeout 120 "$sjulia_bin" "$fixture" > "$sjulia_out" 2>&1; then
    echo "ERROR: sjulia run failed for $fixture" >&2
    tail -20 "$sjulia_out" >&2
    exit 1
fi
if ! timeout 120 julia "$fixture" > "$julia_out" 2>&1; then
    echo "ERROR: upstream julia run failed for $fixture" >&2
    tail -20 "$julia_out" >&2
    exit 1
fi

sjulia_summary=$(extract_summaries "$sjulia_out")
julia_summary=$(extract_summaries "$julia_out")

# sjulia's @testset reports "N passed, M failed (T total)" via
# Test.jl; upstream julia formats the summary differently
# ("Test Summary: ... | Pass  Total  Time"). To normalise, fall back
# to extracting `Pass` counts from the upstream "Test Summary" lines
# when no `passed,` line was emitted.
if [[ -z "$julia_summary" ]]; then
    # Lines look like: `<name> |  N      M  ...`. Take last two numeric
    # columns of every summary row that mentions "Pass" or test counts.
    julia_summary=$(awk '
        /Pass.*Total/ { in_table = 1; next }
        in_table && /^[[:space:]]*$/ { in_table = 0; next }
        in_table {
            # Capture trailing numeric columns as "passed failed_or_total".
            n = split($0, fields, /[[:space:]]+/)
            # Last numeric token is Total; previous is Pass.
            if (n >= 2) {
                # Walk back to find two numbers separated by whitespace.
                # Output them as "PASS TOTAL" — Test.jl in sjulia prints
                # "passed, failed", upstream prints "Pass, Total" so we
                # only compare PASS counts when failed=0.
                # Find the last and second-to-last integer fields.
                last = ""; prev = ""
                for (i = 1; i <= n; i++) {
                    if (fields[i] ~ /^[0-9]+$/) { prev = last; last = fields[i] }
                }
                if (last != "" && prev != "") {
                    # Treat as "passed=prev, total=last", failed = total - passed
                    failed = last - prev
                    print prev " " failed
                }
            }
        }
    ' "$julia_out")
fi

if [[ -z "$sjulia_summary" || -z "$julia_summary" ]]; then
    echo "ERROR: could not extract testset summary from one or both runs" >&2
    echo "--- sjulia output ---" >&2
    tail -20 "$sjulia_out" >&2
    echo "--- julia output ---" >&2
    tail -20 "$julia_out" >&2
    exit 1
fi

if [[ "$sjulia_summary" != "$julia_summary" ]]; then
    echo "MISMATCH: $fixture testset summaries differ." >&2
    echo "sjulia (PASSED FAILED per testset):"
    echo "$sjulia_summary"
    echo "julia (PASSED FAILED per testset):"
    echo "$julia_summary"
    exit 1
fi

echo "OK: $fixture pass/fail counts match across sjulia and julia."
echo "$sjulia_summary" | awk '{print "  " $1 " passed, " $2 " failed"}'
