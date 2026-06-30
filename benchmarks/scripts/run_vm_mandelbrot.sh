#!/usr/bin/env bash
# VM-only Mandelbrot benchmark runner (Issue #4301).
# Compares official Julia process execution with target/release/sjulia.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BENCH_FILE="${BENCH_FILE:-$ROOT_DIR/benchmarks/vm_mandelbrot.jl}"
SJULIA_BIN="${SJULIA_BIN:-$ROOT_DIR/target/release/sjulia}"
RUNS="${RUNS:-5}"
RESULTS_DIR="${RESULTS_DIR:-$ROOT_DIR/benchmarks/results/vm_mandelbrot_$(date +%Y%m%d_%H%M%S)}"

mkdir -p "$RESULTS_DIR"

if [[ ! -x "$SJULIA_BIN" ]]; then
    echo "sjulia binary not found: $SJULIA_BIN" >&2
    echo "Run: cargo build --release --bin sjulia --features repl" >&2
    exit 1
fi

if ! command -v julia >/dev/null 2>&1; then
    echo "julia not found on PATH" >&2
    exit 1
fi

run_timed() {
    local label="$1"
    shift
    local out_file="$RESULTS_DIR/${label}.out"
    local time_file="$RESULTS_DIR/${label}.time"
    /usr/bin/time -p "$@" >"$out_file" 2>"$time_file"
    awk '/^real / { print $2 }' "$time_file"
}

record_series() {
    local label="$1"
    shift
    local series_file="$RESULTS_DIR/${label}_times.txt"
    : >"$series_file"
    for i in $(seq 1 "$RUNS"); do
        local seconds
        seconds="$(run_timed "${label}_${i}" "$@")"
        echo "$seconds" >>"$series_file"
        printf "%s run %s: %ss\n" "$label" "$i" "$seconds"
    done
}

summarize_series() {
    local display_label="$1"
    local safe_label="$2"
    local file="$3"
    local sorted_file="$RESULTS_DIR/${safe_label}_times.sorted"
    sort -n "$file" >"$sorted_file"
    awk -v label="$display_label" '
        { vals[++n] = $1; sum += $1; if (n == 1 || $1 < min) min = $1; if (n == 1 || $1 > max) max = $1 }
        END {
            if (n > 0) {
                median = vals[int((n + 1) / 2)]
                printf("- %s: min %.6fs, median %.6fs, avg %.6fs, max %.6fs\n", label, min, median, sum / n, max)
            }
        }
    ' "$sorted_file"
}

echo "Benchmark file: $BENCH_FILE"
echo "Runs: $RUNS"
echo "Results: $RESULTS_DIR"
echo

julia --startup-file=no --history-file=no "$BENCH_FILE" >"$RESULTS_DIR/julia_check.out"
"$SJULIA_BIN" "$BENCH_FILE" >"$RESULTS_DIR/sjulia_check.out"
SJULIA_VM_PROFILE=1 "$SJULIA_BIN" "$BENCH_FILE" \
    >"$RESULTS_DIR/sjulia_profile.out" \
    2>"$RESULTS_DIR/sjulia_profile.err"

if ! cmp -s "$RESULTS_DIR/julia_check.out" "$RESULTS_DIR/sjulia_check.out"; then
    echo "result mismatch" >&2
    echo "Julia:" >&2
    cat "$RESULTS_DIR/julia_check.out" >&2
    echo "sjulia:" >&2
    cat "$RESULTS_DIR/sjulia_check.out" >&2
    exit 1
fi

if ! cmp -s "$RESULTS_DIR/julia_check.out" "$RESULTS_DIR/sjulia_profile.out"; then
    echo "profiled sjulia result mismatch" >&2
    echo "Julia:" >&2
    cat "$RESULTS_DIR/julia_check.out" >&2
    echo "profiled sjulia:" >&2
    cat "$RESULTS_DIR/sjulia_profile.out" >&2
    exit 1
fi

EXPECTED_RESULT="$(tr -d '\n' < "$RESULTS_DIR/julia_check.out")"
echo "Result: $EXPECTED_RESULT"
echo

record_series julia julia --startup-file=no --history-file=no "$BENCH_FILE"
record_series sjulia "$SJULIA_BIN" "$BENCH_FILE"

{
    echo "# VM Mandelbrot Benchmark"
    echo
    echo "- Benchmark: \`$BENCH_FILE\`"
    echo "- Result: \`$EXPECTED_RESULT\`"
    echo "- Runs: $RUNS"
    echo
    echo "## Raw Times (seconds)"
    echo
    echo "### Julia"
    cat "$RESULTS_DIR/julia_times.txt"
    echo
    echo "### sjulia VM"
    cat "$RESULTS_DIR/sjulia_times.txt"
    echo
    echo "## Summary"
    echo
    summarize_series "Julia" "julia" "$RESULTS_DIR/julia_times.txt"
    summarize_series "sjulia VM" "sjulia" "$RESULTS_DIR/sjulia_times.txt"
    echo
    echo "## sjulia VM Instruction Profile"
    echo
    echo "Captured with a separate untimed \`SJULIA_VM_PROFILE=1\` run."
    echo
    echo '```text'
    cat "$RESULTS_DIR/sjulia_profile.err"
    echo '```'
} >"$RESULTS_DIR/report.md"

cat "$RESULTS_DIR/report.md"
