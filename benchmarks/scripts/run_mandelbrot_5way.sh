#!/usr/bin/env bash
# Reproducible Mandelbrot benchmark comparing Julia upstream, sjulia VM typed,
# sjulia VM untyped, sjulia AoT, and Python 3.14(uv) for both the scalar
# for-loop and broadcast forms.
#
# Usage:
#   bash benchmarks/scripts/run_mandelbrot_5way.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BENCH_DIR="$ROOT_DIR/benchmarks"
RESULTS_DIR="${RESULTS_DIR:-$BENCH_DIR/results/mandelbrot_5way_$(date +%Y%m%d_%H%M%S)}"
SJULIA_BIN="$ROOT_DIR/target/release/sjulia"
JULIARS_BIN="$ROOT_DIR/target/release/juliars"

mkdir -p "$RESULTS_DIR"

run_timed() {
    local label="$1"
    shift
    local out_file="$RESULTS_DIR/${label}.out"
    local time_file="$RESULTS_DIR/${label}.time"
    /usr/bin/time -p "$@" >"$out_file" 2>"$time_file"
    awk '/^real / { print $2 }' "$time_file"
}

require_cmds() {
    for cmd in "$@"; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            echo "required command not found: $cmd" >&2
            exit 1
        fi
    done
}

echo "=== Building binaries ==="
require_cmds julia uv cargo
(
    cd "$ROOT_DIR"
    cargo build --release -p subset_julia_vm --bin sjulia --features repl
    cargo build --release -p subset_julia_vm --bin juliars --features aot
)

echo ""
echo "=== Form 1: scalar for-loop (ComplexF64) ==="
echo ""

# Warm runs to normalize caches / JIT.
julia --startup-file=no --history-file=no "$BENCH_DIR/mandelbrot_bench_for.jl" >"$RESULTS_DIR/julia_for_warmup.out" 2>&1 || true
"$SJULIA_BIN" "$BENCH_DIR/mandelbrot_bench_for.jl" >"$RESULTS_DIR/sjulia_for_warmup.out" 2>&1 || true
"$SJULIA_BIN" "$BENCH_DIR/mandelbrot_bench_for_untyped.jl" >"$RESULTS_DIR/sjulia_for_untyped_warmup.out" 2>&1 || true

FOR_JULIA_T="$(run_timed julia_for julia --startup-file=no --history-file=no "$BENCH_DIR/mandelbrot_bench_for.jl")"
FOR_SJULIA_T="$(run_timed sjulia_for "$SJULIA_BIN" "$BENCH_DIR/mandelbrot_bench_for.jl")"
FOR_SJULIA_UNTYPED_T="$(run_timed sjulia_for_untyped "$SJULIA_BIN" "$BENCH_DIR/mandelbrot_bench_for_untyped.jl")"
FOR_PYTHON_T="$(run_timed python_for uv run --python 3.14 --no-project "$BENCH_DIR/mandelbrot_bench_for.py")"

# AoT: compile once, then time the generated binary.
AOT_FOR_BIN="$RESULTS_DIR/mandelbrot_for_aot"
"$JULIARS_BIN" "$BENCH_DIR/mandelbrot_bench_for.jl" --minimal-prelude --emit-binary "$AOT_FOR_BIN" \
    >"$RESULTS_DIR/aot_for_compile.out" 2>&1
FOR_AOT_T="$(run_timed sjulia_aot_for "$AOT_FOR_BIN")"

echo "Julia for-loop:          ${FOR_JULIA_T}s"
echo "sjulia VM typed:         ${FOR_SJULIA_T}s"
echo "sjulia VM untyped:       ${FOR_SJULIA_UNTYPED_T}s"
echo "sjulia AoT for-loop:     ${FOR_AOT_T}s"
echo "Python 3.14 for-loop:    ${FOR_PYTHON_T}s"

echo ""
echo "=== Form 2: broadcast over ComplexF64 grid ==="
echo ""

