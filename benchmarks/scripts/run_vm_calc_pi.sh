#!/usr/bin/env bash
# VM calc_pi benchmark runner.
#
# `benchmarks/calc_pi_benchmark.jl` prints @time lines, so exact stdout differs
# between Julia and sjulia. This runner compares only the deterministic result
# lines (`N=...`) and records full process wall time for the whole script.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BENCH_FILE="${BENCH_FILE:-$ROOT_DIR/benchmarks/calc_pi_benchmark.jl}"
SJULIA_BIN="${SJULIA_BIN:-$ROOT_DIR/target/release/sjulia}"
RUNS="${RUNS:-3}"
RESULTS_DIR="${RESULTS_DIR:-$ROOT_DIR/benchmarks/results/vm_calc_pi_$(date +%Y%m%d_%H%M%S)}"

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

result_lines() {
    grep '^N=' "$1"
}

echo "Benchmark file: $BENCH_FILE"
echo "Runs: $RUNS"
echo "Results: $RESULTS_DIR"
echo

julia --startup-file=no --history-file=no "$BENCH_FILE" >"$RESULTS_DIR/julia_check.out"
"$SJULIA_BIN" "$BENCH_FILE" >"$RESULTS_DIR/sjulia_check.out"

result_lines "$RESULTS_DIR/julia_check.out" >"$RESULTS_DIR/julia_results.out"
result_lines "$RESULTS_DIR/sjulia_check.out" >"$RESULTS_DIR/sjulia_results.out"

if ! cmp -s "$RESULTS_DIR/julia_results.out" "$RESULTS_DIR/sjulia_results.out"; then
    echo "result mismatch" >&2
    echo "Julia:" >&2
    cat "$RESULTS_DIR/julia_results.out" >&2
    echo "sjulia:" >&2
    cat "$RESULTS_DIR/sjulia_results.out" >&2
    exit 1
fi

echo "Result lines:"
cat "$RESULTS_DIR/julia_results.out"
echo

record_series julia julia --startup-file=no --history-file=no "$BENCH_FILE"
record_series sjulia "$SJULIA_BIN" "$BENCH_FILE"

{
    echo "# VM calc_pi Benchmark"
    echo
    echo "- Benchmark: \`$BENCH_FILE\`"
    echo "- Runs: $RUNS"
    echo
    echo "## Result Lines"
    echo
    echo '```text'
    cat "$RESULTS_DIR/julia_results.out"
    echo '```'
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
} >"$RESULTS_DIR/report.md"

cat "$RESULTS_DIR/report.md"
