#!/usr/bin/env bash
# Run the nightly multi-bench performance gate benchmarks (Issue #9003).
#
# Runs 8 representative Criterion benchmarks with a reduced-sample CI mode
# (--sample-size 10 --measurement-time 2 --warm-up-time 1) so the full suite
# finishes in ~5 minutes on ubuntu-latest rather than the 30+ minutes a
# full-count run would take.
#
# The ssa_pipeline_gate and register_vm_gate benchmarks have sample sizes and
# measurement times hardcoded in the bench code itself; we still pass the
# reduced-mode flags so CI overrides for the non-gated cases are consistent.
#
# Usage:
#   bash scripts/run_perf_gate_benches.sh
#
# Environment:
#   CRITERION_ROOT   Criterion output root (default: target/criterion)
#   BENCH_SAMPLE     Sample size override (default: 10)
#   BENCH_MEASURE    Measurement time in seconds (default: 2)
#   BENCH_WARMUP     Warm-up time in seconds (default: 1)
#
# After running, call:
#   python3 benchmarks/scripts/check_criterion_thresholds.py \
#     benchmarks/baselines/multi_bench_nightly_thresholds.json \
#     "${CRITERION_ROOT:-target/criterion}"
#
# to check results against the stored baselines.

set -euo pipefail

CRITERION_ROOT="${CRITERION_ROOT:-target/criterion}"
BENCH_SAMPLE="${BENCH_SAMPLE:-10}"
BENCH_MEASURE="${BENCH_MEASURE:-2}"
BENCH_WARMUP="${BENCH_WARMUP:-1}"

# Reduced-mode flags passed to every bench run.
# ssa_pipeline_gate and register_vm_gate have their own hardcoded sample_size(10)
# and measurement_time(10s / 15s) in the bench code; the flags below will be
# overridden by those internal settings for those two benchmarks.
REDUCED_FLAGS="--warm-up-time $BENCH_WARMUP --measurement-time $BENCH_MEASURE --sample-size $BENCH_SAMPLE"

echo "=== nightly perf gate: 8 selected Criterion benchmarks (Issue #9003) ==="
echo "Reduced-sample mode: sample=$BENCH_SAMPLE measure=${BENCH_MEASURE}s warmup=${BENCH_WARMUP}s"
echo ""

# 1. calc_pi/run_only/100  — NS-4 representative bench + existing catastrophic guard
echo "-- [1/8] calc_pi_benchmark: vm_calc_pi/run_only/100 --"
cargo bench -p subset_julia_vm --bench calc_pi_benchmark -- \
  "vm_calc_pi/run_only/100" $REDUCED_FLAGS

# 2. vm_dynamic_dispatch/run_only  — dispatch overhead (Issue #6853)
echo "-- [2/8] vm_dynamic_dispatch_benchmark: vm_dynamic_dispatch/run_only --"
cargo bench -p subset_julia_vm --bench vm_dynamic_dispatch_benchmark -- \
  "vm_dynamic_dispatch/run_only" $REDUCED_FLAGS

# 3. dispatch_loop_overhead_20000  — hot-path dispatch loop
echo "-- [3/8] hot_paths_benchmark: dispatch_loop_overhead_20000 --"
cargo bench -p subset_julia_vm --bench hot_paths_benchmark -- \
  "dispatch_loop_overhead_20000" $REDUCED_FLAGS

# 4. vm_broadcast_mixed_float_int/run_only/pow_float  — Float broadcast path
echo "-- [4/8] vm_broadcast_mixed_float_int_benchmark: pow_float --"
cargo bench -p subset_julia_vm --bench vm_broadcast_mixed_float_int_benchmark -- \
  "vm_broadcast_mixed_float_int/run_only/pow_float" $REDUCED_FLAGS

# 5. vm_string/run_only/join_split_concat  — string ops
echo "-- [5/8] vm_string_benchmark: join_split_concat --"
cargo bench -p subset_julia_vm --bench vm_string_benchmark -- \
  "vm_string/run_only/join_split_concat" $REDUCED_FLAGS

# 6. vm_int128_arith/run_only  — Int128 arithmetic
echo "-- [6/8] vm_int128_arith_benchmark: run_only --"
cargo bench -p subset_julia_vm --bench vm_int128_arith_benchmark -- \
  "vm_int128_arith/run_only" $REDUCED_FLAGS

# 7. register_vm_gate/fib_25/register_vm  — register VM go/no-go (Issue #8440)
#    Note: register_vm_gate_benchmark has sample_size(10)/measurement_time(15s)
#    hardcoded; the REDUCED_FLAGS are passed but the internal settings take
#    precedence for sample_size.
echo "-- [7/8] register_vm_gate_benchmark: fib_25/register_vm --"
cargo bench -p subset_julia_vm --bench register_vm_gate_benchmark -- \
  "register_vm_gate/fib_25/register_vm" $REDUCED_FLAGS

# 8. ssa_pipeline_gate/calc_pi_loop_carried/ssa_pipeline  — SSA go/no-go
#    Note: ssa_pipeline_gate_benchmark has sample_size(10)/measurement_time(10s)
#    hardcoded.
echo "-- [8/8] ssa_pipeline_gate_benchmark: calc_pi_loop_carried/ssa_pipeline --"
cargo bench -p subset_julia_vm --bench ssa_pipeline_gate_benchmark -- \
  "ssa_pipeline_gate/calc_pi_loop_carried/ssa_pipeline" $REDUCED_FLAGS

echo ""
echo "=== All 8 benchmarks complete. Results in ${CRITERION_ROOT}/ ==="
echo "Run: python3 benchmarks/scripts/check_criterion_thresholds.py \\"
echo "       benchmarks/baselines/multi_bench_nightly_thresholds.json ${CRITERION_ROOT}"
