# StaticArrays.jl Support Audit

Issue: #7456 / parent milestone #7433
Reference upstream: StaticArrays.jl 1.9.18 and StaticArraysCore.jl.

## MVP Scope

The sjulia MVP should follow the upstream include structure without vendoring
Rust intrinsics:

- `StaticArraysCore`: pure-Julia compatibility package for the public API used
  by StaticArrays (`StaticArray`, `FieldVector`, `SArray`, `SVector`, `SMatrix`,
  `Size`, `similar_type`, `StaticArraysCore.SOneTo`).
- `StaticArrays` package skeleton: package load through `using StaticArrays`
  and re-export of the StaticArraysCore surface needed by the MVP.
- Static indexing and shape traits: `SOneTo.jl`, `traits.jl`,
  `abstractarray.jl`, `indexing.jl`.
- Immutable static arrays: `SArray.jl`, `SVector.jl`, `SMatrix.jl`.
- Constructors and macro front-end: `SVector(...)`, supported fully-applied
  tuple constructors, partial static-vector/static-matrix constructors,
  `@SVector` literal vectors, and `@SMatrix` / `@SArray` matrix literal inputs.
- Operations: `broadcast.jl`, `arraymath.jl`, and the first linear algebra
  tranche required for small vector/matrix arithmetic.

## Dependency Decision

Implement `StaticArraysCore` as a pure-Julia compatibility package in
`subset_julia_vm/packages/StaticArraysCore` rather than adding Rust intrinsics.
`StaticArrays` should be a bundled pure-Julia package in
`subset_julia_vm/packages/StaticArrays` and should import the existing sjulia
`LinearAlgebra`, `Random`, and `Statistics` surfaces as needed. `PrecompileTools`
is a no-op compatibility dependency for this MVP because sjulia does not use
Julia package precompile hooks at runtime.

## Seed Acceptance Fixtures

The baseline fixtures live in `subset_julia_vm/tests/fixtures/static_arrays/`
and are now enabled as package and constructor support has landed:

- `using_staticarrays_7456.jl`: `using StaticArrays` plus a minimal `SVector`
  constructor smoke.
- `seed_api_baseline_7456.jl`: `SVector`, `@SMatrix`, `Size`, indexing, and
  shape checks.

They capture the Julia-vs-sjulia boundary for Phase 0. Upstream Julia should run
them with `JULIA_LOAD_PATH="$(pwd)/subset_julia_vm/packages:@stdlib"` so it uses
the bundled compatibility package rather than an installed upstream StaticArrays
version.

## Phase 1 Package Skeleton

Issue #7457 adds bundled `StaticArraysCore`, `StaticArrays`, and a
`PrecompileTools` compatibility shim under `subset_julia_vm/packages/`.
`StaticArraysCore` exposes the public names needed by the first StaticArrays MVP
tranche (`StaticArray`, `StaticVector`, `StaticMatrix`, `SArray`, `SVector`,
`SMatrix`, `Size`, `SOneTo`, and `similar_type`). `StaticArrays` preserves the
upstream-style include layout with separate `abstractarray.jl`, `SArray.jl`,
`SVector.jl`, `SMatrix.jl`, `indexing.jl`, `broadcast.jl`, and `arraymath.jl`
files, but leaves constructors, macros, indexing, broadcast, and arithmetic to
later phases.

`PrecompileTools` is bundled as a no-op compatibility shim because sjulia does
not execute Julia package precompile hooks at runtime. The shim is intentionally
pure Julia and only exposes `@compile_workload` / `@setup_workload` so bundled
packages can keep upstream-shaped sources without adding VM intrinsics.

## Phase 2 Static Type Foundation

Issue #7458 implements the first concrete StaticArraysCore surface in pure
Julia: parameterized `StaticArray` / `StaticVector` / `StaticMatrix` families,
tuple-backed `SArray`, `SVector`, and `SMatrix` values, `Size`, `Length`,
`size`, `length`, `eltype`, `ndims`, `Tuple`, tuple-size utilities, and the
minimal `StaticArrayStyle` / `Dynamic` / `StaticDimension` names needed by later
dispatch work.

