---
name: sjulia-coprime-pi-benchmark
description: Use when asked to run or compare the coprime pi benchmark in the SubsetJuliaVM repo across Julia upstream, juliars, sjulia VM typed/untyped, and Python 3.14 via uv, especially when results must be reflected in docs/vm/BLOG.md.
---

# Coprime π Five-Way Benchmark

Run the coprime π estimator in a 5-way comparison for `N=5000` and `N=10000`, and reflect the results in `docs/vm/BLOG.md`.

## Source files

| Runtime | N=5000 | N=10000 |
|---|---|---|
| Julia upstream | `benchmarks/calc_pi_n5000.jl` | `benchmarks/calc_pi_n10000.jl` |
| sjulia VM typed | `benchmarks/calc_pi_n5000_typed.jl` | `benchmarks/calc_pi_n10000_typed.jl` |
| sjulia VM untyped | `benchmarks/calc_pi_n5000.jl` | `benchmarks/calc_pi_n10000.jl` |
| juliars | `benchmarks/calc_pi_n5000_aot.jl` | `benchmarks/calc_pi_n10000_aot.jl` |
| Python 3.14 (uv) | `benchmarks/calc_pi_n5000.py` | `benchmarks/calc_pi_n10000.py` |

If the AoT files are missing, create them by keeping only `calc_pi(N)` at the end of the matching untyped source (no `@time`, no `println`).

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

## Commands

Run each benchmark at least twice and report the second (warm) wall time.

```bash
# 1. Julia upstream
time julia --startup-file=no benchmarks/calc_pi_n5000.jl
time julia --startup-file=no benchmarks/calc_pi_n10000.jl

# 2. sjulia VM typed
time ./target/release/sjulia benchmarks/calc_pi_n5000_typed.jl
time ./target/release/sjulia benchmarks/calc_pi_n10000_typed.jl

# 3. sjulia VM untyped
time ./target/release/sjulia benchmarks/calc_pi_n5000.jl
time ./target/release/sjulia benchmarks/calc_pi_n10000.jl

# 4. juliars
./target/release/juliars benchmarks/calc_pi_n5000_aot.jl \
  --minimal-prelude --emit-binary /tmp/calc_pi_n5000_aot
time /tmp/calc_pi_n5000_aot

./target/release/juliars benchmarks/calc_pi_n10000_aot.jl \
  --minimal-prelude --emit-binary /tmp/calc_pi_n10000_aot
time /tmp/calc_pi_n10000_aot

# 5. Python 3.14 (uv)
time uv run --python 3.14 --no-project benchmarks/calc_pi_n5000.py
time uv run --python 3.14 --no-project benchmarks/calc_pi_n10000.py
```

## Reporting

Present results as a table:

| Runtime | N=5000 | N=10000 |
|---|---:|---:|
| Julia upstream | ... | ... |
| sjulia VM typed | ... | ... |
| sjulia VM untyped | ... | ... |
| juliars | ... | ... |
| Python 3.14 (uv) | ... | ... |

All runtimes should print the same π estimate:

- N=5000: `3.1413097643199746`
- N=10000: `3.141534239016629`

## Reflecting results in `docs/vm/BLOG.md`

Update **only the timing table** inside the existing `### coprime π 推定`
subsection. Do NOT replace the subsection wholesale: it also contains source
listings (`#### untyped / ...` sub-subsections) that must be preserved, and
the `## 実行パフォーマンス` intro carries a runtime legend table shared by all
benchmarks. The current BLOG table also has a **Cython (uv)** row — it is a
hand-optimized 追試 (see `### Cython 版の追試`); leave that row unchanged
unless you re-measured Cython as well.

```markdown
| Runtime | N=5000 | N=10000 |
|---|---:|---:|
| Julia upstream | X.XX s | X.XX s |
| sjulia VM typed | X.XX s | X.XX s |
| sjulia VM untyped | X.XX s | X.XX s |
| juliars | X.XX s | X.XX s |
| Python 3.14 (uv) | X.XX s | X.XX s |
| Cython (uv) | (据え置き・再計測時のみ更新) | (同左) |
```

## Important caveats

- **Warm runs only:** The first sjulia VM run includes Base cache compilation and is much slower. Report the second `real` time.
- **Cache embedding:** BLOG.md numbers use a cache-embedded release binary. Without embedded caches, the CLI process time is dominated by Base compilation, not VM execution.
- **AoT compile time is separate:** `juliars --emit-binary` takes several seconds; report only the generated binary's execution time.
- **No project overhead for Python:** Use `uv run --no-project` so uv does not search for a `pyproject.toml`.
- **Typed vs untyped gap:** The typed variant uses explicit `Int64`/`Float64` annotations. On simple integer loops, the untyped VM's runtime specialization can be competitive with or faster than the typed path.
