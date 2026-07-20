#!/usr/bin/env bash
# fixture_julia_parity.sh
#
# Run a single fixture under both $CARGO_TARGET_DIR/release/sjulia and upstream
# julia, then compare the trailing `N passed, M failed` testset summary
# lines. For legacy fixtures that return a bare boolean instead of a
# Test.jl summary, run a temporary `println(begin ... end)` wrapper on
# both sides and compare the printed final value to the manifest's
# `expected` value. Exit non-zero if the observed results differ or
# either side aborts.
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
#   bash scripts/fixture_julia_parity.sh [--strict] subset_julia_vm/tests/fixtures/io/summary.jl
#
# The upstream julia used for comparison is version-checked against the
# repository-root PARITY_TARGET file via scripts/parity_julia_version.sh
# (Issue #8644 / #8667): a PATH julia outside the target series produces a
# WARNING (and a juliaup `julia +X.Y` channel is auto-selected when
# available); with --strict (or SJULIA_PARITY_STRICT=1) a mismatch is a
# hard error instead.
#
# Requirements:
#   - julia on PATH (target series — see docs/vm/PARITY_TARGET.md)
#   - $CARGO_TARGET_DIR/release/sjulia already built (cargo build --release
#     --bin sjulia --features repl)

set -euo pipefail

strict_flag=""
red_green=""
while [[ "${1:-}" == --* ]]; do
    case "$1" in
        --strict)
            strict_flag="--strict"
            shift
            ;;
        # Red/green outcome mode (Issue #10246): compare only whether each
        # interpreter runs the fixture green (exit 0), plus the wrapped
        # final value for fixtures without any Test.jl summary. Skips the
        # per-testset pass-count comparison, which is ill-defined for nested
        # @testsets until sjulia aggregates outer-testset counts like
        # upstream (Issue #10338). Used by scripts/check_fixture_parity_sweep.sh.
        --red-green)
            red_green=1
            shift
            ;;
        *)
            echo "Usage: bash scripts/fixture_julia_parity.sh [--strict] [--red-green] <fixture.jl>" >&2
            exit 2
            ;;
    esac
done

