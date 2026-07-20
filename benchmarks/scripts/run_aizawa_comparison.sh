#!/usr/bin/env bash
# Reproducible aizawa attractor benchmark comparing Julia, sjulia (VM/source), sjulia AoT, and Python 3.14(uv).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BENCH_FILE="${BENCH_FILE:-$ROOT_DIR/benchmarks/aizawa_attractor_bench_n500m.jl}"
PY_BENCH_FILE="${PY_BENCH_FILE:-$ROOT_DIR/benchmarks/aizawa_attractor_bench_n500m.py}"
RUNS="${RUNS:-3}"
RESULTS_DIR="${RESULTS_DIR:-$ROOT_DIR/benchmarks/results/aizawa_comparison_$(date +%Y%m%d_%H%M%S)}"
CACHE_DIR="${CACHE_DIR:-$ROOT_DIR/target/benchmark-caches}"
BYTECODE_DIR="${BYTECODE_DIR:-$ROOT_DIR/target/benchmark-bytecode}"
SJULIA_BIN="$ROOT_DIR/target/release/sjulia"
AOT_BIN="$ROOT_DIR/target/release/aot"
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
    # Last non-empty line is the single result value
    grep -v '^[[:space:]]*$' "$file" | tail -1
}

echo "Benchmark file: $BENCH_FILE"
echo "Python bench file: $PY_BENCH_FILE"
echo "Runs: $RUNS"
echo "Results: $RESULTS_DIR"
echo

for cmd in julia uv cargo rustc; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "required command not found: $cmd" >&2
        exit 1
    fi
done

if [[ ! -f "$BENCH_FILE" ]]; then
    echo "benchmark file not found: $BENCH_FILE" >&2
    exit 1
fi
if [[ ! -f "$PY_BENCH_FILE" ]]; then
    echo "Python benchmark file not found: $PY_BENCH_FILE" >&2
    exit 1
fi

echo "== [1/7] Build helper sjulia =="
cargo build --release -p subset_julia_vm --bin sjulia --features repl

echo "== [2/7] Generate prelude/Base caches =="
"$SJULIA_BIN" --precompile-prelude "$PRELUDE_CACHE"
"$SJULIA_BIN" --precompile-base "$BASE_CACHE"

echo "== [3/7] Rebuild sjulia with embedded caches =="
SJULIA_PRELUDE_PROGRAM_CACHE="$PRELUDE_CACHE" \
SJULIA_BASE_CACHE="$BASE_CACHE" \
    cargo build --release -p subset_julia_vm --bin sjulia --features repl

echo "== [4/7] Compile persisted VM bytecode =="
"$SJULIA_BIN" --compile-vm "$BENCH_FILE" -o "$VM_BYTECODE"

echo "== [5/7] Validate outputs =="
"$SJULIA_BIN" "$BENCH_FILE" >"$RESULTS_DIR/sjulia_source_check.out"
"$SJULIA_BIN" --run-vm-bytecode "$VM_BYTECODE" >"$RESULTS_DIR/sjulia_vm_bytecode_check.out"
canonical_output "$RESULTS_DIR/sjulia_source_check.out" >"$RESULTS_DIR/sjulia_source_results.out"
canonical_output "$RESULTS_DIR/sjulia_vm_bytecode_check.out" >"$RESULTS_DIR/sjulia_vm_bytecode_results.out"

if ! cmp -s "$RESULTS_DIR/sjulia_source_results.out" "$RESULTS_DIR/sjulia_vm_bytecode_results.out"; then
    echo "result mismatch between sjulia source and VM bytecode paths" >&2
    diff -u "$RESULTS_DIR/sjulia_source_results.out" "$RESULTS_DIR/sjulia_vm_bytecode_results.out" >&2 || true
    exit 1
fi

julia --startup-file=no --history-file=no "$BENCH_FILE" >"$RESULTS_DIR/julia_check.out"
canonical_output "$RESULTS_DIR/julia_check.out" >"$RESULTS_DIR/julia_results.out"
if ! cmp -s "$RESULTS_DIR/julia_results.out" "$RESULTS_DIR/sjulia_source_results.out"; then
    echo "result mismatch between Julia and sjulia" >&2
    diff -u "$RESULTS_DIR/julia_results.out" "$RESULTS_DIR/sjulia_source_results.out" >&2 || true
    exit 1
fi

uv run --python 3.14 "$PY_BENCH_FILE" >"$RESULTS_DIR/python_check.out"
canonical_output "$RESULTS_DIR/python_check.out" >"$RESULTS_DIR/python_results.out"
if ! cmp -s "$RESULTS_DIR/python_results.out" "$RESULTS_DIR/julia_results.out"; then
    echo "result mismatch between Python and Julia" >&2
    diff -u "$RESULTS_DIR/python_results.out" "$RESULTS_DIR/julia_results.out" >&2 || true
    exit 1
