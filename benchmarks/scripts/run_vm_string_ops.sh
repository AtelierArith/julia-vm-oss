#!/usr/bin/env bash
# VM string-operations benchmark runner (Issue #8629, parent #8612).
#
# Baseline mode (default): compares official Julia process execution with
# a single sjulia binary (SJULIA_BIN).
#
# Interleaved A/B mode: set SJULIA_BIN_B to a second sjulia binary. Each
# timed round then runs A immediately followed by B, so ambient load
# affects both sides equally. Use this to compare a baseline binary
# against a candidate (e.g. before/after the Rc<str> migration, #8630).
#
# Optional: set TASKSET_CPUS (e.g. "0-3") to pin every timed process with
# `taskset -c` for less noisy results on shared machines.
#
# Usage:
#   RUNS=5 ./benchmarks/scripts/run_vm_string_ops.sh
#   SJULIA_BIN=/path/a SJULIA_BIN_B=/path/b RUNS=7 TASKSET_CPUS=0-3 \
#     ./benchmarks/scripts/run_vm_string_ops.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BENCH_FILE="${BENCH_FILE:-$ROOT_DIR/benchmarks/vm_string_ops.jl}"
SJULIA_BIN="${SJULIA_BIN:-$ROOT_DIR/target/release/sjulia}"
SJULIA_BIN_B="${SJULIA_BIN_B:-}"
RUNS="${RUNS:-5}"
WARMUP_RUNS="${WARMUP_RUNS:-1}"
TASKSET_CPUS="${TASKSET_CPUS:-}"
RESULTS_DIR="${RESULTS_DIR:-$ROOT_DIR/benchmarks/results/vm_string_ops_$(date +%Y%m%d_%H%M%S)}"

mkdir -p "$RESULTS_DIR"

PIN=()
if [[ -n "$TASKSET_CPUS" ]]; then
    PIN=(taskset -c "$TASKSET_CPUS")
fi

if [[ ! -x "$SJULIA_BIN" ]]; then
    echo "sjulia binary not found: $SJULIA_BIN" >&2
    echo "Run: cargo build --release -p subset_julia_vm --bin sjulia --features repl" >&2
    exit 1
fi

if [[ -n "$SJULIA_BIN_B" && ! -x "$SJULIA_BIN_B" ]]; then
    echo "sjulia B binary not found: $SJULIA_BIN_B" >&2
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
    /usr/bin/time -p "${PIN[@]}" "$@" >"$out_file" 2>"$time_file"
    awk '/^real / { print $2 }' "$time_file"
}

warmup_series() {
    local label="$1"
    shift
    for i in $(seq 1 "$WARMUP_RUNS"); do
        local warmup_seconds
        warmup_seconds="$(run_timed "${label}_warmup_${i}" "$@")"
        printf "%s warmup %s: %ss (discarded)\n" "$label" "$i" "$warmup_seconds"
    done
}

