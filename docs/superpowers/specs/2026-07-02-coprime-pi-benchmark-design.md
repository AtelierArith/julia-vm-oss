# Coprime Pi Benchmark Comparison Design

## Goal
Obtain reproducible benchmark numbers for the coprime-probability π estimation across four runtimes:
1. Official Julia (JIT)
2. sjulia AoT
3. sjulia VM
4. Python 3.14 (managed via `uv`)

## Background
The repository already contains Julia/sjulia implementations of the benchmark under:
- `benchmarks/calc_pi_benchmark.jl` — VM/source CLI version with `@time` for N=100/500/1000
- `benchmarks/calc_pi_aot.jl` — AoT entry point (N=100)

There is no Python equivalent, so one will be created.

## Approach
Use a dedicated runner (Option A): create a new Python benchmark file and a new bash runner without modifying existing benchmark infrastructure.

## Files to Create
1. `benchmarks/calc_pi.py`
   - Pure Python implementation matching `benchmarks/calc_pi_benchmark.jl`
   - `mygcd(a, b)` and `calc_pi(N)` functions
   - Prints `N=100: π ≈ <value>` for N=100, 500, 1000
   - Uses only `math.sqrt` from the standard library
2. `benchmarks/scripts/run_calc_pi_comparison.sh`
   - Builds required sjulia artifacts
   - Generates/embedded prelude and Base caches for fair sjulia source CLI timing
   - Compiles persisted VM bytecode (`--compile-vm`) for the VM tier
   - Runs each runtime `RUNS` times (default 3) and records wall time with `/usr/bin/time -p`
   - Validates that deterministic result lines (`N=...`) match across runtimes
   - Writes `benchmarks/results/calc_pi_comparison_<timestamp>/report.md`

## Measurement Tiers

| Tier | Command shape | Notes |
|---|---|---|
| `julia_cli` | `julia --startup-file=no --history-file=no benchmarks/calc_pi_benchmark.jl` | Official Julia JIT |
| `sjulia_embedded_cli` | `target/release/sjulia benchmarks/calc_pi_benchmark.jl` | sjulia source CLI with embedded prelude/Base caches |
| `sjulia_vm_bytecode` | `target/release/sjulia --run-vm-bytecode <file>.sjvmbc` | Precompiled VM bytecode |
| `python314_uv` | `uv run --python 3.14 benchmarks/calc_pi.py` | Python 3.14 via uv |

## Error Handling
- Missing commands cause hard failure with a clear message.
- Mismatched `N=...` result lines abort after printing a diff.
- AoT build failures abort immediately.

## Outputs
- New benchmark files under `benchmarks/`
- Timestamped result directory under `benchmarks/results/`
- Console summary printed by the runner

## Success Criteria
- All four runtimes produce the same π estimates for N=100/500/1000.
- Report contains min/median/avg/max wall time per runtime and N.