fi

echo
echo "Canonical result:"
cat "$RESULTS_DIR/julia_results.out"
echo

AOT_AVAILABLE=false
if cargo build --release -p subset_julia_vm_runtime && \
   cargo build --release -p subset_julia_vm --bin aot --features aot 2>/dev/null; then
    AOT_AVAILABLE=true
else
    echo "AoT build not available; skipping AoT timing." >&2
fi

if [[ "$AOT_AVAILABLE" == "true" ]]; then
    echo "== [6/7] Build AoT binary =="
    AOT_RS="$RESULTS_DIR/aizawa_aot.rs"
    AOT_BIN_OUT="$RESULTS_DIR/aizawa_aot"
    "$AOT_BIN" "$BENCH_FILE" -o "$AOT_RS" --minimal-prelude
    rustc -O "$AOT_RS" -o "$AOT_BIN_OUT" \
        --extern subset_julia_vm_runtime="$ROOT_DIR/target/release/libsubset_julia_vm_runtime.rlib" \
        -L "$ROOT_DIR/target/release/deps"
    echo "== [7/7] Run benchmark series =="
else
    echo "== [6/7] skipped; [7/7] Run benchmark series =="
fi

record_series julia_cli julia --startup-file=no --history-file=no "$BENCH_FILE"
record_series sjulia_embedded_cli "$SJULIA_BIN" "$BENCH_FILE"
record_series sjulia_vm_bytecode "$SJULIA_BIN" --run-vm-bytecode "$VM_BYTECODE"
record_series python314_uv uv run --python 3.14 "$PY_BENCH_FILE"
if [[ "$AOT_AVAILABLE" == "true" ]]; then
    record_series sjulia_aot "$AOT_BIN_OUT"
fi

{
    echo "# Aizawa Attractor Benchmark Comparison (N=500M)"
    echo
    echo "- Benchmark: \`$BENCH_FILE\`"
    echo "- Python benchmark: \`$PY_BENCH_FILE\`"
    echo "- Runs: $RUNS"
    echo "- Prelude cache: \`$PRELUDE_CACHE\`"
    echo "- Base cache: \`$BASE_CACHE\`"
    echo "- VM bytecode: \`$VM_BYTECODE\`"
    echo
    echo "## Measurement Tiers"
    echo
    echo "- \`julia_cli\`: official Julia process execution."
    echo "- \`sjulia_embedded_cli\`: source CLI with prelude/Base caches embedded in the release binary."
    echo "- \`sjulia_vm_bytecode\`: persisted \`CompiledProgram\` path via \`--run-vm-bytecode\`."
    echo "- \`python314_uv\`: Python 3.14 executed via \`uv run\`."
    echo "- \`sjulia_aot\`: Julia → Rust → native binary via the AoT backend."
    echo
    echo "## Canonical Result"
    echo
    echo '```text'
    cat "$RESULTS_DIR/julia_results.out"
    echo '```'
    echo
    echo "## Raw Times (seconds)"
    echo
    echo "### julia_cli"
    cat "$RESULTS_DIR/julia_cli_times.txt"
    echo
    echo "### sjulia_embedded_cli"
    cat "$RESULTS_DIR/sjulia_embedded_cli_times.txt"
    echo
    echo "### sjulia_vm_bytecode"
    cat "$RESULTS_DIR/sjulia_vm_bytecode_times.txt"
    echo
    echo "### python314_uv"
    cat "$RESULTS_DIR/python314_uv_times.txt"
    echo
    if [[ "$AOT_AVAILABLE" == "true" ]]; then
        echo "### sjulia_aot"
        cat "$RESULTS_DIR/sjulia_aot_times.txt"
        echo
    fi
    echo "## Summary"
    echo
    summarize_series "julia_cli" "julia_cli" "$RESULTS_DIR/julia_cli_times.txt"
    summarize_series "sjulia_embedded_cli" "sjulia_embedded_cli" "$RESULTS_DIR/sjulia_embedded_cli_times.txt"
    summarize_series "sjulia_vm_bytecode" "sjulia_vm_bytecode" "$RESULTS_DIR/sjulia_vm_bytecode_times.txt"
    summarize_series "python314_uv" "python314_uv" "$RESULTS_DIR/python314_uv_times.txt"
    if [[ "$AOT_AVAILABLE" == "true" ]]; then
        summarize_series "sjulia_aot" "sjulia_aot" "$RESULTS_DIR/sjulia_aot_times.txt"
    fi
} >"$RESULTS_DIR/report.md"

cat "$RESULTS_DIR/report.md"
