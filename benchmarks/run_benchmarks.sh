#!/usr/bin/env bash
# Reproducible sjulia benchmark runner (Issue #8458).
#
# Measures:
#   1. Official Julia CLI (if julia is on PATH)
#   2. sjulia source CLI with embedded prelude/Base caches
#   3. sjulia persisted VM bytecode CLI, which skips source parse/lower and
#      user bytecode compilation for a one-shot path closest to VM execution

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BENCH_FILE="${BENCH_FILE:-$ROOT_DIR/benchmarks/calc_pi_benchmark.jl}"
RUNS="${RUNS:-3}"
WARMUP_RUNS="${WARMUP_RUNS:-1}"
RESULTS_DIR="${RESULTS_DIR:-$ROOT_DIR/benchmarks/results/reproducible_$(date +%Y%m%d_%H%M%S)}"
CACHE_DIR="${CACHE_DIR:-$ROOT_DIR/target/benchmark-caches}"
BYTECODE_DIR="${BYTECODE_DIR:-$ROOT_DIR/target/benchmark-bytecode}"
SJULIA_BIN="$ROOT_DIR/target/release/sjulia"
PRELUDE_CACHE="$CACHE_DIR/prelude_program_cache.bin"
BASE_CACHE="$CACHE_DIR/base_cache.bin"
VM_BYTECODE="$BYTECODE_DIR/$(basename "$BENCH_FILE" .jl).sjvmbc"

mkdir -p "$RESULTS_DIR" "$CACHE_DIR" "$BYTECODE_DIR"

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
    for i in $(seq 1 "$WARMUP_RUNS"); do
        local warmup_seconds
        warmup_seconds="$(run_timed "${label}_warmup_${i}" "$@")"
        printf "%s warmup %s: %ss (discarded)\n" "$label" "$i" "$warmup_seconds"
    done
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

canonical_output() {
    local file="$1"
    if grep -q '^N=' "$file"; then
        grep '^N=' "$file"
    else
        sed '/^[[:space:]]*$/d' "$file"
    fi
}

echo "Benchmark file: $BENCH_FILE"
echo "Runs: $RUNS"
echo "Warmup runs: $WARMUP_RUNS"
echo "Results: $RESULTS_DIR"
echo

if [[ ! -f "$BENCH_FILE" ]]; then
    echo "benchmark file not found: $BENCH_FILE" >&2
    exit 1
fi

echo "== [1/5] Build helper sjulia =="
cargo build --release -p subset_julia_vm --bin sjulia --features repl

echo "== [2/5] Generate prelude/Base caches =="
"$SJULIA_BIN" --precompile-prelude "$PRELUDE_CACHE"
"$SJULIA_BIN" --precompile-base "$BASE_CACHE"

echo "== [3/5] Rebuild sjulia with embedded caches =="
SJULIA_PRELUDE_PROGRAM_CACHE="$PRELUDE_CACHE" \
SJULIA_BASE_CACHE="$BASE_CACHE" \
    cargo build --release -p subset_julia_vm --bin sjulia --features repl

echo "== [4/5] Compile persisted VM bytecode =="
"$SJULIA_BIN" --compile-vm "$BENCH_FILE" -o "$VM_BYTECODE"

echo "== [5/5] Validate outputs =="
"$SJULIA_BIN" "$BENCH_FILE" >"$RESULTS_DIR/sjulia_source_check.out"
"$SJULIA_BIN" --run-vm-bytecode "$VM_BYTECODE" >"$RESULTS_DIR/sjulia_vm_bytecode_check.out"
canonical_output "$RESULTS_DIR/sjulia_source_check.out" >"$RESULTS_DIR/sjulia_source_results.out"
canonical_output "$RESULTS_DIR/sjulia_vm_bytecode_check.out" >"$RESULTS_DIR/sjulia_vm_bytecode_results.out"