if [[ $# -ne 1 ]]; then
    echo "Usage: bash scripts/fixture_julia_parity.sh [--strict] [--red-green] <fixture.jl>" >&2
    exit 2
fi

fixture="$1"

if [[ ! -f "$fixture" ]]; then
    echo "ERROR: fixture not found: $fixture" >&2
    exit 2
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$script_dir/.." && pwd)"
source "$script_dir/cargo_target_dir.sh"
cargo_target_dir="$(resolve_cargo_target_dir "$ROOT")"
export CARGO_TARGET_DIR="$cargo_target_dir"
# May be two words (e.g. "julia +1.12"); expand unquoted on purpose.
JULIA_CMD="$(bash "$script_dir/parity_julia_version.sh" $strict_flag)"

# SJULIA_BIN override lets CI point at a non-default profile build,
# e.g. target/release-fast/sjulia in the pr-fast workflow (Issue #8632).
SJULIA_BIN="${SJULIA_BIN:-$cargo_target_dir/release/sjulia}"
export SJULIA_BIN
if [[ ! -x "$SJULIA_BIN" ]]; then
    echo "ERROR: sjulia binary not built ($SJULIA_BIN). Run:" >&2
    echo "  cargo build --release --bin sjulia --features repl" >&2
    echo "  (or set SJULIA_BIN to an existing sjulia binary)" >&2
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

extract_last_nonempty_line() {
    awk 'NF { line = $0 } END { if (line != "") print line }' "$1"
}

manifest_expected_for_fixture() {
    local fixture_path="$1"
    local fixture_file
    local manifest
    fixture_file="$(basename "$fixture_path")"
    manifest="$(dirname "$fixture_path")/manifest.toml"

    if [[ ! -f "$manifest" ]]; then
        return 1
    fi

    awk -v target="$fixture_file" '
        /^\[\[tests\]\]/ {
            in_block = 1
            file_matches = 0
            expected = ""
            next
        }
        in_block && /^file[[:space:]]*=/ {
            file = $0
            sub(/^[^=]*=[[:space:]]*"/, "", file)
            sub(/".*$/, "", file)
            if (file == target) {
                file_matches = 1
            }
            next
        }
        in_block && /^expected[[:space:]]*=/ {
            expected = $0
            sub(/^[^=]*=[[:space:]]*/, "", expected)
            sub(/[[:space:]]*#.*/, "", expected)
            next
        }
        in_block && file_matches && expected != "" {
            print expected
            exit
        }
    ' "$manifest"
}

# Final-value wrapper for legacy fixtures without a Test.jl summary.
#
# Strategy (Issue #10246): keep the fixture verbatim up to the START of its
# final top-level expression, then wrap that whole expression — the last
# column-0 non-comment line that does not begin with a closer token, through
# EOF — in a multi-line `println(begin ... end)`. The fixture rules require
# the file to end with the expression producing the manifest `expected` value,
# and this shape handles uniformly:
#   * top-level `struct` / `mutable struct` / `function` / `macro` / `module`
#     definitions earlier in the file (they stay at top level — they are
#     illegal inside a `println(begin ... end)` argument in BOTH interpreters;
#     an `include()`-based wrapper would also keep them top-level upstream,
#     but sjulia's eval path cannot define `mutable struct` yet, Issue #10329);
#   * a final ASSIGNMENT line (`result = a && b` — naive `println(result = …)`
#     would parse as a println keyword argument upstream);
#   * multi-line final expressions (`a &&\n    b`, trailing-comma braces);
#   * trailing `# comments` (legal inside the begin block).
#
# Fallback: when no column-0 expression starter is found, use the historical
# whole-file `println(begin ... end)` wrapper with using/import hoisted.
write_result_wrapper() {
    local fixture_path="$1"
    local wrapper_path="$2"

    if awk '
        { lines[NR] = $0 }
        END {
            start = 0
            for (i = NR; i >= 1; i--) {
                line = lines[i]
                if (line ~ /^[[:space:]]*(#|$)/) continue   # blank / comment-only
                if (line ~ /^[[:space:]]/) continue          # indented: continuation
                # Column-0 closer/continuation tokens cannot start an expression;
                # keep walking back to the line that opens the final expression.
                if (line ~ /^(end|else|elseif|catch|finally|\)|\]|\})/) continue
                start = i
                break
            }
            if (start == 0) exit 1
            for (i = 1; i < start; i++) print lines[i]
            print "println(begin"
            for (i = start; i <= NR; i++) print lines[i]
            print "end)"
        }
    ' "$fixture_path" > "$wrapper_path"; then
        return 0
    fi

    {
        awk '/^(using|import)[[:space:]]/ { print }' "$fixture_path"
        printf 'println(begin\n'
        awk '!/^(using|import)[[:space:]]/ { print "    " $0 }' "$fixture_path"
        printf '\nend)\n'
    } > "$wrapper_path"
}

run_wrapped_value() {
    local runner="$1"
    local wrapper_path="$2"
    local out_path="$3"

    if [[ "$runner" == "julia" ]]; then
        # shellcheck disable=SC2086 # JULIA_CMD may carry a juliaup channel arg
        timeout 120 $JULIA_CMD --startup-file=no "$wrapper_path" > "$out_path" 2>&1
    else
        timeout 120 "$SJULIA_BIN" "$wrapper_path" > "$out_path" 2>&1
    fi
}

sjulia_out=$(mktemp)
julia_out=$(mktemp)
trap 'rm -f "$sjulia_out" "$julia_out"' EXIT

if ! timeout 120 "$SJULIA_BIN" "$fixture" > "$sjulia_out" 2>&1; then
    echo "ERROR: sjulia run failed for $fixture" >&2
    tail -20 "$sjulia_out" >&2
    exit 1
fi
# shellcheck disable=SC2086 # JULIA_CMD may carry a juliaup channel arg
if ! timeout 120 $JULIA_CMD "$fixture" > "$julia_out" 2>&1; then
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
        # Upstream table values are right-aligned so each value ENDS at its
        # header name'"'"'s end column. Read the Pass/Fail/Error/Broken cells
        # by that alignment instead of "last two numeric fields", which
        # misreads tables that carry a Broken (or Fail/Error) column — e.g.
        # a @test_skip testset prints "Pass Broken Total" and the old walk
        # reported passed=Broken, failed=Total-Broken (Issue #10350).
        function col_at(rest, ep,    pre, m) {
            if (ep == 0) return 0
            pre = substr(rest, 1, ep)
            if (pre ~ /[0-9]$/) {
                m = pre
                sub(/^.*[^0-9]/, "", m)
                return m + 0
            }
            return 0
        }
        /Pass.*Total/ {
            # Column ends are measured RELATIVE to the header pipe: the
            # testset-name column to its left may contain multibyte
            # characters (e.g. a unicode operator in a testset name), which
            # shift byte-based absolute positions while the rendered table
            # stays aligned (Issue #10695).
            in_table = 1
            hp = index($0, "|")
            hrest = substr($0, hp)
            pass_end = index(hrest, "Pass") + 3
            fail_end = index(hrest, "Fail") ? index(hrest, "Fail") + 3 : 0
            err_end = index(hrest, "Error") ? index(hrest, "Error") + 4 : 0
            total_end = index(hrest, "Total") + 4
            next
        }
        in_table && /^[[:space:]]*$/ { in_table = 0; next }
        in_table {
            dp = index($0, "|")
            if (dp == 0) { in_table = 0; next }
            rest = substr($0, dp)
            # A real summary row always fills its Total cell; anything else
            # (trailing program output after the table) ends the table.
            if (col_at(rest, total_end) == 0) { in_table = 0; next }
            passed = col_at(rest, pass_end)
            failed = col_at(rest, fail_end) + col_at(rest, err_end)
            print passed " " failed
        }
    ' "$julia_out")
fi

# Red/green mode: both direct runs already exited 0. A @testset-based fixture
# must produce a Test.jl summary under BOTH interpreters. If exactly one side
# emits a summary, the interpreters diverge on whether the testset ran at all
# (e.g. sjulia exits 0 without executing the @testset while julia runs it) —
# that is a real parity failure, NOT a pass. Requiring summaries from BOTH sides
# is the whole point of the parity lane; accepting "either side has a summary"
# would let such a divergence through. Only fixtures where NEITHER side prints a
# summary (legacy final-value fixtures) fall through to the wrapped final-value
# comparison below.
if [[ -n "$red_green" ]]; then
    if [[ -n "$sjulia_summary" && -n "$julia_summary" ]]; then
        echo "OK: $fixture is green under both sjulia and julia (--red-green mode; per-testset counts not compared, Issue #10338)."
        exit 0
    elif [[ -n "$sjulia_summary" || -n "$julia_summary" ]]; then
        echo "MISMATCH: $fixture produced a Test.jl summary under only ONE interpreter (--red-green mode)." >&2
        if [[ -n "$sjulia_summary" ]]; then
            echo "  sjulia emitted a testset summary but upstream julia did not." >&2
        else
            echo "  upstream julia emitted a testset summary but sjulia did not (sjulia exited 0 without running the @testset)." >&2
        fi
        echo "--- sjulia output (tail) ---" >&2
        tail -20 "$sjulia_out" >&2
        echo "--- julia output (tail) ---" >&2
        tail -20 "$julia_out" >&2
        exit 1
    fi
    # Neither side printed a summary: fall through to the final-value comparison.
fi

if [[ -z "$sjulia_summary" && -z "$julia_summary" ]]; then
    expected="$(manifest_expected_for_fixture "$fixture")"
    if [[ -z "$expected" ]]; then
        echo "ERROR: no Test.jl summary and no manifest expected value for $fixture" >&2
        echo "--- sjulia output ---" >&2
        tail -20 "$sjulia_out" >&2
        echo "--- julia output ---" >&2
        tail -20 "$julia_out" >&2
        exit 1
    fi

    sjulia_value_out=$(mktemp)
    julia_value_out=$(mktemp)
    wrapper="$(mktemp "$(dirname "$fixture")/.fixture_parity_value_XXXXXX.jl")"
    write_result_wrapper "$fixture" "$wrapper"
    trap 'rm -f "$sjulia_out" "$julia_out" "$sjulia_value_out" "$julia_value_out" "$wrapper"' EXIT

    if ! run_wrapped_value sjulia "$wrapper" "$sjulia_value_out"; then
        echo "ERROR: sjulia final-value wrapper run failed for $fixture" >&2
        tail -20 "$sjulia_value_out" >&2
        exit 1
    fi
    if ! run_wrapped_value julia "$wrapper" "$julia_value_out"; then
        echo "ERROR: upstream julia final-value wrapper run failed for $fixture" >&2
        tail -20 "$julia_value_out" >&2
        exit 1
    fi

    sjulia_value="$(extract_last_nonempty_line "$sjulia_value_out")"
    julia_value="$(extract_last_nonempty_line "$julia_value_out")"

    # The manifest may store a numeric expected value in a different textual
    # representation than println emits (e.g. `expected = 600.0` for a fixture
    # whose final value prints as `600` — the Rust harness matches those with
    # Expected::Float-vs-I64 epsilon logic). Accept numerically-equal values.
    values_equal() {
        local a="$1" b="$2"
        [[ "$a" == "$b" ]] && return 0
        awk -v a="$a" -v b="$b" 'BEGIN {
            num = "^-?[0-9]+([.][0-9]+)?([eE][+-]?[0-9]+)?$"
            if (a ~ num && b ~ num && a + 0 == b + 0) exit 0
            exit 1
        }'
    }

    if ! values_equal "$sjulia_value" "$julia_value" || ! values_equal "$sjulia_value" "$expected"; then
        echo "MISMATCH: $fixture wrapped final value differs." >&2
        echo "sjulia final value: $sjulia_value"
        echo "julia final value:  $julia_value"
        echo "manifest expected:  $expected"
        exit 1
    fi

    echo "OK: $fixture wrapped final value matches across sjulia and julia."
    echo "  final value: $sjulia_value"
    echo "  manifest expected: $expected"
    exit 0
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