`SVector(1, 2, 3)`, `SVector{3,Int64}((1,2,3))`, and
`SMatrix{2,2,Int64}((1,2,3,4))` now construct tuple-backed values. Indexing,
broadcast, conversion, and arithmetic remain explicitly deferred to Phases 4
and 5. The `StaticArray` parent currently uses `AbstractArray{Any,N}` rather
than `AbstractArray{T,N}` because sjulia loses the concrete element-type subtype
edge through value-parameter abstract parents (Issue #7728); `eltype` is
provided explicitly for the static array families.

The bundled `StaticArrays` package mirrors this Phase 2 surface locally while
`StaticArraysCore` remains separately loadable for packages that import the core
API directly.

## Phase 3 Constructors And Macro Front-End

Issue #7459 adds the supported constructor and literal macro tranche shared by
`StaticArraysCore` and `StaticArrays`: `SVector(...)`,
`SVector{3,Int64}((...))`, `SMatrix{2,2,Int64}((...))`,
`SArray{Tuple{2,2},Int64,2,4}((...))`, basic `getindex`, `Tuple`, and
`@SVector [1, 2, 3]`.
After Issue #7736 (revised by #8084), `SMatrix{M,N,T}` indexing uses the
**column-major** value-parameter formula `x.data[(j - 1) * M + i]`, matching
upstream StaticArrays / Julia.
After Issue #7734, static constructor methods such as `SVector{3}(...)`,
`SVector{3,Int64}(...)`, `SMatrix{2,2}(...)`, and
`SMatrix{2,2,Int64}(...)` bind their static callable parameters into the
constructor frame and construct tuple-backed values through the pure-Julia
outer constructor bodies. A single flat tuple argument (`SMatrix{M,N}((...))`,
`SVector{N}((...))`) is unwrapped and stored column-major (Issue #8084).

Matrix-literal macro inputs such as `@SMatrix [1 2; 3 4]` and
`@SArray [1 2; 3 4]` are enabled after Issue #7733: quoted matrix macro
arguments arrive as `Expr(:vcat, Expr(:row, ...), ...)`. The pure-Julia macros
collect the literal in row-major source order and reorder it into the
**column-major** backing tuple (Issue #8084), so `Tuple(@SMatrix [1 2; 3 4]) ==
(1, 3, 2, 4)` exactly as upstream. The same column-major convention is shared by
the Rust inline (`StaticArrayInline`) and boxed (`StaticArray`) representations
and their `matvec`/`matmat` kernels, so `getindex`, `Tuple`, `*`, and flat-tuple
construction all agree with official Julia.

## Explicit Deferrals

- Mutable static arrays: `MArray`, `MVector`, `MMatrix`.
- `SizedArray` wrappers and mutable-size adaptation APIs.
- Decompositions: `lu`, `qr`, `svd`, `eigen`, and factorization wrapper types.
- BLAS-specific or LAPACK-specific paths.
- Package extensions and exhaustive upstream tests.
- Full upstream generated-function behavior beyond the static-array macro MVP.

## Phase Mapping

- Phase 1 (#7457): bundle `StaticArraysCore` / `StaticArrays` skeleton through
  `using StaticArrays`. Done for package loading; static array construction
  is covered by later phases.
- Phase 2 (#7458): static type and trait foundation.
- Phase 3 (#7459): supported constructors, basic indexing, and `@SVector`
  literal macro front-end; partial static-vector/static-matrix constructors are
  enabled by Issue #7734; `@SMatrix` / `@SArray` matrix literals are enabled by
  Issue #7733.
- Phase 4 (#7460): protocol, indexing, conversion, broadcast, map/reduce.
- Phase 5 (#7461): arithmetic, linear algebra MVP, performance, docs, and
  release hardening.