if ! cmp -s "$RESULTS_DIR/sjulia_source_results.out" "$RESULTS_DIR/sjulia_vm_bytecode_results.out"; then
    echo "result mismatch between sjulia source and VM bytecode paths" >&2
    diff -u "$RESULTS_DIR/sjulia_source_results.out" "$RESULTS_DIR/sjulia_vm_bytecode_results.out" >&2 || true
    exit 1
fi

JULIA_AVAILABLE=false
if command -v julia >/dev/null 2>&1; then
    JULIA_AVAILABLE=true
    julia --startup-file=no --history-file=no "$BENCH_FILE" >"$RESULTS_DIR/julia_check.out"
    canonical_output "$RESULTS_DIR/julia_check.out" >"$RESULTS_DIR/julia_results.out"
    if ! cmp -s "$RESULTS_DIR/julia_results.out" "$RESULTS_DIR/sjulia_source_results.out"; then
        echo "result mismatch between Julia and sjulia" >&2
        diff -u "$RESULTS_DIR/julia_results.out" "$RESULTS_DIR/sjulia_source_results.out" >&2 || true
        exit 1
    fi
fi

echo
echo "Canonical result:"
cat "$RESULTS_DIR/sjulia_source_results.out"
echo

if [[ "$JULIA_AVAILABLE" == "true" ]]; then
    record_series julia_cli julia --startup-file=no --history-file=no "$BENCH_FILE"
else
    echo "Julia not found on PATH; skipping Julia CLI timing."
fi
record_series sjulia_embedded_cli "$SJULIA_BIN" "$BENCH_FILE"
record_series sjulia_vm_bytecode "$SJULIA_BIN" --run-vm-bytecode "$VM_BYTECODE"

{
    echo "# Reproducible Benchmark Report"
    echo
    echo "- Benchmark: \`$BENCH_FILE\`"
    echo "- Runs: $RUNS"
    echo "- Warmup runs discarded per tier: $WARMUP_RUNS"
    echo "- Prelude cache: \`$PRELUDE_CACHE\`"
    echo "- Base cache: \`$BASE_CACHE\`"
    echo "- VM bytecode: \`$VM_BYTECODE\`"
    echo
    echo "## Measurement Tiers"
    echo
    echo "- \`julia_cli\`: official Julia process execution."
    echo "- \`sjulia_embedded_cli\`: source CLI with prelude/Base caches embedded in the release binary."
    echo "- \`sjulia_vm_bytecode\`: persisted \`CompiledProgram\` path via \`--run-vm-bytecode\`; excludes prelude/Base and user bytecode compilation."
    echo "- True \`Vm::run()\`-only measurements are reported by Criterion, for example \`cargo bench -p subset_julia_vm --bench calc_pi_benchmark\`."
    echo
    echo "## Canonical Result"
    echo
    echo '```text'
    cat "$RESULTS_DIR/sjulia_source_results.out"
    echo '```'
    echo
    echo "## Raw Times (seconds)"
    echo
    if [[ "$JULIA_AVAILABLE" == "true" ]]; then
        echo "### julia_cli"
        cat "$RESULTS_DIR/julia_cli_times.txt"
        echo
    fi
    echo "### sjulia_embedded_cli"
    cat "$RESULTS_DIR/sjulia_embedded_cli_times.txt"
    echo
    echo "### sjulia_vm_bytecode"
    cat "$RESULTS_DIR/sjulia_vm_bytecode_times.txt"
    echo
    echo "## Summary"
    echo
    echo "Warmup launches are excluded from these statistics."
    echo
    if [[ "$JULIA_AVAILABLE" == "true" ]]; then
        summarize_series "julia_cli" "julia_cli" "$RESULTS_DIR/julia_cli_times.txt"
    fi
    summarize_series "sjulia_embedded_cli" "sjulia_embedded_cli" "$RESULTS_DIR/sjulia_embedded_cli_times.txt"
    summarize_series "sjulia_vm_bytecode" "sjulia_vm_bytecode" "$RESULTS_DIR/sjulia_vm_bytecode_times.txt"
} >"$RESULTS_DIR/report.md"

cat "$RESULTS_DIR/report.md"
