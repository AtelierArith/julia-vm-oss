# SubsetJuliaVM Benchmark Suite

This directory contains benchmark scripts to compare performance across different execution modes:
- **Julia**: Official Julia interpreter (JIT compiled)
- **sjulia**: SubsetJuliaVM bytecode interpreter
- **AOT**: Ahead-of-Time compilation to native Rust

## Benchmark Programs

### AoT Benchmarks (`julia/`)

| Name | Description | Complexity |
|------|-------------|------------|
| `fib.jl` | Recursive Fibonacci (n=30) | O(2^n) - tests recursion overhead |
| `array_sum.jl` | Sum of 1M integers | O(n) - tests loop and array access |
| `matmul.jl` | 100x100 matrix multiplication | O(n^3) - tests nested loops |
| `mandelbrot.jl` | 200x200 Mandelbrot set | O(n^2) - tests floating-point arithmetic |

### Legacy Benchmarks

#### 1. Pi Estimation (`calc_pi_benchmark.jl`)

Estimates π using the coprime probability method:
- P(gcd(a,b) = 1) = 6/π²
- Tests nested loops and integer arithmetic

#### 2. Mandelbrot Set (`mandelbrot_benchmark.jl`)

Computes the Mandelbrot escape time algorithm:
- Tests complex number arithmetic
- Tests broadcasting operations
- Tests 2D matrix operations

## Running Benchmarks

### AoT Benchmark Suite

```bash
# Run all AoT benchmarks (interpreter vs AoT comparison)
./benchmarks/scripts/run_benchmarks.sh

# Results are saved to benchmarks/results/<timestamp>/
```

### VM-Only Mandelbrot Benchmark

Issue #4301 tracks VM execution performance without AoT. The VM-only runner
compares official Julia process execution with `target/release/sjulia`, checks
that both produce the same Mandelbrot count, and writes raw wall-time results to
`benchmarks/results/`.

The optimization target is the same VM/interpreter path embedded by the iOS app:
no JIT, no AoT-only shortcut, and no platform-specific native code generation.
Any VM speedup tracked from this benchmark should keep the iOS C ABI/runtime
embedding path working.

```bash
cargo build --release --bin sjulia --features repl
RUNS=5 ./benchmarks/scripts/run_vm_mandelbrot.sh
```

The benchmark source is `benchmarks/vm_mandelbrot.jl`. It intentionally starts
with a small `120x80x60` workload, avoids array output, and focuses on scalar
`Float64` arithmetic, nested loops, branches, local variable load/store, and
function-call overhead in the VM.

The runner also captures an untimed instruction profile with
`SJULIA_VM_PROFILE=1` so VM optimizations can be tied back to observed hot
bytecode. The same environment variable can be used directly:

```bash
SJULIA_VM_PROFILE=1 target/release/sjulia benchmarks/vm_mandelbrot.jl
```

Issue #4302 moved hot primitive numeric binary operations from
`CallIntrinsic(...)` dispatch to existing typed VM bytecode when the compiler
has proven `Int64` / `Float64` operands. On the current `120x80x60` Mandelbrot
workload, the top VM arithmetic instructions are now `MulF64`, `AddF64`,
`AddI64`, `SubF64`, `LeF64`, `LtI64`, and `DivF64`; `CallIntrinsic` no longer
appears in the top instruction profile. Remaining VM cost is dominated by typed
slot loads, `CallDynamicBinaryBoth`, branches, and generic stores/calls.
Issue #4305 further splits `CallDynamicBinaryBoth` profile labels by intrinsic
and candidate count; the current Mandelbrot Any-path hot spots are
`CallDynamicBinaryBoth::MulFloat/14` and `CallDynamicBinaryBoth::AddFloat/11`.
Issue #4306 adds a primitive fast path inside `CallDynamicBinaryBoth` for the
common `Float64`/`Float64` and `Int64`/`Int64` cases, avoiding resolver
preparation when Julia-compatible primitive fallback is already known to win.
The profiler records `BinaryBothPrimitiveFastHit`; on the small Mandelbrot
workload it currently fires `498795` times.

