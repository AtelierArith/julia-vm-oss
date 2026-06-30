# `vm_array_benchmark` baseline — pre-`Value::NativeArray` removal (Issue #6805)

This is the recorded performance baseline taken on `main` **before** the
`Value::NativeArray` carrier removal (#6806 / #6807). After each migration
subsystem lands, re-run the same benchmark and compare against these numbers to
catch regressions (acceptance criterion of #6806 / #6807).

## Environment

| Field | Value |
|-------|-------|
| Machine | Apple M2 Max, 12 cores |
| OS | macOS (Darwin 25.5.0) |
| rustc | 1.96.0 (ac68faa20 2026-05-25) |
| Baseline commit | `62a930b0a` (milestone #26 branch point) |
| Crate version | `subset_julia_vm` 0.7.3 |

## How to reproduce

```bash
cargo bench -p subset_julia_vm --bench vm_array_benchmark
```

The benchmark separates `Vm::run()` from CLI startup, parsing, lowering, and
bytecode compilation. Each case is validated against an expected `Int64` result
before timing, so a wrong-answer regression fails the bench rather than silently
mis-measuring.

The numbers below were taken in criterion "quick mode"
(`--sample-size 30 --measurement-time 3 --warm-up-time 1`) on an otherwise idle
machine. Use the same flags (or the default full run on a quiet machine) for
before/after comparison; absolute numbers are machine-dependent — what matters
is the **relative** change after migration.

## Baseline numbers (run-only, `Vm::run()`)

| Case | Source | median time |
|------|--------|-------------|
| `index_mutation_push_pop_128` | index read/write + `push!`/`pop!`/`pushfirst!`/`popfirst!` | **8.02 ms** |
| `hof_broadcast_filter_reduce_128` | `map` / `broadcast` / `filter` / `reduce` | **21.31 ms** |
| `multidim_index_32x32` | `a[i, j]` cartesian indexing over a `32×32 Matrix{Int64}` | **39.39 ms** |
| `construction_undef_zeros_128` | `Vector{Int64}(undef, k)` + `zeros(Int64, k)` construction loop | **294.37 ms** |
| `view_subarray_parent_share_64` | repeated `view(a, s:n)` parent-sharing slices + `sum` | **61.63 ms** |

The first two cases predate this issue (Issue #6653). The last three were added
by #6805 to cover the migration gaps called out in the issue: multi-dimensional
indexing, `MemoryRef`-backed construction, and `view`/`SubArray` parent sharing.

## Notes

- `construction_undef_zeros_128` is intentionally allocation-heavy (it
  reconstructs `2*k` element buffers `n` times); it is the most sensitive case
  to any added indirection on the `Array{T,N}` → `MemoryRef` → `MemoryValue`
  construction path.
- The FFI/host-boundary round-trip case (REPL converter, plotting) called out in
  #6805 is exercised by `cargo nextest run --release` (REPL/plotting tests) and
  the `scripts/test_aot.sh` gate rather than by criterion, because those paths
  are not reachable from a pure `Vm::run()` workload. See
  `docs/vm/ARRAY_MEMORY_MIGRATION.md` for the gate list.

## AoT gate baseline (`scripts/test_aot.sh`)

Recorded on the same `main` baseline commit before migration:

```
[1/2] cargo nextest run --release -p subset_julia_vm --features aot --no-fail-fast
      Summary [202.958s] 3779 tests run: 3779 passed (2 slow), 0 skipped
[2/2] cargo clippy -p subset_julia_vm --features aot --all-targets -- -D warnings
      Finished — no warnings from `subset_julia_vm`
      (the only emitted warnings are pre-existing vendored `astro-float-num`
       lints, which are not part of this crate).
exit code: 0
```

The AoT path must stay green after every #6806 migration subsystem; re-run
`bash scripts/test_aot.sh` and compare.