record_run() {
    local label="$1"
    local i="$2"
    shift 2
    local series_file="$RESULTS_DIR/${label}_times.txt"
    local seconds
    seconds="$(run_timed "${label}_${i}" "$@")"
    echo "$seconds" >>"$series_file"
    printf "%s run %s: %ss\n" "$label" "$i" "$seconds"
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
echo "sjulia A: $SJULIA_BIN"
if [[ -n "$SJULIA_BIN_B" ]]; then
    echo "sjulia B: $SJULIA_BIN_B (interleaved A/B mode)"
fi
echo "Runs: $RUNS"
echo "Warmup runs: $WARMUP_RUNS"
if [[ -n "$TASKSET_CPUS" ]]; then
    echo "CPU pinning: taskset -c $TASKSET_CPUS"
fi
echo "Results: $RESULTS_DIR"
echo

julia --startup-file=no --history-file=no "$BENCH_FILE" >"$RESULTS_DIR/julia_check.out"
"$SJULIA_BIN" "$BENCH_FILE" >"$RESULTS_DIR/sjulia_a_check.out"

if ! cmp -s "$RESULTS_DIR/julia_check.out" "$RESULTS_DIR/sjulia_a_check.out"; then
    echo "result mismatch (julia vs sjulia A)" >&2
    echo "Julia:" >&2
    cat "$RESULTS_DIR/julia_check.out" >&2
    echo "sjulia A:" >&2
    cat "$RESULTS_DIR/sjulia_a_check.out" >&2
    exit 1
fi

if [[ -n "$SJULIA_BIN_B" ]]; then
    "$SJULIA_BIN_B" "$BENCH_FILE" >"$RESULTS_DIR/sjulia_b_check.out"
    if ! cmp -s "$RESULTS_DIR/julia_check.out" "$RESULTS_DIR/sjulia_b_check.out"; then
        echo "result mismatch (julia vs sjulia B)" >&2
        echo "Julia:" >&2
        cat "$RESULTS_DIR/julia_check.out" >&2
        echo "sjulia B:" >&2
        cat "$RESULTS_DIR/sjulia_b_check.out" >&2
        exit 1
    fi
fi

echo "Result check passed:"
sed 's/^/  /' "$RESULTS_DIR/julia_check.out"
echo

warmup_series julia julia --startup-file=no --history-file=no "$BENCH_FILE"
for i in $(seq 1 "$RUNS"); do
    record_run julia "$i" julia --startup-file=no --history-file=no "$BENCH_FILE"
done
echo

if [[ -n "$SJULIA_BIN_B" ]]; then
    warmup_series sjulia_a "$SJULIA_BIN" "$BENCH_FILE"
    warmup_series sjulia_b "$SJULIA_BIN_B" "$BENCH_FILE"
    for i in $(seq 1 "$RUNS"); do
        record_run sjulia_a "$i" "$SJULIA_BIN" "$BENCH_FILE"
        record_run sjulia_b "$i" "$SJULIA_BIN_B" "$BENCH_FILE"
    done
else
    warmup_series sjulia_a "$SJULIA_BIN" "$BENCH_FILE"
    for i in $(seq 1 "$RUNS"); do
        record_run sjulia_a "$i" "$SJULIA_BIN" "$BENCH_FILE"
    done
fi

{
    echo "# VM String Operations Benchmark (Issue #8629)"
    echo
    echo "- Benchmark: \`$BENCH_FILE\`"
    echo "- sjulia A: \`$SJULIA_BIN\`"
    if [[ -n "$SJULIA_BIN_B" ]]; then
        echo "- sjulia B: \`$SJULIA_BIN_B\` (interleaved A/B)"
    fi
    echo "- Runs: $RUNS"
    echo "- Warmup runs discarded per tier: $WARMUP_RUNS"
    if [[ -n "$TASKSET_CPUS" ]]; then
        echo "- CPU pinning: \`taskset -c $TASKSET_CPUS\`"
    fi
    echo
    echo "## Verified Output"
    echo
    echo '```text'
    cat "$RESULTS_DIR/julia_check.out"
    echo '```'
    echo
    echo "## Raw Times (seconds)"
    echo
    echo "### Julia"
    cat "$RESULTS_DIR/julia_times.txt"
    echo
    echo "### sjulia A"
    cat "$RESULTS_DIR/sjulia_a_times.txt"
    if [[ -n "$SJULIA_BIN_B" ]]; then
        echo
        echo "### sjulia B"
        cat "$RESULTS_DIR/sjulia_b_times.txt"
    fi
    echo
    echo "## Summary"
    echo
    echo "Warmup launches are excluded from these statistics."
    echo
    summarize_series "Julia" "julia" "$RESULTS_DIR/julia_times.txt"
    summarize_series "sjulia A" "sjulia_a" "$RESULTS_DIR/sjulia_a_times.txt"
    if [[ -n "$SJULIA_BIN_B" ]]; then
        summarize_series "sjulia B" "sjulia_b" "$RESULTS_DIR/sjulia_b_times.txt"
    fi
} >"$RESULTS_DIR/report.md"

cat "$RESULTS_DIR/report.md"
