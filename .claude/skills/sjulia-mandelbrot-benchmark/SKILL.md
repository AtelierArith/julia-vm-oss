---
name: sjulia-mandelbrot-benchmark
description: Use when asked to run or compare the Mandelbrot benchmark in the SubsetJuliaVM repo across Julia upstream, juliars, sjulia VM typed/untyped, and Python 3.14 via uv, especially when results must be reflected in docs/vm/BLOG.md.
---

# Mandelbrot Five-Way Benchmark

Run the Mandelbrot escape-time benchmark in both scalar `for`-loop and `.`-broadcast forms, compare execution times across Julia upstream, sjulia VM typed, sjulia VM untyped, juliars, and Python 3.14 (uv), and reflect the results in `docs/vm/BLOG.md`.

## Source files

| Variant | Typed Julia | Untyped Julia | Python |
|---------|-------------|---------------|--------|
| scalar `for` loop | `benchmarks/mandelbrot_bench_for.jl` | `benchmarks/mandelbrot_bench_for_untyped.jl` | `benchmarks/mandelbrot_bench_for.py` |
| broadcast grid | `benchmarks/mandelbrot_bench_broadcast.jl` | `benchmarks/mandelbrot_bench_broadcast_untyped.jl` | `benchmarks/mandelbrot_bench_broadcast.py` |

The untyped Julia sources are the typed sources with parameter/return type annotations removed. They exercise the sjulia VM's generic interpreter path.

## Build prerequisites

```bash
cargo build --release -p subset_julia_vm --bin sjulia --features repl
cargo build --release -p subset_julia_vm --bin juliars --features aot
```

For warm VM runs comparable to the BLOG.md table, embed prelude/Base caches and rebuild:

```bash
./target/release/sjulia --precompile-prelude "$(pwd)/target/prelude_program_cache.bin"
./target/release/sjulia --precompile-base "$(pwd)/target/base_cache.bin"
SJULIA_PRELUDE_PROGRAM_CACHE="$(pwd)/target/prelude_program_cache.bin" \
SJULIA_BASE_CACHE="$(pwd)/target/base_cache.bin" \
  cargo build --release -p subset_julia_vm --bin sjulia --features repl
```

## Form 1 — scalar `for` loop (1500×1500, maxiter=500)

```bash
# Julia upstream
time julia --startup-file=no benchmarks/mandelbrot_bench_for.jl

# sjulia VM typed
time ./target/release/sjulia benchmarks/mandelbrot_bench_for.jl

# sjulia VM untyped
time ./target/release/sjulia benchmarks/mandelbrot_bench_for_untyped.jl

# juliars
./target/release/juliars benchmarks/mandelbrot_bench_for.jl \
  --minimal-prelude --emit-binary /tmp/mb_for_aot
time /tmp/mb_for_aot

# Python 3.14 via uv
time uv run --python 3.14 --no-project benchmarks/mandelbrot_bench_for.py
```

Expected (release, three warm runs, cache-embedded, M-series macOS, median observed 2026-07-13):

| Runtime | Time | checksum |
|---|---:|---:|
| Julia upstream | 0.52 s | `247910238` |
| sjulia VM typed | 2.36 s | `247910238` |
| sjulia VM untyped | 2.92 s | `247910238` |
| juliars | 0.52 s | `247910238` |
| Python 3.14 (uv) | 21.03 s | `247910238` |

Python's boundary-point rounding can vary by platform or runtime build. On the
2026-07-13 measurement it matched Julia at `total=247910238`; if it differs,
confirm the result is stable across warm runs before treating it as a regression.

## Form 2 — broadcast over ComplexF64 grid (1700×1360, maxiter=500)

```bash
# Julia upstream
time julia --startup-file=no benchmarks/mandelbrot_bench_broadcast.jl

# sjulia VM typed
time ./target/release/sjulia benchmarks/mandelbrot_bench_broadcast.jl

# sjulia VM untyped
time ./target/release/sjulia benchmarks/mandelbrot_bench_broadcast_untyped.jl

# juliars
./target/release/juliars benchmarks/mandelbrot_bench_broadcast.jl \
  --minimal-prelude --emit-binary /tmp/mb_broadcast_aot
time /tmp/mb_broadcast_aot

# Python 3.14 via uv + numpy
time uv run --python 3.14 --no-project --with numpy benchmarks/mandelbrot_bench_broadcast.py
```

Expected (release, three warm runs, cache-embedded, M-series macOS, median observed 2026-07-13):

| Runtime | Time | checksum |
|---|---:|---:|
| Julia upstream | 0.54 s | `254750243` |
| sjulia VM typed | 2.23 s | `254750243` |
| sjulia VM untyped | 3.88 s | `254750243` |
| juliars | 0.53 s | `254750266` |
| Python 3.14 + NumPy (uv) | 3.94 s | `254750230` |

- Python uses the same explicit grid formulas as Julia, but numpy's vectorized `z*z + c` can round differently for a handful of boundary points. The gap is ~1e-7 relative and does not affect timing validity.
- sjulia VM typed matches Julia upstream's checksum on this benchmark.
- juliars's small checksum gap is comparable to Python's and is not treated as a blocker for timing comparison.

## Reproducible runner

```bash
bash benchmarks/scripts/run_mandelbrot_5way.sh
```

Note: the script does **not** embed prelude/Base caches, so its wall times are dominated by Base compilation. Use the cache-embedded build above for VM timing that matches the BLOG.md table.

## Reflecting results in `docs/vm/BLOG.md`

Update **only the timing/checksum tables** inside the existing
`### Mandelbrot — scalar for-loop` and `### Mandelbrot — broadcast`
subsections. Do NOT replace the subsections wholesale: they also contain a
runtime legend reference, source-code sub-subsections (`#### ソースコード`),
and narrative paragraphs that must be preserved (only sync the narrative's
untyped seconds/ratios if the values moved materially). Keep the checksum
column — it is the cross-runtime correctness evidence.

```markdown
| Runtime | Time | checksum |
|---|---:|---:|
| Julia upstream | X.XX s | `247910238` |
| sjulia VM typed | X.XX s | `247910238` |
| sjulia VM untyped | X.XX s | `247910238` |
| juliars | X.XX s | `247910238` |
| Python 3.14 (uv) | X.XX s | `247910238` |
```

(broadcast table: same shape with checksums `254750243` / `254750243` /
`254750243` / `254750266` / `254750230`. If a measured checksum differs from
these, rerun it to confirm stability; a stable unexpected sjulia checksum is a
correctness regression — stop and file a bug Issue before updating BLOG.md.)

## Important caveats

- **AoT needs `--minimal-prelude`**: the full prelude hits a `BigInt` constructor unsupported by AoT codegen (Issue #6975).
- **Broadcast AoT**: builds after Issue #8790; use `--minimal-prelude`.
- **Cache embedding:** BLOG.md numbers use a cache-embedded release binary. Without embedded caches, the CLI process time is dominated by Base compilation, not VM execution.
- **Warm runs:** first sjulia VM run includes Base cache compilation; report the second (warm) `real` time.
- **Untyped VM path:** historically ~10-20x slower than typed (generic dispatch + boxed `Value`). Issue #10704 (bulk typed broadcast kernel) and #10799 (specializer ComplexF64 codegen matches the static compiler's fusable op shape, closing a `TypedLoopOp` count gap of 27 vs 8 for `mandelbrot_escape`'s loop body) brought it to ~1.1-1.4x of typed. Expect low-single-digit seconds close to the typed row; a regression back to several seconds or more indicates the specializer/fusion stopped engaging — investigate before reporting.