Issue #4303 adds a VM-only fast path for inferred direct calls whose callee
index, arity, and fixed positional parameter slots are known and that do not
need kwargs, varargs, or runtime type-parameter binding. This preserves the iOS
VM embedding path: the VM still pushes a normal frame and return IP, but avoids
cloning `FunctionInfo`, allocating/reversing an argument vector, and cloning
argument values while binding slots.

The direct-call microbenchmark is `benchmarks/vm_direct_calls.jl`. It repeatedly
calls a small typed leaf function to isolate call/frame setup overhead.

```bash
julia --startup-file=no --history-file=no benchmarks/vm_direct_calls.jl
SJULIA_VM_PROFILE=1 target/release/sjulia benchmarks/vm_direct_calls.jl
BENCH_FILE=benchmarks/vm_direct_calls.jl RUNS=5 ./benchmarks/scripts/run_vm_mandelbrot.sh
```

On the current `50000`-call microbenchmark, sjulia VM median improved from
`2.72s` to `2.61s`, and the profiler records `CallDirectFastHit` for all
`50002` fixed direct calls. On the small Mandelbrot workload, the same fast path
records `9602` hits and sjulia VM median measured `2.82s`.

### VM Local Slot Microbenchmark

Issue #4304 tracks VM local load/store overhead in typed hot loops. The
microbenchmark `benchmarks/vm_local_slots.jl` keeps the workload scalar and
allocation-light while repeatedly reading and writing `Int64`, `Float64`, and
`Bool` locals.

The compiler already rewrites local name-based loads/stores into slot-indexed VM
instructions through `vm/slot.rs`. Inferred `Int64` and `Float64` loads/stores
now remain typed after slotization as `LoadSlotI64` / `StoreSlotI64` and
`LoadSlotF64` / `StoreSlotF64`, while `Any` and unstable bindings stay on the
generic slot path.

After #4302, the same microbenchmark also verifies that primitive arithmetic in
the hot loop is emitted as typed VM instructions (`AddI64`, `AddF64`, `MulF64`)
instead of `CallIntrinsic`.

```bash
julia --startup-file=no --history-file=no benchmarks/vm_local_slots.jl
SJULIA_VM_PROFILE=1 target/release/sjulia benchmarks/vm_local_slots.jl
BENCH_FILE=benchmarks/vm_local_slots.jl RUNS=5 ./benchmarks/scripts/run_vm_mandelbrot.sh
```

### VM calc_pi Benchmark

`benchmarks/calc_pi_benchmark.jl` estimates pi from coprime probability for
`N=100`, `N=500`, and `N=1000`. It intentionally keeps the `@time` output in the
Julia source for CLI-level comparison, so the dedicated runner compares only the
deterministic `N=...` result lines and records full process wall time.

```bash
cargo build --release --bin sjulia --features repl
RUNS=3 ./benchmarks/scripts/run_vm_calc_pi.sh
```

For CLI startup/frontend comparisons, `.sjbc` files skip source parse/lower but
still compile VM bytecode at run time. Use `.sjvmbc` files to persist the final
`CompiledProgram` and measure a one-shot CLI path closer to VM execution:

```bash
SJULIA_PRELUDE_PROGRAM_CACHE="$(pwd)/target/prelude_program_cache.bin" \
SJULIA_BASE_CACHE="$(pwd)/target/base_cache.bin" \
  cargo build --release --bin sjulia --features repl
./target/release/sjulia --compile-vm benchmarks/calc_pi_benchmark.jl \
  -o target/calc_pi_benchmark.sjvmbc
./target/release/sjulia --run-vm-bytecode target/calc_pi_benchmark.sjvmbc
```

On 2026-06-11, the embedded-cache CLI timings for
`benchmarks/calc_pi_benchmark.jl` were `1.41s` from source, `1.24s` from IR
`.sjbc`, and `0.47s` from VM `.sjvmbc`.

### Criterion Benchmarks (Rust)

For detailed performance analysis with statistical significance:

```bash
# VM-only calc_pi from precomputed bytecode
cargo bench -p subset_julia_vm --bench calc_pi_benchmark

# Detailed phase-separated benchmark
cargo bench --bench detailed_benchmark

# VM-only Mandelbrot from precomputed bytecode
cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark

# All benchmarks
cargo bench
```