julia --startup-file=no --history-file=no "$BENCH_DIR/mandelbrot_bench_broadcast.jl" >"$RESULTS_DIR/julia_broadcast_warmup.out" 2>&1 || true
"$SJULIA_BIN" "$BENCH_DIR/mandelbrot_bench_broadcast.jl" >"$RESULTS_DIR/sjulia_broadcast_warmup.out" 2>&1 || true
"$SJULIA_BIN" "$BENCH_DIR/mandelbrot_bench_broadcast_untyped.jl" >"$RESULTS_DIR/sjulia_broadcast_untyped_warmup.out" 2>&1 || true

BC_JULIA_T="$(run_timed julia_broadcast julia --startup-file=no --history-file=no "$BENCH_DIR/mandelbrot_bench_broadcast.jl")"
BC_SJULIA_T="$(run_timed sjulia_broadcast "$SJULIA_BIN" "$BENCH_DIR/mandelbrot_bench_broadcast.jl")"
BC_SJULIA_UNTYPED_T="$(run_timed sjulia_broadcast_untyped "$SJULIA_BIN" "$BENCH_DIR/mandelbrot_bench_broadcast_untyped.jl")"
BC_PYTHON_T="$(run_timed python_broadcast uv run --python 3.14 --no-project --with numpy "$BENCH_DIR/mandelbrot_bench_broadcast.py")"

# AoT: compile once, then time the generated binary.
AOT_BC_BIN="$RESULTS_DIR/mandelbrot_broadcast_aot"
"$JULIARS_BIN" "$BENCH_DIR/mandelbrot_bench_broadcast.jl" --minimal-prelude --emit-binary "$AOT_BC_BIN" \
    >"$RESULTS_DIR/aot_broadcast_compile.out" 2>&1
BC_AOT_T="$(run_timed sjulia_aot_broadcast "$AOT_BC_BIN")"

echo "Julia broadcast:         ${BC_JULIA_T}s"
echo "sjulia VM typed:         ${BC_SJULIA_T}s"
echo "sjulia VM untyped:       ${BC_SJULIA_UNTYPED_T}s"
echo "sjulia AoT broadcast:    ${BC_AOT_T}s"
echo "Python 3.14 broadcast:   ${BC_PYTHON_T}s"

echo ""
echo "=== Outputs ==="
echo ""
echo "for-loop:"
cat "$RESULTS_DIR/julia_for.out"
echo "broadcast:"
cat "$RESULTS_DIR/julia_broadcast.out"

{
    echo "# Mandelbrot 5-way benchmark"
    echo ""
    echo "Measured: $(date)"
    echo ""
    echo "## Form 1 — scalar for-loop (ComplexF64)"
    echo ""
    echo "| Runtime | Time (s) |"
    echo "|---------|----------|"
    echo "| Julia upstream | $FOR_JULIA_T |"
    echo "| sjulia VM typed | $FOR_SJULIA_T |"
    echo "| sjulia VM untyped | $FOR_SJULIA_UNTYPED_T |"
    echo "| sjulia AoT | $FOR_AOT_T |"
    echo "| Python 3.14 (uv) | $FOR_PYTHON_T |"
    echo ""
    echo '```text'
    cat "$RESULTS_DIR/julia_for.out"
    echo '```'
    echo ""
    echo "## Form 2 — broadcast over ComplexF64 grid"
    echo ""
    echo "| Runtime | Time (s) |"
    echo "|---------|----------|"
    echo "| Julia upstream | $BC_JULIA_T |"
    echo "| sjulia VM typed | $BC_SJULIA_T |"
    echo "| sjulia VM untyped | $BC_SJULIA_UNTYPED_T |"
    echo "| sjulia AoT | $BC_AOT_T |"
    echo "| Python 3.14 (uv, numpy) | $BC_PYTHON_T |"
    echo ""
    echo '```text'
    cat "$RESULTS_DIR/julia_broadcast.out"
    echo '```'
} >"$RESULTS_DIR/report.md"

echo ""
echo "=== Report ==="
echo "$RESULTS_DIR/report.md"
