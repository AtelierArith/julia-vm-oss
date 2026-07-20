#!/usr/bin/env bash
# Convenience wrapper: coprime-pi benchmark at N=10000.
# Delegates to run_calc_pi_comparison.sh with N=10000 fixtures.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

export BENCH_FILE="$ROOT_DIR/benchmarks/calc_pi_n10000.jl"
export AOT_BENCH_FILE="$ROOT_DIR/benchmarks/julia/calc_pi_n10000.jl"
export PY_BENCH_FILE="$ROOT_DIR/benchmarks/calc_pi_n10000.py"

exec "$SCRIPT_DIR/run_calc_pi_comparison.sh" "$@"