HTML reports are generated in `target/criterion/report/index.html`

`vm_mandelbrot_benchmark` compiles `benchmarks/vm_mandelbrot.jl` once during
Criterion setup, validates that the program prints `166265`, then reports both
`run_only` and `clone_new_program_run`. Use `run_only` for VM dispatch-loop
changes and `clone_new_program_run` when changes may affect `CompiledProgram`
cloning or VM construction from cached bytecode.

With the predecoded typed loop executable layer (Issue #6169), the short
Criterion run on 2026-06-07 measured `run_only` at about `15.1 ms` and
`clone_new_program_run` at about `21.4 ms` for `benchmarks/vm_mandelbrot.jl`.

`calc_pi_benchmark` reuses the function definitions from
`benchmarks/calc_pi_benchmark.jl`, compiles `calc_pi(100)` and `calc_pi(500)`
once during Criterion setup, validates the returned `Float64`, then reports
`run_only` and `clone_new_program_run` under the `vm_calc_pi` group. Use this
benchmark for gcd-heavy integer-loop VM work without CLI, parser, lowering, or
bytecode compile noise.

The same benchmark also exposes `vm_calc_pi_large` for `calc_pi(1000)`.
It reports `run_only` for both the user-defined `mygcd` implementation and
Base `gcd`; use it when comparing VM-only execution against external runtimes
or one-shot `sjulia` CLI timings.

## Directory Structure

```
benchmarks/
├── julia/              # AoT benchmark source files
│   ├── fib.jl
│   ├── array_sum.jl
│   ├── matmul.jl
│   └── mandelbrot.jl
├── scripts/            # Benchmark runner scripts
│   └── run_benchmarks.sh
├── results/            # Benchmark results (timestamped)
│   └── <timestamp>/
│       ├── report.md
│       └── *.txt
├── calc_pi_benchmark.jl
├── mandelbrot_benchmark.jl
└── README.md
```

## Benchmark Results (Sample)

### AoT Benchmarks (Expected)

| Benchmark | Interpreter (ms) | AoT Rust (ms) | Speedup |
|-----------|------------------|---------------|---------|
| fib | ~1500 | ~150 | ~10x |
| array_sum | ~200 | ~20 | ~10x |
| matmul | ~3000 | ~300 | ~10x |
| mandelbrot | ~500 | ~50 | ~10x |

### Compilation Time Breakdown

The benchmark script now reports detailed compilation time metrics:

| Metric | Description |
|--------|-------------|
| AoT Gen | Time to generate Rust source from Julia |
| rustc -O | Time for rustc to compile optimized binary |
| Total | Total compilation time |

### Binary Size

The benchmark also tracks generated code sizes:

| Metric | Description |
|--------|-------------|
| Rust Source | Size of generated .rs file |
| Binary | Size of compiled executable |

### Backend Comparison

| Feature | Rust Backend | Cranelift Backend |
|---------|--------------|-------------------|
| Compile Time | Seconds | Milliseconds |
| Execution Speed | Fastest (LLVM) | Fast |
| Binary Size | Large | Medium |
| SIMD | Auto-vectorization | None |
| External Deps | rustc | None |

### Pi Estimation (N=100)

| Runtime | Time | Notes |
|---------|------|-------|
| Julia | ~0.004s | After JIT warmup |
| sjulia | ~0.044s | Full pipeline each run |
| sjulia (VM only) | ~0.046s | Pre-compiled bytecode |

### Mandelbrot (50x25, maxiter=50)

| Runtime | Time | Notes |
|---------|------|-------|
| Julia | ~0.22s | First run (99.95% compilation) |
| sjulia | ~0.38s | Including compilation |

## Notes

- First run times include compilation overhead for both Julia and sjulia
- Julia's JIT compiler produces highly optimized native code after warmup
- sjulia compiles to bytecode which is interpreted, not JIT compiled
- AOT compilation to native Rust is available but requires all types to be statically known
- For accurate benchmarks, Criterion is recommended as it handles warmup and statistical analysis
