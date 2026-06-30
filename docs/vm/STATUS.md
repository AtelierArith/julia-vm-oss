# 現状分析

**最終更新**: 2026-06-30. 新しい項目は下の日付別「最新対応」セクションを正とし、先頭メタデータには長い issue 要約を重複させない。

> - 実装済みの機能は [DONE.md](./DONE.md) を参照してください。
> - 未実装の機能は [UNIMPLEMENTED.md](./UNIMPLEMENTED.md) を参照してください。
> - 更新方針 (Issue #3760): 新しい項目は日付ごとの共有 `## ...YYYY-MM-DD...` 見出しの下に、Issue ごとの `### ... (Issue #NNNN)` 小見出しとして追加する。同日の見出しが既にある場合は、その下に新しい「最新対応」ブロックを増やさない。
> - 過去分(2026-06-06 以前)は [archive/STATUS-2026.md](./archive/STATUS-2026.md) にアーカイブ済み (Issue #6341)。年が変わったら前年分を `archive/STATUS-<YYYY>.md` へ移す。

---

## 最新対応 (2026-06-30)

### REPL callable residue-ring parent persistence fixed (Issue #8496)

- REPL global type resolution now keeps unresolved parametric struct globals in
  the provided dynamic type context instead of dropping them when the current
  struct table has no exact instantiated entry.
- Persisted package parent objects such as `Z7 = residue_ring(ZZ, 7)[1]` now
  compile later `Z7(10)` inputs through the runtime callable-value path instead
  of falling back to an unknown global function.
- Regression coverage exercises the split-eval REPL path used by iOS.

### iOS full test suite restarts fixed (Issue #8489)

- Swift now treats native FFI result structs as stable prefixes and reads newer
  artifact fields only through optional exported C accessors, preventing stale
  simulator libraries from making Swift read past the old struct layout.
- The REPL FFI result gained matching artifact accessors, the iOS simulator
  xcframework was regenerated locally for the current x86_64 slice, and the
  full iOS unit suite now completes without Simulator process restarts.
- Sample performance benchmarks are opt-in via `SJULIA_IOS_PERF_TESTS=1`, so
  normal `xcodebuild test` runs no longer repeat every sample inside XCTest
  performance measurement.

### AoT CodeInstanceKey / InferenceCacheKey unification (Issue #8372)

- AoT specialization keys now retain ABI/codegen `StaticType` arguments for
  layout while storing the shared compile-side `InferenceCacheKey` as the
  specialization identity.
- AoT literal call-site collection now normalizes argument slots through
  `CacheArgType` / `widen_argtype_for_cache_key`, so compile and AoT cache-key
  construction cannot diverge on the const-specialization policy.
- Regression coverage pins that `CodeInstanceKey` directly stores an
  `InferenceCacheKey` and that the existing Issue #4272 const-preserve/widen
  cases construct identical compile/AoT keys.

### Base cache schema fingerprint invalidation (Issue #8444)

- Persistent/embedded Base cache envelopes now store a schema fingerprint and a
  compiler build fingerprint alongside the source hash, and stale schema/build
  mismatches are rejected before Base bytecode payload sections are decoded.
- `CACHE_VERSION` is now 66, and `compute_base_cache_hash()` includes the
  schema fingerprint explicitly in addition to the Base/prelude source and
  compiler build hash.
- `scripts/audit_base_cache_schema_fingerprint.sh` fails when
  schema-sensitive files change without updating the snapshot and
  `CACHE_VERSION`; CI wiring is tracked by Issue #8491 because the automation
  token cannot update workflows, and precise user-extension invalidation
  remains tracked by Issue #8442.

### Parser diagnostics line/column formatting and recovery (Issue #8454)

- Parser `Span` display now uses stable `line:column` / range formatting
  instead of Rust debug structs in user-facing parse errors.
- Parser context formatting now underlines multi-line spans line by line, so
  array/block-style syntax errors can show all covered source lines.
- Exact parser tests now pin recovered multi-error output for independent
  syntax errors and compare representative diagnostic shape with upstream
  Julia's line/column/source-context output.

### iOS code completion state crash fixed (Issue #8487)

- iOS code-completion and Unicode-completion state now store replacement
  ranges as character offsets instead of `String.Index` values, so applying a
  completion to an equivalent copied `String` instance cannot reuse stale
  indices.
- Both completion state objects publish changes manually and use a
  `nonisolated deinit`, avoiding the Swift task executor deallocation path that
  crashed Simulator runs after code-completion tests completed.

### FFI typed result and header audit (Issue #8455)

- Native detailed and streaming FFI results now carry a structured typed
  `value_json` payload alongside the legacy scalar projection and plot artifact
  fields.
- The public C header documents ownership rules, declares the streaming and
  typed-result accessors, and is checked as both C and C++ while auditing that
  every exported FFI function is declared.

### WASM typed result API and unsupported-feature cleanup (Issue #8456)

- `run_from_source` / `run_ir_json` results now include `typed_value`, and
  `run_from_source_typed` provides an explicit typed-result entrypoint for JS
  callers that need array or complex values without string parsing.
- `get_unsupported_features()` no longer lists implemented macro definitions.

### iOS sample catalog schema and count audit (Issue #8457)

- `samples.json` is now validated against the Swift `CodeSample.Category` and
  `CodeSample.Difficulty` enum raw values, backing `.jl` files, and README
  sample/category counts.
- The iOS README and related docs now report the current 38-sample,
  9-category catalog and describe the JSON-backed sample contribution flow.

### iOS AbstractAlgebra.jl sample FFI warm-start overlap (Issue #8463)

- Method-table construction now defers eager return inference for package and
  stdlib module methods outside `Main`/Base/Core when they have no declared
  return annotations, recording a safe `Any` dispatch snapshot instead of
  abstract-interpreting every package method during `using Package` startup.
- Native FFI detailed/streaming entrypoints now begin the same Base-cache
  warm-start prefetch used by the CLI before parse/lower work starts.
- The iOS `AbstractAlgebra.jl` sample remains on the Base-cache HIT path and
  local CLI proxy time is about 1.04s in release and about 1.22s under
  `dev-fast` after dropping from the earlier ~2.25s baseline.

### Base-cache keyword/block-local callable shadowing bypass (Issue #8469)

- Cached Base bytecode is now bypassed when user code defines a function whose
  name collides with a Base keyword-parameter callable name, including user
  main-block functions and expression `let` blocks produced by `@testset`
  lowering.
- This fixes cached-Base method visibility failures such as `No method matching
  check([Any, Any])` while keeping non-colliding user/package programs on the
  Base-cache path.

### Dense symmetric eigvals ordering fixed (Issue #8475)

- The `LinearAlgebra.eigvals` builtin now sorts the real eigenvalue vector
  produced by `SymmetricEigen` before wrapping it for sjulia, matching the
  sorted dense symmetric values expected by Julia-compatible `eigvals!`
  surfaces.
- This restores `linalg_factorization_inplace_values_7464`, where `eigvals!`
  must return `1.0, 3.0` for `[2.0 1.0; 1.0 2.0]` while preserving the
  existing sjulia in-place work-buffer mutation behavior.

### Tuple equality for OneTo/UnitRange axes fixed (Issue #8478)

- Tuple `==` now treats inline `OneTo` struct snapshots and native `UnitRange`
  values as equal when their logical range sequences match.
- This restores `axes(product(...)) == (1:2, ...)` in
  `iteration_product.jl` after the post-merge validation exposed the tuple
  range equality gap.

### Iterators.product vararg wrapper recursion fixed (Issue #8479)

- The embedded `Base.Iterators.product(args...)` wrapper now constructs
  `Base.ProductIterator(args)` directly, matching Base's vararg product body
  and avoiding recursive qualified vararg dispatch through `Base.product`.
- This restores three-or-more-argument `using Iterators; product(...)`
  construction, which was overflowing before the second `iteration_product`
  testset could run.

### Dict getindex on statically typed Dict slots fixed (Issue #8480)

- The `getindex` compiler path for statically typed `Dict` values now emits the
  shared `IndexLoad` operation, which dispatches StructRef-backed Dicts through
  `getindex`.
- This keeps `d[k]` distinct from `get(d, k, default)` and restores the
  `dict_getbang_writeback_5225` counting-loop fixture after `get!` returns a
  typed `Dict`.

### Built-in irrational constants preserve singleton bindings (Issue #8481)

- Bare `pi`, `π`, and `ℯ` now resolve through global const-struct bindings
  before the legacy Float64 fallback path.
- This restores the `mathconstants_irrational_singletons_5133` singleton
  identity checks while preserving explicit numeric conversion behavior.

### Testset-local varargs tuple slots accept dynamic values (Issue #8482)

- Tuple-typed compiler slots now accept dynamically typed values as boxed
  aggregate values, matching the existing runtime-validated Struct/Array paths.
- This restores `splat_tuple_literal_7741`, whose `@testset`-local
  `f(xs...) = (10, 20, xs...)` helper previously failed during compilation with
  `Cannot convert Any to Tuple`.

### Typed varargs fixture restored to upstream parity (Issue #8483)

- `varargs_typed.jl` now avoids reducing an empty varargs tuple with `sum(())`,
  which upstream Julia rejects.
- The fixture still covers typed zero-varargs arity through `count_typed` and
  typed computation through non-empty varargs.

### Clippy all-targets Function literal build fixed (Issue #8468)

- Updated the predicate-narrowing integration test's hand-built `Function`
  literals to initialize the new `is_runtime_eval` field added for world-age
  runtime eval support.
- Restored `cargo clippy --all-targets -- -D warnings` on current `main`.

### `@eval` function definitions obey world age (Issue #8452)

- Statement-position `@eval f() = ...` now lowers to a runtime global method
  definition instead of a nested local function, so calls already executing in
  an older frame keep seeing the older applicable method.
- Runtime-eval methods are installed into the method table with a new minimum
  world and bind the global function value, allowing later top-level calls and
  `Base.invokelatest` to see the newly defined method.
- Added `macros_eval_function_world_age_8452`; re-verified upstream Julia
  parity, direct `sjulia` MWEs, and the release `macros::` fixture category.

### LinearAlgebra in-place factorization fixture parity fixed (Issues #8411, #8465)

- The `linalg_factorization_inplace_values_7464` fixture now asserts upstream
  Julia behavior: nonsymmetric `eigen!` / `eigvals!` samples check meaningful
  mutation, and failing `isposdef!` expects the attempted Cholesky work matrix.
- Pure-Julia `cholesky!` / `isposdef!` now share an upper-Cholesky work path, so
  failure after a later pivot leaves the matrix mutated like upstream Julia.
- Reproduced the `release-fast` `linalg::chunk_000` failure from #8411 and
  re-verified the chunk after the fix.

### Where lower/cross-bound static parameter parity fixed (Issue #8427)

- Value-position `where` lower bounds and cross-variable bounds now match
  upstream Julia applicability: methods can still be selected for value-only
  bodies, while invalid static parameters remain unbound when read as values.
- Runtime `LoadTypeBinding` fallback now reuses the same declared-bound check
  as function entry binding, so skipped static parameters are not reconstructed
  later from frame arguments.
- Added `dispatch_where_bounds_upstream_applicability_8427`; re-verified the
  `dispatch::` fixture category and release lib tests.

### `DataType.name` exposes `TypeName` identity (Issue #8451)

- `DataType.name` now returns a runtime `TypeName` identity value instead of
  failing with `type DataType has no field name`.
- Same-named user types declared in distinct modules keep distinct `TypeName`
  identities, while Array-family parameterizations such as `Vector{Int64}` and
  `Vector{Float64}` share the canonical `Array` `TypeName`, matching upstream
  Julia.
- Added `reflection_datatype_name_typename_identity_8451` and re-verified the
  release `reflection::` fixture category.

### Core type lattice parity regressions fixed (Issue #8415)

- Fixed the #8415 Core type parity cluster: tuple `typejoin` now preserves
  element joins and prefix `Vararg` widening, value-parameter joins such as
  `Val{1}`/`Val{2}` no longer call `typejoin` on raw values, and Array joins
  widen element/rank axes in the upstream shape.
- Tightened diagonal tuple intersection and where-bound solving so
  `Tuple{T,T} where T` rejects incompatible concrete slots, lower bounds and
  cross-variable bounds are enforced during dispatch, and nested
  `where T<:S where S<:Real` subtype checks preserve the outer bound.
- Parenthesized UnionAll application now lowers through the dynamic
  `ApplyTypeDynamic` path, so `(Vector{T} where T){Int}` canonicalizes to
  `Vector{Int64}` instead of constructing `Int{...}`.
- Added `types_coretype_parity_8415`; re-verified the `types_tests::` fixture
  category in release.

### Runtime include works from `sjulia -e` expression position (Issue #7766)

- Expression-position `include(path)` now lowers as a normal Base call instead
  of being rejected as a sandboxed lowering-time include.
- Added a pure-Julia `include(path::AbstractString)` wrapper over `evalfile`,
  so `sjulia -e 'println(include("/tmp/file.jl"))'` reads and evaluates the
  file like upstream Julia on native platforms.
- Added `include_tests::test_eval_include_file_path_in_expression_position_7766`.

### Base.retry wrapper and function-valued splat forwarding (Issue #8371)

- `retry(f; delays=ExponentialBackOff(), check=nothing)` is now implemented in
  pure Julia and returns a callable wrapper that retries after caught
  exceptions, honors `check=false`, and forwards positional/keyword arguments.
- Function-valued splat calls that cannot be resolved statically now compile as
  dynamic callable-value dispatch instead of failing with
  `Cannot find function 'f' for splat call`, covering the `f(args...; kwargs...)`
  shape used by `retry` closures (follow-up Issue #8434).
- Added `retry_8371` and extended `hof/variadic_splat.jl` for the captured
  function-value splat path.

### Nested `rethrow()` propagation fixed (Issue #8435)

- Direct `rethrow()` calls now compile to the VM rethrow primitive before the
  documented Base stubs can win normal method dispatch.
- Catch blocks keep the caught exception available after clearing the active VM
  error state, so an inner catch can `rethrow()` to an outer catch.
- Removed the `retry(f; check=...)` `throw(err)` workaround; rejected retries
  now use upstream-style `rethrow()` again.
- Regression fixtures: `exceptions_rethrow_nested_8435`, existing
  `exceptions/rethrow.jl`, and `retry_8371`.

### Partial parametric constructors infer remaining field parameters (Issue #8393)

- Partially applied parametric struct constructors now compile through a
  runtime `DataType` call when fewer explicit type parameters are supplied than
  the struct declares.
- The runtime constructor merges explicit leading parameters with parameters
  inferred from field values, so `SymTridiagonal{Float64}(dv, ev)` constructs
  `LinearAlgebra.SymTridiagonal{Float64, Vector{Float64}}` instead of failing
  the static full-arity check.
- Added `linalg_symtridiagonal_partial_constructor_8393` and re-verified the
  QuadGK scalar fixture that exercises `QuadGK.gauss(Float64, 3)`.

### Parenthesized UnionAll application lowers to runtime type application (Issue #8430)

- Parametrized type expressions whose base is itself an expression, such as
  `(Vector{T} where T){Int}`, now lower the parenthesized base expression and
  apply the explicit type parameters with `ApplyTypeDynamic`.
- This keeps the expression aligned with upstream Julia:
  `(Vector{T} where T){Int} === Vector{Int}` now returns `true` instead of
  treating `Int` as a static base name or tripping later display/type parsing.
- Added `types_unionall_apply_parenthesized_8430`.

### `truncated` return override survives runtime dispatch (Issue #8421)

- Calls to `truncated(...)` that compile through the multi-method runtime
  dispatch path now still receive the known `Distributions.Truncated` return
  override instead of widening the assigned local to `Any`.
- This keeps `td = truncated(...); rand(td, n)` and
  `rand(Xoshiro(...), td, n)` on the Distributions `rand` methods, instead of
  treating `td` as a random-array dimension and failing with
  `Cannot convert Array to F64` / `DynamicToI64`.
- Re-verified `distributions_truncated_7325` and the full `distributions::`
  fixture category.

### DataType call-site return inference keeps fitted distribution values (Issue #8414)

- Re-verified the `fit_mle(Binomial, n, data)` path that previously narrowed a
  call-site assignment to `nothing`: current main keeps both the suffstats form
  and direct data form as `Binomial{Float64}` values.
- Added `distributions_fit_mle_datatype_return_8414` so future call-site return
  inference changes must preserve the assigned result and `params(b2)` remains
  callable.

### Rational power preserves Int64 element type (Issue #8418)

- `^(::Rational{T}, ::Int64)` now uses a pure-Julia parametric Rational method
  that computes numerator and denominator powers in the original integer type
  and reconstructs `Rational{T}`.
- `(2 // 3) ^ 3` now matches upstream as `8//27 :: Rational{Int64}` with
  `Int64` numerator/denominator fields, so downstream `Float64(r.num)` no
  longer trips over unintended `BigInt` widening.
- Added `rational_power_preserves_int64_8418`.

### Struct-backed range slices preserve range results (Issue #8416)

- `IndexSlice` now handles `AbstractRange` values indexed by another range with
  the same lazy range formula as `getindex(::AbstractRange, ::AbstractRange)`,
  including empty index ranges whose bounds sit outside the target range.
- Runtime dispatch now gives range-family method candidates an extra specificity
  signal when the actual argument is a range, so public
  `getindex(Base.OneTo(10), 2:4)` selects the range-slice method instead of the
  vector-index materialization method.
- Re-verified `range_struct_indexed_by_range_5842`, including `OneTo` slices,
  typed `slice(r::AbstractRange, inds::AbstractRange)`, and `view(::OneTo, ...)`.

### Module method bodies resolve private type objects under specialization (Issue #8410)

- Ordinary method bodies defined inside a module now resolve unqualified
  module-private type objects through the method's defining module, including
  when the VM lazily specializes that method at runtime.
- Added `modules_method_body_private_type_object_8410` for a method returning
  a private type object and a specialized caller that constructs through the
  returned `DataType`. The AbstractAlgebra permutation MVP trigger now reaches
  `elem_type(G) == AAPerm` without `UndefVarError`.

### QuadGK semi-infinite interval cache invalidation (Issue #8408)

- Bumped the persistent package-loader cache version to 15 so QuadGK modules
  cached before the `let (s0, si) = ...` destructuring lowering fix are ignored.
  The stale version-14 cache could still load `handle_infinities` with `si`
  unbound even after the source-level lowering bug was fixed.
- Added `packages_quadgk_infinite_intervals_8408` covering both `quadgk(x ->
  exp(-x), 0.0, Inf, rtol=1e-3)` and `quadgk(x -> exp(x), -Inf, 0.0,
  rtol=1e-3)`.

### Matrix(::SymTridiagonal) materializes before eigvals range calls (Issue #8391)

- Bare `Matrix(x)` constructor routing now lets non-dense matrix arguments fall
  through to ordinary Julia method dispatch instead of the array-constructor
  no-op path. This restores `Matrix(A::AbstractMatrix)` for structured matrix
  wrappers such as `LinearAlgebra.SymTridiagonal`.
- Added `linalg_symtridiagonal_matrix_eigvals_range_8391` to cover dense
  materialization and `eigvals(::SymTridiagonal, ::AbstractRange)`. The original
  package reproducer `packages_quadgk_scalar_integrals_8140` now reaches
  `QuadGK.gauss(Float64, 3)` without passing a `SymTridiagonal` StructRef to the
  linalg builtin.
- During reduction, a separate partial type constructor gap for
  `SymTridiagonal{T}(...)` was filed as unsupported-feature Issue #8393.

### Typed Matrix(::SymTridiagonal) constructor dispatch (Issue #8395)

- `Matrix{T}(A::AbstractMatrix)` now reaches the Julia constructor-method path
  instead of the vector-like `Array{T}(x)` compiler intercept.
- This restores `Matrix{Float64}(::SymTridiagonal)` dense materialization,
  preserving the 2-D `Matrix{Float64}` shape and converted off-diagonal values.
- Added `linalg_symtridiagonal_typed_matrix_constructor_8395`.

### Module-qualified @eval exposes runtime module bindings (Issue #8362)

- `@eval M begin ... end` already lowered through `Core.eval`; the remaining
  gap was that a later `M.y` read was compiled only as a known module function
  reference and errored before runtime-created globals could be observed.
- Known module field references now fall back to runtime module `getfield`, and
  `GetFieldByName` can project module bindings through the same reflection path
  as function-form `getfield(::Module, ::Symbol)`.

### QuadGK batched integrand bug closure (Issues #8373, #8375, #8377, #8378, #8380, #8382, #8383, #8384, #8385, #8386, #8387)

- `similar(arr, Nothing, dims...)` now preserves the logical element type as
  `Vector{Nothing}` instead of falling through to boxed `Vector{Any}`. This
  keeps QuadGK's default `BatchIntegrand(..., x=similar(y, Nothing))` path typed
  as `Vector{Nothing}` and lets the parametric constructor infer `X == Nothing`.
- Verified the remaining open `bug` issues in milestone 52 against their MWEs:
  parametric bounded-vector constructors, tuple-vector literal eltypes,
  `fieldtypes(::Type{Tuple{...}})`, `promote_type()`, keyword forwarding/default
  binding, tuple RHS broadcast assignment, and `eval(:(println(...)))`.

### QuadGK milestone closure: parser/lowering/dispatch/runtime gaps (Issues #8363, #8364, #8366, #8367, #8368, #8369, #8370)

- Completed the remaining VM/parser compatibility fixes needed by the bundled
  QuadGK scalar slice: numeric coefficient powers (`4n^2`), runtime typed
  comprehensions in `where` methods, `@inbounds x[i], w[i] = ...` tuple-index
  assignment lowering, keyword default binding for `function f(...; kw=...) where
  ...`, broadcast assignment `x .= a .+ ...`, n-ary operator reduction in
  untyped keyword/default frames, and stateful `Iterators.filter` iteration.
- `QuadGK.gauss(Float64, 3)` and `QuadGK.gauss(Float64, 3, 0.0, 2.0)` are now
  covered in `packages_quadgk_scalar_integrals_8140` in addition to finite
  scalar `quadgk` and `cachedrule`.
- Bumped the persistent package-loader cache version so old cached QuadGK modules
  compiled before the `@inbounds` tuple-assignment lowering fix are ignored and
  rebuilt.

### OrdinaryDiffEq keyword solve dispatch (Issue #8396)

- Bare exported `solve(...; kwargs...)` calls now preserve the resolved method
  table candidate set instead of falling back to an unresolved function name.
  Qualified module-call kwargs also defer to runtime dispatch when compile-time
  resolution needs the concrete algorithm value.
- Keyword method prefiltering now admits `kws...` candidates, restoring the
  Tsit5 linear solve path covered by `packages_ordinarydiffeq_linear_solve_7363`
  and the algorithm dispatch coverage in `ordinarydiffeq_alg_dispatch_7996`.

### Symbolics Num Dict-key indexing through Any receivers (Issue #8397)

- Runtime `IndexLoad(1)` now detects pure-Julia `Dict` receivers before throwing
  an array-index type error for struct keys. This lets `Dict(Symbolics.Num=>...)`
  lookups through an imprecise `Any` receiver dispatch to `getindex`.
- Added regression coverage for a user `struct <: Real` key in
  `dict_indexing_any`, and restored the `packages_symbolics_substitute` Dict-key
  subtest.

### AbstractAlgebra YoungTableau linear indexing (Issue #8400)

- The local Base `AbstractMatrix` linear-index fallback now accepts `::Integer`,
  matching the method shape expected by concrete subtypes such as
  `AbstractAlgebra.YoungTableau`.
- Added `dispatch_abstract_matrix_integer_getindex_specificity_8400`; the
  bundled `packages_abstract_algebra_young_tableau_mvp_8302` fixture now reaches
  `getindex(::YoungTableau, ::Integer)` for direct calls and index syntax.

### AbstractAlgebra alias where-bound dispatch (Issue #8406)

- `where` bounds now expand type aliases during lowering and compile-context
  method signature construction, including uniquely resolvable qualified alias
  leaves from included package files.
- Added `dispatch_alias_where_bound_parametric_struct_8406`, restoring the
  bundled AbstractAlgebra polynomial MVP path that relies on the `RingElement`
  alias bound.

### AbstractAlgebra union-alias typevar binding (Issue #8409)

- Runtime selected-method binding can now recover missing type variables from
  the chosen method pattern when a union alias admits a user-defined abstract
  struct value.
- Added `dispatch_union_alias_user_struct_typevar_binding_8409`, fixing the
  `Fraction{T,R}` `UndefVarError` path in
  `packages_abstract_algebra_fraction_residue_7491`.

### Module-private type objects in specialized methods (Issue #8410)

- Bare module-private type names in method bodies now resolve to the defining
  module's `DataType` value during both main compilation and runtime
  specialization.
- Added `dispatch_module_private_type_object_return_8410`, restoring
  `elem_type(G) == AbstractAlgebra.AAPerm` in
  `packages_abstract_algebra_perm_mvp_8306`.

### Call-site return inference does not collapse dynamic DataType dispatch to Nothing (Issue #8414)

- Generic call-site body re-inference now refuses to refine an `Any` method
  return to the singleton `Nothing`. This preserves runtime values for methods
  that branch on a `DataType` argument and delegate to another method.
- The existing `distributions_fit_suffstats_7326` fixture covers the regression:
  `fit_mle(Binomial, 5, data)` now stores the returned `Binomial` instead of
  compiling the local as `nothing`.

### DataStructures heap helper validation for QuadGK (Issue #8365)

- Added focused validation coverage for the bundled DataStructures
  array-backed heap helper slice required by QuadGK. The new
  `packages_data_structures_heap_validation_8365` fixture exercises
  `heapify!`, non-mutating `heapify`, `heappop!`, `heappush!`, `isheap`,
  `percolate_down!`, and `percolate_up!` across `Forward` and `Reverse`
  orderings.
- The fixture also covers QuadGK's bounded active-prefix path via
  `DataStructures.percolate_down!(xs, i, x, Reverse, len)`, ensuring the
  parked suffix element remains untouched while the active prefix restores
  heap order.

### QuadGK finite domains, segment buffers, in-place/batch dispatch (Issues #8286, #8287, #8288, #8289, #8290, #8401, #8403, #8404, #8405, #8407)

- Extended the accepted QuadGK milestone slice beyond scalar finite intervals:
  `kronrod(Float64, 3)`, finite multi-domain calls, vector/tuple segment input,
  `quadgk_segbuf` + `eval_segbuf`, concrete `segbuf=` reuse, `quadgk!`, and
  direct `BatchIntegrand` wrapper calls are now covered by package fixtures.
- Closed the supporting VM gaps found while driving those fixtures: dense
  `Matrix(SymTridiagonal)` and symmetric `eigvals`, tuple destructuring in `let`,
  length-only `NTuple{N}` dispatch/reflection, expression-position
  `@inbounds a, b = ...`, partial parametric struct `isa`, vararg-aware
  runtime-dispatch deferral, and kwargs-dispatch retention of `kwargs...`
  candidates.

### Milestone 56 structural debt inventory ratchet (Issues #8327/#8329/#8332/#8333/#8334/#8335/#8336/#8337)

- Added `scripts/check_structural_debt_inventory.sh` and registered it in the
  `code-audits` CI job. The audit pins the current inventory for hardcoded
  Julia package/type-name branches, scattered env/target/artifact strings,
  unwrap/expect sites, cross-crate `#[path]` bypasses, safe raw-pointer FFI
  exports, large Rust files/functions, inline `src/` tests, Julia workaround
  comments without Issue links, stale closed-Issue TODO references, and active
  `Issue #XXXX` placeholders.
- Cleaned the stale #1447/#3510 TODO references by replacing them with live
  follow-up Issues #8371 and #8372, and removed the remaining active-source
  `Issue #XXXX` placeholder.

### AoT broadcast call-site collection accepts bare function vars (Issue #8374)

- AoT broadcast specialization now records call sites for lowered
  `Broadcasted(Var("f"), ...)` forms, not only `FunctionRef`. This preserves the
  element-wise `mandelbrot_escape(::Complex, ::Int64)` specialization for
  `mandelbrot_escape.(C, Ref(maxiter))` and keeps the existing mandelbrot
  broadcast codegen regression passing.

### Foo{<:Bound} covariant bound type args lower again (Issue #8352)

- `Foo{<:Bound}` / `Foo{>:Bound}` (sugar for `Foo{T} where T<:Bound`) failed to
  lower with `UnsupportedOperator("<:")` — a regression from #8339, whose #8330
  change defaulted every non-listed `{}` type-arg node to a *dynamic value
  parameter*. A `<:Real` arg (a `UnaryExpression`) was then routed through
  expression lowering, where the prefix `<:` has no left operand.
- `is_dynamic_type_arg` now treats `<:`/`>:` bound shorthands as static bounded
  type expressions. Restores `types/typeof_*` fixtures; new fixture
  `types/covariant_bound_type_arg_8352.jl`. (A sibling #8339 regression — Char /
  `-Inf` value-param *constructors* like `Val{'x'}()` → `Val{Any}()` — is tracked
  separately in #8353.)

### Val Char / -Inf constructor value params stay concrete (Issue #8353)

- Explicit `Val` constructor calls with a Char or negated-Inf value parameter now
  preserve the constructed instance type: `typeof(Val{'x'}()) == Val{'x'}` and
  `typeof(Val{-Inf}()) == Val{-Inf}`. The regression is covered directly in
  `types/value_param_binding_4268.jl`, alongside the existing value-parameter
  binding checks for `f(::Val{N}) where N`.

### Bare exported parametric inner constructor resolves in scope (Issue #8313)

- A parametric struct with an inner constructor, exported and brought in via
  `using .M`, was not reliably callable by its bare name: bare `Perm([1,2,3])`
  collided with the bundled `Base.Order.Perm` (2 type params), and
  `resolve_parametric_struct_name` picked a `*.Perm` key by `HashMap` order — so
  the call *nondeterministically* resolved to `Order.Perm` and failed with
  `Order.Perm{...} expects 2 type parameters, got 1`.
- `resolve_parametric_struct_name` now resolves a bare name scope-first: the
  current module, then `using`-imported modules (`visible_using_modules_for_name`,
  mirroring `resolve_visible_type_alias`), before the suffix-match fallback, which
  is now deterministic (`min` key). The in-scope `M.Perm` wins; `Base.Order`'s own
  internal `Perm` references (compiled with `current_module = Order`) still
  resolve to `Order.Perm`, so `sort`/`sortperm` are unaffected.
- Fixture `modules/parametric_inner_ctor_using_8313.jl`.

### AbstractAlgebra.Generic Young diagram namespace (Issue #8302)

- The bundled `AbstractAlgebra` now defines the upstream-shaped
  `AbstractAlgebra.Generic` submodule for the Young diagram/tableau MVP.
  `AbstractAlgebra.Generic.Partition([4, 2, 1, 1, 1])` and
  `AbstractAlgebra.Generic.YoungTableau([4, 3, 1])` route to the existing
  iOS-safe `Partition` / `YoungTableau` implementation.
- Regression fixture `packages_abstract_algebra_young_tableau_mvp_8302` now
  covers the original qualified `Generic.Partition` issue MWE.
### Parser: Unicode superscript identifier suffixes (Issue #8298)

- Identifier lexing now accepts Latin-1 superscript digits `¹²³` in addition to
  the U+2070 superscript block, so names such as `dderiv⁻¹` parse after infix
  operators and in normal binding positions.
- Covered by `milestone55_unicode_superscript_identifier_after_infix_8298`.

### Base: @views in statement position (Issue #8300)

- Statement-position `@views` now routes through the existing expression macro
  lowering, so assignment forms like `@views y = x[1:2]` lower instead of being
  rejected as an unknown macro.
- Covered by `milestone55_views_macro_assignment_8300`.

### Base: @pure compatibility metadata (Issue #8301)

- `Base.@pure` is accepted as compiler metadata in statement position and
  preserves the wrapped definition/expression as a no-op for sjulia execution.
- Covered by `milestone55_pure_macro_noop_8301`.

### Lowering: field broadcast assignment (Issue #8303)

- Field broadcast destinations now lower to `materialize!(getfield(...), ...)`
  instead of field reassignment, matching Julia's behavior for immutable structs
  whose field value is mutable.
- Covered by `milestone55_field_broadcast_assignment_8303`.

### Parser: multiline return tuple continuation (Issue #8304)

- `return a,` now skips following newlines before parsing the next tuple element,
  so unparenthesized return tuples can continue on the next line.
- Covered by `milestone55_multiline_return_tuple_8304`.

### Base: @. in statement position (Issue #8305)

- Statement-position `@.` / `@__dot__` now uses the expression dotification path,
  allowing assignment forms like `@. x = x + 1`.
- Covered by `milestone55_dot_macro_assignment_8305`.

### Base: Matrix constructor (Issue #8307)

- `Matrix(x)` is handled by the same public array constructor bridge as `Array`
  and `Vector`, making matrix conversion calls available to Base/package code.
- Covered by `milestone55_matrix_constructor_8307`.

### Compile: imported parametric inner constructors (Issue #8313)

- Bare constructor calls imported via `using .M` now try the visible qualified
  module constructor chain (`M.T`) before falling back to unqualified dispatch.
- Covered by `milestone55_imported_parametric_inner_constructor_8313`.

## 最新対応 (2026-06-29)

### Parser: range in ternary else-branch (Issue #8318)

- `cond ? a : b:c` mis-parsed as `(cond ? a : b):c` (so `true ? 1 : 4:6` returned
  `1:6` instead of `1`). The colon-break guard in the Pratt loop fired for *any*
  `:` at `Conditional` precedence, which incorrectly included the else-branch.
- Gated the colon-break on `in_ternary_then` (set only while parsing the
  then-branch, Issue #8314), so the else-branch — and other Conditional-level
  parses — keep `:` as a range operator. `cond ? a : b:c` now parses as
  `cond ? a : (b:c)`, matching upstream Julia; nested ternaries in the else
  (`cond ? a : x ? y : z`) are unaffected.
- Parser unit test + fixture `ternary/else_branch_range_8318.jl`.

### iOS AbstractAlgebra.jl sample (Issue #8295)

- The iOS sample catalog now includes an `AbstractAlgebra.jl` package example
  under Mathematics / Advanced.
- The sample demonstrates the bundled AbstractAlgebra MVP surface that is
  already iOS-viable: `polynomial_ring(ZZ, :x)`, polynomial evaluation and
  derivative, `residue_ring(R, x^2 + x + 1)`, `residue_ring(ZZ, 7)`, exact
  dense matrix operations, basic permutation group operations, and Young
  diagram/tableau basics.
- The source is available both as a bundled `.jl` resource and as a Swift
  fallback catalog entry; a focused XCTest guards catalog visibility.

### AbstractAlgebra polynomial residue ring MVP (Issue #8299)

- Bundled `AbstractAlgebra` now supports the iOS sample's polynomial quotient
  ring shape: `R, x = polynomial_ring(ZZ, :x)` followed by
  `Q, alpha = residue_ring(R, x^2 + x + 1)`.
- Monic polynomial moduli reduce products and powers through the quotient, so
  `alpha^2 + alpha + 1` displays as `0` and `alpha^3` displays as `1`.
- Regression fixture: `packages_abstract_algebra_poly_residue_ring_8299`.

### AbstractAlgebra permutation group MVP (Issue #8306)

- Bundled `AbstractAlgebra` now includes a pure-Julia permutation group slice
  exposed by `using AbstractAlgebra`: `SymmetricGroup`, `Perm`, composition,
  inverse, powers, `sign`, `parity`, `permtype`, parent metadata, and cycle
  display.
- The implementation keeps the public constructor/API shape needed by upstream
  docs examples while using an internal VM-friendly element type for the MVP.
  The imported parametric constructor visibility gap previously referenced here
  is fixed by #8313.
- Regression fixture: `packages_abstract_algebra_perm_mvp_8306`; package
  include registry guard: `julia::packages::tests::test_abstractalgebra_includes`.

### AbstractAlgebra Young diagram/tableau MVP (Issue #8302)

- Bundled `AbstractAlgebra` now supports the iOS sample's Young diagram surface:
  `Partition([4, 2, 1, 1, 1])` and `YoungTableau([4, 3, 1])`.
- The MVP exposes partition metadata (`n`, `part`), tableau shape, column-major
  linear indexing matching the docs examples, equality, and compact ASCII
  diagram helpers for sample output.
- Regression fixture: `packages_abstract_algebra_young_tableau_mvp_8302`.
### Parser: comparison operator in ternary then-branch (Issue #8314)

- `cond ? a > b : c` failed to parse ("expected Colon"): the then-branch was
  parsed at `Conditional` precedence, but a comparison operator (higher
  precedence) recursed into its right operand at a deeper `min_prec`, where the
  `:` separator was consumed as a range operator — so the ternary found no `:`.
- Added a parser flag `in_ternary_then`: while parsing a ternary then-branch, a
  whitespace-preceded `:` (the always-space-delimited ternary separator, vs a
  no-space range `1:2`) terminates the branch at any recursion depth. The flag is
  cleared on entering any grouping (`(...)`, `[...]`, `{...}`, call/index args),
  mirroring `in_matrix_row`, so genuine ranges like `cond ? (1 : 2) : c` still
  parse.
- Parser unit tests + fixture `ternary/comparison_then_branch_8314.jl`. (A
  distinct pre-existing gap — an *unparenthesized range in the else-branch* — was
  found and filed separately.)

### SpecialFunctions: Hurwitz zeta(s, z) and Dirichlet eta(s) (Issue #8310)

- Added the generalized (Hurwitz) zeta `zeta(s, z)` and the Dirichlet eta
  `eta(s)` to the bundled `SpecialFunctions` package, completing the zeta family
  started in Issue #8297. Real arguments only (Complex out of scope, cf. `erfi`,
  Issue #7178 Phase 5).
- `zeta(s, z)` ports upstream `_zeta(s, z)` (Float64 path): recurrence shift +
  the Bernoulli/Stirling asymptotic series shared with `_zeta_real`, handling
  `z > 0`, `z < 0` (via the `-z` recurrence), and the `z == 1`/`z == 0`
  reductions to the Riemann zeta. The `s == 2` fast path uses the general series
  rather than depending on the still-stubbed `trigamma`.
- `eta(s)` uses `-zeta(s) * expm1(log(2) * (1 - s))` with a Taylor branch near
  `s == 1` (`eta(1) == log 2`).
- Fixture `special_functions/special_functions_zeta_hurwitz.jl` verifies values
  against upstream within `1e-6`.

### Accurate sinpi/cospi/sincospi (Issue #8309)

- `Base.sinpi`/`cospi`/`sincospi` were naive `sin(pi*x)`/`cos(pi*x)`, losing the
  exactness upstream guarantees at integer/half-integer arguments (e.g.
  `sinpi(1.0)` returned `~1e-16` instead of `0.0`) and accuracy for large `x`.
- Ported the upstream `Base.Math` algorithm (`julia/base/special/trig.jl`):
  range-reduce `x` to `[0, 0.5]`, then evaluate the Float64 minimax `sinpi`/
  `cospi` kernels on the small remainder. Added exact `Integer` methods.
- Now exact at integer/half-integer args (`±0.0`, `0.0`, `±1.0`), accurate for
  large `x`, and within ~1 ULP of upstream elsewhere (the residual 1-ULP gap is
  the VM's non-fused `muladd`). Fixture `math/sinpi_cospi.jl` strengthened to
  assert exactness instead of `atol=1e-15`.

### SpecialFunctions: Riemann zeta function (Issue #8297)

- Added `zeta(s)` to the bundled `SpecialFunctions` package, ported from
  upstream `SpecialFunctions._zeta` (the Float64 path of its Hurwitz-derived
  algorithm specialized to `z == 1`): reflection formula + Taylor branch for
  `s < 0.5` and the Bernoulli/Stirling asymptotic series for `s >= 0.5`.
- Scope is real `s` only, matching the rest of this subset module (Complex
  remains out of scope, cf. `erfi`, Issue #7178 Phase 5). Handles the pole at
  `s == 1` (`NaN`), the trivial zeros at negative even integers, and the
  non-finite cases `zeta(Inf) == 1`, `zeta(-Inf) == NaN`, `zeta(NaN) == NaN`.
- Fixture `special_functions/special_functions_zeta.jl` checks reference values
  against upstream within `1e-6`.

### Public Base stdlib escape-hatch audit (Issue #8278)

- Added a CI audit that scans compiler/module-call Base-submodule special cases
  and rejects public `Base.<stdlib>` routes for stdlib roots. The allowed shape
  is now explicit: real Base submodules such as `Base.Iterators` /
  `Base.Broadcast` may be routed, while stdlibs must use root modules or private
  stdlib wrapper bridges.
- Removed the old `Base.Random.<fn>` route from `is_base_submodule_function`,
  aligning `Base.Random` with upstream Julia's undefined behavior.
- Regression coverage now checks both direct property access (`Base.<stdlib>`)
  and import form (`using Base.<stdlib>`) for the embedded stdlib roots that are
  not public Base submodules.

### LinearAlgebra stdlib module loading and builtin bridge fix (Issue #8276)

- Aligned `LinearAlgebra` with upstream Julia's stdlib model: `using
  LinearAlgebra` loads a root stdlib module named `LinearAlgebra`; it is not a
  public `Base.LinearAlgebra` submodule.
- Stopped canonicalizing `Base.LinearAlgebra` to `LinearAlgebra`, while keeping
  real bundled Base submodules such as `Base.Order` available through the Base
  preload path.
- Replaced the old `Base.LinearAlgebra.<fn>` wrapper escape hatch with a private
  compiler-only `LinearAlgebra.__sjulia_builtin_<fn>` bridge for `lu`, `det`,
  `inv`, `svd`, `qr`, `eigen`, `eigvals`, `cholesky`, and `cond`.
- Added focused integration tests for `det`, `inv`, `svd`, and `eigen`, plus a
  guard that `Base.LinearAlgebra` is undefined like upstream Julia.

### DataStructures heap helper MVP for QuadGK dependency (Issue #8141)

- Bundled `DataStructures` now includes the package metadata and the
  array-backed binary heap helper surface used by `QuadGK.jl`: `heapify!`,
  `heapify`, `heappop!`, `heappush!`, `isheap`, `percolate_down!`, and
  `percolate_up!`.
- The implementation follows `extern/DataStructures.jl/src/heaps/arrays_as_heaps.jl`
  for the heap algorithms and uses the upstream-compatible `Base.Order`
  ordering surface (`Forward`, `Reverse`, `Ordering`, `lt`).
- Uses the bundled `Base.Order` ordering constants and helpers without adding
  Rust intrinsics.
- 回帰 fixture: `packages/data_structures_heap_8141.jl`, verified under upstream
  Julia with the bundled package path and direct sjulia.
- QuadGK v2.11.3 の `DataStructures` 参照は `adapt.jl` の
  `heapify!`/`heappop!`/`heappush!` と `batch.jl` の bounded
  `percolate_down!`/`percolate_up!` に限定されることを確認した。
- Added `packages/data_structures_quadgk_segment_heap_8141.jl` to validate the
  heap slice against a QuadGK-like `Segment` whose `isless` compares `E`,
  including the bounded active-prefix `percolate_down!` path from batched
  refinement (Issue #8293).

### QuadGK scalar finite-interval integration bundle (Issue #8140)

- Bundled the upstream `QuadGK.jl` source under `subset_julia_vm/packages/QuadGK`
  and registered the package metadata/includes in the embedded package loader.
- `using QuadGK; quadgk(f, a, b)` now runs the cached Gauss-Kronrod rule path
  for scalar finite intervals, including `cachedrule(Float64, 7)`,
  `quadgk(x -> x^2, 0.0, 1.0)`, and `quadgk(sin, 0.0, 1.0)`.
- Closed the parser/lowering/runtime compatibility gaps hit by the upstream
  source instead of rewriting the package: superscript identifier suffixes
  (#8298), multiline tuple returns/assignments (#8304), numeric parenthesized
  juxtaposition, `@views`/`@.`/`Base.@pure` lowering, explicit broadcast
  assignment, local DataType broadcast callees (#8323), `float(::Type)` (#8324),
  NTuple `where` parameter binding (#8325), Unicode comparison operator calls
  in macro-expanded code (#8326), NTuple value parameter type inference (#8328),
  `Val{N-1}` value-parameter evaluation (#8330), and keyword defaults that
  reference functions (#8331).
- Added `packages/quadgk_scalar_integrals_8140.jl`, verified against upstream
  Julia with the bundled package source and direct `sjulia`.

### Nested module and Base.Order binding fix (Issue #8269)

- Nested submodule bodies are emitted before later parent-module statements, so
  `module Parent; module Child; const x = 1; end; const y = Child.x; end`
  resolves the child module binding when compiling/running the parent body.
- `Base.Order.<name>` direct access and calls now resolve through the bundled
  ordering implementation after Base preload, covering `Forward`, `Reverse`,
  `Ordering`, and `lt`.
- 回帰 fixture: `modules/nested_module_order_binding_8269.jl`.
### Fix: abstract_algebra core_traits seed-gate fixture red on main (Issue #8273)

- `tests/fixtures/abstract_algebra/core_traits_7489_7490.jl` asserted
  `occursin("NotImplementedError", err)` on
  `sprint(showerror, NotImplementedError(:demo, ZZ, QQ))`. The custom
  `Base.showerror` (a faithful port of upstream AbstractAlgebra.jl) emits
  `"function demo is not implemented for arguments …"` — it never echoes the
  type name, so the assertion was wrong.
- Semantic merge conflict: #8268 added the fixture while #8256 was still broken
  (package-defined `Base.showerror` ignored → type-name fallback contained the
  string → fixture passed). Once #8256 was fixed, the custom method dispatched
  and the assertion flipped to `false`, leaving the full
  `cargo nextest run --release` red even though each PR was green on its own
  branch. Surfaced by the #7797 full-suite verification (per the #5966
  parallel-merge full-suite rule).
- Corrected the assertion to match the actual upstream message
  (`occursin("function demo is not implemented", err)`); the VM behavior was
  already correct.

### AbstractAlgebra Phase 3/4 seed: ZZ/QQ traits and exact arithmetic (Issues #7489/#7490)

- Bundled `AbstractAlgebra` now includes the Phase 3/4 seed files in upstream
  order: `julia/JuliaTypes.jl`, `fundamental_interface.jl`,
  `KnownProperties.jl`, `error.jl`, `julia/Integer.jl`, and
  `julia/Rational.jl`.
- `ZZ` / `QQ` are real parent objects (`Integers{BigInt}()` /
  `Rationals{BigInt}()`), with parent/element traits for `Int`, `BigInt`,
  `Rational{Int}`, and `Rational{BigInt}`: `parent`, `elem_type`,
  `parent_type`, `base_ring`, `base_ring_type`, `is_exact_type`,
  `is_domain_type`, `characteristic`, `is_known`, and `check_parent`.
- The Phase 4 seed covers exact integer/rational operations needed by the next
  AbstractAlgebra tranche: `zero`, `one`, `is_unit`, `is_zero_divisor`,
  `canonical_unit`, `divides`, `is_divisible_by`, `divexact`,
  `numerator`, `denominator`, `sqrt`, `is_square`, and `root`.
- Three sjulia gaps found during the implementation are tracked separately:
  parametric `Rational{T}(x)` for `T = BigInt` (#8253), same-module const
  function aliases inside later method bodies (#8254), rational-over-rational
  `//` (#8255), and package-defined `Base.showerror` dispatch (#8256).
  The active package workarounds are documented as W-44/W-45/W-46.
- 回帰 fixture: `abstract_algebra/core_traits_7489_7490.jl`。Upstream Julia
  parity is checked with the bundled package path; direct `sjulia` fixture is
  green.

### AbstractAlgebra Phase 5 seed: polynomial, fraction, and residue constructors (Issue #7491)

- Added bundled `AbstractAlgebra/src/Poly.jl` as the first Phase 5 slice:
  dense univariate `GenericPolyRing` / `GenericPoly` parents and elements over
  `ZZ` and `QQ`.
- Supported MVP operations: `polynomial_ring(ZZ, "x")`,
  `polynomial_ring(QQ, "y")`, `gen`, `gens`, `parent`, `base_ring`,
  `elem_type`, `parent_type`, `zero`, `one`, `+`, `-`, `*`, `^`,
  `degree`, `coeff`, `evaluate`, `derivative`, and exact polynomial
  `divexact` for fixture-covered exact divisions.
- Display methods produce upstream-style strings through direct `show`; sjulia
  still routes `println`/`string`/some `sprint(show, ...)` paths through default
  struct formatting (#8263), so the fixture prints with direct `show` and pins
  helper-rendered strings.
- New VM gap found during this tranche: `BigInt` arithmetic through `Any` array
  slots widens to `Float64` (#8262). The polynomial MVP re-coerces/rebuilds
  coefficient vectors as W-47 until the VM arithmetic path is fixed.
- Added `FractionResidue.jl` as the constructor slice for Phase 5:
  `fraction_field(R)` for univariate polynomial rings and `residue_ring(ZZ, n)`
  for integer residue rings. The fixture covers fraction parent/element traits,
  numerator/denominator, fraction arithmetic/equality, residue normalization,
  `modulus`/`data`/`lift`, arithmetic, unit/zero-divisor predicates, and
  characteristic.
- Additional VM gap: callable fraction-field parent dispatch (`F(num, den)`)
  fails for polynomial arguments (#8264), so fixture/internal arithmetic use
  `_frac_make` as W-48.
- 回帰 fixtures: `packages/abstract_algebra_poly_mvp_7491.jl` and
  `packages/abstract_algebra_fraction_residue_7491.jl` pass under upstream Julia
  with the bundled package path and direct `sjulia`.

### AbstractAlgebra Phase 6 seed: matrices, free modules, and maps (Issue #7492)

- Added bundled `AbstractAlgebra/src/Matrix.jl`, `Module.jl`, and `Map.jl` as
  the first Phase 6 slice. Matrix support follows the upstream `MatSpace{T}`
  parent shape and adds dense `GenericMatrix{T}` elements over the existing MVP
  rings.
- Supported MVP operations: `matrix_space`, `matrix(R, r, c, entries)`,
  `zero_matrix`, `identity_matrix`, indexing, parent/base-ring traits,
  `number_of_rows`/`number_of_columns` with qualified `nrows`/`ncols`,
  equality, `+`, `-`, `*`, `transpose`, `det`, `tr`, and small `rank`
  (empty/1x1/2x2).
- Added `free_module`, `gen`, `gens`, `number_of_generators`, module element
  arithmetic, `identity_map`, `hom`, `domain`, and `codomain` for small
  fixture-covered module/map workflows.
- New VM gap: typed `Vector{BigInt}` / `Matrix{BigInt}` storage reads back as
  `Float64` (#8266), so the dense matrix MVP stores flat `Any` entries with
  base-ring coercion as W-49. The callable-parent dispatch limitation remains
  tracked by #8264; fixtures use the upstream public `matrix(...)` constructor.
- 回帰 fixture: `packages/abstract_algebra_matrix_module_map_7492.jl` passes
  under upstream Julia with the bundled package path and direct `sjulia`.

### AbstractAlgebra Phase 7 validation and readiness (Issue #7493)

- Final MVP fixture surface now covers `using AbstractAlgebra`, `ZZ`/`QQ`,
  exact arithmetic, dense univariate polynomials, fraction/residue constructors,
  dense matrices, free modules, and simple maps.
- Cold CLI smoke timings on the release binary (not VM-only timings): Phase 6
  matrix/module/map fixture `2.31s real`; compact `using AbstractAlgebra`
  polynomial-matrix determinant smoke `2.10s real`.
- Release validation passed:
  `timeout 1800 cargo nextest run --release` completed with 4067/4067 tests
  passing. The focused Phase 6 gate
  `timeout 1800 cargo nextest run --release --test fixture_tests linalg:: packages::`
  also passed (6/6 chunks).
- iOS Rust targets are installed on this Linux host, and both
  `subset_julia_vm_ffi` iOS builds compile through Rust code but fail at link
  because Xcode SDK tooling is unavailable (`xcrun` missing; host `cc` rejects
  Apple `-target`). `wasm-pack` and the wasm Rust target are not installed in
  this host, so the requested WASM command could not be run here.
### Open bug sweep: Rational / BigInt / display / callable dispatch regressions (Issues #8253, #8254, #8255, #8256, #8262, #8263, #8264, #8266)

- #8253: `Rational{BigInt}(1)` が parametric method typevar 経由で malformed struct になる。
  dynamic parametric struct call は declared field count と call arity が合わない場合、raw allocation
  ではなく concrete `DataType` を作って constructor dispatch へ戻す。
- #8254: 同一 module の `const` function alias は qualified binding (`M.f`) として保存されるため、
  後続 method body からの variable call も module constant table を見て qualified global load する。
- #8255: `//(::Rational, ::Rational)` / `//(::Rational{BigInt}, ::Rational{BigInt})` を
  existing `/` normalization path へ追加し、exact rational division parity を回復。
- #8256/#8263: module-qualified `showerror` / `show` extension の registry 判定を修正。
  `Module.Struct(...)` constructor return inference は struct 型にし、`import Base: show` された
  module-qualified `M.show` も show registry に登録する。
- #8262/#8266: array store path が `BigInt` を numeric scalar conversion へ通して `Float64` 化していた。
  `Any`/abstract slot と typed `Matrix{BigInt}` の boxed `BigInt` storage を維持する。
- #8264: callable struct dispatch candidate collection が concrete `__callable_Type` のみを見ていた。
  concrete type の registered parent chain から `__callable_AbstractParent` も集め、既存の method
  resolver で通常どおり dispatch する。

### REPL: 再構築不能なグローバルを実 Value で世代跨ぎ保持 (Issue #8260)

- 症状: `prob = ODEProblem(f, u0, tspan)` を作っても、次の REPL 行で
  `solve(prob, …)` が `UndefVarError: prob not defined` になる（OrdinaryDiffEq
  サンプルが Editor では動くが REPL では落ちる）。#8243/#8249/関数グローバルの修正で
  `tspan`/`u0`/`f` は復元できるようになり `prob` 生成までは成功するが、`prob`
  自体が次の eval に持ち越せなかった。
- 根本原因: REPL は各 eval 前に全グローバルを **init 式へ再構築** して注入する
  (`inject_globals` → `value_to_init_expr`)。`ODEProblem` の `kwargs::Base.Pairs`
  フィールドには init 式形が無く `value_to_init_expr` が `None` を返すため、構造体
  ごと黙って drop されていた。
- 修正(アーキ): 再構築できなかったグローバルだけ、**実行時 Value をそのまま次の VM へ
  キャリー**する。`inject_globals` が drop したグローバルを返し、`eval()` が
  `Vm::seed_persisted_globals` を介して前 eval の struct heap を移植（キャリーした
  `StructRef` index を有効に保つ）し、各インスタンスの `type_id` を名前で本
  プログラムの struct table に再マップしてから、グローバルをモジュール (frame 0)
  スコープへ束縛する。これで Pairs を含むあらゆる複雑構造体が忠実に持ち越せる。
- 併せて潜在不整合を修正: `LoadStruct(name)` がスロットのみ参照し、`StoreStruct` が
  書く `locals_any` を見ていなかった。スロット未割当（代入されず読むだけ）のグローバル
  構造体を `get_local` フォールバックで解決するようにした（seed したグローバルの読み出しに必須）。
- 回帰テスト: `repl::tests::test_repl_value_carried_global_with_pairs_field_persists_8260`
  (Pairs フィールドを持つ合成構造体), `..._odeproblem_global_persists_8260`
  (実 OrdinaryDiffEq で `prob` 永続 + `solve` まで)。
- 既知のトレードオフ: seed 時は前 eval の struct heap を丸ごと移植するため、複雑構造体を
  保持し続けるセッションでは heap が単調増加する（再構築可能なグローバルのみの通常
  セッションでは発生しない）。関連知見は memory `reference_repl_global_persistence_reconstruct`。

### array-like wrapper equality / inference drift prevention (Issue #8246)

- #8240 の再発予防として、array-like wrapper constructor の compile-time 推論と
  runtime equality normalization を同時に守る契約を追加。
- `array_like_view_constructor_contract_infers_concrete_subarray_8246` は
  `view(Vector{Int64}, UnitRange)` が `Any` ではなく具体的な
  `SubArray{Int64,1,Vector{Int64},Tuple{UnitRange{Int64}},true}` へ narrowing
  されることを pin する。
- fixture `subarray_array_like_wrapper_contract_8246` は #8240 の `view == view`
  MWE、view/native array の双方向 `==`/`isequal`、非 `SubArray` wrapper として
  `reshape` 同士および native matrix との比較を検証する。
- `docs/vm/CHECKLISTS.md` に、array-like wrapper 追加時の推論/equality 同期ルールと、
  broad method-table `Any` body re-inference を追加する場合の recursion/work-budget 保護、
  REPL user `show` regression 実行ルールを追加。

### Symbolics.jl サンプルのロード時推論を短縮 (Issue #8213)

- 症状: `using Symbolics` を含む基本サンプル（変数・代数・微分）と
  `Symbolics + LinearAlgebra` 行列サンプルがどちらも約 10 秒かかっていた。
  `SJULIA_COMPILE_PROFILE=1` では VM 実行ではなく `compile.build_method_tables`
  が支配的（`using Symbolics` 単体で約 9.7s、行列サンプルで約 8.5s）。
- 根本原因: バンドル Symbolics の再帰的な式ウォーカー（`_simplify`/`_expand`/
  `substitute`/show/derivative 周辺）と記号行列 `det`/`inv` がロード時に全関数
  戻り値推論され、本文推論が再帰木を展開していた。実行時は基本サンプル 31ms、
  行列サンプル 0.45s 程度で、遅さの大半はロード時推論だった。
- 修正: 再帰ウォーカーと記号行列 helper に正直な戻り値注釈（`::Any`/`::Bool`/
  `::String`/`::Num`/`::Nothing`）を付与し、#7215/#8182 と同じ
  `build_method_tables` 本体推論短絡へ載せた。`using Symbolics` の
  `build_method_tables` は約 9.7s → 88ms、基本サンプルは約 9.8s → 1.1s、
  行列サンプルは約 10.3s → 1.7s。
- 回帰: `work_budget_8185` に `using_symbolics_load_inference_stays_bounded_8213`
  を追加し、`using Symbolics` の推論 work が旧 159k 級へ戻るのを検出。

### SubArray view equality が identity fallback になる問題を解消 (Issue #8240)

- 症状: `view([1,2,3,4], 1:3) == view([0,1,2,3], 2:4)` が upstream Julia では
  `true` だが sjulia では `false`。`view(...)` の戻り値が call-site で `Any` 相当に広がり、
  `==` が `SubArray <: AbstractArray` の要素比較ではなく struct identity/field fallback に
  落ちていた。
- 修正: expression compiler で 1D `view(Vector{T}, UnitRange)` を
  `SubArray{T,1,Vector{T},Tuple{UnitRange{Int64}},true}` として推論し、runtime equality 側では
  1D contiguous `SubArray` を `ArrayValue` の logical view に正規化して native array / view 間の
  `==` を要素比較へ載せる。
- 回帰 fixture `subarray/view_equality_8240.jl` で `view == view`、`view == Vector`、
  `Vector == view`、`Float64`/`Bool` view を検証。direct `sjulia` MWE と
  `fixture_tests subarray::` green。

### Plots: `push!` 3D アニメ（Aizawa/Lorenz）が REPL で空の 2D アニメになる (Issue #8214)

- `plot3d(1)` + `push!(plt,x,y,z)` + `@animate` の 3D アトラクタサンプル（Aizawa/Lorenz）が
  REPL で**空の 2D アニメーション**として描画される（ユーザ報告: 「Editor では 3D で正しく
  出るが REPL では 2D アニメのような画面」）。
- **根本原因（アーティファクト dump + セッション probe で特定）**: `@animate` は各フレームを
  `frame(_anim) == frame(_anim, current())` でスナップショットするが、`current()` はプロット
  **構築時**にのみ書かれるグローバル `_CURRENT_*` から再構成される。素の `push!(plt,…)` は
  `plt` を変更するが（sjulia では `plt.series` と `_CURRENT_SERIES[1]` が別オブジェクト）この
  ホルダを更新しないため、`current().series[1].x` は長さ 0 のまま（`plt.series[1].x` は 200）。
  全フレームが push 前の空プロットになり、`generate_plotly_animation_json` は空シリーズを
  `extract_series`（空 x/y を除外）で全て落とし `"traces":[]`・scatter3d 無し・3D `scene` 非検出
  → 2D xaxis/yaxis レイアウトに。2D `@animate` サンプルは毎フレーム新規 `plot()` を作り
  `current()` を更新するため無傷だった。
- **修正**: in-place `push!` 拡張ヘルパ（`_plots_extend_y!`/`_plots_extend_xy!`/
  `_plots_extend_xyz!`）が変更後の `plt` を `current()` として再公開（`push!` は current figure
  を伸ばすという上流不変条件を回復）。フレームが 3D パスを保持し、アーティファクトは
  `scene`+scatter3d+非空フレームに。
- **検証**: dumper で `current().series[1].x` 0→200、フレーム点数 1/41/…/161、gif アーティファクト
  scene=true・scatter3d=6（修正前 scene=false・scatter3d=0）。回帰テスト
  `test_push_based_3d_animation_frames_are_3d_and_nonempty_8214`。fixture_tests(169) と
  plot_artifact_mime_tests(31) 全通過。知見は memory `reference_plots_push_current_sync`。

### iOS REPL: 複数プロットの一括ペーストでメインスレッド・レイアウト嵐による固まり (Issue #8214)

- 2D Plots サンプル（`using Plots; plot(sin); plot!(cos); t = 0:0.1:2π;
  scatter!(cos.(t), sin.(t), aspect_ratio=:equal); bar!(...)`）を REPL に丸ごと
  ペーストすると、履歴が「3 evals」で止まり UI が固まる現象を**シミュレータ上で再現**。
- **根本原因（実機プロセスの `sample` で特定）**: メインスレッドが VM でもロックでもなく
  **SwiftUI レイアウト/アニメーション**で 100% 占有（`CA::Transaction::commit` →
  `List.applyNodes` / `ScrollViewLayoutComputer.sizeThatFits` / `ViewListTransition` /
  `InterpolatedDisplayList`）。`historyView` の自動スクロールが
  `proxy.scrollTo(...)` を `withAnimation(.easeOut(duration: 0.2))` で包んでおり、
  スクロールだけでなく**行の挿入もアニメーション**化。各履歴行は WKWebView（Plotly）を
  内包しうるため毎フレームの再レイアウトが高コストで、ペーストは 0.2s より速く行を追加→
  アニメーションが積み重なりレイアウトが収束しない。飽和したメインスレッドが UI を固め、
  かつ `main.async` の eval/描画コールバックを枯渇させ、ペーストが途中で停止していた。
  （VM と eval ロックは無実。過去の #8227/#8231/#8233/#8235 は別側面の修正。）
- **修正**: 自動スクロールをアニメーション無しの素の `proxy.scrollTo(...)` に変更
  （追加ごとに 1 回の安価なレイアウトで済み、eval ループが進む）。
- **検証**: iPad (A16) シミュレータで同一ペーストを再実行。修正前は 3/6 evals で無限停止
  （メインスレッド 2345/2345 サンプルがレイアウト）、修正後は 6/6 evals が ~20s で完了し
  全プロット描画、メインスレッドはアイドル（2579/2579 サンプルが runloop 待機）、履歴も
  スムーズにスクロール。詳細な知見は memory `reference_ios_swiftui_scrollto_animation_storm`。
- **続き（#8235 の再評価）**: 上記レイアウト嵐の解消により、履歴プロットを再び
  **インタラクティブ化**（pan/zoom/hover・3D orbit、Editor タブと同等）。#8235 は
  「インタラクティブ WebView がスクロールを奪う」と推定して非インタラクティブ化したが、
  実際の「スクロール不可/固まる」原因はこのメインスレッド嵐であり、嵐が消えれば内側
  ScrollView が縦パンを取りインタラクティブ・プロット上でも履歴は普通にスクロールする。
  シミュレータで検証（6/6 完了・プロット上ドラッグで履歴スクロール・eval 数不変・
  Plotly ツールバー/3D 回転動作）。
### AbstractArray サブタイプ vs オペランドの `==`/`isequal` 要素比較 + ディスパッチ (Issue #8229)

- `==`/`isequal` を **native array でも StaticArrays carrier でもない `AbstractArray`
  サブタイプ**（ユーザ `struct <: AbstractVector`、`view(...)` の `SubArray`）に対して
  呼ぶと、要素比較されず `false`（identity 相当）になっていた。#8149 の `==` ルーティング
  gate の下流ギャップ。
- **2層の根因**:
  1. **ディスパッチ**: `builtin_abstract_param_name`（built-in abstract メソッド引数を
     宣言済み supertype チェーン walk へ振り分ける gate）が数値 abstract のみで
     `AbstractArray` を落としていた。そのため `MyVec <: AbstractVector{Float64}` が
     `f(::AbstractArray)` にマッチせず（祖先リンク `AbstractVector{T} <: AbstractArray`
     を core matcher が walk しない）、コンパイル時静的ディスパッチが MethodError を即発行し
     runtime dispatch fallback に到達しなかった。`AbstractArray` を追加（`AbstractRange` 等は
     `Memory` の conservative-accept 誤マッチを招くため**追加しない**）。
  2. **要素読み取り**: equality ビルトインの `array_like_logical_view` は native/Memory/
     StaticArray carrier しか読めず、汎用 `StructRef`/`SubArray` を読めない。
- **修正方針 (Pure Julia First)**: `base/abstractarray.jl` に
  `isequal(A::AbstractArray, B::AbstractArray)`（`size`/`getindex` プロトコル経由の要素比較）を
  追加（汎用 `isequal(x,y)=x===y` にディスパッチで勝つため必須）。`==(::AbstractArray,
  ::AbstractArray)` は**意図的に追加しない**（binary-op codegen が静的解決し `Memory`/native を
  `Array` へ誤強制変換するため）。`isequal` ビルトインは、読めない `AbstractArray` オペランドに
  対しこの Pure-Julia メソッドへディスパッチ fallback（`value_is_unreadable_abstractarray` で
  native/Memory carrier を除外し fast path 維持）。`compile_binary_op` の gate が
  `struct == struct`（#8132/#8149 gate が扱わない）を `isequal` ビルトイン経由で要素比較へ。
- **落とし穴**: pure-Julia `::AbstractArray` メソッドは native/Memory 配列も捕捉する
  （`Vector{T} <: AbstractArray`）。`==(::AbstractArray)` を足すと `mem == arr` が静的 coercion
  で `Cannot convert MemoryOf to Array/Range` 化（`AbstractRange` を pillar に足すと
  `==(::AbstractRange,::AbstractArray)` が Memory を誤捕捉）。`isequal` メソッドは静的 native は
  ビルトイン直行で非捕捉なので安全。dev-fast プロファイルは Memory `!=` を誤判定するアーティ
  ファクトがある（clean baseline でも再現）ので、配列等価の退行確認は **release** で行う。
- 残ギャップ（別 issue 起票）: `isequal(scalar, array)` が MethodError（#8239）;
  `view == view`（両 `Any` 推論）が false（#8240, view 戻り型推論）。`length`/`collect`/
  `iterate` の AbstractArray protocol は #8229 本文どおり別軸で未対応。

### 一般 Base 関数の修飾 `Base.<fn>` 値アクセス (Issue #8137)

- `f = Base.map`（`Base.filter`/`sin`/`cos`/`reduce`/`foldl`/`sum` …）が
  `Compilation error: "Base has no function named map"` で失敗。非修飾 `map(...)` や直接呼び
  `Base.map(...)` は動作。これらは Pure Julia 化され `is_base_function` allowlist に無く、
  method table 裏付けのみだったため `compile_module_function_ref` がエラー分岐へ落ちていた。
- 修正: `Base.<fn>` 値アクセスを一般化。method table 裏付けの任意の Base 名を非修飾 `<fn>` と
  同じ呼び出し可能な関数値（`emit_function_value` → `PushResolvedFunction`）に解決。bare-`Var`
  経路へ委譲せず直接 emit（修飾 `Base.<fn>` は同名ローカルシャドウや import 状態に依らず関数を
  指すべきため）。#4960-#4966 / umbrella #4119 の一般ケース。

## 最新対応 (2026-06-28)

### using-package 戻り値型推論爆発の再発防止: 作業量バジェット + ロード時スモーク (Issue #8185, prevention)

- #8182（`using Optim` ~5.5s; `_bfgs` 戻り値型推論爆発、`build_method_tables` が 97%）の
  再発防止。`_bfgs` はループ内クロージャ `phidphi` を HagerZhang 深い相互再帰木へ引き回し、
  ループ fixpoint 下で再特殊化され指数膨張する。深さは `MAX_INTERPROCEDURAL_ANALYSIS_DEPTH=10`
  で有界だが**作業量**が無界だったのが穴。
- **実装**: (a) `infer_block_with_fixpoint` の入口に**ルート単位の作業量カウンタ
  `analysis_work`**（全相互手続き戻り値型展開がここを通る）を追加。`analysis_depth==0` で
  リセットし、`MAX_INTERPROCEDURAL_ANALYSIS_WORK` 超過で `Top` に widen（例外型 `depth>16`
  ガードと同型）。(b) 常時オンの決定的メトリクス `work_budget_metrics`（profiling 非依存）。
  (c) ロード時スモークテスト `using_optim_load_inference_stays_bounded_8185`（`using Optim`
  の peak work < 50_000、注釈撤去なら 174k で fail）+ バックストップ単体テスト。
  (d) CHECKLISTS.md / PURE_JULIA_DESIGN.md にパターンとチェック項目を追記。
- **重要な経験的知見**: 作業量バジェットは **#8182 を直接潰す一般修正にはならない**。
  注釈無し `_bfgs` 爆発（~174k）が、当時未注釈だった `using Symbolics`（~159k）と
  **同オーダー**で、カウントだけでは区別不能。`_bfgs` を捕える程低い cap は Symbolics を `Top` に widen して
  退行させる。よってバジェットは **catastrophe（host-OOM 級）バックストップ**（cap=2M、
  正当上限の ~12x）に留め、`_bfgs` の戻り値型注釈は**撤去せず維持**、実際の #8182 ガードは
  注釈 + per-package スモークテストとした（#8185 の当初想定「一般修正で注釈撤去」は不成立）。

### native-array vs AbstractArray-subtype-struct `==` を名前 gate から一般階層へ (Issue #8149)

- #8132/PR #8144 で導入した「native array vs StaticArray の `==`/`!=` を `isequal` 配列
  ビルトインへルーティングする」判定が、**StaticArrays struct ファミリ名のハードコード
  リスト** (`is_static_array_struct_julia_type`) に依存していた。任意の
  `AbstractArray` サブタイプ struct（リスト外）には効かない。
- 修正 (`compile/expr/binary/mod.rs`, `compile/method_table.rs`): ルーティング判定を
  `CoreCompiler::is_abstractarray_subtype_struct` に集約。名前リストを **fast path** として
  残しつつ、外れた型は登録済み struct 階層で `<: AbstractArray` を解決する。階層解決は
  **strict** な新述語 `struct_is_registered_subtype_of_abstract`（未登録 struct は
  `false`＝`struct_is_subtype_of_abstract` の "conservatively accept unknown" 分岐を持たない）
  を使うため、グローバル `==` パスで無関係な `native-array == struct` ペアを誤ルーティング
  しない（issue が指摘した回帰リスクを解消）。`MethodTableProjection` は共有 Arc (#6348) を
  任意の method table 経由で取得。フル `--release` 4063/4063 green。
- **下流の別ギャップ #8229**: gate を一般化しても、`isequal` ビルトイン
  (`array_like_logical_view`) が native / StaticArray carrier しか読めず、汎用 `StructRef`
  （ユーザ `<: AbstractArray` struct）や `SubArray` view を読めないため end-to-end の
  要素比較は別途 #8229 が必要。本 PR は #8149 の「gate 一般化」を完遂。

### convert 失敗例外が String でなく InexactError オブジェクトに (Issue #8212)

- `convert(T, x)` の変換失敗で `catch e` が束縛する値が `InexactError` 例外オブジェクト
  ではなく **`String`** になっており、`typeof(e)` / `isa(e, InexactError)` が upstream と
  乖離していた（メッセージ自体は一致するため uncaught では露見しない）。直呼び・typed
  local (`x::Int64 = 1.5`)・typed for ループ変数 (`for i::Int64 in itr`, #8208) いずれも同症状。
- 根因: `vm/exec/error_handling.rs::vm_error_to_exception_value`（#5648 で導入した Rust 製
  `VmError` → catch 可能例外構造体の復元器）に `InexactError` アームが無く、`_ => None`
  に落ちて呼び出し側が `Value::Str` にフォールバックしていた。
- 修正: `VmError::InexactError(msg)` を `InexactError(func::Symbol, T, val)` 構造体へ復元
  （`msg` の `"{T}({val})"` 形を `inexact_error_fields` でパース）。あわせて
  `base/errorshow.jl` の `_showerror_str(ex::InexactError)` を upstream に合わせ、
  `nameof(ex.T) === ex.func` のとき `T` を省く（`InexactError: Int64(1.5)`）よう修正。
  fixture `exceptions/catchable_inexact_error_8212.jl`（julia と 11/11 パリティ）。

### 外部ローカルを捕捉する相互再帰ネストクロージャ (Issue #8118)

- PR #8142 が #8118 の自己再帰・捕捉なし相互再帰を解決済みだったが、**呼ばれる兄弟が
  外部ローカルを捕捉する**相互再帰（例: `s=9; a(n)=…b(n-1); b(n)=n<=0 ? s : a(n-1)`）は
  実行時 `ErrorException: Unknown function: b` のままだった。3-way 相互再帰も同様に失敗。
- 根因: クロージャ `b` が外部ローカル `s` を捕捉すると環境経由で呼ぶ必要があるが、
  兄弟 `a` は `s` を直接参照しないので `s` を捕捉せず、呼び出し地点で `b` の環境を
  再構築できない（`compile_self_or_sibling_closure_call`）。さらに `a` を捕捉するよう
  すると、`b` の捕捉集合に**兄弟クロージャ名 `a`** が入り再構築不能（値が未構築の別
  クロージャ）になる二重苦。
- 修正 (`compile/stmt.rs::prescan_mutual_closure_captures`): 関数本体コンパイル前に
  直下ネスト関数群を事前スキャンし、(1) 各関数の base 捕捉を**完全な外側ローカル集合**
  (`collect_block_local_bindings`) に対して計算、(2) **兄弟関数名を捕捉から除外**
  （兄弟は呼び出し地点で再構築/by-name 解決され、データ捕捉しない）、(3) 呼ぶ兄弟
  クロージャの外部ローカル捕捉を**不動点で推移伝播**。結果（外部ローカルのみ、非空）を
  `mutual_closure_captures` に記録し、`Stmt::FunctionDef` がそれを権威的に使う。これで
  グループ各員が共有捕捉から互いを再構築可能になる。空集合（捕捉なし=PR #8142 が扱う
  ケース）は既存経路に委ねるのでブラスト半径は最小。フル `--release` 4061/4061 green。
### iOS REPL: プロット貼り付けで完全フリーズ/クラッシュするデータ競合を修正 (Issue #8214)

`using Plots; plot(sin); …; scatter!(cos.(t), sin.(t), aspect_ratio=:equal)` を iOS
REPL に貼ると、稀に**完全フリーズ（無反応）**、シミュレータでは**クラッシュ**することが
あった。原因は VM ではなく **iOS アプリ側のデータ競合**。`REPLSession`（Rust）は `eval`
ごとに `&mut self` で内部状態（globals / struct heap / モジュール状態）を変更し
スレッドセーフではないが、`REPLSessionManager` は評価を `DispatchQueue.global()`
（**並行**キュー）へ投げており直列化していなかった。評価中に再投入が重なると
（複数行ペーストの評価が走っている最中にもう一度送信、低速端末で起きやすい）
2 本のワーカースレッドが同じ session ポインタで `repl_session_eval` を同時実行し、
`&mut` がエイリアスして Rust ヒープを破壊 → `SIGABRT`（double free）または
破壊状態の無限ループ（= 完全フリーズ）。`REPLSessionManager` に `NSLock` を導入し
`eval`/`reset`/`newSession` の session 触接を相互排他化して解消。

- 切り分け: VM/FFI は host 140 回・シミュレータ 60 回（実行時コンパイル Base・
  ランダムシード）で完全に無罪を確認。Plotly 描画も WebKit/Blink で固まらず。
  実機相当のシミュレータ上で**重なり評価**を強制して `SIGABRT` を再現し、ロック
  導入後は再現しないことを検証。
- 回帰テスト: `SubsetJuliaVMAppTests/REPLConcurrentEvalSafetyTests`（実 FFI →
  Rust 経路を同一 session に並行投入）。ロック除去で確実にプロセス abort、導入で pass。

追加修正（描画完了までの逐次化, PR #8233）: 同 issue で「`plot(sin)` の**グラフ描画完了**を
待たずに `plot!(cos)` が評価され、逐次ステップで進まない」症状も判明。`evalAsyncSplit` が各行を
`DispatchQueue.main.async`（撃ちっぱなし）で投げ、表示も描画も待たず次の eval に先走り、6 行分の
結果がメインスレッドに一斉到達して Plotly WebView を同時多数生成 → フリーズ要因。
当初 `main.sync`（PR #8231）で「評価→表示→次」までは直列化したが、これは履歴追加までで
**実際の Plotly 描画完了は待たない**（描画は WebView 内で非同期）。そこで **WebView の描画完了を
JS から `WKScriptMessageHandler`(`plotRendered`) でホストに通知**し、eval ループは各プロット行で
その通知（`onEachResult` の `done` クロージャ → semaphore）を**タイムアウト付き(8s)で待ってから**
次行を評価するゲートに変更。非プロット行は即 `done()`。これで「コード実行 → 描画完了 →
次のコード実行」の厳密な逐次化、かつ WebView の同時描画が無くなりフリーズ解消。デッドロック無し
（eval はバックグラウンド・メインは待たない、未描画でもタイムアウトで前進、`reset()` で待機解放）。
シミュレータのタイムライン計測で各プロット行 `EVAL→DISPLAY→RENDER-DONE→次の EVAL`（全 signaled,
timeout 無し）を確認。回帰テスト `testPasteSequenceEvaluatesAndDisplaysInSourceOrder`。

追加修正（履歴スクロール, PR #8235）: 実行完走後も「REPL 履歴をスクロールできない（固まって見える）」
症状。原因は履歴の各プロットが**生きた WKWebView（Plotly）**で、縦ドラッグ（スクロール）を
プロット側が奪うため（420pt のプロットが画面を占有しスクロール不能に見える。メインスレッドは
応答しておりハングではない）。`PlotlyView` に `interactive: Bool` を追加し、**履歴のプロットは
`interactive=false`**（`staticPlot:true` + `webView.isUserInteractionEnabled=false`）にしてタッチを
奪わせず、ジェスチャを内側 ScrollView へ通す。Editor 単体出力は `interactive=true` のまま
（パン/ズーム維持）。描画完了通知(`plotRendered`)は JS から出るので #8233 のゲートに影響なし
（貼り付け完走 2.66s, 全 signaled をシミュレータで確認）。トレードオフ: 履歴内のパン/ズーム/
ホバーは無効（表示は同一）。

### 二項演算 codegen の二重化を共有化 + 再発防止ガード (Issue #8192)

- **背景 (フットガン)**: 二項演算 (`+ - * /` 等) のバイトコード生成が **2 経路**に
  重複していた。(1) 主コンパイラ `compile/expr/binary/`、(2) 実行時引数型 specializer
  `vm/specialize/expr.rs` (#8167)。型付き命令選択を片方だけ直すと他方に伝播せず、
  テストが緑のまま最適化が抜ける。実際に #8183 / PR #8189 で表面化（specializer の
  `Swap; ToF64; Swap` がネイティブ typed-loop 認識を中断）。
- **共通化**: 型ペア → 命令の選択を単一の真実
  `compile::typed_scalar_binary_instr(op, result_is_float)` に集約。主経路の
  `typed_instr_for_intrinsic` はその薄いアダプタに、specializer の `emit_binary_op` は
  直接これを呼ぶよう変更。両経路の命令テーブルが構造的に乖離できなくなった。
- **specializer の Swap 除去拡張**: `compile_binary_op` の fast path を一般化し、
  混在 Int/Float の `+ - * /` に加えて **`Int64 / Int64` 除算**（常に Float64 化）も
  コンパイル時にオペランドを各々 `ToF64` するようにして、ホットループ本体から残存
  `Swap` を排除（認識器ホワイトリスト適合）。
- **ガード**: (a) untyped な `+ - * /`（混在/Int除算/純 Float64）ホットループが実行時
  特殊化後に typed-loop として認識され、かつ specializer 出力に `Swap` を含まないこと
  を end-to-end で検査、(b) `typed_scalar_binary_instr` が出し得る全命令が typed-loop
  認識器に受理されることを単体トリップワイヤで検査。
- ドキュメント: `docs/vm/BINARY_DISPATCH.md` に「Two Binary-Op Codegen Paths」節を
  追加し、両 `compile_binary_op` と共有ヘルパに相互参照コメント。
### Bool 同士のビット演算子 `&` / `|` / `⊻` 対応 (Issue #8197)

- upstream で動く `true & false` / `true | false` / `true ⊻ true` が sjulia では
  `MethodError: no method matching &(::Bool, ::Bool)` だった（関数形 `xor(::Bool,::Bool)`
  は動作）。upstream `base/bool.jl` の `(&)(x::Bool,y::Bool)=and_int(x,y)` /
  `(|)=or_int` / `xor=(x!=y)` を移植。`⊻` と `xor` は sjulia では別関数なので両方に
  Bool メソッドを定義。`and_int`/`or_int` イントリンシックは両オペランドが Bool の
  とき Bool を返す（`vm/intrinsics_exec.rs`）ので upstream と一致。
- **配置がキモ（回帰回避）**: Bool メソッドは `base/bool.jl` ではなく `base/int.jl` の
  `Int64` メソッド**直後**に定義した。`&`/`|`/`⊻` は `Expr::Call` にロワリングされ
  メソッドテーブル経由でディスパッチされる。generic 関数内や mixed 型
  (`0x05 & 5`) のように exact な同型メソッドが無い呼び出しは、ランタイムに
  `CallTypedDispatch` の no-match フォールバック=**最初に登録されたメソッド**へ落ちる。
  `Int64` メソッド(`and_int` が両者を Int64 へ widen)は型安全なフォールバックで
  upstream 一致(`0x05 & 5 === 5`)だが、Bool メソッドを先頭にすると Bool 戻りスロットに
  widen 済み Int64 が入り `InternalError: LoadSlotBool` で破綻する。Int64 を先頭に
  保つことでこれを回避（フル `--release` 4057/4057 green）。

### ループ変数の型注釈 `for i::T in itr` 対応 (Issue #8208)

- upstream で動く `for i::Int64 in 0:(n-1)` 等が sjulia ではパーサで `unexpected
  token '::' ... expected 'in' or '='` になっていた（パーサ/lowering ギャップ、
  #8204 の typed for-loop 性能修正とは独立）。
- 修正2点:
  1. **パーサ** (`parser/collections.rs::parse_for_binding`): 単一識別子の後に
     `::` が続く場合 `parse_type_declaration` で `TypedExpression` 束縛を生成。
     `for (a,b)::T in itr` は upstream でも構文エラーのため非タプル経路に限定。
  2. **lowering** (`lowering/stmt/control_for.rs`): `TypedExpression` 束縛を検出して
     型を `var_type` に退避。`apply_var_type_convert` が upstream と同じ desugar
     `for #i in itr; i = convert(T, #i); <body>` を行い、各反復値を `T` へ変換。
- **隠し反復変数がキモ**: 整数レンジ fast path は**ループ変数自身を I64 カウンタ
  スロット**に使うため、`for x::Float64 in 1:3` のように body 内で `x` を再代入すると
  `AddConstI64Slot: expected I64` で破綻する。upstream 同様に反復は隠し変数
  `i#fortyped<span>` で回し、ユーザ可視の `i` は body 先頭の convert で別スロットに
  束縛する。これで Range/`=`/array-iterable/cartesian の全形が成立。
- convert 経由のため `InexactError`（`for i::Int64 in [1.5]`）も upstream とメッセージ
  一致。`typeof(e)` が `String` になる差異は convert 全般の既存バグ（本変更とは独立）。
- テスト: parser `test_for_loop_typed_variable_issue_8208`、lowering
  `test_typed_loop_variable_injects_convert_with_hidden_counter_issue_8208` /
  `test_untyped_loop_variable_has_no_convert_issue_8208`、fixture
  `loops/for_loop_typed_variable.jl`（julia パリティ）。

### untyped 引数の実行時特殊化本体を peephole 融合 (Issue #8205)

- untyped 引数の関数 `f(n)` を具体型 `Int64` で呼ぶと実行時に本体を特殊化する
  (#8167) が、その**特殊化本体だけ peephole 融合を通っていなかった**。結果
  `LoadSlotF64; LoadSlotF64; MulF64` のような未融合命令列で回り、typed 版
  (`LoadMulF64Slot` 等に融合済み・ops≈65) より命令数が肥大 (ops≈99) し、typed-loop
  fast path に乗ってもなお約 1.4x 遅かった。Aizawa attractor の untyped 版 for/while が
  これに該当 (引数 `n` が `Any` のままループ全体が未融合)。
- 修正: 特殊化本体の append を `Vm::install_specialized_body`
  (`vm/exec/call.rs`) に集約し、slotize 後に main コンパイラと同じ
  `peephole::optimize` (post-slotize) を実行。2 つの特殊化 site
  (`try_specialized_entry_for_runtime_call` と dispatch-loop specializer) が同じ
  ヘルパを通るようにして codegen 重複も解消 (#8192 の方向)。融合後命令は #8206 が
  back-edge として認識する `AddConstI64SlotAndJumpIfLe(.., header+1)` を含むため
  typed-loop 認識は維持される。
- 効果 (release, N=2M, aizawa): untyped FOR 0.53→0.42s, untyped WHILE 0.53→0.41s
  (いずれも typed とほぼ同速に)。結果 `r` は upstream julia と完全一致。
- テスト: unit `specialized_body_peephole_8205_tests` (VM 実行後の append 領域に
  `LoadMulF64Slot` が増えることを検証、fix 無しで fail することを確認済み) +
  fixture `loops/for_loop_untyped_arg.jl` (typed twin と一致 / julia パリティ)。

### 型付き内包表記 `Int[expr for ...]` の要素型欠落を修正 (Issue #8198)

- `Int[i^2 for i in 1:3]` が `Vector{Any}` になっていた（値は正しいが `eltype` が
  upstream と不一致）。`Float64[...]` や固定幅の `Int32[...]` は正常で、
  プラットフォームエイリアス `Int`/`UInt` のみ回帰。
- 根因: 型付き内包表記は本体を `Int(expr)` でラップして要素型を `infer_expr_type`
  （ValueType 経路）から決めるが、その数値コンストラクタ解決テーブル
  `normalized_constructor_tfunc` (`compile/expr/infer/expr_tfuncs.rs`) が
  `Int8`…`Int128`/`Float*` は持つのに**ワードサイズエイリアス `Int`/`UInt` を欠いて**
  いたため `Int(x)` が `Any` に落ちていた（JuliaType 経路は `Int` を別途特別扱い、
  compile 経路 `value_type_for_type_name` も対応済みだったので食い違い）。
- 修正: `normalized_constructor_tfunc` に `native_int_type_name()` /
  `native_uint_type_name()` に従う `Int`/`UInt` の分岐を追加（wasm32 の Int32 も考慮）。
- テスト: `array/typed_comprehension_int_alias_eltype_8198.jl`（julia パリティ 17 件、
  bare/scaled/filtered/2-D + `Int32`/`Int8` 回帰ガード、最終値は全 check の論理積）。
### 失敗した `@test`/`@testset` で CLI が非0終了するよう修正 (Issue #8191)

- upstream Julia は最上位 `@testset` の `@test` 失敗時に `TestSetException` を送出し
  プロセスが **exit 1** で終わるが、sjulia は失敗を**表示するだけで exit 0** だった。
  CI / `run.jl` 形式のスクリプトがテスト失敗を検出できない（fixture が末尾 `true` だと
  ハーネスも素通り＝false-green）。
- 修正方針: sjulia は `@testset` 失敗時に**例外を送出しない**設計（2565 個の `@testset`
  fixture を一括で赤化する巨大なブラスト半径を避ける）。代わりに VM に sticky フラグ
  `any_test_failed`（`vm/mod.rs`）を持たせ、失敗記録箇所（`builtins_macro/mod.rs` の
  `TestRecord`/`TestRecordBroken`、`exec/error_handling.rs` の `Instr::Test`/`TestThrowsEnd`）で
  立てる。CLI (`bin/sjulia/runners.rs::run_compiled_program`) が `vm.any_test_failed()` を見て
  非0終了。プログラム意味論と値ベースの fixture ハーネスには無影響。
- 観測挙動が julia と一致: 失敗 `@testset`→exit 1、成功→exit 0、bare 失敗 `@test`→exit 1、
  テスト無し→exit 0。
- テスト: `tests/testset_exit_code_8191_tests.rs`（5 ケース: 失敗/成功 testset、bare 失敗、
  テスト無し、後続成功 testset でも先行失敗が残る sticky 性）。
- 残課題（別 follow-up）: nextest ハーネス自体が `@testset` 失敗を検出する（既存の
  false-green fixture を赤化して個別修正する）大規模タスクは本 PR の範囲外。新規 fixture は
  最終値を `all(checks)` にして検証可能にする方針を継続。

### 混合 Int64/Float64 比較を値ベースの厳密比較に修正 (Issue #8187)

- `==` / `!=` / `<` / `<=` / `>` / `>=`（および `isequal` / `in` / tuple-`==`）の
  混合 `Int64`/`Float64` が、整数を `Float64` に昇格してから比較していたため、
  2^53 超の整数で結果が食い違っていた（例: `9007199254740993 == 9.007199254740992e15`
  が誤って `true`）。upstream `base/float.jl` の値ベース厳密比較に合わせて修正。
- 実装（Pure Julia の concrete メソッドは**使わない**: sjulia の compile-time dispatch が
  `==(::Int64,::Float64)` を `BigFloat`/`Float64` 呼び出しに coercion でmis-match し
  `big % big == 1.5` 等を壊すため。代わりに VM 側で厳密化）:
  - VM (`vm/numeric_identity.rs`): `cmp_i64_to_f64`（整数を丸めない厳密順序、NaN→None）を追加。
  - コンパイラ (`compile/expr/binary/mod.rs`): 静的に `Int64`×`Float64` の比較演算子を
    `CallDynamicBinaryBoth` 経由にルーティング（小さい整数リテラルは既存の安全な promote 速路）。
  - VM 実行: `binary_both.rs`（`exact_int_float_comparison` を promote 前に挿入）・
    `eval_numeric_binary_default`（`call.rs`）・`isequal`/`in`/tuple-`==` 系
    （`equality.rs`, `builtins_equality.rs`, `builtins_types.rs`）の昇格パスを `cmp_i64_to_f64` で置換。
- テスト: `comparison/mixed_int_float_exact_8187.jl`（julia パリティ 32 件）+
  `cmp_i64_f64_tests`（単体）。
- 残課題: (1) `UInt64`/`Int128`/`Float32`/`Float16` 混合、(2) **関数呼び出し/カリー化形**
  （`==(a,b)`, `filter(<(x),arr)`）は依然 promote（いずれも Issue #8199）。

### 混合 int/float 比較の値ベース厳密化を全幅へ一般化 (Issue #8199)

- #8187 の `Int64`×`Float64` 厳密比較を、`Int8`…`Int128` / `UInt8`…`UInt128` ×
  `Float16`/`Float32`/`Float64` の**全組**へ拡張。`UInt64(2^53+1) == 9.0e15` /
  `Int128`/`UInt128` × `Float64`、整数 × `Float32`（2^24 超）、`Float16` 混合がいずれも
  upstream と一致。`Float32`/`Float16` オペランドは可逆に `f64` 拡幅してから値ベース比較する
  ため精度非依存。
- 実装:
  - VM (`vm/numeric_identity.rs`): `cmp_i64_to_f64` を汎用 `cmp_integer_to_f64`
    (`NumericInteger`×`f64`, i128/u128 全域で厳密) へ置換し、ペア検出
    `mixed_int_float_ordering` / `mixed_int_float_values_equal` /
    `is_negative_zero_fixed_float` を追加。全呼び出し箇所（`binary_both.rs` /
    `call.rs` / `equality.rs` / `builtins_equality.rs` / `builtins_types.rs`）を一般化。
  - コンパイラ (`compile/expr/binary/mod.rs`): 静的ルーティングを `is_integer_type` ×
    `is_float_type` の任意の固定幅混合へ拡張。
  - Pure Julia (`base/operators.jl`): `isequal(::Integer, ::AbstractFloat)` /
    逆向きを追加し、符号付きゼロ規則（`isequal(0, -0.0)` は false）を全幅で適用
    （concrete メソッドではなく抽象シグネチャなので #8187 の BigFloat coercion ハザード非該当）。
- テスト: `comparison/mixed_int_float_widths_8199.jl`（julia パリティ 35 件、最終値は
  全 check の論理積で #8191 の false-green を回避）+ `mixed_int_float_tests`（単体）。
- 残課題: **関数呼び出し/カリー化形**（`==(a,b)`, `map(==(x),arr)`）は依然
  `==(x::Number,y::Number)` の promote-fallback 経由（#8187 由来・幅非依存の既知制約。
  `unsafe_trunc(UInt64/Int128,…)` 未実装と VM ディスパッチ介入が必要）。別 follow-up で追跡。

### 複数行の型付き配列リテラル `T[...]` の parse error 修正 (Issue #8188)

- `Bool[⏎ true,⏎ false,⏎]` のように要素を改行区切りで書くと parse error になっていた
  （単一行型付き・複数行型なしは通る）。`[` 直後の改行が読み飛ばされていなかったのが原因。
- 修正 (`parser/expressions/index.rs`): `T[...]`/`obj[...]` で `[` 直後の改行をスキップし、
  末尾カンマなしの改行や単一要素 + 末尾改行（複数行インデックス `v[⏎ 2⏎]`）も許容。
- テスト: `array/typed_array_multiline_8188.jl`（julia パリティ 9 件）。

### `a!=b`（識別子直後・空白なし `!=`）の lexer 誤字句解析を修正 (Issue #8194)

- lexer が末尾 `!` を直前の識別子に貪欲に取り込み、`a!=b` を `a!` `=` `b`（連鎖代入）と
  誤解釈していた（`c = a!=b` が `3`=b を返す等）。
- 修正 (`parser/lexer.rs`): 識別子正規表現の greedy `!?` は維持（`in!`/`push!` が
  キーワード/基底識別子に勝つために必要）しつつ、識別子末尾 `!` の直後が `=` の場合は
  lexer ラッパで `restart_from` により `!` を返却し `!=`/`!==` として再字句解析。
- テスト: `parse/bang_not_equal_8194.jl`（julia パリティ 13 件）。
### VM 計測: typed-loop 認識器の中断理由ログ (Issue #8193)

- #8183 で Float64 ホットループを native typed-loop 高速路に乗せたが、どのループが
  どの理由で認識に失敗し汎用解釈へ落ちるかを体系的に把握できていなかった。#8159 の
  sub-issue 群 (#8167-8173) は整数×汎用ディスパッチ経路が対象で、認識器経路は未カバー。
- 受け入れ条件①（計測のみ・挙動不変）として、認識器の中断点に理由を記録する診断を追加
  (`vm/executable.rs` `TypedLoopReject`)。`SJULIA_TYPED_LOOP_DEBUG` を設定すると
  predecode/特化時に `[typed-loop-accept]` / `[typed-loop-reject] reason=...` を stderr
  出力 (OnceLock キャッシュ・実行ホットパスには影響なし、`writeln!(stderr)` で
  `print_stderr` deny 回避)。理由種別: `unsupported-instr:<命令>` / `op-count-over-cap`
  / `slot-count-over-cap` / `no-exit` / `other-stack-or-target`。
- 計測結果 (Aizawa/IFS × untyped/typed, Base ノイズ差分): コアのホットループは
  4 変種すべて accept。untyped は specialize 前の汎用 Any 本体だけが
  `CallDynamicBinaryBoth` で reject (特化後 F64 本体は accept = #8183 機構の裏付け)。
  Base/prelude 側の主因は `StoreSlot`(99) / `CallDynamicBinaryBoth`(63) /
  `IndexLoad`(30) / `PushNothing`(26)… で、whitelist 拡張 (受け入れ条件②) の優先候補。
- テスト: `typed_loop_reject_reason_{unsupported_instr,op_count_over_cap,no_exit}_issue_8193`
  / `typed_loop_accept_leaves_reject_reason_unset_issue_8193` (`vm/executable.rs`)。

### AoT: n 項 `+`/`*`（`a + b + c`）の reachability 修正 (Issue #8180)

- Julia パーサは `a + b + c` を n 項呼び出し `+(a, b, c)` に畳む。AoT IR 変換は
  これを `((a + b) + c)` の入れ子 binary に戻すが、call graph の callee 収集
  (`collect_calls_in_expr`) が `+` へ辺を張っていたため、変分メソッド
  `+(xs...)`（`afoldl`）が到達可能扱いになり AoT が `HasShape{1}` を生成できず
  unsupported になっていた（upstream julia / sjulia VM では動作）。
- 修正: 畳み込み対象の演算子呼び出し（非 splat・kwargs なし・引数 ≥2）には
  call graph の辺を張らないようにした（`is_folded_binary_operator`、
  `map_operator_to_binop` と同期）。Aizawa attractor ベンチを自然な
  `sx + sy + sz` で AoT ビルド可能に。
- テスト: `nary_operator_call_does_not_mark_operator_reachable_8180`（call graph 単体）/
  `aot_nary_addition_compiles_without_afoldl_8180`（e2e）。
- 既知の残課題: n 項 `+` の**呼び出し結果型は依然 `Any` 推論**で、typed スロットへ
  代入すると Any→Float64 変換が AoT 非対応（Issue #6978/#6968）。二項化で回避。

### AoT: 分岐内初代入・分岐後参照の局所変数を関数スコープへ hoist (Issue #8181)

- `if`/`elseif`/`else` の分岐内で初めて代入した局所変数を分岐後に参照すると、
  codegen が `let` をその分岐ブロック内に出力していたため Rust スコープから外れ
  `cannot find value` でコンパイル不能だった（`--check` は通り `--emit-binary`
  で初めて失敗）。
- 修正: codegen が「入れ子ブロックで初代入され別スコープから参照される局所変数」を
  検出し、関数先頭の遅延宣言 `let mut x: T;` に巻き上げ、ブロック内の `Let` は
  代入として出力する（`compute_hoisted_locals` + `current_function_hoisted_locals`）。
  関数引数・ループ束縛変数は対象外。
- テスト: `aot_branch_assigned_local_used_after_if_compiles_8181` /
  `aot_multi_branch_assigned_local_compiles_8181`（e2e, `-D warnings`）。

### ベンチマーク: Aizawa attractor / IFS フラクタル 5 実装比較 (Issue #8183)

- Float64 スカラー演算が支配的な 2 ワークロードを Julia / juliars(AoT) / sjulia /
  sjulia(型注釈) / Python 3.14 で計測（`docs/benchmark/aizawa_ifs_comparison.md`）。
- 知見: AoT ≈ 公式 Julia / 汎用 float ループで sjulia は公式 Julia・AoT 比
  100〜200x、Python 3.14 比でも数倍遅い（`calc_pi` と逆転）/ 型注釈は Aizawa で
  −30% だが IFS では +29% 悪化。sjulia 最適化追跡として起票。
### VM: 汎用 Float64 ホットループの native 高速化 (Issue #8183)

- Float64 スカラーが支配的なホットループ (Aizawa Euler 積分 / Barnsley fern IFS)
  が公式 Julia/AoT 比 100-200x 遅く、型注釈が IFS で逆効果だった件を 3 段階で解消。
  N=5M で untyped/typed とも **3.6-5.8x** 高速化し、4 変種すべてが native
  typed-loop 高速路に乗る (出力は upstream と bit 一致)。
  - **Stage 1**: 混合 `Int/Float` 算術 (`+ - * /`) を動的メソッド `Call` ではなく
    typed 昇格経路 (`…ToF64; <op>F64`) へ (`compile/expr/binary/mod.rs`)。比較は
    厳密性のため除外。
  - **Stage 2**: `TypedLoopOp` に `DivF64`/`ModI64`/融合 I64 load/`NegF64` を追加、
    `MAX_TYPED_LOOP_OPS` 64→128・`TYPED_LOOP_SLOT_CAP` 16→24 (`vm/executable.rs`)。
  - **Stage 3**: 引数型特化が混合除算を `Swap;ToF64;Swap` で昇格し認識を阻んで
    いたのを `compile_numeric_as_f64` で各オペランド個別 F64 強制し解消
    (`vm/specialize/expr.rs`)。
- 副産物: 混合 `== / <= / >=` の 2^53 超精度欠落 (既存バグ) を検出 → #8187 起票。

### VM 実行エンジン: 動的二項演算ディスパッチのper-call-siteキャッシュ (Issue #8168)

- `CallDynamicBinaryBoth`（両オペランドが `Any` で実行時解決する二項演算）は、
  毎回オペレータの候補メソッド一覧（例: `+` は 24 候補）を走査して型適合する
  メソッドを選んでいた。
- 構造体×構造体オペランドに限り、選ばれるメソッドはオペランドの型名で完全に
  決まる（resolver 内の値依存ガード `is_rust_dict_parametric_mismatch` /
  `is_struct_dict_bare_mismatch` は構造体ペアでは発火しない）ため、
  `call_site_ip → (left_type_hash, right_type_hash) → Option<func_index>` の
  キャッシュ `binary_both_dispatch_cache` を追加。既存 `dispatch_cache` と同じ
  「実行中は無効化しない」寿命方針に揃えた。resolver 本体は
  `resolve_binary_both_candidate` に抽出してキャッシュ miss 時のみ呼ぶ。
- 効果（`Vector{Any}` 中の `V2` 構造体を `+` で畳むホットループ, @time min）:
  n=1M 9.31s→6.95s (約 −25%)、n=2M 18.64s→13.91s (約 −25%)。
  calc_pi は無回帰（ホットパスが resolver を通らないため; 値は #8167 と同等で
  upstream julia と一致）。構造体・自作数値型など多態的な二項演算が対象。
- 回帰テスト: `any_struct_add_cached_dispatch_matches_upstream_8168`（正当性）/
  `any_struct_add_takes_binary_both_resolver_cache_8168`（fast-path; profiling）。

### VM 実行エンジン: untyped 関数の呼び出し型特殊化を直接ディスパッチ化 (Issue #8167)

- untyped な callee (`mygcd` など) を `CallSpecializeI64Slots` で呼ぶと、毎回
  `SpecializationKey { func_index, arg_types: vec![I64; n] }` を再構築して
  `Vec` キーの HashMap を引き、さらに callee の `param_slots` を毎回 clone して
  いた (gcd 内側ループでは 1 呼び出しあたり 2 回のヒープ確保 + Vec ハッシュ)。
- `(spec_func_index, arity)` をキーにした軽量キャッシュ
  `specialization_i64_cache` を追加。`CallSpecializeI64Slots` の引数は構造上
  常に I64 なので、初回特殊化後は解決済みレコード `I64SpecDispatch`
  (entry / code_end / fallback_index / local_slot_count / 共有 `Rc<[usize]>`
  param_slots) へ直接ディスパッチする。#8159 案1 の「2 回目以降は
  `CallResolvedI64Slots` 相当の直接呼び出しで飛ぶ」を no-JIT 制約内で実現
  (生成するのは VM バイトコードのみ、ネイティブコード生成なし)。
- 効果 (calc_pi untyped, コード内 `@time` の min): N=2000 0.372s→0.265s
  (約 −29%)、N=3000 0.844s→0.595s (約 −30%)。untyped 定義が typed-args 版と
  ほぼ同速になった。結果は upstream julia と一致。回帰テスト
  `untyped_calc_pi_uses_specialize_i64_dispatch_cache_8167` (profiling feature)
  で fast-path が踏まれることを保証。詳細: `docs/benchmark/calc_pi_gcd_comparison.md`。
### `using Optim` の起動が ~5.5s → ~0.28s (Issue #8182)

- iOS サンプル `advanced/optim_package.jl` が遅い件のベンチマークから判明。Base キャッシュ
  ロード済みでも `using Optim` だけで **~5.5 秒**かかり、`SJULIA_COMPILE_PROFILE` で
  **`compile.build_method_tables` が ~5.3 秒(全体の 97%)**を占めていた。サンプルが実際に
  走らせるソルバ(Brent/GoldenSection/NelderMead/GradientDescent)は合計 <50ms で無関係。
- 原因は **`_bfgs`(`packages/Optim/src/.../bfgs.jl`)のコンパイル時戻り値型推論の組合せ爆発**。
  `_bfgs` は `while` ループ内でクロージャ `phidphi` を定義し、それを HagerZhang ライン
  サーチの深い相互再帰呼び出し木(`hagerzhang_search → _hz_secant2! → _hz_update! →
  _hz_bisect!`)へ引き回す。ループ fixpoint 下で具体クロージャに対し木全体が再特殊化され
  指数的に膨張する。BFGS はサンプルで未使用だが `using Optim` で必ずロードされ全プログラムが
  この費用を払っていた。依存単体(`using NLSolversBase`/`LineSearches`)は ~50-90ms と高速で、
  爆発は `_bfgs` 本体推論に局在。
- 修正は **`_bfgs(...)::MultivariateOptimizationResults` の戻り値型注釈**(常に厳密に成立)。
  `build_method_tables` は宣言型がある関数の本体推論をスキップするため、
  `build_method_tables` が **5097ms → 42ms**、`using Optim` が **~5.5s → ~0.28s**、フル
  サンプルが **~5.57s → ~0.32s** に短縮。サンプル出力・BFGS の厳密パリティ
  (`iterations==1`, `f_calls==g_calls==3`)は不変(`optim_bfgs_*` フィクスチャ green)。
- `hagerzhang_search` 単体への注釈は無効(5073ms のまま)で、爆発が `_bfgs` 本体側にある
  ことを確認済み。一般修正(相互手続き戻り値型推論への深さ/作業量ガード。例外型推論は
  既に `depth > 16` で打ち切る)は #8182 で追跡。注釈には #8182 参照コメントを付与。詳細は
  `docs/vm/OPTIM.md` の「Load-time performance」節。

### StaticArrays: all-static 融合ネスト broadcast (`abs.(SVector .+ SVector)`) のクラッシュ (Issue #8176)

- `abs.(v .+ w)`(`v`,`w` ともに `SVector`)のような **全 static の融合ネスト** broadcast が
  `Index [1] out of bounds for array with shape [0]` で落ちていた。sjulia は `.`-chain を
  ネストした `Broadcasted` ツリーへ融合する。外側 `materialize(Broadcasted(abs, (inner,)))`
  は `copy(instantiate(bc))` を呼ぶが、内側の全 static `Broadcasted` は軸が空 `()`
  (素の static 配列を汎用 shape 系が 0 次元スカラー扱いするため)になり、
  `_broadcastable_shape(::Broadcasted)` が空の `ax[1]` を参照して **`copy` 到達前に**
  クラッシュ。よって `copy` の static hook(#8161 のツリー分類)が発火できなかった。
- 修正: `_broadcastable_shape(x::Broadcasted)` に `n == 0` ケースを追加し、空軸では
  スカラー shape `()` を返す(`base/broadcast.jl`)。これで `instantiate` が通り、`copy` の
  hook が all-static ネストを **static path** で処理して `SVector`/`SMatrix`(upstream と同型)を
  返す。mixed static/dynamic の融合ネスト(#8161)は引き続き動的 `Array` を返す。回帰:
  `tests/fixtures/static_arrays/static_arrays_fused_nested_broadcast_8176.jl`(upstream パリティ)。

### StaticArrays: SVector .- Vector など mixed static/dynamic broadcast (Issue #8161)

- `StaticArray` と **動的配列** を broadcast すると(`SVector .- Vector`,
  `abs.(SVector .- Vector)`, `SMatrix .- Matrix`, `SVector .+ range`)、動的 operand が
  スカラー扱いされ `-(SVector, Float64)` の誤 `MethodError` になっていた。static-broadcast
  hook(`packages/StaticArrays/src/broadcast.jl`)が「static ⊙ scalar」のみ処理し、
  コンテナ operand があると generic pipeline に委譲、そこで `StaticArray` が 0 次元スカラー
  扱いされるのが原因。
- 修正は **upstream StaticArrays の `BroadcastStyle` 優先規則を再現**:
  `StaticArrayStyle ⊙ DefaultArrayStyle{0}`(scalar)→静的結果、
  `StaticArrayStyle ⊙ DefaultArrayStyle{N≥1}`(動的配列)→動的結果。hook が融合・ネストを
  含む operand ツリー全体を再帰分類し、static のみ(+scalar)→ `SVector`/`SMatrix`、
  動的配列が混在→ static 葉を `collect` して generic pipeline で再 materialize し plain
  `Array` を返す(upstream は `Sized*` を返すが subset に `Sized*` 型は無く、plain `Array`
  が値・要素型・表示を再現)。Base に `Broadcasted` introspection ヘルパ
  (`_is_broadcasted`/`_broadcasted_args`/`_materialize_broadcasted` 等)を追加し、
  package 側が Base 型名を跨いで参照せず・脆い struct-typed dispatch を入れずにツリーを歩く。
  回帰: `tests/fixtures/static_arrays/static_arrays_mixed_broadcast_8161.jl`(upstream パリティ)。
- 既知の別バグ: all-static の**融合ネスト**(`abs.(@SVector .+ @SVector)`)は lowering 層で
  hook 未発火のため別途 #8176 起票(本 issue の範囲外)。

### module-qualified call が runtime dispatch を行わず catch-all を静的バインド (Issue #8158)

- `Module.f(x)` の修飾呼び出しが、引数の静的型が `Any`(無注釈パラメータや
  既定値付き kwarg)で callee に catch-all `f(::Any)` がある場合に、実行時の値で
  dispatch せず **catch-all を静的バインド**していた。unqualified `f(x)` は正しく
  実行時 dispatch するのに修飾形だけ誤る非対称。実害は
  `SciMLBase._callbacks(cb::CallbackSet)` が catch-all `(cb,)` に誤 dispatch し、
  `CallbackSet` 内の全コールバックを無言で無効化(`solve(...; callback=CallbackSet(...))`
  でバウンスせず床を突き抜け、カウンタも 0 のまま)。
- 根因は bare-call path(`compile_generic_dispatch_call`)が持つ広い
  `use_runtime_dispatch` 判定を、qualified path(`compile_module_call_via_method_table`)
  が共有せず、狭い abstract-array-family probe(case4)のみ持っていたこと。判定を
  共有ヘルパ `should_runtime_dispatch`(`compile/expr/call/dispatch.rs`)へ抽出し、
  両 path から使用。これで `Any` 引数 + 複数メソッドの単項/多項どちらでも修飾呼び出しが
  unqualified と同一に runtime dispatch する。SciMLBase の `_callbacks` は 2 メソッド
  形式(`_callbacks(cb::CallbackSet)=cb.callbacks; _callbacks(cb)=(cb,)`)に戻し、
  isa-branch ワークアラウンドを撤去。回帰:
  `tests/fixtures/modules/module_qualified_call_runtime_dispatch_8158.jl`(upstream パリティ)
  と `tests/fixtures/packages/ordinarydiffeq_callbacks_7983.jl` の CallbackSet ゲート。

### OrdinaryDiffEq milestone #50: Plots recipe pipeline (Issue #7987)

- `plot(sol)` の hard-coded special case を **recipe pipeline** へ置換。
  `apply_recipe(sol; idxs, vars, denseplot, plotdensity, label)` を
  AbstractODESolution に登録し、`Series` リスト + 3D ヒントを返す recipe とした。
  `plot`/`plot!(::ODESolution)` がそれを apply して `Plot` を assemble する
  (artifact 形状は従来の直接変換と同一 = 無回帰)。
- recipe 属性が pipeline を流れる: `idxs`(成分/phase) / `vars`(idxs 上流別名) /
  `denseplot`+`plotdensity`(callable `sol(t)` #7982 を fine grid サンプル)。
  `plot!(sol)` overlay も追加。
- 完全な `RecipesBase.@recipe` マクロは未実装: sjulia のクロスモジュール抽象
  dispatch 制約のため、concrete `plot(::ODESolution)` entry が 抽象
  `apply_recipe(::AbstractODESolution)` へ委譲する indirection で代替
  (generic `plot(x)`→recipe や `Plots.apply_recipe` 直呼びは外部モジュールから
  dispatch 不発)。fixture: `ordinarydiffeq_recipe_plot_7987`(無回帰),
  `ordinarydiffeq_recipe_idxs_7987`(idxs/vars/denseplot/overlay)。

### OrdinaryDiffEq milestone #50: integrator interface 残スコープ (ReturnCode / tstops / step!-by-dt) (Issue #7981)

- **ReturnCode**: `sol.retcode` を `:Success` symbol から実 `ReturnCode` 値へ。
  `ReturnCode` は **struct-namespace**（`ReturnCodeNamespace` の `const` インスタンス）
  として実装 — module だと re-export/alias 経由の member access (`ReturnCode.Success`)
  が sjulia で "expected struct, got Module" になるため。`OrdinaryDiffEq` 側は
  `const ReturnCode = SciMLBase.ReturnCode`（struct インスタンスの alias なら field
  access が効く）。`successful_retcode` は単一メソッド+isa 分岐（#8158 回避）で
  `ReturnCodeValue`/`ODESolution`/`Symbol`(旧 parity) を処理。
- **tstops**: `solve`/`init` が `tstops=[...]` を受け、`_merge_tstops` で出力グリッドへ
  マージ（要求時刻に step 着地。MVP は tstops を保存点化）。
- **step!(integ, dt, stop_at_tdt)**: 次出力点ではなく `dt` だけ進める形を追加。
- 既存 fixture の `sol.retcode === :Success` 6 箇所を `successful_retcode(sol)` へ更新
  （`===` は override 不可のため必須）。新 fixture: `ordinarydiffeq_remake_7981`,
  `ordinarydiffeq_retcode_7981`。

### OrdinaryDiffEq milestone #50: view/SubArray 状態対応 + out-of-place vector RHS 回帰修正 (Issue #7986, #8163)

- **#7986 broader array surfaces**: `view`/`SubArray` を初期状態にした `solve` /
  `init`/`step!` が `SubArray + Vector` の operator gap と `ismutable(SubArray)==false`
  により失敗していた。solve 開始時に `SubArray` を dense `Vector` へ densify
  （upstream も u0 を内部 dense コピー、backing buffer 非破壊）して対応。`SVector`/
  scalar は不変なので static-state 経路（#7984）は維持。densify は **live** な
  `OrdinaryDiffEq._tsit5_solve` override（#8104）と `SciMLBase.init` の両方に適用。
  sparse は同規則で densify されるが `SparseArrays` subset が `sparse`/`sparsevec`
  未実装のため solver に到達できない（densify 方針を文書化）。
- **#8163 out-of-place vector RHS 回帰修正**: #8094 の buffered fast path が
  `_rhs!`（out-of-place では buffer を埋めない）を使い、out-of-place の **vector**
  RHS で stale stage を読んで誤結果（`[1.98,1.97]` vs 正解 `[0.368,0.135]`）を
  返していた。buffered path を `prob.isinplace` でゲートし、out-of-place は
  generic operator 経路（`Vector`/`SVector`/scalar で正しい）へ。3 箇所
  （override / `SciMLBase._tsit5_solve` / `_tsit5_solve_interval`）を修正。
- 教訓: `OrdinaryDiffEq._tsit5_solve` が `SciMLBase._tsit5_solve` を **override** する
  ため、solver core の変更は override 側に入れないと `using OrdinaryDiffEq` 経路に
  反映されない（#8104）。

## 最新対応 (2026-06-27)

### OrdinaryDiffEq milestone #50: CallbackSet 修正 / StaticArrays variant / solve benchmark (Issue #7983, #7984, #8094, #8158)

- **#7983 CallbackSet が全コールバックを黙って無効化**: `solve(prob, Tsit5(); callback=CallbackSet(cb1, cb2))`
  が 1 つもコールバックを発火しなかった。`_callbacks(callback)` の dispatch が
  specific `_callbacks(cb::CallbackSet)` ではなく catch-all `_callbacks(cb)` に
  解決し、`CallbackSet` が展開されず `(cb,)` に包まれていた。実行時 `isa` 分岐の
  単一メソッドに置換して修正（根因の specialization 依存 VM mis-dispatch は #8158）。
  fixture は最終値 `true` のみ検証され `@test` 失敗は throw しないため、壊れた
  CallbackSet が見逃されていた → 実結果から計算した boolean で終わるよう堅牢化。
- **#7984 StaticArrays variant**: out-of-place `@SVector` RHS + `@SVector` 初期状態を
  `solve(prob, Tsit5(); ...)` で解け、static 要素型が end-to-end で保持される（widening
  しない）ことを fixture で固定（dynamic `Vector` 解と tight parity）。
- **#8094 perf**: Lorenz solve の Criterion benchmark `vm_ode_tsit5_lorenz_benchmark`
  を追加（in-place buffered stepper の回帰検出）。`SVector` 経路はこの VM では
  in-place `Vector` より遅いと判明したためサンプルは in-place のまま。
- 関連: mixed `SVector .- Vector` broadcast の gap を #8161 として起票。
- `ordinarydiffeq_dense_output_7982` / `ordinarydiffeq_secondorder_7985` の fixture も
  最終値 boolean gate へ堅牢化（#8158-class 予防）。

### AoT top-level `@time` の末尾 `result;` パス文を除去 (Issue #8150, #8154 重複)

top-level `@time <expr>` がマクロ展開の末尾 `result`(戻り値)を生成 `main()` に
`result;` という dead path statement として残し、rustc `path_statements` lint
(`-D warnings`、AoT クレートは #7076 で必須)で弾かれていた。`convert_let_block_stmt`
が `#` 接頭辞 temporary しか drop せず、`@time` の result(`#`無し)が漏れていた。
statement 位置の末尾 bare `Expr::Var` は副作用無し・値破棄の no-op なので `#` に
関係なく drop するよう一般化(`println(...)` 等の `Expr::Call` は保持)。e2e の
top-level `@time` が `-D warnings` を通る回帰テストを追加。

### 型注釈 (`::Int` 戻り値 / `tmp::Int = b` ローカル) のホットパス Convert 削除 (Issue #8147)

戻り値型注釈 `function f()::Int` と型付きローカル `tmp::Int = b` は lowering で
`convert(Int, x)` に展開され、コンパイル時に `CallBuiltin(Convert, 2)` を常に出して
いた。値が既に `Int64` でも毎回メソッド探索 + dispatch が走り、さらに
`compile_convert` が結果型を常に `Any` として返すためスロット型が unknown に縮退し、
後続の typed 命令(`LoadSlotI64` 等)を失って大幅に遅くなっていた(issue 計測で
戻り値付き ~5x、ローカル付き ~28x)。`compile_convert` で第1引数が具体型名のとき、
値の推論型が一致すれば `convert(::Type{T}, x::T) = x` の恒等変換として Convert を
**完全に省略**(値のみコンパイル)し、変換が必要な場合も結果型を具体型として返して
スロット specialization を維持するよう修正。MWE の 3 変種は同一バイトコード・同一
速度になった。詳細は [DONE.md](./DONE.md)。

### AoT: `@time` 生成コードの `as i64.wrapping_sub()` 括弧欠落 (Issue #8146)

`juliars`(AoT)が `@time` を含むコードを Rust に変換すると、`time_ns()` が
`... as i64` を出力し、続く経過時間の減算が `... as i64.wrapping_sub(t0)` となって
Rust では `as (i64.wrapping_sub(t0))` と解釈され「cast cannot be followed by a method
call」でコンパイル不能だった。`emit_arithmetic` の wrapping 整数演算で receiver を
常に括弧で囲む(`({}).{}({})`)よう修正。生成コードは `(... as i64).wrapping_sub(t0)`
となり `cargo check -D warnings` を通る。詳細は [DONE.md](./DONE.md)。
### bare/braces パラメトリックコンストラクタがユーザ inner/outer を選ぶ (Issue #8121, #8103 続き)

明示的 inner コンストラクタを持つパラメトリック struct は upstream Julia では
デフォルトフィールドコンストラクタを**生成しない**ため、`Foo(args)`(bare)/
`Foo{T}(args)`(braces)はユーザ inner/outer を呼ばねばならない。根因は
`register_inner_constructors` の `skip_this_struct`(キャッシュ Base struct を
スキップする最適化)が、**outer コンストラクタを持つユーザ struct**を非空メソッド
テーブルゆえに誤って「キャッシュ済み Base struct」と分類し inner 登録をスキップして
いたこと(precompiled Base 使用時)。判定を作業用 `method_tables` ではなく元の
`cached_method_tables` 基準に修正。併せて、inner と outer が同一値パラメータ
シグネチャを持つ場合(例 `Angle2d{T}(theta::Number)` vs `Angle2d(theta::Number)`)、
`add_method` の dedup が inner で outer を置換し、bare 呼びが未束縛 `T` の inner に
ディスパッチして `UndefVarError: T` になる回帰を、両メソッドを保持する
`add_method_keep_existing`(セレクション tie-breaker 4 が `where` 数の少ない outer を
bare 呼びに選ぶ)で解消。braces 経由の inner 呼びは型パラメータを束縛する
`CallStaticParametric` を発行し `new{T}` / `T(x)` を解決(Rotations `Angle2d`、
名前替え inner `Q{S}(x) where {S}` を含む)。

### ユーザー定義 `Base.getproperty` override を尊重 (Issue #8127)

構造体の `x.f` は上流 Julia では常に `getproperty(x, :f)` に lower され、既定実装が
`getfield` にフォールバックする。従来 sjulia はコンパイル時に宣言フィールドへ直接
解決していたため、ユーザー定義 `getproperty` が無視され、未宣言の計算プロパティは
`Unknown field` でコンパイルエラーになっていた。`compile_field_access` で、レシーバ
構造体型に対し `getproperty` dispatch がユーザーメソッドへ解決される場合に
`getproperty(x, :f)` 呼び出しへ経路付けするよう修正。宣言フィールドも override の
`getfield` フォールバック経由で従来どおり動作する。関数 specializer も override 検出時
(`disable_field_access_specialization`)は直接 `GetField` を出さずインタプリタ経由に
フォールバックさせ、ホットループでの bypass を防止。詳細は [DONE.md](./DONE.md)。

### 再帰・相互再帰なネスト closure の自己/兄弟参照 (Issue #8118)

外側ローカルを capture するネスト関数(= closure)が、自分自身を再帰呼び出ししたり
相互再帰の兄弟を呼べず `Unknown function` になっていた(自己/兄弟名が capture 環境に
無く、closure は環境経由でしか呼べないため)。自己呼び出しは現フレームの capture から
closure を再構築して呼ぶよう、兄弟(非 closure)呼び出しは `CreateClosure` がフレーム
に無い capture 名を enclosing scope の `parent#name` 関数値へフォールバック解決する
よう修正(Issue #8105 の非 closure 経路と対になる)。詳細は [DONE.md](./DONE.md)。
### ネストしたパラメトリックコンストラクタ引数の型パラメータ欠落でディスパッチ失敗 (Issue #8090)

`SMatrix{2,2}((1.0,2.0,3.0,4.0))` のように「書いた型パラメータ数(2)」が
「宣言された具体型のパラメータ数(`SMatrix{M,N,T}` の 3)」より少ない
コンストラクタ呼び出しの結果を、**ローカル変数に束縛せず直接(ネストして)**
別メソッドの引数に渡すと、引数型が末尾パラメータ未束縛の `SMatrix{2,2}` として
推論され、フルパラメータで特殊化したメソッド `Wrap(m::SMatrix{N,N,T})` に
静的ディスパッチが一致せず `MethodError` になっていた(束縛してから渡すと
スロット型が `Any` に広がり実行時ディスパッチに回るため動作)。修正は
`infer_julia_type` のパラメトリックコンストラクタ枝で、書いた型パラメータ数が
宣言数より少ない(末尾未束縛)場合に静的型を `Any` へ広げ、束縛経路と同様に
実行時多重ディスパッチへ回すようにした(`SMatrix{2,2,Float64}` の具体値で正しい
メソッドが選ばれる)。fixture `static_arrays/nested_parametric_ctor_arg_8090.jl`
(ネスト vs 束縛、2×2 Float64 / 3×3 Int64、上流 julia と出力一致)。

### StaticArrays.jl Phase 4–5 完了でマイルストーン #7433 完了 (Issue #7460, #7461)

StaticArrays immutable MVP を Phase 4(プロトコル: 索引・反復・map・単項 `-`・
broadcast・convert)と Phase 5(小型線形代数: `transpose`/`tr`/`diag`/`det`/`inv`/
`dot`/`norm`)まで完成。broadcast は `v .+ 10`/`sin.(v)` 等が**静的配列**を返し、
非 broadcast `v + 10`/`sin(v)` は上流同様エラー(parity 維持)。実装は VM の
`iterate` アーム(`StaticArrayInline`)と Base broadcast の `_STATIC_BROADCAST_HOOK`
(`Ref{Any}` 経由の動的呼び出し)による。行列結果は runtime `SMatrix{M,N}` 構築
未対応(#8125)のためリテラルサイズ分岐。保留: `adjoint`(#8132)/型一致 `collect`
(#8131)/`MArray`/`SizedArray`/分解系。詳細は [DONE.md](./DONE.md)。
### collect(::StaticVector) が要素型を保持 (Issue #8131)

`collect(SVector(1,2,3))` が `Vector{Any}` に広がるバグ(値は正しいが要素型喪失)を
修正。根因は当初の issue 推測(`_collect` の特異点ディスパッチ)ではなく **1点**:
`eltype` ビルトインが `StaticArrayInline`/`StaticArray` キャリアを処理せず `Any` を
返していた(不透明 `itr::Any` のジェネリック `_collect` 内で `eltype(itr)` が `Any`
→ `Vector{Any}`。静的型が判明する位置では純 Julia `eltype(::StaticArray)` が走り
マスクされていた)。`eltype` ビルトインに静的配列 arm を追加(コード差分は eltype の
み。`StaticArrayInline` の iterate は origin/main の #7460 Phase 4 が既に対応済み)。
`collect(::StaticVector)` が `Vector{T}` で上流一致、`sum`/`for`/内包表記も動作。
`collect(::StaticMatrix)` の平坦化(Matrix でなく Vector)は #8139 で追跡。fixture:
`static_arrays/static_arrays_collect_eltype_8131.jl`。

### collect(::StaticMatrix) が 2-D 形状を保持 (Issue #8139)

`collect(SMatrix{2,2}(1,2,3,4))` が平坦 `Vector{Int64}` になり 2-D 形状を喪失する
バグ(#8131 の続き、値・要素型は正しく形状のみ誤り)を修正。ジェネリック
`collect(itr)` は `IteratorSize(itr)`(`itr::Any`)で経路を決めるが、base には
非 `Array` の `AbstractArray`(StaticArrays の `SMatrix{2,2,Int64} <: ... <:
AbstractArray{Int64,2}`)に一致する規則が無く、ジェネリックな
`IteratorSize(::Type)=HasLength()` に落ちて `_collect` が 1-D Vector を生成して
いた。上流は型レベル `IteratorSize(::Type{<:AbstractArray{<:Any,N}})=HasShape{N}()`
だが、sjulia のディスパッチャは静的配列の抽象スーパータイプ鎖越しに `N` を束縛
できない(`::Type{<:AbstractArray{T,N}}` は plain `Vector{Int64}` でも `T`/`N`
未束縛)。そこで値ベースの `IteratorSize(a::AbstractArray)=HasShape{ndims(a)}()` を
`base/generator.jl` に追加(`Array`/`AbstractRange`/`Memory` は既存のより特異な
メソッドを保持)。これは静的 `Any` 引数を通る `collect` 内でも実行時値で正しく
ディスパッチする。効果: 正方/非正方/Float の `SMatrix` が `Matrix{T}` に、
`SVector` は `Vector{T}` のまま(#8131)、plain Array は不変。残: 完全に静的型が
判明する `IteratorSize(SVector(1,2,3))` のトップレベルインライン呼びは依然
コンパイル時に `HasLength` へ devirtualize する(`collect` の実行時経路は不変、
ユーザ可視のバグは解消)。fixture:
`static_arrays/static_arrays_collect_matrix_shape_8139.jl`。

### Rotations.jl サポート MVP 完成 (Issue #7434, Phase 0–5 #7471–#7476)

`extern/Rotations.jl/src/` を参照に、純 Julia の Rotations.jl MVP を
`subset_julia_vm/packages/Rotations/` にバンドル。サポート型: `Rotation`,
`RotMatrix{2,3}`/`Angle2d`, `RotX/Y/Z`, `AngleAxis`, `RotationVec`,
`RodriguesParam`, `MRP`, `QuatRotation`(`.w/.x/.y/.z` + `slerp`),
ジェネレータ(`Angle2dGenerator`/`RotationVecGenerator`/`skew`/
`isrotationgenerator`)。インターフェース: `rotation_angle`/`rotation_axis`/
`rotation_between`(2D/3D)/`isrotation`/`Rotations.params`/`Tuple`/`*`(回転・合成)
ほか。上流 Rotations 1.7.1 と数値一致(fixtures 8 本、oracle 照合済み)。
sjulia 適応は **単一メソッド+`isa`分岐(#7960)**、**`StaticMatrix` を継承しない
(#8103-B)**、**Tuple ベース matvec(#8090)**、**`QuatRotation` は `.w/.x/.y/.z`
を実フィールド化(#8127 回避)**。途中で発見した依存ギャップを修正/起票:
StaticArrays スカラ除算・`norm`/`normalize`(#8125 修正)、列優先格納(#8084)、
中核型システム(#8092)、コンストラクタ特定性(#8103)。詳細と保留範囲(ForwardDiff/
RecipesBase/Unitful/乱数/`RotMatrixGenerator`/exp・log マップ/全 Euler 変種/eigen)は
[ROTATIONS.md](./ROTATIONS.md) を参照。
### Module 値を介したメンバアクセス（ネスト/const エイリアス）(Issue #8113/#8114)

`Module` 値を介してメンバ（型/関数/const）にアクセスすると失敗していた:
- #8113 ネスト sub-module: `module Outer; module Inner; struct T1 end end end;
  Outer.Inner.T1` → コンパイルエラー `Field access requires a struct type, got Module`。
- #8114 const モジュールエイリアス: `const MA = Mod1; MA.S` → 実行時
  `GetFieldByName: expected struct, got Module`。

根因（共通）: 修飾アクセス経路がトップレベル `Module.member` は解決するが、(a) `const X =
Mod`（ユーザモジュール）をエイリアス登録せず、(b) ネストした多段パス（object が `Var` でなく
`FieldAccess`）を `compile_module_function_ref` に回さず、中間 `Module` 値を struct として
扱っていた。さらに修飾呼び出し `AA.B.C.g()` はエイリアス解決が**全体名のみ**で、ルート
セグメントのエイリアス（`AA`→`A`）を辿らなかった。

修正:
1. `compile/stmt.rs`: `const X = <ユーザモジュール>` を `module_aliases` に登録
   (`module_functions`/`module_exports` で判定)。
2. `compile/expr/struct_.rs`: 多段の既知モジュールパス（`resolve_user_module_path`、ルート
   エイリアス解決込み）を `compile_module_function_ref` に回す。パスのルートが非モジュール
   ローカルでシャドウされる場合は除外（#7245 維持）。
3. `compile/expr/call/module_call.rs`: `resolve_module_alias_path` でルートセグメントの
   エイリアスも解決（`AA.B.C` → `A.B.C`）。

型/関数/const メンバ、2段以上、エイリアス起点のネスト鎖まで上流パリティ。
回帰: `module_tests::module_value_field_access_8113_8114`（17 assertions）。

### ネストしたローカル関数が同名グローバルをシャドウしない (Issue #8105)

`g() = 1`（グローバル）が存在する状態で `function h(); g() = 2; return g(); end` の
ように内側で同名のネストローカル関数を定義しても、`h()` 内の `g()` 呼び出しがローカルの
`h#g`（=2）ではなくグローバル `g`（=1）に解決される回帰。さらに値参照経路
（`f = zztop; f()`）が内側関数の本体を拾うという第二の症状もあった。根因は2つ:
(1) 小さな純粋関数のインライナ (`ir_inline.rs`) が `Stmt::FunctionDef` のネスト関数名を
**ローカル束縛として登録していなかった**ため、後続の `g()` 呼び出しが同名グローバル本体に
インライン展開されていた。(2) ネスト関数を **共有ショート名メソッドテーブル**に登録して
いたため、`MethodTable::add_method` のシグネチャ dedup により内側の `g()` が
グローバルの `g()` を**置換**し、`emit_function_value` 経由の値参照が内側本体を指していた。
修正: (1) `inline_block` でブロック内の直下 `FunctionDef` 名を事前にローカル束縛として登録
（Julia のローカル関数定義巻き上げに準拠）。(2) `build_method_tables` でネスト関数を
`parent#name` の**修飾名テーブルのみ**に登録（`function_infos`/`function_indices` と整合）。
(3) これだけだとショート名テーブル依存だった**自己/兄弟再帰**が壊れるため、
`try_compile_nested_scope_call` を追加し、ネスト関数本体内の素の呼び出しを
`current_function_name` のレキシカルスコープ連鎖 (`a#b#c`) を辿って修飾名テーブルへ解決
（クロージャはキャプチャ経路に委ねるため除外）。fixture
`closures/nested_local_shadows_global_8105.jl`（julia/sjulia parity 12/12）。
**既知の未対応** (Issue #8118): 他のローカルをキャプチャするネスト**クロージャ**の
自己/相互再帰（`g(n)=…g(n-1)` で `x` もキャプチャ、相互 `a`/`b`）は本修正対象外。

### 回帰修正: インラインラムダ HOF の戻り値型がローカル束縛へ伝播しない (Issue #8105 後退)

#8105 のネスト関数登録変更（ショート名テーブル除外、`parent#name` 修飾名のみ）の副作用で、
`y = reduce((acc, x) -> acc + x * 0.5, [1, 2, 3])` の `y` が `Float64` でなく `Any`
（→ `StoreSlot`）として格納される回帰。`reduce`/`mapreduce`/`foldl` などインラインラムダを
取る HOF が対象（`map` も同様に劣化していたが、テストの assertion が緩く露見せず）。
根因: 巻き上げられたインラインラムダ実引数は末尾が**素のネスト名** `__lambda_nested_N` の
`LetBlock` で、その素名がショート名メソッドテーブルから消えたため `infer_julia_type` が
`Any` に widen → `has_any_arg=true` → `table.dispatch([Any, Vector{Int64}])` が
`NoMethodFound` → ランタイムディスパッチ arm が `CallTypedDispatch` を emit して
`Ok(ValueType::Any)` を返し、**呼び出し点 HOF 戻り値型推論に到達しない**。#8105 前は素名が
解決され dispatch 成功側の HOF 推論（`dispatch.rs` 成功パス）が走っていた。
修正: `NoMethodFound`/ランタイムディスパッチ arm で、戻り値を `Any` に widen する前に
`infer_hof_call_site_return_type(function, args)`（map/broadcast/filter/reduce/foldl/foldr/
mapreduce/mapfoldl/mapfoldr を呼び出し点式から推論。インラインラムダは
`resolve_hof_callable` の `LetBlock` arm で解決）で静的戻り値型を回収。#8105 のディスパッチ
挙動（ランタイムは引き続き正しく解決）には非干渉で、ローカル束縛の型注釈のみ復元。非 HOF
呼び出しは従来どおり `Any`。回帰:
`type_propagation_call_tests::test_reduce_inline_lambda_return_type_inference_issue_5094` /
`test_qualified_reduction_hof_return_type_inference_issue_5094`。

### `type` / `as` を識別子に使うとパースエラー (Issue #8108)

`#8099`（`outer`）と同根の回帰。`function type() … end` / `type() = …` / `type` 変数・
引数・フィールド、`function as() … end` / `as` 変数が `unexpected token 'type'/'as' …
expected function name`（または `expected expression`）で失敗していた。上流 Julia では
`type` と `as` は **コンテキスト依存キーワード**で、`type` は `abstract`/`primitive` 直後
（`abstract type … end`, `primitive type … N end`）だけ、`as` は import/using の別名
（`import X as Y`, `using M: f as g`）だけが特別扱い。それ以外では普通の識別子。sjulia は
両者を予約語トークン `KwType`/`KwAs` として字句解析していたため弾かれていた。修正＝レキサから
`#[token("type")]`/`#[token("as")]` を撤去し普通の `Identifier` として字句解析。コンテキスト
依存位置はテキスト一致で検出する共通ヘルパ `check_contextual_keyword`/
`expect_contextual_keyword` を `Parser` に追加し、`abstract`/`primitive type` は
`expect_contextual_keyword("type")`、import/using の別名は `check_contextual_keyword("as")`
へ置換（`outer` の検出も同ヘルパに統一）。`abstract type`/`primitive type`/`using … as …`
のパース・降ろしは不変。注: `import X as Y` の別名 **束縛** 自体は main でも未実装（no-op）で、
本修正はパース層のみ（束縛セマンティクスは対象外）。`abstract`/`mutable`/`primitive` も同様に
上流では識別子化可能だが本 PR の対象外（別途）。fixture `parse/type_as_identifiers_8108.jl`
（julia/sjulia parity 15/15）+ パーサ単体テスト追加。

### ローカル変数が保持する parametric `DataType` への明示 apply-type コンストラクタ (Issue #8101)

`t = A.Pt; t{Float64}(1.0, 2.0)` のように、ローカル変数が保持する parametric struct の
`DataType` 値へ明示的に型パラメータを与えて構築する形が `Compilation error:
Unknown parametric struct: t` で失敗していた。`t{Float64}(...)` のベース名 `t` はコンパイル
時に静的解決できない（`parametric_structs` に無い）ため。修正＝ベースがローカル `DataType`
の場合は実行時に `ApplyTypeDynamic` で `Base{Float64}` を組み立て、その `DataType` 値を
コンストラクタとして `CallFunctionVariable` で呼ぶ（型無し動的形 `t(1.0, 2.0)` #8070 の
明示-`{T}` 版）。実行時 `try_construct_parametric_datatype` は名前に明示型パラメータが
あれば**推論せずそれを使い引数を convert**（上流の `Base{T...}(args)` 意味論に一致;
`t{Float64}(1, 2.0)` は Int を Float64 に変換）、無ければ従来通り引数値型から推論する。
非-`T` フィールドは自由なまま。fixture: `modules/local_datatype_applytype_ctor_8101`。

### parametric デフォルトコンストラクタの非統一引数を昇格せず MethodError に (Issue #8102)

`struct Pt9{T}; x::T; y::T; end; Pt9(1, 2.0)` は単一 `T` が `Int64` と `Float64` の両方には
なれず、デフォルトコンストラクタ `Pt9(x::T, y::T) where T` に**合致するメソッドが無い**ため
上流は `MethodError`。sjulia は型パラメータ推論で `Int64`/`Float64` を `Float64` に**誤って
昇格**して `Pt9{Float64}(1.0, 2.0)` を構築していた。修正＝`record_binding` を厳密統一に変更
（同一型変数の異なる**具体**型は昇格せずエラー）。ただし `Any`（コンパイル時に型を確定
できない引数のプレースホルダ）は具体型へ refine/defer して保持する（`Truncated(...)` 等の
不確定だが正当な構築を維持）。コンパイル経路は推論失敗時に `ThrowMethodError` を emit。
明示 `Pt9{Float64}(1, 2.0)` は別経路で convert するので不変。同一 `T` 構築・独立型パラメータ・
非-`T` フィールドは従来通り成功。fixture: `struct/parametric_ctor_no_widen_methoderror_8102`。
### モジュール内の短い型名を値として参照すると TypeVar 化 (Issue #8100)

`module M; struct E end; getE() = E; end` で `typeof(M.getE())` が upstream の
`DataType` ではなく `TypeVar` になる回帰。根因は2点:
1. **typeof 経路**: 短い大文字名 (`E`/`T1` など `is_type_variable_name` に一致する綴り)
   は `CoreType::TypeVar` に解釈される。`kind_for` はそれが宣言済み型なら DataType に
   再分類するが、判定 `declares_base_name` が `struct_defs` を **完全一致**で照合していた。
   モジュール private 型は `struct_defs` に修飾名 `M.E` で登録される一方、本体内の裸参照は
   `Struct("E")` を射影するため不一致 → TypeVar 扱い。長い名前 (`Elem`) は TypeVar 綴りに
   一致しないため無傷だった。
2. **`===` 経路**: `M.getE()`（=`Struct("E")`）と `M.E`（=`Struct("M.E")`）の型同一性
   比較が `CoreType::from` 経由で短名を TypeVar 化し reconcile できず false。

修正: `declares_base_name` をモジュール修飾名の **末尾（unqualified tail）一致**でも
照合（裸クエリ `E` が `M.E` に一致; 修飾クエリは完全一致のみ＝別モジュール誤一致を回避）。
`type_objects_equal` に Struct 同士の正規化名一致 fast-path を追加（module prefix 除去 +
alias 正規化済みの名前が等しければ同型 — 加算的で、長い名前の既存挙動と整合）。`where {T}`
の真の短い型パラメータは無影響（宣言済み型でなければ依然 TypeVar）。
回帰: `module_tests::module_short_type_name_value_8100`（22 assertions, julia パリティ）。
### キーワード引数のデフォルト値 `Inf` が `0` に解決される (Issue #8078)

`g(; a=Inf) = a; g()` が上流 `Inf` に対し sjulia では `0` を返す回帰。`-Inf`/`NaN`/
`Inf32`/`Inf16`/`Inf64`/`NaN*`、`@kwdef` の `Inf` フィールド、転送キーワードも同様に壊れて
いた（位置引数デフォルトは無事）。根因＝`Inf`/`NaN` は式位置では float リテラルとして emit
される Base グローバル定数だが**実行時グローバルには束縛されない**ため、キーワード
デフォルトの2つの評価器（`compile::utils::eval_literal_default` のベイク定数と
`vm::exec::call::value_from_bound_name` の実行時ミニインタプリタ）が名前束縛検索に失敗し
`Value::I64(0)` フォールバックに落ちていた。修正＝共有 `float_special_constant_value`
リゾルバ（`Inf`/`Inf32`/`Inf16`/`Inf64`/`NaN*` + `pi`/`ℯ`）を両評価器に配線（束縛名が優先＝
同名パラメータでシャドウ可）。`infer_default_type` も精密な float 型を返し単項 `-` を再帰
（→ `-Inf`/`-1.5` の `@kwdef` フィールドが内側コンストラクタの dispatch スロットを `Int64` に
誤型付けする別バグ #8109 も解消）。W-40（`HagerZhang.alphamax` の負センチネル回避策）を撤去。
回帰: `kwargs::kwargs_inf_nan_default_8078`, `optim::`（センチネル無し BFGS）。
### `outer` を関数名/変数名に使うとパースエラー (Issue #8099)

`function outer() … end` が `parse error: … unexpected token 'outer' … expected
function name` で失敗する回帰。上流 Julia では `outer` は **コンテキスト依存キーワード**で、
`for outer x in …`（外側ローカル変数修飾子）の中だけが特別扱い。それ以外（関数名・変数名・
引数名・フィールド名・呼び出し対象）では普通の識別子。sjulia は `outer` を字句解析段階で
予約語トークン `KwOuter` にしていたため、`Token::Identifier` を要求する関数名パース
（`parse_function_name`）等で弾かれていた。修正＝レキサから `#[token("outer")]` を撤去し
`outer` を通常の `Identifier` として字句解析、`for outer` 修飾子の検出は
`parse_for_binding` でテキスト一致（識別子 `outer` かつ次トークンが `in`/`=`/`∈` 以外）に
変更（`for outer in itr` のループ変数名 `outer`＝Issue #6414 の挙動も維持）。`for outer x`
修飾子そのものは別途 lowering で未対応のまま（Issue #6465、本修正の対象外）。fixture
`parse/outer_as_identifier_8099.jl`（julia/sjulia parity 7/7）。

### REPL: 空配列フィールドを持つ struct の配列グローバルが eval をまたいで消える (Issue #8086)

`@gif for … push!(ps, p) end` の後 `ps` が次の eval で `UndefVarError` になる回帰。
根因は #8063 (#7850) が `Plot` struct に空配列フィールド（`hlines`/`vlines = Float64[]`）
を追加したこと。REPL のグローバル永続化は struct を位置コンストラクタで再構築するが、
`value_to_init_expr` の「空配列は `None`（モジュール初期化子に委譲, #5296）」規則が
**ネスト（struct フィールド/配列要素）にも漏れて**いて、空配列フィールドで struct 全体の
再構築が失敗 → `ps`（Plot 配列）が丸ごとドロップ。修正＝`value_to_init_expr` に `nested`
フラグを導入し、ネスト時のみ空配列を `TypedEmptyArray` で再構築（トップレベルの #5296 委譲は維持）。
回帰: `repl::tests::test_repl_gif_with_global_accumulator_7151`（再 green）+ Plots 非依存の
`test_repl_persist_array_of_struct_with_empty_array_field_8086`。

### 修飾 `Base.f` 呼び出しが自モジュールの同名シャドウに誤再ディスパッチ (Issue #8079)

モジュールが Base ライブラリ関数と同名・同シグネチャの自前関数（例: NaNMath の
`log2`/`log10`）を定義すると、共有の短縮名メソッドテーブル上で `add_method` がシグネチャ
重複として **Base のメソッドを上書き** していた。そのため、シャドウ本体内の明示的
`Base.log2(float(x))` 修飾呼び出しがシャドウ自身へ再ディスパッチし、自己再帰
（NaNMath.log2 → Base.log2 → NaNMath.log2 → …）で `MAX_CALL_DEPTH` を超えて
偽の `StackOverflowError` を投げていた（BFGS 直線探索で顕在化、W-41）。`sqrt` の W-34/#8042
と同型だが、`log2`/`log10` は builtin を持たない純 Julia Base 関数なので汎用修正に。
`build_method_tables` がユーザシャドウで base メソッドが実際に上書きされた瞬間に
`Base.<name>` テーブルへ退避し、`compile_module_call` が明示 `Base.<name>(...)` 呼び出しを
そのテーブル経由でディスパッチする。退避はシグネチャ衝突する単一メソッド base 関数の場合のみ
（型付き `log(::Float64)` は無型 `log(::Any)` シャドウで上書きされないので退避不要）。
W-41 撤去（`iterfinitemax` を上流 `ceil(Int, -log2(eps(Float64)))` に復元）。fixture
`modules/module_qualified_base_shadow_8079.jl`（julia/sjulia parity 9/9）。

### Optim `BFGS` 準ニュートンソルバ (Issue #8059)

bundled Optim.jl に BFGS を追加（`optimize(f, g!, x0, BFGS())` / `optimize(f, x0, BFGS())`）。
上流の `HagerZhang` 直線探索 + `InitialStatic` を `LineSearches` に忠実移植し、`NLSolversBase`
に value/gradient キャッシュと中心差分勾配を追加。1 ステップ 2 次形式は f/g 呼び出し回数まで
上流完全一致（1 反復・3/3 calls）。Rosenbrock は minimizer/minimum が許容誤差内で一致するが
反復・f/g 呼び出し回数は線形探索内部と縮約順序差で不一致（sjulia 16 反復 vs installed Optim
2.2.1 の 21）なので assert しない。fixture は上流 Optim 2.2.1 / julia 1.12.6 と sjulia で同一 pass
数（parity 検証済み）。移植中に VM バグ 3 件を発見・起票（#8078/#8079/#8080、W-40/W-41/W-42）。
詳細は [DONE.md](./DONE.md) / [OPTIM.md](./OPTIM.md)。

### 目的関数を `f` という変数に束縛した `optimize(f, …, BFGS())` のクロージャ捕捉衝突 (Issue #8080)

目的関数を `f` という**変数名**に束縛して `optimize(f, …, BFGS())` に渡すと
`UndefVarError: Captured variable not found: f` で失敗していた（`myf` 等の別名や
名前付き `function` 定義、`GradientDescent` では成功）報告。原因候補は呼び出し側の `f` と
`optimize`/内部ソルバの**パラメータ `f`**、および中心差分勾配の**クロージャファクトリ**
`_central_difference_gradient(f)`（`f` を捕捉して返すクロージャ）が捕捉名 `f` で衝突する
キャプチャ解決バグ。**調査結果: 現 `origin/main` では再現しない。** BFGS フィーチャ
コミット（582648adc）にクロージャファクトリ形を復元して二分探索したが、目的関数名 `f` でも
全ケース成功する。すなわち基盤のキャプチャ解決バグは BFGS マージ前にマージされた
ネストクロージャ捕捉修正群（#7600 / #7618 / #7759）で**既に解消済み**で、回避策 W-42 は
不要だった。よって W-42（非捕捉の `_central_diff_gradient!` 直接呼び）を撤去し、上流忠実な
クロージャファクトリ形に戻した（VM 変更は不要）。回帰 fixture 2 本を追加（standalone の
名前衝突 + 実 Optim 経路の `f` 名目的関数）。詳細は [DONE.md](./DONE.md)。

### Plots.jl — `title!`, `xlims!`/`ylims!`, `hline!`/`vline!` 等 (Issue #7850)

`Plots.Plot` 構造体に `xlims`/`ylims`（軸表示範囲）と `hlines`/`vlines`（参照線 y/x 値）フィールドを追加。
純 Julia 側 (`packages/Plots/src/`) に `title!`/`xlims(!)`/`ylims(!)`/`hline(!)`/`vline(!)` API を実装し、
Rust 描画パイプライン (`plotly.rs`) が `xaxis.range`/`yaxis.range` と Plotly `shapes`（破線）として JSON に反映。
未設定時は従来 JSON と完全一致（既存 fixture 破壊なし）。
Rust ユニットテスト 8 件追加、fixture 4 本 追加。

### macro の `quote` 内 function 定義 3 件: hygiene / `local` 短縮形 / esc'd 名 (Issues #8064 #8065 #8066)

ユーザマクロの **bare `quote`** 内で関数を定義する系の相互に関連する 3 ギャップを
一括修正。

- **#8064 — 非 esc の関数名が hygienic でなく top-level にリークしていた**:
  `macro m() quote gg(x)=x+1 end end; @m` の後で upstream は `UndefVarError: gg`
  だが sjulia は `gg` を呼べてしまい、さらに内部ヘルパ名が同じ 2 つのマクロが
  メソッドテーブルを共有して誤ディスパッチしていた。マクロ展開値に対し、esc
  サブツリー外で定義された **bare `Symbol` 名の関数定義** (`Expr(:function,…)` /
  短縮形 `Expr(:(=),Expr(:call,…),…)` とその `where`) を収集し、`name##<macro>#<span>`
  形式の **module-private gensym** へ定義・参照とも一括リネーム (`macro_runtime.rs`
  の `apply_quote_function_hygiene` ほか)。esc'd 名 (callee が `Expr(:escape,…)`) と
  qualified/parametric 名は対象外なので #8066 とは衝突しない。適用は
  **「マクロ本体の末尾が直接 `quote`(=`Expr::QuoteLiteral`/`ExprNew`) を返す」場合のみ**
  にゲートする — MacroTools の `@qq` のように `esc(to_line(...))` 経由で名前を
  エスケープするマクロは展開後に escape マーカーが既に解決済みで構文的に判別できず、
  リネームすると可視であるべき名前を壊すため (macrotools fixture で検出)。module
  マクロ (bundled package) は既存の member 修飾 hygiene を持つので対象外。
- **#8065 — `local f(x) = ...` 短縮形が quote 内 (および通常関数本体) で lower 不可**:
  パーサの `parse_var_declaration_item` が名前だけ読んで止まり、`(...) = body` を別文に
  誤分割していた。`local`/`global` 宣言項目で識別子の直後が `(` なら短縮形関数定義と
  みなし一般式パーサに委譲 (call/where LHS を持つ `Assignment` を生成)。lowering 側
  `lower_local_statement` も `is_short_function_definition` を見て関数定義として降ろす。
- **#8066 — esc'd / 補間された関数名を call target にできない**: 短縮形
  `$(esc(:f))(x)=...` は `$` の貪欲 postfix 取り込みで callee が括弧グループとして
  parse される。quote 変換 (`cst_to_constructor.rs`) で `$(...)` callee を新ヘルパ
  `paren_dollar_payload` で補間 (esc マーカー保存) するよう修正。完全形
  `function $(esc(:f))(x) ... end` はパーサの `parse_function_name` が `$` の後の
  `(...)` を受理するよう拡張。

`macros/quote_funcdef_hygiene_8064.jl`, `quote_local_funcdef_8065.jl`,
`quote_esc_funcdef_8066.jl` を追加 (全て julia parity 確認済み)。
### パラメトリック struct の `DataType` 値を動的に呼ぶコンストラクタ (Issue #8070)

`t = A.Pt; t(1.0, 2.0)` のように、パラメトリック struct の **`DataType` 値**を
ローカル変数経由で動的に呼ぶとコンストラクタが見つからず
`Function 'A.Pt' not found` で失敗していた。`A.Pt(1.0, 2.0)` の静的呼びは動作するが、
`PushDataType` が積む `Value::DataType` を呼ぶ経路 (`call_function_variable.rs`) は
メソッド候補が空のとき `try_construct_default_datatype` にフォールバックし、これは
`struct_defs` の**具体エントリ**しか見ない。パラメトリック base (`Pt`) は
`parametric_structs` にしか登録されず具体行が無いため構築に失敗していた。#8058 で
具体 struct の同型ギャップを修正済みで、本件はそのパラメトリック版。

- **修正**: `try_construct_default_datatype` が具体行を見つけられないとき、新しい
  `try_construct_parametric_datatype` にフォールバック。引数値の型からコンパイル時と
  同じ `infer_parametric_type_args` で型パラメータを推論し、インスタンス名
  (`A.Pt{Float64}`) を組んで `StructInstance` を構築する。名前解決
  (`resolve_runtime_parametric_def`) はコンパイラの `resolve_parametric_struct_name` を
  ミラーし、`A.Pt` / bare `Pt` / 再エクスポート alias (`const Pt = A.Pt` + `using .B`) を
  同じパラメトリック base に解決する。新規 intrinsic/BuiltinOp は不要。
- **一般性**: 1型パラメータ (`Pt{T}` の `x::T,y::T`)、2型独立パラメータ
  (`Pair{S,U}` の `a::S,b::U`)、struct を field 値に取る場合 (`Box{Pt{Float64}}`)
  まで上流 julia と一致 (`Main.` プレフィックス省略は既存の表示差)。動的呼びは静的
  `A.Pt(...)` と完全に一致する (混合型 `t(1,2.0)` の widening も静的経路と一致)。
- **範囲外の関連ギャップ**: (1) 動的値への明示型パラメータ呼び `t{Float64}(1.0,2.0)` は
  コンパイル時に `t` をパラメトリック struct 名と誤認し `Unknown parametric struct: t` で
  失敗 (本修正の対象外、別経路・既存バグ)。(2) `Pt{T}; x::T; y::T` に `Pt(1, 2.0)` の
  ような混合同一パラメータ引数を渡すと上流は MethodError だが sjulia は widening して
  受理する (静的・動的とも、既存の divergence)。fixture
  `modules/dynamic_parametric_ctor_8070.jl`。

### DataType を値として扱う 3 つのギャップ: generic Dict キー / guarded field assign / dynamic `new{}` (Issues #7940 #7941 #7935 #7934)

AbstractAlgebra Phase 2 で見つかった、型 (`DataType`) を**値として**渡す系の
コンパイル時ギャップ 3 件を修正。

- **#7940 — generic `DataType` を Dict キーに使うとコンパイルエラー**:
  `D[T]` / `D[T] = v` (`T` は `where` 型パラメータ) が、コレクション型が
  `ValueType::Dict` と推論されない経路で**インデックスを I64 に強制変換**しようとして
  `Cannot convert DataType to I64` で失敗していた。配列は型を添字に取れない以上、
  `DataType` キーは Dict 操作確定なので: getindex 側は `DataType` 添字を I64 強制せず
  そのまま積んで実行時 `IndexLoad` の Dict ディスパッチに委ねる
  (`compile/expr/builtin_array.rs`)。setindex 側は `Stmt::IndexAssign` で
  `DataType` 添字を検出したら `setindex!`/`DictSet` 経路へ振り、配列ストアの
  数値→F64 強制 (`d[T]=1` が `1.0` になるバグ) を回避し値型を保持
  (`compile/stmt.rs`、`builtin_array.rs` の `has_non_numeric_idx`)。fixture
  `dict/dict_generic_datatype_keys_7940.jl`。なお、**module-global const Dict** の
  実行時 getindex は別の既存バグ (#8068) が残るため fixture は Dict を引数で渡す形を使用。

- **#7941 — guarded な generic フィールド代入がコンパイル時に拒否**:
  `function f(G::T) where T; if !isdefined(G,:__attrs); G.__attrs = Dict(); end; end`
  が、受け手 `G` の型が `Any` (generic 型変数) なのに、`G.__attrs = …` を
  「どの struct にも `:__attrs` が無い」としてコンパイル時に `Unknown field` で拒否
  していた。受け手が具象 struct でない (`ValueType::Any`) フィールド代入は upstream 同様
  **実行時 `SetFieldByName` に遅延**する (存在しなければ実行時エラー) よう変更
  (`compile/stmt.rs` の `FieldAssign` `Any` arm)。具象 struct への不正フィールド代入は
  従来どおりコンパイル時エラー。fixture `struct/struct_guarded_generic_field_assign_7941.jl`。

- **#7935 — inner constructor の `new{...}` 型パラメータが計算式だと `{Any}` に潰れる**:
  `new{elem_type(R), elem_type(coefficient_ring(R))}(R)` が、型引数を
  `TypeExpr::TypeVar("elem_type(R)")` 文字列として解析→`type_expr_is_resolvable` が
  `false`→`NewParametricStruct` フォールバック (型引数なし→`{Any}`) になっていた。
  lowering で `new{...}` の型引数をブラケット対応で分割し、呼び出し/パラメトリック式は
  `TypeExpr::RuntimeExpr(text)` に分類 (`lowering/expr/call.rs`)。`type_expr_is_resolvable`
  が `RuntimeExpr` を resolvable 扱いにし、既存の `compile_type_expr_as_value`
  (RuntimeExpr をその場で再 lowering→実行時に `DataType` 値を生成) と
  `NewDynamicParametricStruct` で**計算された具象パラメトリック型**を構築
  (`compile/expr/collection.rs`)。`typeof(r).parameters == (MyElem, MyElem)` を確認。
  型名の表示は module 修飾なし (`UniversalRing{MyElem, MyElem}`) で、既存の `new{T}`
  inner-ctor 経路と同じ挙動。helper が ctor スコープで解決可能な場合に動作
  (module-private helper は #8069 で解決済み)。fixture
  `struct/struct_dynamic_new_type_params_7935.jl`。

- **#7934 — DataType パラメータ付き typed Dict コンストラクタ**: `Dict{Type, Dict{Symbol, Any}}()`
  は現行 main で既に動作。回帰 fixture `dict/dict_typed_datatype_param_ctor_7934.jl` を追加。

### inner constructor 本体の名前解決を定義モジュール scope で行う (Issue #8069)

`module M; struct E end; helper() = E; mutable struct UR; x; function UR(v); h = helper(); new(v); end; end; end`
を caller が `using .M` せずに `M.UR(5)` すると、`Compilation error: function 'helper' is
not imported` で失敗していた。原因は inner constructor 本体が**トップレベルの import
scope** (`imported_functions` + `program.usings`) でコンパイルされており、定義モジュール
`M` の module-private 関数 / const / 型が見えていなかったこと。upstream Julia はメソッド
本体の名前を常に定義モジュールで解決する。`compile_inner_constructors` を、通常の
module メソッド本体を扱う `compile_functions` と同じ module-scope セットアップに揃えた:
`InnerCtorInfo` に定義モジュール path を持たせ、`module_functions[path]` /
`module_imports_map[path]` を import 集合へ加え、`module_usings_map[path]` から
`resolved_usings` を作り、`compiler.current_module_path` / `current_module_imports` を設定
(`compile/pipeline_ctx.rs`)。これで module-private 関数呼び出し (`helper()`、`new(mk())`)、
module-private `const` (`new(K)`)、通常長の module-private 型を値として参照する場合が
解決される。fixture `modules/inner_ctor_body_module_scope_8069.jl` (julia/sjulia 5/5)。
派生して発見した別の既存バグを起票: 2 文字以下の短い module-private 型名 (`E`) を
module メソッド本体で**値として**参照すると `TypeVar` に誤解決される (inner-ctor 固有
ではなく通常の module 関数にも出る、`name.len() <= 2` ヒューリスティック由来)。

### quote 内の補間型注釈付き短縮関数定義が lower できない (Issue #7933)

`quote ... g(x::$T) = ... end` のように、短縮形関数定義の引数型注釈を補間 (`$T`) した
ものが lower 時に `UnsupportedFeature { MacroCall, "macro expansion returned unsupported
assignment expression target" }` で失敗していた (AbstractAlgebra の `@attributes` が生成する
メソッド形)。macro 展開は `Expr(:(=), Expr(:call, f, Expr(:(::), x, <補間型>)), body)` を
返すが、macro 結果→IR 変換 (`lowering/macro_runtime.rs`) の `Expr(:(=), ...)` 処理に
`:call` (および `:where`) を LHS に持つケースが無く、assignment-expression 経路へ落ちて
エラーになっていた。`value_to_stmt` の `:assign` 分岐に `:call`/`:where` ターゲットの
関数定義ケースを追加し、`Expr(:function, ...)` と同じ `function_stmt_from_values` へ委譲。
さらに block tail 判定 (`value_requires_stmt_path_in_tail`) で短縮関数定義の `=` を
statement 経路必須として扱い、`function_stmt_from_values` は `constructor_signature_from_value`
を再利用して `where` の型パラメータを保持するようにした。補間された型は実型として保持され
ディスパッチに反映される (補間 `$T` 違いで正しいメソッドが選ばれ、不一致型は MethodError)。
fixture `macros/macro_interp_typed_param_7933.jl`。本修正の調査で見つかった独立した
別ギャップは個別 issue に分離: macro 定義関数名が gensym されず top-level に leak する
(#8064 bug)、`local f(x)=...` 短縮形が quote 内で lower できない (#8065)、
esc した関数名 `$(esc(:f))(x)=...` を call target にできない (#8066)。
### 無名/アロー関数の省略可能な位置引数デフォルトが束縛されない (Issue #8047)

`(x, d=2) -> (x, d)` は parse は通るがデフォルトが適用されず、縮約アリティ呼び出し
`a(1)` が `UndefVarError: d`、フルアリティ `a(1, 9)` が `NoMethodFound`（1引数ラムダ
しか生成されず default-arg stub が無い）になっていた。named/short/block 形式は
`extract_defaults_from_function_def` + `generate_default_arg_stubs`（`lowering/function/defaults.rs`）
を通るのに対し、アロー lowering 経路（`lower_arrow_function`・`lower_arrow_function_with_name`・
IIFE/nested 変種）はデフォルト抽出も stub 生成もしていなかったのが原因。CST 上、アロー引数の
`d=2` は `Assignment` ノード、`x::Int` は `TypedExpression` ノードで、どちらも param 収集の
`match` で `_ => {}` に落ちて捨てられていた（後者は param/default のインデックスずれも誘発）。
両収集関数（`collect_arrow_parameters` / `collect_lifted_arrow_parameters`）に `Assignment`
（LHS=パラメータ, RHS=デフォルト式）と `TypedExpression` を追加し、`generate_default_arg_stubs`
を `pub(crate)` 化して全アロー経路で縮約アリティ stub（同名メソッドとしてアリティ dispatch）を
emit するよう修正。`lower_arrow_function_with_name` / `lower_lambda_assignment` は
`Vec<Function>`（本体 + stub 群）を返すようにし、呼び出し3箇所を更新。int/float/string/bool/
symbol/nothing/missing/const/type/call/typed デフォルト・複数デフォルト・型付き引数・IIFE 形を
網羅。fixture `functions/anonymous_default_arg_8047.jl`（#8040 マトリクスの無名行を補完）。

### `using` で再エクスポートした `const` 型エイリアスがコンストラクタ呼び出しできない (Issue #8049)

モジュール B が `const Foo = A.Foo; export Foo` で型エイリアスを再エクスポートし、呼び出し側が
`using .B` した場合、`Foo` は**値としては読めるが** (`t = Foo`、`t()` も可) **コンストラクタ
として直接呼べず** `Foo()` が `UndefVarError: Foo not defined` になっていた。原因は、値参照の経路は
using-import / const-エイリアスを解決して `PushDataType("Foo")` を出すのに対し、**関数呼び出しの名前
解決経路が**、`const` バインディングが登録した **`Any` 型グローバル `Foo`** を callable-variable と
誤認し `LoadAny("Foo")` (実在するのは `B.Foo` のみ) を出していたこと。`try_compile_callable_variable_call`
の `Any` グローバル分岐で、名前が**可視な型** (struct / parametric struct / 型エイリアス) の場合は
`Ok(None)` を返して `compile_call` の通常のコンストラクタ解決チェーンへ委譲するよう修正。直接呼び出し
`Foo()` が値経路 (`t = Foo; t()`) と同じ underlying 型に解決され、static に inner constructor へ解決
(`CallResolved`) されるようになった。parametric struct エイリアス・selective `using .B: Foo` も網羅。
fixture `modules/const_alias_ctor_via_using_8049.jl`。

### 再エクスポート(import)したバインディングへの修飾アクセス `Module.X` が失敗 (Issue #8053)

`module Facade; import ..Defn: T, g; end` のように selective import で再エクスポートした名前へ
**修飾アクセス** (`t isa Facade.T`、`Facade.g(t)`) すると、`compile_module_function_ref` /
`compile_module_call` が「`Facade` に `T` という関数は無い」とコンパイルエラーになっていた。原因は、
これらの解決経路が**モジュール内で定義された名前** (struct_table / 型エイリアス / `module_functions`)
しか見ず、`import ..Defn: T, g` で usings に記録される**取り込み済みバインディング**を解決構造へ集めて
いなかったこと。collect 時に selective import を解決し `module_imported_bindings`
(`"Facade.T" -> "Defn.T"`) を構築 (`resolve_using_module_name` を再利用)。両解決経路は通常解決に
失敗した時だけ再エクスポート鎖を辿り、ソース修飾名 (`Defn.T` / `Defn.g`) で**同じ**型/関数解決を
再実行する。型位置 (`Facade.T` → `PushDataType("Defn.T")`) と呼び出し位置 (`Facade.g(t)` →
`Defn.g` のメソッド表) の両方、および連鎖再エクスポートに対応。非選択 `using M`(再エクスポートを
getproperty で公開する)は IR が `import M` と区別できないため対象外。fixture
`modules/reexported_qualified_access_8053.jl`。

### `using` 取り込みの const 型エイリアスを局所変数経由で動的呼び出しできない (Issue #8058)

`const Bar = A.Bar`(デフォルトフィールドコンストラクタのみの struct)を `using .B` で取り込み、
`t = Bar; t(7)` と**局所変数経由で動的に**呼ぶと `Function 'Bar' not found` になっていた。`t = Bar` は
短縮名の `Value::DataType(Struct("Bar"))` を保持するが、ランタイムの動的コンストラクタ解決
`try_construct_default_datatype` が `struct_defs` を**完全一致名でしか**探さず、実体が `A.Bar` で登録
されているため見つからなかった(inner constructor を持つ struct は短縮名のメソッド表があるため従来から
成功していた)。完全一致に失敗した場合、最終 `.` セグメントが**一意**に一致する struct へフォールバック
するよう修正(曖昧時は従来通りフォールバックしない)。デフォルトコンストラクタ struct・修飾エイリアス
値経由 (`u = A.Bar; u(11)`)・inner constructor を網羅。なお、parametric struct の DataType 値からの
動的構築 (`t = A.Pt; t(1.0,2.0)`) は再エクスポートと無関係に未対応で、Issue #8070 として起票。fixture
`modules/dynamic_const_alias_ctor_8058.jl`。

### 他モジュールの関数へメソッドを追加できない (`function OtherMod.f(...)`) (Issue #8052)

`function Inner.f(x::Float64) ... end` のように**他モジュールの関数を拡張**する定義が lowering で
`missing function name` になっていた。原因は、関数シグネチャの `FieldExpression` callee
(`Module.f` 形) を `module == "Base"` の時しか受理していなかったこと。Base 以外のモジュールも受理し、
非 Base は**修飾名 `Inner.f`** を関数名として採用(`module_extension_function_name` ヘルパに統一、
full-form / short-form / where 形を網羅)。`build_method_tables` は修飾名のユーザ関数を `Inner.f` と
裸の `f` の**両方**のメソッド表へ登録するので、後の `Inner.f(2.0)` と `using .Inner` 由来の無修飾
`f(2.0)` の両方が全メソッドへディスパッチする。`import Inner: f; function f(...)` 変種も、トップレベル
selective import のソースモジュールを記録して `Inner.f` 表へ join するため shadow ではなく拡張になる。
fixture `modules/cross_module_method_extension_8052.jl`。

### builtin `sqrt` が他モジュールの `sqrt` に誤ディスパッチして無限再帰する (Issue #8042)

bundled package (NaNMath/Optim) で **別モジュールが新規 `sqrt(x) = … Base.sqrt(float(x))`**
を定義すると、`NaNMath.sqrt` がグローバル bare `"sqrt"` メソッドテーブルに混入し、ヘルパ
連鎖で得た **`Any` 型 `Float64`** への bare `sqrt`/明示 `Base.sqrt` が builtin ではなく
`NaNMath.sqrt(::Any)` に解決 → 本体 `Base.sqrt(float(x))` が再び自分自身に解決して
**Stack overflow**。リテラルは具象 `Float64` で builtin 直行のため再現せず、別モジュールを
ロードしたコンテキスト依存だった (#5966 の promote-fallback 再帰トラップに類似)。
`compile_sqrt` の候補から単一セグメント `"<Module>.sqrt"` (Module≠Base) を foreign として
除外し、`Any` 経路は generic dispatch にフォールスルーせず常に builtin 裏付き
`CallTypedDispatchOrBuiltin` を発行。真の `Base.sqrt` 拡張 (`"Symbolics.Base.sqrt"` 等) は
single-segment ガードで温存。bundled Optim の `_sqrt` 回避策 (W-34) を撤去し builtin `sqrt`
に復帰。fixture `module/module_base_sqrt_foreign_shadow_8042.jl`、`optim_nelder_mead_mvp`。
### `SciMLBase.solve(prob, Tsit5())` 修飾呼び出しの alg dispatch 回帰を修正 (PR #8050 review, Issue #7996)

PR #8050 の `OrdinaryDiffEq` alg dispatch は独自の `OrdinaryDiffEq.solve` を定義して
おり、`Tsit5` メソッドが `SciMLBase.solve` に登録されないため、修飾呼び出し
`SciMLBase.solve(prob, Tsit5())` がエラーフォールバックに落ちる回帰があった（Codex
レビュー指摘）。`Tsit5` 型と `solve(::ODEProblem, ::Tsit5)` を `SciMLBase` 側
（`_tsit5_solve` と同居）へ移し、`OrdinaryDiffEq` は `Tsit5` を再エクスポート、`solve`
を `SciMLBase.solve` への forwarder にすることで単一メソッドテーブルに統一。非修飾
`solve(prob, Tsit5())` と修飾 `SciMLBase.solve(prob, Tsit5())` が一致して dispatch する。
回避策 W-35。**新規 sjulia ギャップ（要追跡）**: 別モジュールの関数をこのモジュールから
拡張できない（`function OtherMod.f(...)` が lowering「missing function name」/
`import OtherMod: f; function f(...)` が別関数化、**#8052**）、再エクスポート binding への
修飾アクセスが解決しない（`OrdinaryDiffEq.Tsit5` 失敗、`isdefined` は true、**#8053**）。
これらが解消すれば上流通り `Tsit5` を OrdinaryDiffEq に戻し `SciMLBase.solve` を拡張できる。

### ブロック形式関数の素の識別子デフォルト引数が脱落する (Issue #8017)

`function f(x, l=nothing) ... end` のように **`function … end` ブロック形式** で
**デフォルト値が素の識別子 / グローバル参照** (`nothing`, `missing`, 定数, 型名 `Int`
など) のオプショナル引数は、デフォルトが**黙って脱落**し、デフォルト引数スタブメソッド
が生成されず、減アリティ呼び出しが `No method matching …` でディスパッチ失敗していた。
原因は lowering の `extract_default_from_parameter_node` がパラメータの「`=` の後ろ」を
探す際に **識別子の子ノードを型注釈とみなして読み飛ばしていた**こと。素の識別子デフォルトも
`Identifier` ノードなので一緒に飛ばされ `None` (デフォルト無し) になっていた。Pure Rust
パーサの `Parameter` ノードは常に `[name, 型?, デフォルト?]` の順 (`=` の後ろが末尾子) なので、
`=` を含む場合は**末尾の名前付き子をデフォルト値として採用**するよう修正。リテラルデフォルト
(`1`, `""`, `true`, `:auto`) は別ノード種なので元から動作しており回帰なし。短形式
`f(...) = ...` は別経路で元から正しく、オプショナル数やモジュール内外も無関係 (Issue の
切り分けでは module/3+ 個に見えたが、真の条件は「ブロック形式 + 識別子デフォルト」)。
fixture `functions/block_form_identifier_default_8017.jl` で upstream julia 1.12 と照合。

### `Value::StaticArrayInline` Phase 3 — ゼロアロケーション inline 格納 (Issue #7964)

N≤4 の `<:Real` StaticArray を `Value::StaticArrayInline(StaticArrayInlineData)` で
格納する Phase 3 を完成させた。`StaticArrayInlineData` は `Copy` な 40-byte payload
であり、`stack.push/pop` でヒープアロケーションが発生しない。

主な修正点:
- **`get_value_julia_type`** (`state.rs`): `StaticArray`/`StaticArrayInline` に arm を追加。
  WHERE 句バインド（`bind_type_params`）が M/N/T を正しく抽出できるようにした
  (`UndefVarError: 'M' not defined` の原因を除去)。
- **`get_type_name`** (`introspection.rs`): 同様に arm を追加。dispatch 時の抽象型検査
  (`Size(x::StaticArray)`、`isa AbstractArray`) がフォールスルーしなくなった。
- **`isa` 演算子** (`builtins_types.rs`): `StaticArray`/`StaticArrayInline` を
  `struct_name_opt` 経路に誘導し、`check_isa_with_abstract_resolved` が呼ばれるよう修正。
  `v isa AbstractArray{Int64, 1}` 等が正しく `true` を返す (Issue #7819 fixture 通過)。
- **`GetField(0)`/`GetFieldByName("data")`** (`struct_ops.rs`): 以前は `StaticArrayInline`
  自体を push していたが、`to_tuple_value()` で実体 TupleValue を生成して push するよう変更
  (`Base.Tuple(x::SVector) = x.data` が正しい型を返すよう修正)。
- **`IndexLoad`** (`array_index.rs`): `StaticArray`/`StaticArrayInline` への 1D/2D 整数
  インデックスを追加。2D の公式は sjulia の `getindex` と一致する行優先 `(row-1)*cols+(col-1)`。
- **`inline_matvec`/`inline_matmat`** (`static_real.rs`): `@SMatrix` リテラルのデータが
  行優先（ソース行から左→右）に格納されるため、行列乗算のインデックスを行優先
  `data[i*k+j]` に修正（従来の列優先式は誤りだった）。
- **SMatrix{M,1} をインライン化しない**: `cols==1` を `is_vector()` と同一視する
  `StaticArrayInlineData` では `SMatrix{M,1,T}` が `SVector{M,T}` と識別できないため、
  N==1 の SMatrix は `StaticArray` (型名文字列を保持) パスに通すよう制限。

すべての `static_arrays` fixture 8 件、全 163 fixture tests、全 3013 unit tests が通過。
Clippy ゼロ警告。

### `floor(T, x)` 等を CallTypedDispatch から CallBuiltin へコンパイル時降格 (PR #8043)

`floor(Int, x[1])` / `ceil` / `round` / `trunc` の型変換形式が、ループ内で第 2 引数が
Any 型のとき `has_datatype_arg && has_typeof_methods` 分岐で `CallTypedDispatch` を生成し、
毎イテレーション完全 dispatch を実行していた問題を修正。`compile_generic_dispatch_call` に
「丸め関数 + 既知型名」の短絡ガードを追加し、`FloorF64 + DynamicToI64` 命令を直接生成
するよう変更。`vm_staticarrays_matvec_benchmark` が 23.9 ms → 3.3 ms（～7× 高速化）。

### iOS サンプル：StaticArrays 追加・IFS フラクタル更新 (PR #8044)

`intermediate/static_arrays.jl` を新規追加（SVector/SMatrix 基本操作 + Barnsley fern
カオスゲーム）。`ifs_fractals.jl` の `Affine` 構造体スカラー手展開ワークアラウンド
（Issue #7949 INTERIM）を `SMatrix`/`SVector` 実装に置き換え。

### SciMLBase.solve の alg 引数を型 dispatch するよう実装 (Issue #7996)

`solve(prob::ODEProblem, alg; ...)` が渡した `alg` を無視して常に Tsit5 を実行していた
問題を修正した。`SciMLBase.jl` の Tsit5 ソルバーロジックを `_tsit5_solve` に改名し、
`OrdinaryDiffEq.jl` に `solve(prob::ODEProblem, alg::Tsit5; ...)` と汎用エラー fallback
`solve(prob::ODEProblem, alg; ...) = error("Algorithm ... not supported")` を追加。
これにより未サポートアルゴリズム（`Euler()` 等）を渡すと明示的なエラーが返るようになった。
fixture `packages/ordinarydiffeq_alg_dispatch_7996.jl` で動作確認済み。
なお、`const TypeAlias = OtherModule.Type` でエクスポートされた型に対して `using` 後の
コンストラクタ呼び出しが失敗する sjulia の制限を Issue #8049 として起票した。

### Optim.jl サポート MVP (Issue #7432, milestone #39)

Optim.jl の上流適応 pure-Julia MVP を bundled package として実装
(`subset_julia_vm/packages/Optim/`、上流レイアウト保持)。決定的な no-AD /
ユーザ勾配ワークフローを対象:

- **単変数**: `optimize(f, lower, upper, GoldenSection())` / `Brent()`、整数境界
  promotion、`x_lower > x_upper` の precise error。
- **結果 API / 設定**: `minimizer` / `minimum` / `iterations` / `converged` /
  `f_calls` / `g_calls` / `x_converged` / `f_converged` / `g_converged` /
  `lower_bound` / `upper_bound`、`Options(iterations=, ...)`、`maximize` ラッパ。
- **多変数 (微分なし)**: `optimize(f, x0, NelderMead())`、`NLSolversBase` の
  `NonDifferentiable` / `value` / `f_calls` 経由。
- **一次 (ユーザ勾配)**: `optimize(f, g!, x0, GradientDescent())`、`OnceDifferentiable` /
  `value_gradient!` / `g_calls` と `LineSearches.BackTracking`(Armijo)。

依存は上流の dependency set を bundled の機能実装 (`NLSolversBase`, `LineSearches`)
または文書化済み stub (`ADTypes`, `NaNMath`, `EnumX`, `FillArrays`,
`PositiveFactorizations`) として追加 (Issue #7478)。fixture (`tests/fixtures/optim/`)
は upstream Optim.jl と sjulia で**同一にパス**: GoldenSection (40 iter / 41 fcalls)、
Brent (5 iter / 6 fcalls)、NelderMead (34 iter / 70 fcalls) は上流と完全一致。
詳細・in-scope/deferred マップは [OPTIM.md](./OPTIM.md)。

実装中に bundled package 内 helper で生成された `Float64` に対し builtin `sqrt` が
stack overflow する VM バグを発見 (Issue #8042) → Newton 法 `_sqrt` で回避 (W-34)。
BFGS/LBFGS/Newton/制約付き solver・完全 AD・完全 LineSearches・trace は意図的に未対応
([UNIMPLEMENTED.md](./UNIMPLEMENTED.md))。

### ODE `sol.u` 全エントリが最終状態にエイリアスする (Issue #8094 / W-43)

PR #8094 (Tsit5 in-place バッファ + SVector 最適化) が導入した `OrdinaryDiffEq.jl` の
`SciMLBase._tsit5_solve` オーバーライド内で `SciMLBase._copy_state(u)` を修飾呼び出し
すると、sjulia の **クロスモジュール修飾呼び出しディスパッチバグ** (Issue #8104) により
`AbstractVector` 専用メソッド (`copy(u)`) ではなく総称メソッド (`u` をそのまま返す) が
選ばれ、`us` に積まれる全エントリが同一の可変ベクタ（最終状態）を指すエイリアスになった。
結果として `sol.u[1]` が初期条件でなく最終状態を返し、Lorenz アトラクタ・振り子 iOS サンプル
のプロットが崩壊していた。回避策 (W-43): `SciMLBase._copy_state(u)` 呼び出しを
`ismutable(u) ? copy(u) : u` インライン式で置き換え。

合わせて振り子アニメーションサンプル (`ordinarydiffeq_pendulum_animation.jl`) を新規追加
（iOS アプリ `intermediate/` フォルダ、`samples.json`、Swift フォールバック更新）。

## 最新対応 (2026-06-26)

### 空白区切り `for ... for` 内包表記が 1 次元 Vector になる (Issue #8014)

`[expr for x in A for y in B]` のように **空白区切りの複数 `for` 句** を持つ内包表記は
`Iterators.flatten` 意味論で **1 次元 `Vector`** を返すべきだが、sjulia は `for` 句の
数だけ次元を持つ N 次元配列として扱い、**2 次元 `Matrix`** を生成していた
(カンマ形式 `for x in A, y in B` のカルテシアン積と混同)。lowering で「単一 `for` 句
(カンマ区切り束縛) = カルテシアン / 多次元」と「複数 `for` 句 = flatten / 1 次元」を
CST から区別し (`Expr::MultiComprehension.flatten`)、flatten 用のコンパイル経路を追加した。
flatten 経路は反復子を **外側→内側** にネストし、内側反復子を外側ステップごとに
**再評価** する (依存レンジ `for i in 1:3 for j in 1:i` に対応)、reshape を行わない。
カンマ形式 (2 次元 Matrix) は従来どおりで回帰なし。flatten / カンマ / 3 句 / 依存 /
混在 (カンマ群 + flatten) / フィルタ / 型付き / 空レンジを upstream julia 1.12 と照合。

### Symbolics 記号式の上流同形表示（項順序・負係数）(Issue #7894, Epic #7888)

`simplify` の和の項整序（昇順 degree → `x^2` が `x*y` より先、定数先頭）と `show` の
負係数の減算描画（`a + (-1)*b` → `a - b`）により、`det([x y; x x])` の文字列が上流と
同じ `x^2 - x*y` になる（fixture `packages_symbolics_canonical_form`）。Epic #7888
Phase 2。スコープは det/inv の単項式和。完全多項式正準化・`2x` 表記・生 `x*x` 折り畳みは
範囲外。これで Epic #7888 の子 Issue（#7889/#7892/#7894）が全て完了。

### Symbolics 記号行列の `det` / `inv` / `\` (Issue #7892, Epic #7888)

記号行列のラプラス展開 `det`、余因子 `inv`、線形ソルブ `\` を bundled Symbolics
パッケージ `linear_algebra.jl` に Pure Julia で実装（数値 builtin を踏まない）。
`det` は untyped `det(A)` を parametric override、`inv` は stdlib `inv` を untyped 化
（#8025 specificity 回避, W-33）して override、`\` は `inv(A)*b` 経由で自動。値は
`substitute` で検証（fixture `packages_symbolics_linear_algebra`）。Epic #7888 Phase 1。
### bare な imported type-alias 境界 `{<:Num}` が qualified な `Matrix{Symbolics.Num}` に一致する (Issue #8019)

`using Symbolics` で取り込んだ別名 `Num` (`Num === Symbolics.Num`) を使った
パラメトリック境界 `f(::AbstractMatrix{<:Num})` が、要素型が完全修飾で表示される
実引数 `Matrix{Symbolics.Num}` にディスパッチで一致せず MethodError になっていた。
qualified 表記 `{<:Symbolics.Num}` は一致するため、bare↔qualified の `Named`
正規化のずれ (#7263/#7265 と同系統) が原因。コア subtype エンジンの
`(Named, Named)` アーム (`type_core/subtype.rs`) に、隣接する `(Struct, Named)` /
`(Named, AbstractUser)` アーム同様の module 接頭辞を除去した reflexive 等価判定
(`base_type_name(child) == base_type_name(parent)`) を追加し、module 修飾の違いだけの
2つの型を同一とみなすようにした。これにより `Symbolics.Num <: Num` の境界判定が真に
なり、bare 表記でも一致する。回帰テスト `packages_symbolics_dispatch_bare_alias_bound_8019`
(競合メソッドのない単独メソッドで一致のみを検証)。修正範囲はマッチング (Named 正規化)
に限定し、specificity ランキング (#8025) には触れていない。Symbolics `linear_algebra.jl`
の qualified 境界 (W-32) は #8025 のランキングと絡むため本 PR では据え置く。
### 即時適用される無名アロー `(x -> ...)(arg)` が関数内の制御フローを壊す (Issue #8018)

関数本体内で末尾以外に置かれた即時適用の無名アロー `(x -> body)(arg)` が、
持ち上げたラムダ呼び出しを `Stmt::Return` で包んでいたため、その `return` が
外側の関数フレームに漏れていた。結果として `r = (x -> body)(arg)` は
ラムダの値を即座に return してしまい (継続が実行されない)、もしくは後続で
結果を使うと `Cannot convert Nothing to I64` でコンパイルに失敗していた。
名前付き束縛 (`g = x -> body; g(arg)`) や同じ IIFE をトップレベルに置いた場合は
正しく動いていた。`lower_iife_as_nested` (`subset_julia_vm/src/lowering/expr/call.rs`)
の末尾文を `Stmt::Return` から `Stmt::Expr` に変更し、IIFE が外側関数の末尾 return
ではなく値を生成する通常の呼び出し式に lowering されるようにした。
fixture `closures_iife_arrow_control_flow_8018`。
### マクロ展開された bare quote 内の export がモジュール export に記録される (Issue #7959)

マクロが `esc` を伴わない bare な `quote ... end` を返し、その末尾要素が
`export`（または `if ... export ... end`）の場合、`expand_macro_to_stmt` が
ブロックを「値を返す式」経路へ流していた。式経路では `Expr(:export, ...)` が
`nothing` リテラルに lower されるため export 効果が消失し、`collect_module_body_exports`
が `Stmt::Export` を観測できず `names(M)` / `using .M` に反映されなかった
（`esc(quote ...)` 版 (PR #7955) はトップレベル値が `Expr(:escape, ...)` でブロックでない
ため文経路を通り偶然成立していた）。`expand_macro_to_stmt` に
`block_contains_module_export_decl` を追加し、ブロック内（ネストした block や
`if`/`elseif` の分岐、`escape`/`hygienic-scope` ラッパ越しを含む）に export/public 宣言が
あれば文経路 (`value_to_stmt`) を選ぶようにして、宣言を `Stmt::Export` として保持する。
直接ソースの条件付き export（既存 fixture `module_conditional_export_7959`）に加え、
bare macro-quote 版を fixture `module_macro_conditional_export_7959` で固定。値生成マクロ
(`@show` / #7764) の値保持は回帰なし。
### パラメータ付き `::AbstractMatrix{<:Num}` が裸の `::AbstractMatrix` より特定と判定される (Issue #8025)

要素型がユーザ struct (`MyNum` / `Symbolics.Num`) の `Matrix{T}` を、裸の
`::AbstractMatrix` とパラメータ付き `::AbstractMatrix{<:T}`(および `::Matrix{T}`)の
両メソッドへディスパッチすると、sjulia は誤って裸の方(`"generic"`)を選んでいた。
原因は特異性順序ではなくマッチング段にあった: ディスパッチ用の実行時型を返す
`get_value_julia_type` が、配列ラッパの要素型をレジストリ非依存の
`array_wrapper_julia_type()` で解決していたため、`StructOf(type_id)` のユーザ struct 要素を
`Any` に潰し、`Matrix{MyNum}` を `Matrix{Any}` と見なしていた。結果として
`AbstractMatrix{<:MyNum}` が要素 `Any ⊄ MyNum` でマッチに失敗し候補から脱落、裸の
メソッドだけが残っていた。`typeof`/reflection は既に struct_defs を参照する
`array_wrapper_julia_type_resolved` を使う(Issue #7304)ため `Matrix{MyNum}` を正しく
報告しており、ディスパッチ経路だけが取り残されていた。`get_value_julia_type` の
`Value::Struct`/`Value::StructRef` 腕を resolved 版へ揃えた。組み込み要素型
(`Real`/`Float64`)は要素型が精密に追跡されていたため元から正しかった。真に
ヘテロな `Any[...]` 行列は引き続き `"generic"`。fixture
`dispatch_parametric_matrix_specificity_8025`。

### isdefined(::Module, Symbol("@macro")) がマクロ束縛を参照する (Issue #7948)

関数形式 `isdefined(::Module, ::Symbol)` がマクロ束縛 (`Symbol("@alias")` など) に対し
常に `false` を返していた。マクロは lowering 時に展開・消去され、VM 実行時には
通常のグローバル/関数レジストリに痕跡が残らないため、reflection 経路がマクロ表を
参照していなかったのが原因。コンパイル時に「モジュールごとに可視なマクロ名集合」
(`module path -> {"@name", ...}`) を `CompiledProgram.macro_bindings` に記録し、
`module_binding_is_defined` が `@`-始まりの名前をこの表で照合するようにした。
所有モジュール (`isdefined(AbstractAlgebra, Symbol("@alias"))`)、`using` で取り込んだ
export 済みマクロ (`isdefined(Main, Symbol("@alias"))`)、トップレベル Main マクロを
upstream Julia と一致させる。fixture `module_isdefined_macro_bindings_7948`。

### Symbolics 記号行列積 `A*v` / `A*B` (Issue #7889, Epic #7888)

記号要素配列の行列積を bundled Symbolics パッケージの新規 `linear_algebra.jl` に
Pure Julia で実装（要素型ジェネリックな `Base.:*(::AbstractMatrix{<:Symbolics.Num}, …)`、
`similar` ベースの結果確保）。数値 `matmul` builtin を踏まず、純数値配列は従来の
`Instr::MatMul` 高速経路を維持（Rust ディスパッチ変更なし）。値の正しさは `substitute`
で検証（fixture `packages_symbolics_matmul`）。Epic #7888 の Phase 0。実装中に bare
`{<:Num}` 束縛の dispatch ギャップ #8019 を起票・回避（W-32）。
### SubArray (view) の copy/similar/zero と element-wise 表示 (Issue #8003)

`view` の結果 `SubArray` に `similar`/`copy`/`zero`/`show` を配線。従来は
`Array`/`Memory` 専用の `similar` しかなく、`similar(view(...))` /`copy` が
「similar requires an array or memory argument」、`zero` が convert エラーで失敗し、
`println(v)` も生 struct を dump していた。`AbstractArray` 契約に合わせ、`similar`
は新規 `Array`、`copy` は要素 materialise した独立 `Vector`、`zero` は
`zeros(eltype, size)`、`show(io, ::SubArray) = show(io, collect(v))` で element-wise
表示。`subset_julia_vm/src/julia/base/subarray.jl`、fixture
`subarray_copy_similar_zero_display_8003`。`#7986` のブロッカ解消。

### OrdinaryDiffEq callbacks & events (Issue #7983)

#7865 から昇格した子 Issue（4 件目）。bundled `SciMLBase`/`OrdinaryDiffEq` に callback
subset を追加: `DiscreteCallback`、`ContinuousCallback`（bisection root-find）、
`CallbackSet`。`solve(prob, alg; callback=...)` の fixed-step RK4 経路で event 検出し、
`affect!(integrator)` が状態を変更できる軽量 `CallbackIntegrator` を渡す。代表例の
バウンシングボール（初回バウンス t≈0.4515s、床貫通なし、減衰で複数回バウンス）を
fixture `packages_ordinarydiffeq_callbacks_7983` で検証。`VectorContinuousCallback` /
adaptive 経路 / `save_positions` は #7983 残スコープ。実装中に sjulia の 2 つの
lowering/parser ギャップを発見・起票: 無名関数のインデックス代入本体 (#8007)、
複数行フィルタ内包表記 (#8008)。fixture は名前付き関数・1 行内包表記で回避。
### bare `Module` の `isa` がモジュールローカル抽象型に短縮名一致してしまうバグ (Issue #7963)

`module TypeOwner abstract type Module end; struct Box <: Module end end` のとき
`Box() isa Module`（bare `Module` は `Base.Module`）が upstream の `false` に対し
sjulia は `true` を返していた。原因: モジュールローカル抽象型 `TypeOwner.Module`
が短縮名 `Module` で `abstract_type_name_index` と `struct_hierarchy`（`Box` の親）に
登録されており、`isa` の `normalize_type_for_isa` がモジュール接頭辞を剥がして
`check_subtype("Box","Module")` を真にしていた（抽象型パスも同様）。修正:
`vm/builtins_types.rs` の `isa` で、対象が **接頭辞なし** かつ `JuliaType::from_name`
で **builtin concrete DataType**（`Module`/`Int64`/`String` 等）に解決する場合は、
短縮名ファミリ一致・抽象型インデックス一致を抑止し、正準な `type_values_subtype`
にフォールバックする（ユーザ構造体は builtin concrete 型の subtype になり得ない）。
修飾参照 `TypeOwner.Module`（ドットあり）はゲートを通らず従来通り。回帰 fixture
`module/module_bare_module_isa_shortname_7963.jl`。
### 複数行の内包表記・ジェネレータの `for`/`if` 節パース修正 (Issue #8008)

`[...]`/`(...)` の内部では改行は非意味的であるべきだが、内包表記・ジェネレータの
パーサが `for`/`if` の連鎖節の直前に現れる改行で節リストを打ち切り、
`expected RBracket` を出していた。`[x for x in xs⏎ if x > 0]` のような複数行形が
単一行形 (`[1, 3, 5]`) と同一にパースされるよう修正。

- `subset_julia_vm_parser`: `parse_comprehension_rest` / `parse_generator_rest_opts`
  が後続の `for`/`if` 節・閉じ括弧の前で改行をスキップするように変更
  (新ヘルパ `Parser::skip_newlines`)。2D 内包表記の束縛区切りカンマ直後の改行も
  `parse_for_clause` で許容 (新ヘルパ `Parser::peek_non_newline_token_after_current`)。
- 回帰テスト: パーサ corpus (`corpus_collections.rs`) に改行入り `for`/`if`/カンマ
  形を追加し、ランタイム fixture
  `comprehension/multiline_for_if_clauses_8008.jl` を追加 (julia 1.12 とパリティ確認)。
- 付随して発覚した別バグ (空白区切りの `for ... for` フラット内包表記が 1D Vector では
  なく 2D Matrix になる) は #8014 として起票（本修正の対象外、parse のみ可能化）。
### 関数値の `===`（object identity / egal）を修正 (Issue #7993)

`Value::Function` が `Egal` match の専用 arm を欠き `_ => false` に落ちていたため
`ff === ff` も `false`、struct フィールド往復で identity 喪失（`Box(ff).f === ff` も
`false`）。`builtins_equality.rs` の `Egal` に関数名比較の arm を追加し upstream の
シングルトン semantics に一致させた。詳細は DONE.md。

### `StructInstance.struct_name` を `Box<str>` 化 + enum 64B の真因を実測 (Issue #7976)

`StructInstance.struct_name` を `String`→`Box<str>` にし、`StructInstance` を
56→48B に縮小（名前は immutable なので capacity word が不要。`Box<str>` は `str`
へ deref するため `is_complex`/`is_rational`/display 等の名前述語は不変）。

**重要な実測知見**: これだけでは `Value` enum は 64B のまま下がらず、`struct_name`
は enum 天井の lever ではなかった。`Value` は inline `I128(i128)`/`U128(u128)` の
ため **align=16**（`align_of::<Value>()==16`）で、サイズは 16 の倍数。最大ペイロード
は 48B（`Struct`/`Pairs`/`NamedTuple`/`Function`）、`48+tag` が 16 アラインで 64 へ
切り上がる。64→56B には **(1) `I128`/`U128` の 16-align 除去（Box 化 or `[u64;2]`
格納）と (2) ペイロード ≤48B 維持の両方**が必要で、(1) は約 385 箇所・Int128 perf
トレードオフを伴うため別 Issue で設計判断（本 PR では未実施）。`struct_name` 縮小は
その前段（`Struct`=48B 確保）として有効。`test_value_enum_size_is_compact` のコメントに
この知見を記録（上限は 64B 据え置き）。検証: full `cargo nextest run --release` パス、
struct/Complex/Rational/Irrational/可変struct の表示は upstream julia とバイト一致。
### OrdinaryDiffEq SecondOrderODEProblem + symplectic 積分 (Issue #7985)

#7865 から昇格した子 Issue（3 件目）。bundled `SciMLBase` に `SecondOrderODEProblem`
(`f(du, u, p, t)` out-of-place / `f(ddu, du, u, p, t)` in-place の加速度 RHS) と
velocity-Verlet symplectic 積分器を追加し、`OrdinaryDiffEq` から `VelocityVerlet` を
export。保存状態は upstream の `[v; u]` 順に倣い `[du...; u...]` の結合ベクトル。
調和振動子（解析解 `cos`/`-sin`）と 2D 非結合系でエネルギー有界を fixture
`packages_ordinarydiffeq_secondorder_7985` で検証。`ArrayPartition` / 高次 symplectic
スキーム / その他 refined examples は #7985 残スコープ。

### OrdinaryDiffEq dense output / 連続補間 `sol(t)` (Issue #7982)

#7865 から昇格した子 Issue（#7981 integrator interface に続く 2 件目）。bundled
`SciMLBase` の `ODESolution` を callable（functor）にし、保存グリッド外の任意時刻を
サンプルできるようにした: `sol(t)`、`sol(t; idxs=...)`、`sol(ts::AbstractVector)`。
MVP は保存点間の**線形補間**（scalar/vector 状態を `u0 + θ(u1-u0)` の汎用形で処理、
`tspan` 外は端点にクランプ）。Tsit5 の 4 次 dense interpolant は #7982 Phase B 残。
fixture `packages_ordinarydiffeq_dense_output_7982` で線形補間値が解析解に近いこと・
`idxs` 成分選択・ベクトル化サンプルを検証。

### `Value::DataType` を Box 化 (Issue #7977 / #7966 follow-up)

`Value` enum 縮小 (#7966 item 2) の第二歩。64B 上限を律速する 56B インライン
variant のうち `DataType(JuliaType)` を `DataType(Box<JuliaType>)` に変更
（`Regex`/`Generator`/`Module`/`RegexMatch` と同じ箱化、56→8B）。構築箇所は
`Box::new`、参照箇所は auto-deref。Box を pattern で分解できないため、内側 enum を
destructure していた match は `match jt.as_ref()` / `matches!(jt.as_ref(), ...)`
guard 形へ機械的に書き換え（挙動不変）。残る 56B variant は `Struct`
(`StructInstance`) のみで、`struct_name` 除去 (#7976) が enum 上限を 64B 未満へ
下げる最後の一歩。検証: full `cargo nextest run --release` 3997 passed / 0 failed、
typeof/isa/supertype/fieldtypes/Union/`Type{T}` の DataType MWE は upstream julia と
バイト一致（`Vector{DataType}` eltype 推論の差は #7977 と無関係な既存ギャップで
main でも同一）。
### `Matrix{T}(undef, m, n)` / `Vector{T}(undef, n)` コンストラクタをサポート (Issue #7890)

`Matrix{Float64}(undef, 2, 3)` が `"Unknown parametric struct: Matrix"` で失敗していた。
`try_compile_parametric_constructor_call` の `Array`/`Vector` 攔截条件に `Matrix` を追加し、
`compile_array_constructor` 経由で `_array_undef_from_dims` へ振り分けるよう修正。
`Vector{T}(undef, n)` も維持。回帰: `array::matrix_vector_undef_constructor_7890`。

### multi-param parametric inner ctor の typeof が全 type param を報告 (Issue #7972)

`new{A,B}(...)` 構築の `typeof` が `P3{Int64}`（本家 `P3{Int64,Float64}`）と
最初の param しか報告しなかった。`NewParametricStruct` の単一パラメータ fallback を、
type param 2+ で各々 bare field 型なら全フィールド値から復元するよう拡張。単一
パラメータ struct は無回帰。非スカラ field param は `Any` 表示（残）。回帰:
`struct::multiparam_inner_ctor_typeof_7972`。

### OrdinaryDiffEq/SciML non-MVP parity gap を個別 Issue へ分解 (Issue #7865)

OrdinaryDiffEq README 可視化 MVP (#7360, milestone 33) 後の follow-up tracking
Issue #7865 を、各ギャップごとの実装 Issue に分解した。SciML integrator interface
(#7981)、dense output / `sol(t)` interpolation (#7982)、callbacks & events (#7983)、
StaticArrays variant (#7984)、`SecondOrderODEProblem` / symplectic (#7985)、broader
array surfaces (views/sparse, #7986)、full Plots recipe pipeline (#7987) の 7 件で、
それぞれ upstream 参照・fixture 計画・app-surface 影響・受け入れ条件を持ち、#7865 の
サブイシューとして紐付けた。`docs/vm/ORDINARYDIFFEQ.md` の Supported API Matrix /
Non-Goals を個別 Issue リンクへ更新し、"Promoted Follow-up Issues" 節を追加。
`#7367` は引き続き adaptive Tsit5 backend 専用として残す。

### OrdinaryDiffEq SciML integrator interface subset (Issue #7981)

#7865 から昇格した最初の子 Issue。bundled `SciMLBase` の adaptive Tsit5 stepper の
上に integrator interface の最小 subset を pure Julia で実装した: `init(prob, alg;
...)`、`step!(integ)`(次の出力点まで前進、`true`/`false` を返す)、`solve!(integ)`、
`reinit!(integ[, u0]; t0)`、`remake(prob; f, u0, tspan, p)`、`successful_retcode`。
`solve!(init(prob, alg; ...))` は既存 `solve(prob, alg; ...)` の `t`/`u` を再現する
(fixture `packages_ordinarydiffeq_integrator_7981` で検証)。`OrdinaryDiffEq` から
re-export。`step!(integ, dt, stop_at_tdt)` / 任意 `tstops` / `ReturnCode` enum は
#7981 の残スコープ。実装中に sjulia の 2 バグを発見・起票: オプション位置引数 +
キーワードの縮約アリティで kwarg が脱落 (#7992)、関数の `===` 自己同一性が偽 (#7993)。
どちらも bundled package 側で回避し issue を参照。

### `Bool(x)` 数値コンストラクタを配線 (Issue #7971)

`Bool(1)` が `"Unknown function: Bool"` で呼べなかった。`BuiltinId::Bool`（enum
末尾追加、cache 互換）を配線し、範囲検査済み `convert(Bool,x)`（#7970）経由に。
`Bool(2)`→InexactError、`map(Bool,xs)` 動作、型用法は無回帰。回帰:
`conversion::bool_constructor_7971`。

### `Value::Regex` を Box 化 + 不変集約コピーの回帰ベンチ追加 (Issue #7966)

iOS no-JIT 向け構造的性能改善の棚卸し (#7966) の第一歩。`Value` enum を 64B に
律速していた 56B インライン variant のうち `Regex(RegexValue)` を
`Regex(Box<RegexValue>)` に変更（既存の `RegexMatch` 箱化と同パターン、56→8B）。
構築箇所のみ `Box::new` ラップ、参照箇所は auto-deref で不変。enum サイズは残る
`Struct`/`DataType` の 56B に律速され 64B のままだが、上限を下げるための前段
（`StructInstance.struct_name` 除去 / `Value::DataType` 箱化は #7966 の follow-up
child issue で追跡）。
あわせて不変 `Struct`/`Tuple` のディープクローン帯域を測る回帰ベンチ
`vm_struct_copy_benchmark`（driver `benchmarks/vm_struct_copy.jl`）を追加し、Rc 共有 /
Value 縮小の効果ゲートにできるようにした (#3210)。regex 出力は upstream julia と
バイト一致を確認。
### `convert(Bool, x)` の範囲検証 (Issue #7970)

`convert(Bool, 2)` が `true` を返していた（緩い真偽値判定）。`convert_to_bool` を
faithful なレンジ検査（0/1 のみ、他は `InexactError`）に修正し、Convert builtin が
`InexactError` を pure-Julia fallback せず即 propagate するようにした（fallback は
未配線 `Bool(x)` を呼んで `"Function 'Bool' not found"` に化けていた）。`Bool[2]` も
`InexactError`。`Bool(x)` コンストラクタ未配線は #7971。回帰:
`conversion::convert_bool_inexact_7970`。

### module 修飾 parametric inner ctor の名前付き field access (Issue #7958)

`Mod.Wrapped(41)`（module 修飾 + parametric + inner ctor `new{T}`）で生成した
インスタンスの `w.x` が `"has no field x"` で失敗していた（`getfield(w,1)` は成功）。
根因 = instantiation が runtime `struct_defs` 未登録で `type_id=0` fallback。
`GetFieldByName` に base 名で `compile_context.parametric_structs` を引く fallback を
追加。multi-param の `typeof` 欠落は別 bug #7972。回帰:
`module::module_qualified_parametric_inner_field_access_7958`。

### 型付き配列リテラル `T[...]` が UInt hex 要素を convert (Issue #7953)

`Int[0x30, 0x39]` のように UInt 系 hex リテラルを符号付き/浮動の型付き配列リテラル
へ混ぜると `"Cannot store U8 in I64 array"` で失敗していた。`T[a, b, ...]` の各
数値スカラ要素を上流同様 `convert(T, x)` 経由で格納するよう
`compile_builtin_array` を修正（`ArrayElementType::literal_element_needs_convert()`
で数値+complex のみ対象）。範囲外要素は `InexactError`。回帰:
`arrays::typed_array_literal_uint_convert_7953`。

### AbstractAlgebra Phase 2 driver gate を通過 (Issues #7723/#7488)

AbstractAlgebra の Phase 2 driver surface として `AliasMacro.jl` / `Aliases.jl` /
`Assertions.jl` / `Attributes.jl` / `AbstractTypes.jl` / `ConcreteTypes.jl` を bundled
package に追加し、`AbstractAlgebra.jl` の include order で parse/lower/compile できる
ようにした。`@doc raw"""..."""` header、nested quote、`esc`、macrocall lowering、
module macro hygiene、`Expr(:struct, ...)` 由来の `@attributes mutable struct` を通し、
`PolynomialElem` / `MatrixElem` type aliases と macro-expanded `UniversalRing` が
`using AbstractAlgebra` 後に bare reference / `names(AbstractAlgebra)` /
`isdefined(AbstractAlgebra, ...)` で見える。`names(::Module)` の default exported
binding surface も `Vector{Symbol}` として実装した (Issue #7938)。回帰:
`abstract_algebra_phase2_driver_7723` / `reflection_module_names_7938`。

この過程で、nested statement macrocall の struct metadata 登録漏れ (#7943) と
include merge 後の parent macro context drain 漏れ (#7945) を修正し、loader
`CACHE_VERSION` は 12 へ更新して古い lowered package metadata を無効化した。
残スコープは typed `Dict{...}` constructors (#7934)、
dynamic `new{...}` constructor parameters (#7935)、generic `DataType` Dict keys (#7940)、
macro-injected field assignment (#7941)、macro binding reflection (#7948) に分離済み
(`@attributes Type` の quoted typed-parameter interpolation #7933 は解決済み)。

### StaticArrays `SMatrix*SVector` ほか静的配列の算術 (Issue #7461)

StaticArrays の算術は `packages/StaticArrays/src/arraymath.jl` で stub のまま
（`SMatrix * SVector` などが MethodError）だった。Phase 5 の最初の一手として、
行列×ベクトル積を中心に算術を実装した:

- `Base.:*(A::StaticMatrix, v::StaticVector)` — `(A*v)[i] = Σ_j A[i,j]*v[j]`。
  `getindex`/`size`/`length` 経由で評価し、SMatrix の行優先内部レイアウト
  (`indexing.jl`) に依存しない。結果は upstream と同じく `SVector` を返す。
- `Base.:+ / :-(::StaticVector, ::StaticVector)` — 要素ごとの和差（`W*x + b`
  アフィン核に必要）。
- `Base.:*(::Number, ::StaticVector)` と `Base.:*(::StaticVector, ::Number)` —
  スカラー倍（両順序）。

これで IFS カオスゲーム (`ifs_fractals` サンプル, Issue #7949) の
`W*x + b` を StaticArrays で書けるようになった。回帰:
`static_arrays_matvec_arithmetic_7461`（upstream StaticArrays とバイト一致を確認）。
ただし性能は下記の通り依然スカラー手書きに大きく劣るため、`ifs_fractals`
サンプルはスカラー6係数手書き（#7952）を維持する。

### StaticArrays 静的配列算術の高速化（反射除去 + `.data` 直接アクセス）(Issue #7956)

Phase 5 算術（#7461）は正しいが VM 上で非常に遅く、汎用 `Matrix*Vector` より
遅かった。プロファイル（IFS 2x2 核, dev-fast, 200k 反復）で真因を特定し最適化:

- **真因は中間 `Vector`/splat ではなかった。** 構築は安価（`SVector{2}` 構築
  ~1.5µs）。支配的コストは (1) `size`/`length` の型パラメータ反射、(2) 要素ごと
  の型付き `getindex` だった。
- **`size`/`length` の値メソッド化**（`indexing.jl`）: `size(typeof(x))` →
  `T.parameters[1]` 反射をやめ、where 句で具象構造体から直接取得
  （`size(x::SMatrix{M,N,T}) where {M,N,T} = (M,N)`）。**約20倍**（12.0s→0.58s）。
- **ファストパスの `.data` 直接アクセス**（`arraymath.jl`）: 2/3/4 サイズを手
  アンロールし、backing `.data` タプルを1回取得して行優先で生インデックス
  （要素 (i,j) = `data[(i-1)*N+j]`）、結果タプルを直接構築。型付き `getindex`
  比 **約4倍**（1.65s→0.41s）。汎用ループは fallback として保持。
- 値パラメータ特殊化（`StaticMatrix{2,2,T}`）は sjulia のディスパッチ不具合
  （#7960: 抽象型×複数整数値パラメータ×具象サブタイプで誤選択）を踏むため、
  単一メソッド内のランタイム `size` 分岐で代替。#7960 解決後は真の値パラメータ
  ディスパッチに移行可能。

結果: IFS 2x2 核が **84s → 29s（約2.9倍）**（200k 反復, dev-fast）。残りは VM の
per-call メソッドディスパッチ自体のオーバーヘッドで、純 Julia パッケージコード
では削れない。**スカラー手書きは同条件 0.013s で StaticArrays の約2200倍速い**
ため、`ifs_fractals` は引き続きスカラー実装を使う。回帰ベンチ:
`benchmarks/vm_staticarrays_matvec.jl` + `vm_staticarrays_matvec_benchmark`
（#3210）、fixture は 4x4 ケースを追加。

### macro-expanded `Expr(:struct, ...)` definitions を call-site module に登録 (Issue #7915)

`using ..Provider: @m` で import した macro が `esc(struct ... end)` を返すと、upstream
Julia は `Expr(:struct, mutable, header, body)` を call-site module の struct definition
として扱う。sjulia は `ExprHead` に `:struct` がなく、macro-return statement path が
`macro expansion returned unsupported Expr head :struct` で停止していた。`ExprHead::Struct`
を registry に追加し、macro-runtime で `Expr(:struct, Bool, header, body)` から
`StructDef` を復元する。復元した struct は `LambdaContext` の一時キューへ積み、top-level /
module body lowering が直後に drain して `Program.structs` / `Module.structs` と
compile-time struct table へ登録する。関数 body など非 top-level から出た場合は明示的に
unsupported として落とし、definition が外側へ漏れないようにした。回帰:
`macros_macro_expanded_struct_7915`。

### package entry の no-op top-level doc statement を module 前に許容 (Issue #7913)

パッケージ entry file は `module Package ... end` の外側に user code を置けない前提で、
`extract_module` が `program.main.stmts` を完全禁止していた。そのため upstream Julia が
受け入れる `@doc raw"""..."""` 付き package header が、lowering 後の top-level
`nothing` statement として残り、`invalid package layout ... top-level statements are not
allowed` で読み込み失敗していた。`Base.@doc(str)` を documentation no-op として
`nothing` に lower し、package entry の `program.main.stmts` では
`Literal::Nothing` のみ無害な header statement として許容するようにした。`println(...)`
など effectful な top-level statement は従来どおり拒否する。回帰:
`loader::tests::test_extract_module_allows_noop_package_header_statement` /
`test_extract_module_rejects_effectful_package_header_statement`。

### パッケージローダーキャッシュが古い Module バインディング/型エイリアスメタデータを再利用する (Issue #7921)

`subset_julia_vm/src/loader.rs` のパッケージローダーキャッシュ
(`SUBSETJULIA_CACHE_DIR` 配下の `.ji.json`) は、エントリを `source_hash`
(パッケージソース) と `CACHE_VERSION` で検証していた。lowering 変更で lowered
`Module` のメタデータ形状 (type-alias / module-binding。例: `PolynomialElem` /
`MatrixElem`) が増えても `CACHE_VERSION` が据え置かれていたため、同一ソースの古い
キャッシュが再利用され、デフォルトキャッシュでは
`isdefined(AbstractAlgebra, :PolynomialElem) == false`、新規キャッシュディレクトリ
では正しく `true` になっていた。修正: `loader.rs::CACHE_VERSION` を 9→10 へバンプし、
さらに `module_schema_fingerprint()` (代表的な `TypeAliasDef` を含む probe `Module`
の JSON を SHA-256 でハッシュ) を `CachedModule.schema_fingerprint` として鍵に畳み込
んだ。serde は空コレクションでも全フィールド名を出力するため、`Module` のトップレ
ベルフィールド追加・削除や probe した `TypeAliasDef` の形状変更で fingerprint が変化
し、定数バンプを忘れても古いエントリが無効化される。回帰:
`subset_julia_vm/src/loader.rs` の
`loader::tests::test_stale_cache_with_mismatched_schema_is_rejected` /
`test_cache_roundtrip_hits_with_matching_schema` /
`test_module_schema_fingerprint_is_deterministic`。

### 単一引数 rand/randn が `Any` 経由の struct でユーザ method に defer する (Issue #7901)

`rand(x)` / `randn(x)` で `x` の静的型が `Any` (関数戻り値・struct フィールド・
`Any[]` 要素・`::Any` 引数) のとき、lowering は Rust builtin `Instr::RandArg` /
`Instr::RandnArg` を発行する。そのハンドラは非 RNG 値を `rng_value_to_dim` で次元
として扱うため、`StructRef` を渡すと
`rand/randn expected an RNG or a non-negative integer dimension, got StructRef(...)`
で error になっていた。具象型の引数なら dispatch が compile 時にユーザ method
`rand(d::Picker)` を解決するため成功する — つまり同じ値が束縛形 (具象 vs `Any`) だけで
成否が変わっていた。builtin-defer-through-`Any` の先例 (#6657 getindex/first/last,
#6610 haskey/isempty/empty!, #6638 iterate) に倣い、struct 引数のときは
`rng_value_to_dim` の error 前に method table の `rand`/`randn` を再 dispatch する
ようにした (`try_defer_rand_to_user_method`)。整数次元引数は従来どおり vector を返す
(回帰なし)。回帰: `dispatch_rand_defer_any_struct_7901`。
### マクロの `__module__` が呼び出し元 module ではなく Main に解決される (Issue #7919)

`module M ... end` の中で展開されるマクロの `__module__` が、合成マクロ関数の
呼び出し引数でハードコードされた `Main`
(`subset_julia_vm/src/lowering/macro_runtime.rs` の `evaluate_macro` /
`evaluate_macro_from_value_args`) を受け取っていた。upstream Julia は
`__module__` を「マクロが展開される module」に束縛する。`LambdaContext` に
module 名スタック (`push_current_module` / `pop_current_module` /
`current_module`) を追加し、`lower_module_definition` が module 本体の lowering
中に enclosing module 名を積むようにして、両展開経路がその名前を
`__module__` の `Literal::Module` 値として渡すようにした (トップレベル展開は
従来どおり `Main`)。回帰: `macros_module_macro_module_callsite_7919`。
### module top-level `if` ブロック内の const 代入が module binding を作らない (Issue #7917)

`module M; if true; const x = 1; end; end` の後に `M.x` を参照すると
`Module M has no function named x` で失敗していた (upstream は `1`)。module body の
top-level binding を集める `collect_module_body_binding_names`
(`subset_julia_vm/src/compile/collect.rs`) が `if`/`elseif`/`else` 分岐 body に再帰
していなかったのが原因。`if` は module top-level で新スコープを導入しないため、各分岐の
`const`/`global` 代入も module メンバーに含めるべき。`Stmt::If` の両分岐に再帰するよう
修正 (`for`/`while`/`let` 等のローカルスコープには再帰しない)。AbstractAlgebra の
`@alias` マクロ展開形 (`if isdefined(...) ... else const alias = real end`) を解放する。
fixture: `modules/module_if_const_binding_7917.jl`。

### Macro-returned where bound が同じ where list の TypeVar 参照を落とす (Issue #7924)

`:(Tuple{S} where {T, S<:T})` のように macro が返した `where` 型で、後続
TypeVar の upper/lower bound が同じ `where` list の先行 TypeVar を参照すると、
`UnionAll(T, body)` の参照判定が inner `UnionAll(S<:T, ...)` の bound を見ず、
outer `T` wrapper を落として `Tuple{S} where S<:T` になっていた。
`julia_type_references_typevar` が nested `UnionAll` の lower/upper bound と
`TypeVar` bound 文字列も参照として扱うようにし、nested `UnionAll` の表示も
upstream と同じ `where {T, S<:T}` 形にまとめた。回帰:
`macros_macro_return_where_bound_typevar_7924`。

### isdefined(::Module, ::Symbol) が module 内 struct 定義を見落とす (Issue #7916)

`module M; struct Box ...; end` の struct は `struct_defs` に module 修飾名
(`M.Box`) で登録される一方、`module_binding_is_defined`
(`subset_julia_vm/src/vm/builtins_reflection/mod.rs`) は非修飾名 `d.name == field_name`
だけを照合していたため `isdefined(M, :Box)` が `false` を返していた
(構築 `M.Box(3)` は修飾名で解決されるため成功していた)。`struct_defs` と
`abstract_types` の照合に修飾名 `format!("{}.{}", module_name, field_name)` を追加。
回帰: `reflection_isdefined_module_struct_7916`。

### Matrix{Num}/Vector{Num} 配列表示が要素ごとのスカラー show を経由しない (Issue #7893)

`println([x y; x x])`(`x`,`y` は `@variables`)が
`Symbolics.Num[Num(Sym(:x)) ...]` のように各要素を構造体デバッグ表現で出していた。
配列表示経路が要素ごとに登録済み `Base.show(io, ::Num)` を呼ばず、構造体ダンプに
フォールバックしていたのが原因。`Matrix{Num}` は Memory-backed の `Array{T,N}` ラッパー
struct (`Value::Struct`) として保持されるため、`Value::ExprArgs` 専用だった既存の配列
フォーマッタが届いていなかった。

修正:
- VM 側に `render_array_via_user_show`(`vm/mod.rs`)を追加。配列(`Value::ExprArgs` /
  ラッパー struct の両カリア)の各要素を `render_value_via_user_show` で再入実行し、
  登録済み user `show` を持つ要素だけ事前レンダした文字列を差し込む。
- フォーマッタ側に `format_array_value_prerendered` / `format_array_wrapper_prerendered` /
  `array_wrapper_elements`(`vm/formatting/mod.rs`)を追加し、列優先 linear index で
  事前レンダ文字列を上書きできるようにした(`format_array_wrapper_compact` は共有
  デコーダ `array_wrapper_decode` に整理)。
- `print`/`println`/`string`/`IOPrint`/`PrintAny(NoNewline)` の各 Rust 経路を
  この helper 経由に変更。
- `show`/`repr` は pure-Julia 経路(`base/io.jl` の `show(io::IO, arr::Array)`)を
  通るが、Base ライブラリ関数内の直接 `show(io, x)` は候補メソッド集合が Base コンパイル
  時に凍結されるため後から登録した user `show` を dispatch できない既存制約に当たる。
  そこで `show(io::IO, arr::Array)` の compact 1D/2D 形を `print(io, arr)` に委譲した
  (sjulia では両者の配列出力は数値/文字列/シンボル/ネスト含め完全一致するため、
  以前誤っていた struct 要素ケースだけが変わる)。

数値/文字列/Rational/Complex/Bool/ネスト配列の表示は upstream と完全一致のまま不変。
回帰: `packages_symbolics_matrix_show`。


### Tuple destructuring in esc'd macro body lowered as `=` call (Issue #7900)

`@manipulate` (や同型のユーザーマクロ) が esc した body block を `push!(acc, <body>)`
の引数位置へ splice したとき、body 内の tuple 分割代入 `a, b = f(x)` が destructuring
ではなく `=` 演算子への CALL として lowering され `Runtime error: ErrorException:
Unknown function: =` になっていた。macro-arg constructor は分割代入を
`Expr(:call, :(=), Expr(:tuple, a, b), rhs)` として round-trip するが、macro 展開結果を
IR へ戻す `value_to_stmt` の `:call`/`:(=)` arm は **symbol LHS のみ** を特別扱いし、
tuple LHS が fall-through していた。CST 経路の `DestructureTarget` /
`emit_destructure_assignments` を `lower_destructuring_from_targets` として再利用し、
tuple LHS を statement 位置・式位置 (block 末尾の値) の両方で destructuring へ戻すよう
修正。fixture: `macros/macro_tuple_destructure_7900.jl`。

---

## 最新対応 (2026-06-25)

### Quoted export statements in macro-returned blocks (Issue #7908)

`quote ... export $name ... end` が quote constructor で unsupported になり、
AbstractAlgebra `@alias` 型の macro-returned block を通せなかった。`ExprHead::Export`
を registry に追加し、quoted CST を `Expr(:export, ...)` へ、macro return を
`Stmt::Export` へ戻す経路を追加した。回帰:
`macros_quoted_export_macro_return_7908`。

### AbstractAlgebra Phase 1 package/dependency gate (Issue #7487)

`subset_julia_vm/packages/AbstractAlgebra` を追加し、upstream 0.50.1 の dependency
gate を `Project.toml` に保持したまま通常の bundled package loader で
`using AbstractAlgebra` が解決するようにした。未登録だった `Preferences` /
`RandomExtensions` / `SparseArrays` は package load 用の minimal compatibility shim として
登録。fixture: `abstract_algebra_package_load_7487`。

### AbstractAlgebra Phase 0 audit baseline (Issue #7486)

AbstractAlgebra.jl 0.50.1 の dependency map と top-level include order を
`docs/vm/ABSTRACTALGEBRA.md` に固定した。#7486 は挙動変更を入れず、次 phase の
package loader 追加 (#7487) と macro/lowering gate (#7488) の境界を明確化する。
seed fixture `abstract_algebra_phase0_parse_baseline_7486` は top-level
`@doc raw"""..."""` と early module/type skeleton の各 source 片が `Meta.parse`
可能であることを upstream/sjulia で確認する parse-only baseline。

### Macro-returned quote が Expr.args array を再構築できない (Issue #7898)

`esc(Expr(:quote, ex.args))` のように macro が `Expr.args` を quote して返すと、
runtime macro result に `ExprArgs` array-ref が残り、`macro_runtime` の quote 再構築が
`macro expansion cannot quote value type Array` で停止していた。quote payload が直接
`ExprArgs` の場合だけ `Any[...]` typed array literal として再構築し、各要素を既存の
quote value constructor に通すことで upstream と同じ `Vector{Any}` 値を生成する。回帰:
`tests/fixtures/macros/macro_return_quote_expr_args_7898.jl`。

### LinearAlgebra dispatch-first が splat module call を user method に渡さない (Issue #7896)

`LinearAlgebra.det((A,)...)` / `LinearAlgebra.lu((A,)...)` で、module-qualified
dispatch-first shortcut が `splat_mask` を落として `compile_call` に入っていたため、
user/extension method ではなく builtin fallback に進んでいた。shortcut から generic call
へ渡すときも元の `splat_mask` / `kwargs_splat_mask` を保持し、splat 引数を展開した上で
dispatch するように修正した。回帰:
`tests/fixtures/linalg/det_lu_module_dispatch_first_4020.jl`。
### Interactive fractal explorer サンプル (barnsley_fern を置き換え)

iOS/Web/Flutter の `barnsley_fern` サンプルを、Interact `@manipulate` の dropdown で
複数 IFS フラクタル(Barnsley fern / Sierpinski 三角形 / Heighway ドラゴン)を切り替えて
chaos game で描く `ifs_fractals`(name: "Interactive fractals (@manipulate)")に置き換え。
各フラクタルの maps と `Categorical` picker は `@manipulate` 本体内の `if/elseif` で
**直接ローカル束縛**(Dict/関数ルックアップを経由しない)し、下記 2 つの gap を回避。
登録: `samples.json` + `.jl`(iOS/mobile) + `CodeSamples+Intermediate.swift` 埋め込み +
`SampleCodeTests.testIfsFractals` + `web/samples_ir.js`。

作業中に発見・起票した 2 つの upstream parity gap(いずれも julia では動作):
- **Issue #7900** (`bug`): `@manipulate` 本体内のタプル分割代入 `a, b = f(x)` が
  マクロ展開 lowering で `Unknown function: =` になる(通常ブロックでは動作)。
- **Issue #7901** (`bug`): 引数が静的に `Any` の `rand(x)`(関数戻り値/struct フィールド
  経由の Distribution 等)が builtin に降り、ユーザ/ライブラリの `rand` メソッドへ
  実行時 defer せず StructRef エラー(#6657/#6610/#6638 と同型の defer 不足)。

### push! O(n²) → O(1) for dispatch-routed Vector push! (Issue #7883)

`using Plots`(`push!(::Plot, ::Number)` を定義)が読み込まれると `compile_push` が
すべての `push!(v, x)` を `CallTypedDispatchOrBuiltin` に回し、その `BuiltinId::Push`
builtin フォールバックが backing `Memory` 全体を push 毎に再確保していた(O(n²))。
native `ArrayPush` と同じ in-place `push_array_wrapper` 経路(Issue #6873)を先に試す
ように修正し O(1) 償却に。Aizawa demo 9000 steps/`@animate every 40`/`gif`: 5.88s→1.13s。
回帰 fixture `packages_plots_push_vector_dispatch_7883`、iOS サンプルも 9000/every 40 へ。

### MvNormal explicit RNG sampling implemented (Issue #7756)

Bundled `Distributions.MvNormal` now has an explicit-RNG scalar sampling path:
`rand(rng, d::MvNormal)` dispatches to the package method instead of the VM
`rand` builtin dimension path. The implementation draws scalar `randn(rng)`
values into the standard-normal vector before applying `μ + L*z`, so untyped
public RNG wrappers still advance the supplied RNG correctly. Regression:
`distributions_mvnormal_sampling` covers inline `Xoshiro(...)`, local RNG state
advance, and reproducibility for the same seed.

### OrdinaryDiffEq README 可視化 MVP 完了 (Issue #7360)

milestone 33 の親 Issue #7360 は README visualization MVP として完了した。Phase
#7361-#7367 で scope、bundled packages、`solve` / `ODESolution`、`Plots` 表示、
iOS/Web/Flutter samples、docs/parity policy、Tsit5 adaptive backend を順に landing
した。残る full SciML / OrdinaryDiffEq / Plots parity は milestone 33 外の follow-up
#7865 に分離済み。

### Tsit5 solver backend を OrdinaryDiffEq subset に実装 (Issue #7367)

Phase #7363 の fixed-step RK4 compatibility backend を Tsitouras 5/4 tableau の
adaptive stepper に置き換えた。`solve(prob, Tsit5(); dt, saveat, reltol, abstol)`
は `dt` を initial internal step、`saveat` を saved output grid とし、embedded error
estimate で accepted/rejected steps を制御する。`stats` は `:algorithm => :Tsit5`、
`:steps`、`:attempts`、`:rejected_steps`、`:rhs_evals` を返す。fixture:
`packages_ordinarydiffeq_tsit5_adaptive_7367`。
### MacroTools が load 時に @capture を登録できず chunk_000 が red になる問題を修正 (Issue #7856)

bundled MacroTools を `using MacroTools: @capture` で読み込むと lowering 中に
`unknown macro @capture` で落ち、default-suite の `macrotools::chunk_000` が red
だった。直接の原因は `utils.jl` を include する段階で
`macro expansion cannot quote value type Any`
(`subset_julia_vm/src/lowering/macro_runtime.rs`) が発生して module 全体の load
が失敗し、`match/macro.jl` で定義済みの `@capture` / `@match` も登録されないこと。

根本原因は **nested macrocall の struct-heap 解決漏れ**。macro 展開結果の中に
別の macrocall (`Expr(:macrocall, …)`) が含まれる場合、`expand_macrocall_value`
→ `evaluate_macro_from_value_args` が専用 VM で再展開するが、その戻り値に対して
`resolve_macro_result_struct_refs` を呼んでおらず、破棄される VM の struct heap
を指す `StructRef` がそのまま変換 AST に漏れていた。MacroTools の `splitdef` /
`isshortdef` / `splitarg` は OR パターン `@capture`
(`function (fcall_ | fcall_) body_ end` 等) を使い、その `OrBind` struct が
未解決 `StructRef` として `value_to_literal` に到達して "quote value type Any" を
誘発していた。`evaluate_macro_from_value_args` の両 `vm.run()` 経路で primary
path (`evaluate_macro`) と同じ struct-heap 解決を行うようにして解消。
回帰 fixture: `macrotools_package_load_capture_basic` /
`macrotools_nested_macrocall_structref_7856`。

### OrdinaryDiffEq README MVP の docs・parity gap・回帰テストを仕上げ (Issue #7366)

`docs/vm/ORDINARYDIFFEQ.md` に Phase 5 の supported API matrix、upstream-vs-sjulia
comparison policy、sample/fixture target、MVP 外 follow-up を追加した。README MVP の
regression は upstream OrdinaryDiffEq の full numerical/display parity ではなく、
sjulia の supported surface (`ODEProblem` / `ODESolution` fields、fixed-step RK4 の
代表値、`Plot` / `Series` shape、Plotly MIME routing) を固定する方針とした。
true adaptive Tsit5 は #7367、callbacks/events・dense output・StaticArrays・full
recipe pipeline などは #7865 に分離した。completion fixture:
`packages_ordinarydiffeq_readme_mvp_7366`。

### OrdinaryDiffEq README サンプルを iOS/Web/Flutter に追加 (Issue #7365)

README 由来の linear ODE + analytical overlay と Lorenz 3D path sample を
iOS `Samples/samples.json`、web `samples_ir.js`、Flutter `mobile/assets/samples`
へ追加した。両 sample は `OrdinaryDiffEq.solve` の fixed-step RK4 backend と
Phase 3 の `plot(sol)` / `plot(sol, idxs=(1,2,3))` 経路を使い、final expression
として通常の `Plots.Plot` を返すため、既存の `application/vnd.plotly+json`
artifact routing で表示される。web は `app.js` の `samples_ir.js` cache query も
更新し、Flutter catalog tests は新しい sample IDs を確認する。

### Plots.jl subset に ODESolution 可視化を追加 (Issue #7364)

bundled `Plots` が `SciMLBase.ODESolution` を既存 `Plot` / `Series` 値に変換できる
ようにした。`plot(sol)` は scalar では `sol.t` vs `sol.u`、vector では component
ごとの time series を生成し、`plot(sol, idxs=(1,2))` は 2D phase path、
`plot(sol, idxs=(1,2,3))` は `:path3d` 3D path を返す。`plot!(sol.t, t -> ...)`
は既存 function overlay 経路で README の解析解重ね描きを処理する。Rust Plotly
backend には手を入れず、artifact MIME regression で `application/vnd.plotly+json`
到達を確認する。

### README MVP 向け `solve` / `ODESolution` を fixed-step RK4 で実装 (Issue #7363)

`SciMLBase.solve(prob::ODEProblem, alg; dt, saveat, reltol, abstol)` を追加し、
`OrdinaryDiffEq.solve` から委譲する README 互換 solve path を有効化した。
backend は Phase 0/Issue #7363 で決めた fixed-step RK4 で、scalar out-of-place
RHS (`f(u,p,t)`) と vector in-place RHS (`f!(du,u,p,t)`) の両方を扱う。
`sol.t` / `sol.u` / `sol.prob` / `sol.alg` / `sol.stats` / `sol.retcode` を持つ
`ODESolution` を返し、in-place RHS では `du` と `u` を alias しない。
`reltol` / `abstol` は keyword として受けるが adaptive 制御には使わない。
fixture: `ordinarydiffeq_linear_solve_7363.jl`,
`ordinarydiffeq_lorenz_solve_7363.jl`。

### SciMLBase/OrdinaryDiffEq の最小 bundled package skeleton を追加 (Issue #7362)

`subset_julia_vm/packages/SciMLBase` と `subset_julia_vm/packages/OrdinaryDiffEq`
を追加し、bundled package loader から `using OrdinaryDiffEq` / `using SciMLBase`
を解決できるようにした。SciMLBase 側は `AbstractODEProblem`、`ODEProblem`、
`AbstractODESolution`、`ODESolution`、`NullParameters`、Phase 2 用 `solve` hook を
提供し、OrdinaryDiffEq 側は SciMLBase の problem/solution constructor wrapper と
`solve` wrapper を export し、README 互換の `Tsit5()` algorithm object を公開する。fixture:
`packages/ordinarydiffeq_skeleton_7362.jl`。

### OrdinaryDiffEq README 可視化 MVP の受け入れ条件を固定 (Issue #7361)

`docs/vm/ORDINARYDIFFEQ.md` を追加し、Issue #7360 の milestone 33 を
OrdinaryDiffEq.jl README の線形 ODE と Lorenz in-place ODE の 2 サンプルへ固定した。
対象 API は `using OrdinaryDiffEq`、`ODEProblem`、`Tsit5()`、`solve`、
`ODESolution`、`plot(sol)`、`plot(sol, idxs=(1,2,3))`、`plot!(sol.t, ...)`。
full SciMLBase / OrdinaryDiffEq parity、adaptive Tsit5、callbacks/events、dense
output、StaticArrays、sparse/views、full recipe pipeline は MVP 外として明示した。
後続 Phase の fixture/sample 名と配置も同文書を正とする。

### @generated body の型引数返却と ntuple 風 unroll が通常値 specialization に誤分類される (Issue #7722, refs #5074)

`@generated function f(x); return x; end; f(1)` は generated body 内の `x` が
runtime 値 `1` ではなく generated-time の型オブジェクト `Int64` を指すため、
upstream Julia と同じく `Int64` を返す必要がある。sjulia は untyped 引数を持つ
generated function を通常の runtime value specialization 対象に含め、call-site
戻り型推論も body を普通の値引数関数として読んでいたため、`CallSpecialize` と
`PrintI64` などの typed consumer が生成され、実行時には `DataType` が返って
`expected I64, got "DataType"` になっていた。

修正では generated method を `needs_specialization` から除外し、generated method の
shared return inference / dispatch-side return refinement は `Any` に倒す。これにより
runtime dispatch が existing generated frame binding(`x => typeof(x)`)と returned-Expr
cache を使い、型オブジェクト返却・vararg 型 tuple 返却・`Val{N}` 由来の tuple Expr
unroll を Julia と同じ結果で実行する。回帰:
`tests/fixtures/generated/stage_type_args_ntuple_7722.jl`。

### マクロ返却の `where` 型が内側 TypeVar を束縛できない (Issue #7844)

ランタイム展開されたマクロが返す `where` 型(`Expr(:where, body, var...)`)で、
導入された内側型変数(例 `S`)が普通の変数として lowering され `UndefVarError` に
なっていた。`macro_runtime.rs` の `expr_value_to_expr` に `ExprHead::Where` アームが
無く(`expr_heads.rs` の `macro_return_to_expr=false`)、`Tuple{T,S} where S` を返す
マクロが macro-return 経路から弾かれていた。修正では `ExprHead::Where` を
`macro_return_to_expr=true` にし、新規 `where_expr_from_values` が導入変数を
`let S = TypeVar(:S)` として束縛してから本体を curly/`DynamicTypeConstruct` 経路で
lowering し、`UnionAll(S, …)` でラップする(最左変数が最外 `UnionAll`)。これにより
呼び出し側束縛の `T` は動的解決され、導入 `S` は実行時 `TypeVar(:S)` に束縛される。
`Tuple{Int64, S} where S` / `Tuple{Float64, S} where S` を upstream 同様に出力する。
回帰: `tests/fixtures/macros/macro_return_where_typevar_7844.jl`。
付随して発見した別バグは Issue #7845(quote の境界付き `where S<:Real` が `<:` を
平坦化)・Issue #7847(グローバル `T` と method `where T` の名前衝突)として別途起票。

### メソッドの `where T` 型パラメータが同名グローバルにシャドウされる ✅ 修正済 (Issue #7847)

トップレベル `T = Int64`(非パラメトリック型エイリアス `T -> Int64` として登録)が
存在すると、`function f(x::T) where T; Tuple{T, Int64}; end` のパラメータ注釈 `x::T`
が lowering 時にエイリアス対象 `Int64` へ凍結され、`where` パラメータ `T` が frame の
type_bindings に束縛されず、`Tuple{T, Int64}` が
`UndefVarError: Unbound type parameter: T` を送出していた(upstream は
`Tuple{Int64, Int64}`)。#7840 と同種(`type_alias::expand` の同名グローバル代入)だが
対象がメソッドシグネチャ。pure-Rust パーサは `function f(x::T) where T` を
`[Identifier, ParameterList, WhereClause, Block]` の兄弟ノードで並べるため、注釈は
`WhereClause` より前に解析される。修正: `type_alias` にスレッドローカルのスコープ付き
除外(`ScopedExclusion`)を追加し `expand` がそれを参照。`full_form.rs` は署名解析前に
`where` 名を先読み(`collect_where_param_names`)して除外スコープを張り、
`where_clause.rs`/operator 短形式は制約を先に解析してから署名を解析するよう並べ替え。
`where` パラメータが lexically に同名グローバルをシャドウする upstream 挙動に一致。
#7840 のエイリアス挙動(`where` の無い `U = Int64; h(x::U)`)は保持。回帰:
`tests/fixtures/where/global_collision_7847.jl` と `lowering::type_alias::tests` の
スコープ除外ユニットテスト 3 件。
### quote の境界付き `where S<:Real` が `<:` を平坦化していた (Issue #7845)

値位置の境界付き `where`(`:(Tuple{T,S} where S<:Real)`)を quote すると、`<:`
制約構造が失われ `Expr(:where, body, :S, :Real)`(`S<:Real` がバラの 2 シンボル
引数に平坦化)になっていた。upstream は `Expr(:where, body, Expr(:<:, :S, :Real))`。
原因は `lowering/expr/quote/cst_to_constructor.rs` の `where_clause_args` が、
単一の `SubtypeConstraint`/`SupertypeConstraint` ノード(子は被演算子 `S`・`Real`)を
受け取ったとき、その子へ降りて各々を別引数として push していたこと。修正では
`where_clause_args` に境界付き制約ノードを丸ごと `where_param_constructor`→
`subtype_constraint_constructor` へ流すガードを追加し、単一の `Expr(:<:, …)` 引数を
生成する。あわせて `subtype_constraint_constructor` が(パーサが `<:`/`>:` トークンを
Operator 子として残さないため)`SupertypeConstraint` ノードでは既定演算子を `>:` に
するよう修正し、`where S>:Int` は `Expr(:>:, :S, :Int)`、`where Int<:T<:Real` は
`Expr(:comparison, :Int, :<:, :T, :<:, :Real)` を正しく生成する。無境界の
`where S`(bare Symbol)は不変。回帰:
`tests/fixtures/metaprogramming/where_quote_bounded_constraint_7845.jl`。

### statement-position macro block 末尾の `:function` / `:+=` が value path で拒否されていた (Issue #7805)

#7764 の value-preserving statement-position macro path は、macro が返す outermost
`Expr(:block, ...)` の末尾値を `Stmt::Expr` として保持する。一方、末尾が
`Expr(:function, ...)` や `Expr(:+=, ...)` のような statement-only head の場合も
`value_to_branch_expr` が `value_to_expr` に流していたため、
`macro expansion returned unsupported Expr head :function` / `:+=` で落ちていた。
修正では outer block の最後の non-`LineNumberNode` 要素が `macro_return_to_stmt=true` かつ
`macro_return_to_expr=false` の head なら `value_to_stmt` path に戻す。value tail は
従来どおり value-preserving path を使うため #7764 の program-value 回帰も維持される。
esc された function callee / `+=` target は `macro_assignment_target` で unwrap する。
回帰: `tests/fixtures/macro/macro_statement_tail_stmt_7805.jl`。

### 構造体の型パラメータが宣言親型 lowering で同名グローバル/エイリアスにシャドウされない (Issue #7840)

トップレベルの `T = Int64` は非パラメトリック型エイリアス `T -> Int64` として
登録される。`struct Wrap{T} <: AbstractVector{T}` の宣言親型を lowering する際、
`lowering/struct_.rs` が `type_alias::expand` を通して親文字列を展開していたため、
グローバルの**値**がパラメトリック親テンプレートへ代入され、`AbstractVector{T}`
が `AbstractVector{Int64}` に凍結されていた。結果として
`Wrap{Float64} <: AbstractVector{Float64}` が誤って `false` を返していた。upstream
Julia は構造体の型パラメータを構造体にレキシカルにスコープするため、同名グローバル
は無関係。修正は `type_alias::expand_excluding` を追加し、`struct_.rs` から構造体
自身の型パラメータ名(`{T,...}` 句)を除外集合として渡すことで、親テンプレートを
パラメトリックなまま保つ。fixture:
`struct/parent_typevar_shadows_global_7840.jl`(MWE 2 種 + 非シャドウ回帰)。

### マクロ返却の Expr(:try) 形(catch-only / block-tail)が macro-return lowering で拒否される (Issue #7806, #7832)

ランタイム展開される Base マクロが返す `Expr(:block, ...)` は value 生成の
macro-return 経路を通る。`@lock` は末尾が `Expr(:try, ...)` のブロックに展開される
が、(1) `expr_value_to_expr` の `ExprHead::Try` アーム不在で
`macro expansion returned unsupported Expr head :try` になり (#7806)、
(2) `finally` を持たない catch-only `try`(3 引数 `[try_block, catch_var_or_false,
catch_block_or_false]`)は `handle_try_expr` の `builtin_args.len() < 4` ガードで
拒否されていた (#7832)。両者は同じ try-head macro-return 編集面の姉妹形。修正は
(a) `macro_runtime.rs` に value 生成 `Stmt::Try` を組み立てて
`try_stmt_into_value_expr` に流す `ExprHead::Try` アーム + `try_value_from_args`
を追加(ネスト `Expr(:block, ...)` のボディ値も保持)、
(b) `handle_try_expr` のガードを `< 3` に緩め、catch_var/catch_block 読み取りを
`.get()` で長さガード、(c) `handle_block_expr` でブロック末尾の try のみを
`try_stmt_into_value_expr` で value 生成形へ書き換え(`@test_throws` 等の
非末尾 try は bare 形のまま維持し回帰を回避)。fixture:
`concurrency/lock_macro.jl`(value 位置 `@lock` == 123)、
`macros/macro_return_catch_only_try_7832.jl`(catch-only `@catch_only_try` == 42)。
upstream julia 1.12 と一致。
### 式位置の `@sync` が本体値ではなく nothing を返す (Issue #7813)

`r = @sync begin; @async 1; @async 2; end` のように `@sync` を式位置(代入の
RHS や戻り値)で使うと、upstream Julia は本体の最後の式の値(MWE では `Task`)
を返すのに対し、sjulia は `nothing` を返していた。原因は
`lower_sync_block_expr` / `lower_sync_single_async_expr`
(`subset_julia_vm/src/lowering/expr/macros/mod.rs`)が生成する
`Expr::LetBlock` の最後の文が値を持たない
`if !isempty(exceptions); throw(...); end` ガードで、`LetBlock` がその
nothing を yield していたこと。修正は本体の最後の式の値を span-unique な結果
一時変数に束縛してから throw-if-failed ガードを実行し、`LetBlock` がその一時
変数を yield するようにした。末尾が `@async` の場合は upstream 同様に実 `Task`
を生成して結果とし、待機による失敗集約も維持する。fixture:
`concurrency/sync_expr_returns_value.jl`。文位置 `@sync`(Issue #7831 の範囲)は
変更しない。
### ユーザー定義 outer constructor が field-count default constructor を隠す (Issue #7793)

任意のユーザー **outer** constructor を定義すると struct 名が method table を持つ
関数として登録され、その method table には宣言済み constructor のみが入り、合成された
field-count default constructor は入らない。outer ctor と arity が異なる bare(または
module-qualified)な top-level の field-count default-ctor 呼び出しは dispatch で
`NoMethodFound` になり、`compile_struct_constructor` への fallback が単一引数アーム
(dispatch.rs)にしか無かったため失敗していた(`Foo("hi", :t, :u)` → `No method
matching Foo([Any, Symbol, Any])`)。名前が method を持つと引数型が `Any` に退化する
ため、同 arity の default ctor すら静的一致しないことも一因。修正として multi-arg /
static-miss の `NoMethodFound` 回復アームと qualified 経路の
`compile_module_call_via_method_table` に、`struct_table` の struct で
`fields.len() == args.len()` のとき field-count built-in default ctor へ fallback する
処理を追加した(#7729 と同根)。inner constructor を持つ struct には合成 default ctor が
存在しないため `has_inner_constructor` でガードし、full-arity outer ctor の自己再帰
(case F)は built-in ctor へ落ちるため無限再帰しない。same-arity の型違い outer ctor
overload も両方 dispatch 可能なまま維持。
### 文位置 @sync for ... @async ... end が await を握り潰して空結果を返す (Issue #7831)

`results = Int[]; @sync for i in 1:3; @async push!(results, i^2); end` のように、
**文位置の `@sync` が for ループを本体に取る**形が PR #7811 以降サイレントに誤動作して
いた。`lower_sync_macro_stmt`(`subset_julia_vm/src/lowering/expr/macros/mod.rs`)は
`begin ... end` ブロックと単独 `@async` だけを特別扱いし、それ以外の本体(for ループ
など)は最後の plain-statement lowering に落ちていた。その結果、ループ本体の `@async`
は例外アキュムレータにも task 収集にも wait にも乗らず、**一つも実行されないまま** for
が普通の文として lower され、`@sync for` が空配列を返していた(CLAUDE.md 原則4「サイレント
な誤りより loud error」違反)。修正は、ブロック本体の子文を `@async` / `t = @async`
ごとに `wrap_sync_async_body` で包む処理を `lower_sync_body_stmts` ヘルパに切り出し、
for ループ本体にも適用。`control_for.rs` に既存の binding / cartesian desugaring を共有
したまま本体ブロックだけ差し替える `lower_for_stmt_with_body` を追加し、変換後の本体で
for を再構築 → 共有アキュムレータに対し `sync_throw_if_failed` を発行する。range / 配列
イテラブル / 非 async 文混在 / `t = @async` 代入 / `@async` 内例外の CompositeException
集約まで upstream julia 1.12 と一致。fixture:
`concurrency/sync_stmt_for_async.jl`。
### StaticArray が AbstractArray{Any,N} のままで AbstractArray{T,N} の subtype 辺を失う / #7728 ワークアラウンド撤去 (Issue #7819 / #7728)

`SVector{3,Int64}(1,2,3) isa AbstractArray{Int64,1}` が sjulia では `false`
(upstream は `true`)。閉じた #7728 のために `StaticArraysCore`/`StaticArrays` の
親型が `AbstractArray{Any,N}` という退避形のまま残っていた。upstream どおりの
`abstract type StaticArray{S,T,N} <: AbstractArray{T,N} end` に戻したうえで、値
パラメータを `Tuple{N}` のように中間層へ受け渡す親チェーン
(`SVector{N,T} <: StaticVector{N,T} <: StaticVecOrMat{Tuple{N},T,1} <:
StaticArray{S,T,N} <: AbstractArray{T,N}`)で element 型と rank が末端まで届くよう
に二点を修正:

1. `build_struct_hierarchy_from_program`(`vm/struct_setup.rs`)で**パラメトリック
   テンプレートを具象インスタンスより先に登録**する。`SVector{Any,Any}` /
   `SVector{3,Int64}` などの単相化インスタンスは `nominal_family_name` で同じ
   ファミリーキー `SVector` に潰れ、`type_params` を空リストで上書きしていたため、
   `registered_instantiated_struct_parent_in` が具象引数を親テンプレートへ代入でき
   ず、チェーンの全辺が `false` になっていた。
2. `substitute_parent_name`(`vm/type_objects.rs`)が親引数の**ネストした型変数**
   (`Tuple{N}` 内の `N`)も再帰的に置換するようにした(従来はトップレベルのトーク
   ン一致のみで `Tuple{N}` をそのまま残していた)。

これで `SVector{3,Int64}(1,2,3) isa AbstractArray{Int64,1}` が `true` になり、
`AbstractArray{Any,N}` ワークアラウンド(W-23)を撤去。fixture:
`static_arrays/static_arrays_abstractarray_subtype_7819.jl`,
`types/value_param_abstractarray_parent_chain_7819.jl`(upstream julia 1.12 で照合)。
なお、グローバル変数が同名の型パラメータを上書きするスコープバグ(`T = ...; struct
Wrap{T} ...`)を作業中に発見し、別 Issue #7840 として起票(本修正とは独立)。

### 位置引数を持つ関数で global を参照する kwarg デフォルトが 0 になる (Issue #7774)

`close(a, b; atol=tol)`(top-level `tol`)のように、global / const-global を参照
する keyword デフォルトが、keyword 省略時に `0` と評価されていた。再現は
**関数が位置引数も持つ場合のみ**で、keyword-only な `f(; x=G)` は const/非 const の
どちらでも正しく `G` を解決していた。原因は、位置引数を持つ関数が specialize
ディスパッチ経路(`execute_call_specialize_with_args`)を通り、fallback body を
選んだとき各省略 keyword を `kwparam.default`(畳み込めないデフォルトでは
`I64(0)` の baked リテラル)で直接束縛しており、デフォルト *式* を実フレームで
評価していなかったこと。keyword-only 経路は既に `bind_kwargs_defaults` を通って
いたため正しかった。修正は specialize 経路の直接束縛ループを `bind_kwargs_defaults`
呼び出しに置き換え、`default_expr` を実フレーム(global フォールバック付き)で
評価するようにした。fixture: `kwargs/kwarg_default_global_with_positional_7774.jl`。
### ユーザ型のチェーンが AbstractArray{T,N} に届いても bare AbstractArray の subtype にならない (Issue #7787)

`abstract type AbsContainer{T} <: AbstractArray{T,2} end; struct MyArr{T} <:
AbsContainer{T}; data::Tuple; end` で `MyArr{Float64} <: AbstractArray`(bare)が
`false`(upstream は `true`)。パラメータ付きの `AbstractArray{Float64}` は #7728 で
既に `true` だった。`struct_is_subtype_of_abstract_with_lookup` の array-family
アームが組み込みの array 名しか見ず `hierarchy` を辿らなかったのが原因。数値アームに
倣い、インスタンス化された親チェーンを array 族の祖先まで歩く
`user_struct_array_ancestor` を追加して bare `AbstractArray` /
`DenseArray` / `AbstractVector` / `AbstractMatrix` を OR 拡張した(祖先の
abstractness / wrapper / rank 制約は維持)。fixture:
`types/bare_abstractarray_user_chain_7787.jl`。upstream julia 1.12 と一致。
### for-head のインライン range リテラル(整数始点 + 非リテラル float step)が 0 回反復 (Issue #7800)

トップレベルの for-head に `for u in 0:(2π/12):2π` のような **整数始点 + 非リテラル
float step** のインライン range リテラルを書くと、ループが 0 回しか回らなかった
(`collect`/`length`、変数束縛、関数内のいずれも 13 で正しいのにインラインだけ 0)。
原因は #3551 の対策が lowering 側の `is_non_integer_literal`(`Literal::Float*`/
`BigFloat` とその単項 +/- のみ)でしか ForEach 経路へ迂回しなかったこと。`2π/12` は
`BinaryOp` でリテラルではないため検知されず、`Stmt::For` の I64 高速経路で step が
`ValueType::I64` に切り詰められて 0 になっていた。`compile/stmt.rs` の `Stmt::For`
codegen にある `needs_typed_range` ガードに `F64`/`F32`/`F16`/`BigFloat` を追加し、
start/end/step の **推論型**(`infer_expr_type` は `π`・算術式を解決する)が非整数なら
リテラルでなくても `Stmt::ForEach`(Pure Julia `iterate(::StepRangeLen)`)へ迂回する
ようにした。整数 range と narrow-int/Char range は従来どおり高速経路を使う。
fixture: `range/for_head_nonliteral_float_step_7800.jl`。

### コンストラクタ本体の tuple リテラル splat が varargs をネストする (Issue #7741)

`(A, B, xs...)` のように splat を含む tuple リテラルが、`xs` の要素を展開せず
1 要素としてネストしていた(sjulia は `(2, 2, (1, 2, 3, 4))`、upstream は
`(2, 2, 1, 2, 3, 4)`)。原因は `lower_tuple_expr_impl` が splat 要素を
`SplatExpression` のまま `lower_expr` に渡し、splat マーカーを捨てて内側の値だけを
tuple 要素に積んでいたこと。`Expr::TupleLiteral` の codegen には splat 処理が無い。
upstream の `Core.tuple(a, b, xs...)` / `Core._apply_iterate` lowering に倣い、
splat を含む(かつ named field を含まない)tuple リテラルを `tuple` builtin への
splat-call(per-element splat mask 付き)に lower するよう修正した。あわせて
`compile_splat_call` が `tuple` の戻り値型を `Tuple` と報告するようにし、splat tuple が
`::Tuple` フィールド/引数へ流れる際の "Cannot convert Any to Tuple" 誤検知を解消した。
fixture: `splat/splat_tuple_literal_7741.jl`。
### JSXGraph `parametricsurface3d` (トーラス曲面) と 2 引数 JSFunction

JSXGraph.jl のドキュメントにある `parametricsurface3d` を subset に追加し、トーラス
などの閉曲面を真の曲面として描けるようにした。座標写像 `FX(u,v)`, `FY(u,v)`,
`FZ(u,v)` は生の JavaScript 文字列で渡し、`(u, v)` の **2 引数** JSFunction として
artifact に載せる。これまで `JSFunction` は単一引数 (`t`) 固定だったため、`var2`
フィールドを追加し、Rust シリアライザ (`jsxgraph.rs`) が空でない `var2` を検出したら
`{"jsfunc", "vars":[u,v]}` を出力、iOS (`JSXGraphView.swift`) / Web (`web/app.js`) の
両レンダラが `new Function(...vars, body)` で多引数関数を生成する。レンダラは
`container.create(el.type, ...)` で型名をそのまま JSXGraph に渡すため、ネイティブの
`parametricsurface3d` 要素がそのまま描画される。iOS / Web サンプルに「Torus (Plots.jl)」
(plot3d ワイヤーフレーム) と「JSXGraph Torus」(parametricsurface3d 曲面) を追加。

実装中に、外側コンストラクタを定義するとフィールド数ぶんのデフォルトコンストラクタが
bare 呼び出しでも到達不能になる不具合 (Issue #7793、#7729 の同系統) に遭遇したため、
2 引数の便宜コンストラクタは定義せず常に全フィールドで構築する形に設計した。

### LinearAlgebra forwarder の自己再帰による Stack overflow 修正 (Issue #7772)

旧実装では `Base.LinearAlgebra.lu` などの internal module-qualified call を
「ユーザーメソッド優先」で再ディスパッチするコードが、stdlib forwarder 自身を user
override と誤認し、forwarder が自分自身へ無限再帰していた。`lu` / `det` /
`inv` / `svd` / `eigen` などすべての行列分解(および `inv` 経由の `\`)が iOS
アプリ上で Stack overflow し、`lu_basic` 等の既存 fixture も含めて linalg カテゴリ
全体が落ちていた(91a788376 のリグレッション、PR CI 不在で full suite 未実行のまま merge)。

修正方針: 現行実装では `Base.LinearAlgebra` を public route として扱わず、stdlib
forwarder は private compiler bridge で nalgebra builtin へ直行する。ユーザー
override の dispatch-first (Issue #4020) は素の
`LinearAlgebra.<fn>(A)` を非修飾 `<fn>(A)` と同じ generic call へ落とすことで実現する。
override があれば最特化メソッドが勝ち、なければ forwarder→builtin で終端する。iOS の
matrix-decompositions サンプルを `matrix_decompositions_ios_7772.jl` として fixture 追加。
### 値パラメータ AbstractArray 親チェーンの要素サブタイプ伝播 (Issue #7728)

`abstract type ... <: Parent{...}` を lowering する際、パラメトリックな**親**の
型/値パラメータ(`AbstractArray{T,N}`、`StaticArray7458{Tuple{N},T,1}`)が
親の基底名だけに切り詰められていたため、サブタイプ機構が抽象スーパー型チェーンを
通して具体的な要素型・次元パラメータを代入できず、
`SVector7458{3,Int64} <: AbstractArray{Int64,1}` が誤って `false` になっていた。
struct lowering と同様に親の完全なパラメトリック表記を保持するよう修正
(`subset_julia_vm/src/lowering/abstract_.rs`)。`StaticArray{S,T,N} <: AbstractArray{T,N}`
形の StaticArrays 風階層を通じて要素/次元パラメータが伝播し、不一致は不変
(invariant)のまま `false` を返す。bare な `AbstractArray`(パラメータ無し)への
ユーザ型サブタイプは別の既存 gap として #7787 に分離。upstream julia 1.12 で検証済み。

### Distributions common univariate API (Issue #7324)

Bundled `Distributions` の既存 univariate 分布(連続8種・離散6種)に、
`modes`、`skewness`、`kurtosis`、`mgf`、`cf`、boundedness 判定、
tail quantile helpers、`loglikelihood`、主要ペアの `kldivergence` を追加した。
`mgf` / `cf` は closed form が自然な主要分布に限定し、Beta / LogNormal / Weibull など
閉形式が無い、または bundle の SpecialFunctions 未実装に依存するものは fallback error のままにする。

### Distributions truncated univariate distributions (Issue #7325)

Bundled `Distributions` に `Truncated{D}` wrapper と `truncated(d, lower, upper)` /
`truncated(d; lower=..., upper=...)` を追加した。bounds は内部では `-Inf` / `Inf` の
数値 sentinel として保持し、`lcdf` / `ucdf` / `tp` / `logtp` を構築時に保存する。
`pdf`、`logpdf`、`cdf`、`quantile`、`minimum`、`maximum`、`mean`、`insupport`、
`rand(rng, d::Truncated)`、`rand(d::Truncated, dims...)` を実装し、中心切断 Normal の
正規化、keyword one-sided bounds、再切断、inverse-CDF sampling を fixture で固定した。

### Distributions fit_mle / fit and suffstats expansion (Issue #7326)

Bundled `Distributions` の fitting API を `suffstats(D, x)` →
`fit_mle(D, ss)` の二段構成へ拡張した。#7247 の `::Type{T}` dispatch gap を避けるため、
public entry は従来どおり untyped `D` を受け、type identity branch で
Normal / Uniform / Exponential / Gamma / Beta / LogNormal / Weibull / Cauchy /
Bernoulli / Binomial / Poisson / Geometric / Categorical / MvNormal へ振り分ける。
Gamma / Beta / Weibull は Newton 反復で upstream reference に寄せ、`fit(Beta, x)` と
`fit(Cauchy, x)` は upstream の非 MLE fitting surface に合わせた。

### Distributions classical test distributions (Issue #7327)

Bundled `Distributions` に `TDist(ν)`、`Chisq(ν)`、`FDist(ν1, ν2)` を追加した。
各分布で `params`、`mean`、`var`、`mode`、support bounds、`pdf` / `logpdf` /
`cdf` / `quantile` を提供し、`cdf` は `gamma_inc` / `beta_inc` に基づく upstream
formula に合わせた。sampling は標準構成 (`Chisq` は Gamma、`TDist` と `FDist` は
Chisq 比) で実装し、explicit RNG と dimensions sampling wrapper にも登録した。

### Distributions continuous univariate expansion 1 (Issue #7328)

Bundled `Distributions` に `Laplace(μ, θ)`、`Logistic(μ, θ)`、`Rayleigh(σ)`、
`Pareto(α, θ)`、`Gumbel(μ, θ)`、`Frechet(α, θ)`、`Levy(μ, σ)` を追加した。
各分布で `params`、該当する `location` / `scale` / `shape`、moments、support
bounds、`entropy`、`pdf` / `logpdf` / `cdf` / `quantile`、`rand` を提供する。
`randexp` が bundle にはまだ無いため、指数乱数ベースの sampling は package-local
helper で実装し、Levy quantile は `_norminvcdf` による `erfcinv` 同値式で閉形式を
保った。explicit RNG と dimensions sampling wrapper にも登録した。

### Distributions continuous univariate expansion 2 (Issue #7329)

Bundled `Distributions` に `Chi(ν)`、`Erlang(α, θ)`、`InverseGamma(α, θ)`、
`InverseGaussian(μ, λ)`、`Arcsine(a, b)`、`TriangularDist(a, b, c)`、
`SymTriangularDist(μ, σ)`、`Cosine(μ, σ)`、`Semicircle(r)`、`Kumaraswamy(a, b)` を
追加した。Gamma / Chisq 派生は既存分布を再利用し、有界分布は closed-form CDF /
quantile または `_bisect_quantile` で固定した。SpecialFunctions の `digamma` はまだ
stub のため、entropy 用に package-local digamma approximation を追加した。
各型を explicit RNG と dimensions sampling wrapper に登録した。

### Distributions discrete univariate expansion 1 (Issue #7330)

Bundled `Distributions` に `NegativeBinomial(r, p)`、`Hypergeometric(s, f, n)`、
`BetaBinomial(n, α, β)` を追加した。NegativeBinomial は Gamma-Poisson mixture、
Hypergeometric は逐次 without-replacement、BetaBinomial は Beta-Binomial compound
sampling で `rand` を実装し、PMF/CDF/quantile は support scan と log-gamma/beta
formula で固定した。finite-support 分布は `support` も提供し、3 型を explicit RNG と
integer array sampling wrapper に登録した。

### Distributions discrete univariate expansion 2 (Issue #7331)

Bundled `Distributions` に `Skellam(μ1, μ2)`、`Dirac(value)`、
`PoissonBinomial(p)` を追加した。Skellam は整数次数 modified Bessel I の package-local
級数で PMF/logPMF を固定し、CDF/quantile は有限窓の PMF scan で提供する。
PoissonBinomial は upstream と同じ再帰 DP で PMF ベクトルを計算し、finite support /
mode / moments / sampling を提供する。3 型を explicit RNG と integer array sampling
wrapper に登録した。

### Distributions parity and samples polish (Issue #7332)

Milestone 31 の横断仕上げとして、Test.jl 形式の parity rollup
`distributions_parity_7332.jl` を追加し、`fixture_julia_parity.sh` で upstream Julia と
sjulia の pass/fail summary を比較できるようにした。iOS/mobile/web の
`Distributions.jl` サンプルは `StatsPlots` の pdf plot、`truncated`、`fit_mle`、
`PoissonBinomial` / `Skellam` 例を含む内容に更新した。サポート範囲は
`docs/vm/DISTRIBUTIONS.md` に一覧化した。

### Base macro expansion uses macro_runtime (Issue #7721)

Base registry macro expansion は expression / statement context とも
`lowering/macro_runtime.rs` の expansion-time VM 実行へ統一した。実行時の
`substitute_params_in_macro_expr` 経路は削除し、Base macro body は user macro と同じく
synthetic macro function として実行してから returned AST を IR に戻す。

Bootstrap 前に必要な構造 macro (`@inline` / `@noinline` / `@inbounds` /
`@boundscheck` / metadata wrappers / `@view` / `@views`) と multi-argument `@show`
だけは Rust lowering kernel として残す。macro-returned `Expr(:call, :Symbol, ...)`
は source lowering と同じ builtin mapping に戻し、macro argument constructor の
string literal escape 処理も source literal lowering と揃えた。

validation 中に見つかった Base/runtime macro gap も同じ path で塞いだ。具体的には
statement value preserving (`@time`/`@something` 等, #7764)、matrix quote
constructor (`:hcat`/`:vcat`/`:row`, #7763)、named tuple / indexed assignment /
keyword splat / `where` macro-return lowering (#7765/#7769/#7775/#7798)、nested
stdlib/package macro lookup (#7767/#7780)、module literals and unicode/operator
heads in `@assert` (#7778/#7779/#7786/#7790)、static `Val{...}` call/type operands
(#7773/#7794)、Base subtype / Big numeric literals (#7771) を fixture/full suite で固定した。
関連して AoT inference の `Literal::DataType` (#7761)、statement `@sync` の outer-local
更新 (#7768)、LinearAlgebra wrapper dispatch (#7772) も同時に修正済み。

### Runtime-expanded `@lock` accepts `Expr(:try)` tails (Issue #7806)

`@lock` は Base runtime macro expansion 後に
`Expr(:block, ..., Expr(:try, ...))` を返す。#7764 の statement-value preserving path が
outermost block を `value_to_branch_expr` へ通すようになったため、tail の
`Expr(:try)` が expression lowering に入り "unsupported Expr head :try" で失敗していた。
macro-return expression coverage に `ExprHead::Try` を追加し、source try-expression と同じ
`try_stmt_into_value_expr` 経由で value-producing `LetBlock` へ変換するようにした。
`@lock` の body value も fixture で固定した。

### VM crate integration tests avoid moved C ABI symbols (Issue #7821)

FFI split 後、`subset_julia_vm` crate 側の `integration_compile_sample_tests` が
`compile_and_run_with_output` / `free_string` を直接参照したままだったため、
full nextest が E0425 で compile できなかった。C ABI は `subset_julia_vm_ffi` 側の
責務として分離したままにし、VM crate の integration tests は既存 direct pipeline helper に
`[result] ...` 出力整形を重ねる test helper を使うよう変更した。配列 result 表示は
`ffi_support::vm_format_value` を通すため、旧 FFI output と同じ Julia-like 表示を保つ。

### Web Distributions sample test matches current sample output (Issue #7824)

`distributions_package.jl` のサンプル更新後、web host-side test が削除済みの
`Distribution: Normal{Float64}(2.0, 3.0)` 行を期待したままだったため、full nextest が
`subset_julia_vm_web::tests::test_web_sample_distributions_package_runs` で失敗していた。
サンプル実行自体は成功していたので、assertion を現行サンプルの安定出力
(`Normal mean/std`、`truncated support`、`fit_mle mean/std`) に合わせた。

### Macro-returned parametric types keep caller typevars dynamic (Issue #7830)

macro が `esc(Expr(:curly, :Vector, :T))` のように caller の `where T` を含む
parametric type Expr を返すと、macro-return converter が `Vector{T}` を静的 type-name
literal として `TypeOf` に渡し、caller method instantiation の `T` を使えなかった。
function body lowering 中の active `where` type parameter を `LambdaContext` に記録し、
`static_curly_type_name` はその名前を見つけた場合 `DynamicTypeConstruct`
(`curly_expr_from_values`) へ落とす。これにより `Vector{T}` は呼び出し時の
`T` (`Int64` / `Float64` 等) で解決され、`Vector{Int64}` など既知 static type は
従来通り fast path を使える。

### Runtime-expanded macros accept catch-only `Expr(:try)` (Issue #7832)

upstream Julia の `Expr(:try)` は `try ... catch ... end` (finally 無し) では
`[try_block, catch_var_or_false, catch_block_or_false]` の 3 引数形になる。macro-return
lowering は 4 引数以上を要求していたため、runtime-expanded macro が catch-only try を
返すと "macro expansion returned malformed Expr(:try, ...)" で落ちていた。`finally` slot を
optional にし、3 引数形は `finally_block = None` として source try lowering と同じ
`Stmt::Try` へ戻す。fixture: `macros/macro_return_try_catch_only_7832.jl`。

### Macro-spliced parametric type args evaluate caller bindings (Issue #7835)

macro が `:(Vector{$T})` のように caller binding を parametric type argument へ splice
した場合、返る `Expr(:curly, :Vector, :T)` の `:T` は source `Vector{T}` と同じく
runtime の `T` binding を読む必要がある。macro-return static type-name fast path は
active `where` type parameter 以外の symbol をすべて `TypeOf("Vector{T}")` へ文字列化していた。
`Value::Symbol` type argument は `JuliaType::from_name` などで既知 static type と分かる場合だけ
static にし、普通の caller binding は `DynamicTypeConstruct` へ落として評価する。
fixture: `macros/macro_spliced_typearg_binding_7835.jl`。

### Metaprogramming roundtrip gate (Issue #7720)

`scripts/check_metaprogramming_roundtrip.sh` を追加し、upstream `julia` と
`target/release/sjulia` で同じ seed program を実行して Test.jl pass/fail summary を比較する。
初期 corpus は `Meta.parse` source printing、`Meta.parse`→`eval`、macro が返した
`Meta.parse` 値の lowering→実行を固定する。`Meta.parse` が parser-internal head を漏らす
既存 gap は #7753 / #7754 / #7755 として分離し、対応後に corpus へ追加する。#7754 の
let/if branch seed と #7755 の keyword call eval / macro-return seed は追加済み。

### Meta.parse roundtrip が parser-internal head を漏らす (Issue #7753)

`Meta.parse(src)` / `string(::Expr)` の CST→Expr 変換 (`cst_to_value`) が
upstream 形ではなく parser-internal head を返していたのを修正した。
(a) `var"@q"` は `Symbol("@q")` (表示は `var"@q"`) に変換するようにし、
`Expr(:prefixedstringliteral, ...)` を廃止。`var` 以外の prefixed string は
upstream の `Expr(:macrocall, Symbol("@x_str"), LineNumberNode, "content")` 形にした。
(b) keyword 引数は `Expr(:kw, name, value)` (表示 `name = value`) にし、
`Expr(:keywordargument, ...)` を廃止。
(c) `:a` を `QuoteNode(:a)` で返すようにし、`Dict(:a => 1)` が `:a` の `:` を保持して
upstream 通り `Dict(:a => 1)` と表示されるようにした (従来は `Dict(a => 1)`)。

### Meta.parse keyword call の eval / macro-return lowering が `Expr(:kw)` を拒否していた (Issue #7755)

`Meta.parse("kw(a=2,b=3)")` は #7753 で upstream と同じ
`Expr(:call, :kw, Expr(:kw, :a, 2), Expr(:kw, :b, 3))` 形になったが、runtime `eval` の
`eval_call_arguments` は bare `Expr(:kw, ...)` を positional expression として評価し、
`eval: unsupported Expr head 'kw'` で落ちていた。修正では bare `Expr(:kw, name, value)` を
`Expr(:parameters, ...)` 内の keyword と同じ `eval_call_keyword` helper に通し、
kwargs map へ移す。macro-return lowering は既存の `call_expr_from_values` が bare
`ExprHead::Kw` を keyword call として扱えるため、同じ fixture で eval と macro-return
両方を固定した。`scripts/check_metaprogramming_roundtrip.sh` にも keyword call seed を追加し、
#7755 を除外リストから外した。回帰:
`tests/fixtures/metaprogramming/metaparse_keyword_eval_macro_return_7755.jl`。

### Meta.parse let/else の parser-internal head が macro-return lowering に漏れていた (Issue #7754)

`Meta.parse("let x = 2; x + 3 end")` は parser-internal `:letexpression` / `:letbindings`、
`Meta.parse("if true; 10; else; 20; end")` は tail に `:elseclause` を返していたため、
macro がその値を返すと lowering が unsupported Expr head として拒否していた。CST→Value 変換で
upstream と同じ `Expr(:let, ...)` / `Expr(:if, ...)` / `Expr(:elseif, ...)` 形へ正規化し、
`ExprHead::ElseIf` も macro-returned if tail として lowering できるようにする。roundtrip gate には
let / if-else / if-elseif-else の eval・macro-return seed を追加した。回帰:
`tests/fixtures/metaprogramming/metaparse_let_else_macro_return_7754.jl`。

### Runtime macro-return converter: named tuple & value-in-statement (Issues #7765 / #7764)

`subset_julia_vm/src/lowering/macro_runtime.rs` の runtime macro-expansion 経由の
value→IR 変換を 2 点修正した。#7765: `expr_value_to_expr` の `ExprHead::Tuple` arm が
named-tuple 形 (`Expr(:tuple, Expr(:(=), :name, value), ...)`) を常に plain `TupleLiteral`
に落としていたため、`@timed` 風 NamedTuple のフィールドアクセスが
"Field access requires a struct type, got Tuple" で失敗していた。全要素が
assignment 形なら `NamedTupleLiteral` を生成する (mixed/plain tuple は従来どおり Tuple)。
#7764: 値を返す macro を statement 位置で展開すると `value_to_stmt` の block arm が
`Stmt::Block` に落として最終値を捨てていたため、top-level の `@show f(3)` /
`@time result = f(10)` が値を失っていた。`expand_macro_to_stmt` の outermost block を
`value_to_branch_expr` 経由で value-preserving な `Stmt::Expr` に変換する
(bundled-package path と同じ方針)。再帰的な `value_to_stmt` は据え置きで、nested block の
statement-only 末尾 (`@testset` / MacroTools `@match` 等) は影響を受けない。

### 深いネスト closure からの global/const/builtin 参照 (Issue #7600)

`outer() do a; inner() do b; b + pi; end; end` のように **深さ 2 以上** の
do-block / arrow lambda から `pi`・ユーザ `const K`・非 const global `G` を参照すると、
`UndefVarError: Cannot capture undefined variable: <name>` で実行時に失敗していた。
単層 closure では top-level frame で `CreateClosure` が走るため global resolution に
fallback して成功するが、ネストすると `CreateClosure` は外側 closure の frame で走り、
そこに module global の slot が無いため capture lookup が失敗していた。

修正は 2 点:
1. `Instr::CreateClosure` (vm/exec/stack.rs): capture 名が現 frame に無い場合、
   global frame (frame 0) に fallback してから error を上げる。単層 closure の挙動と対称。
2. module-level lambda capture 解析 (compile/pipeline_ctx.rs): flat に lift される
   `__lambda_N` 同士の参照から親子関係を復元し、外側 do-block の param/local を内側で
   capture できるよう bottom-up に伝播。これにより `a`/`c` などの outer-do-block local が
   3 段以上のネストでも中間 lambda を経由して流れる (named nested function の #1744 相当)。

fixture: `closures/nested_closure_global_capture_7600.jl` (E/F/D/G/H + π + 3 段ネスト)。

### Expr head registry for quote/macro/eval dispatch (Issue #7719)

quoted AST construction、macro-return lowering、runtime `eval` が参照する
canonical `Expr` head 名を `expr_heads.rs` の registry に集約した。各 head は
CST→`Expr` value、macro return→statement/expression、runtime eval の coverage bit を持ち、
既存の string-match dispatcher は `ExprHead` enum 経由に揃えた。`Expr(:try)` と
`Expr(:parameters)` は既存 regression が通る状態を維持しつつ、#7696 / #7676 の bug tail を
fixture で固定した。

### Expr printing keeps QuoteNode Symbol syntax (Issue #7696)

`Expr` source printing は `QuoteNode(:a)` を `QuoteNode(:a)` 表記に漏らさず、
upstream Julia と同じ `:a` syntax として出力する。`string(:(Dict(:a => 1)))` と
`sprint(print, :(Dict(:a => 1)))` はどちらも `Dict(:a => 1)` になり、非 Symbol payload の
`QuoteNode(1)` は expression context で `$(QuoteNode(1))` として残る。

### var-string identifiers pass to macros as Symbols (Issue #7676)

macro argument context の `var"@q"` / `var"@qq"` は string literal ではなく
`Symbol("@q")` / `Symbol("@qq")` として quoted AST に入る。`string` / `print` の
`Expr(:tuple, Symbol("@q"), ...)` 表示も upstream と同じ `var"@q"` form を使う。

### eval'd Expr(:try) with side-effecting catch/finally no longer StoreSlot OOB (Issue #7687)

`eval(:(try error() catch; push!(log, :caught); 123 finally push!(log, :finally) end))`
が `InternalError: StoreSlot: slot out of bounds` で失敗していた問題を修正。
eval 駆動の dispatch では try body の raise が bytecode の例外ハンドラを通らず Rust の
`Err` として伝播するため、失敗した callee の frame・operand stack・return address・
インストール済み try ハンドラが残置され、後続の catch/finally body の `StoreSlot`
（`push!`/`x = 1`）が stale な callee frame の slot table を書いていた。
`eval_dispatch_call` / `eval_dispatch_call_with_kwargs` の error path で frame/stack/
return_ips/handlers の深さを dispatch 前の snapshot まで巻き戻すよう修正し、catch/finally が
元の frame で実行されるようにした。
### Distributions univariate sampler API (Issue #7323)

Bundled `Distributions` の既存 univariate 分布(連続8種・離散6種)に、
`rand(rng, d)`, `rand(d, dims...)`, `rand(rng, d, dims...)`,
`rand!(rng, d, A)`, `rand!(d, A)`, `sampler(d)` の基盤を追加した。
`rand` は VM builtin に吸われやすいため、分布ごとの明示RNG本体は
package-local `_rand_scalar(rng, d::<ConcreteDist>)` に置き、public wrapper から
委譲する形にした。

### RNG values share mutable state across user calls (Issue #7751)

`Value::Rng` の clone が RNG state をコピーしていたため、
`f(rng) = rand(rng); f(rng); f(rng)` が caller 側の RNG を進めず同じ値を返していた。
`RngInstance` を shared mutable handle 化し、Julia の mutable RNG object と同じく
関数境界をまたいでも状態が進むようにした。

### Clippy all-targets gate is clean again (Issue #7623)

`cargo clippy --all-targets -- -D warnings` で出ていた MacroTools WIP 周辺の
mechanical lint を解消した。redundant closure call、collapsible match/if、
needless borrow、signed cast lint を整理し、CI clippy gate が再び通る状態にした。

### Pure-Julia Dict values dispatch to bare Dict annotations (Issue #7632)

Pure-Julia `Dict{K,V}` StructRef を bare `::Dict` annotation から除外していた
runtime dispatch guard を carrier-removal stub にし、`f(d::Dict)` が
`Dict{Symbol,Any}` などの concrete `Dict{K,V}` 値に dispatch できるようにした。
これにより MacroTools `combinedef(dict::Dict)` は upstream signature に戻った。

### Any-typed GlobalRef field access uses VM GlobalRef projection (Issue #7743)

`x = Any[GlobalRef(Core, Symbol("@doc"))][1]` のように `GlobalRef` が
compile-time `Any` へ widen された後でも、`x.mod` / `x.name` を
runtime `GetFieldByName` から専用 `GlobalRef` projection へ routing する。
これにより MacroTools `rmdocs` は upstream と同じ
`m.mod == Core && m.name == Symbol("@doc")` 判定へ戻った。

### Partial parametric constructor calls (Issue #7734)

`M{2,2}(args...)` のように parametric callable 側で一部または全部の
`where` parameter を指定する constructor method を compile できるようにした。
compiler は `M{A,B}(...) where {A,B}` 型の method table を検出し、callee frame に
`A=2` / `B=2` などの static callable parameter を明示 bind してから constructor body を
実行する。これにより constructor body 内の validation / conversion / tuple wrapping を
default field constructor でバイパスせず、StaticArrays の `SMatrix{2,2}(1,2,3,4)` と
`SMatrix{2,2,Int64}(1,2,3,4)` も pure Julia constructor body 経由で動く。

### Quoted docstrings lower to Core.@doc inside quote blocks (Issue #7712)

改行で standalone string の直後に statement が続く quoted block では、
upstream Julia と同じ `Expr(:macrocall, GlobalRef(Core, :@doc), line, doc, stmt)`
を構築する。semicolon-separated `quote; "doc"; stmt; end` は通常の string
statement として残し、MacroTools `stripdocs` が newline docstring だけを
`rmdocs` 経由で取り除けるようにした。

### Quoted bare where clauses keep the where parameter (Issue #7714)

`:(f(a::T) where T)` の quoted `WhereExpression` で、bare identifier `T`
が `Expr(:where, :(f(a::T)))` のように drop されないようにした。
quote constructor の `where_clause_args` は、container を持たない bare
where parameter leaf を parameter 本体として扱い、braced `where {T}` と同じ
`Expr(:where, :(f(a::T)), :T)` shape を作る。MacroTools `gatherwheres` は
sjulia の tuple literal splat gap (#7741) を避けるため `tuple(params1..., params2...)`
経由で where-parameter tuple を作る。

### MacroTools.prettify resolves interpolated Function values (Issue #7711)

`prettify(:($sin(2)))` のように quoted interpolation で入った `Function`
leaf を `nameof(f)` で `Symbol` へ戻す。sjulia では `prewalk(unresolve1, ex)`
の generic function value が `unresolve1(::Function)` へ再 dispatch されず
catch-all に落ちるため、`unresolve` 内の `prewalk` lambda で `Function` branch を
inline する workaround として固定した。

### Value type parameter arithmetic in method bodies (Issue #7736)

`SM{M,N,T}` のような user-defined parametric struct method body で、`N` を
`(i - 1) * N + j` や `N == n` の value context に使う形を compile できるようにした。
binary operand に bare `where` type parameter が出る場合は runtime dispatch に委譲し、
callee frame に bind 済みの integer value parameter を実行時に読む。これにより
StaticArrays の `SMatrix{M,N,T}` indexing は `size(x)[2]` workaround ではなく直接 `N`
を使う形へ戻した。

### Module-qualified inner constructors and escaped Base-extension calls (Issue #7631)

`Plots.Animation()` のような module-qualified inner constructor calls を、
qualified method table が無い場合でも defining module の short constructor method table へ
fallback して解決する。macro `esc` が返す caller body では、`Base.push!` などの
Base extension methods を macro defining module member として `Plots.push!` に誤修飾しない。
REPL の `using Plots`; `ps=[]`; `@gif ... push!(ps,p) ...` も caller scope の `push!` として
配列へ dispatch する。

### Assignment free vars respect same-statement local shadowing (Issue #7685)

`x = rhs` の free-var analysis で、関数 hard scope 内の単純代入名を事前に local として
登録する。これにより同一 statement の RHS や do-block 内 local が outer/global 同名 binding を
誤 capture せず、closure boxing も do-block local `v` を top-level `v` と同じ box として
扱わない。

### Symbolics sqrt dispatch for Any-typed Num values (Issue #7702)

`sqrt(x)` の argument が compile-time `Any` でも、runtime 値が `Symbolics.Num` なら
`Symbolics.sqrt(::Num)` を method dispatch で選び、該当 method が無い primitive numeric では
retained `Sqrt` builtin fallback に落とす。`Symbolics.@variables` 後の `sqrt(x)` が
`SqrtF64` 直行で `StructRef` numeric conversion error になる問題を修正した。

### Stdlib macro loading skips in-progress self imports (Issue #7735)

`using LinearAlgebra` の lowering 中に、early stdlib macro registration が
`LinearAlgebra.LAPACK` 内の `import ..LinearAlgebra: inv, lu, LU` から
`ensure_stdlib_macros_loaded("LinearAlgebra")` へ再入して stack overflow しないよう、
stdlib macro scan に in-progress guard を追加した。

### StaticArrays constructors and @SVector literal macro (Issue #7459)

`StaticArraysCore` / `StaticArrays` の Phase 3 対応として、`SVector(...)`、
fully-applied tuple constructors、`@SVector [1, 2, 3]`、`Tuple`、および最小
`getindex` を fixture で固定した。`using StaticArrays` seed fixture はこの tranche で
有効化した。当初 defer した `@SMatrix [1 2; 3 4]` / `@SArray [1 2; 3 4]` は
#7733 で有効化し、`SMatrix{2,2}(...)` 型の partial parametric constructor は
#7734 で有効化済み。

### Quoted matrix literal macro arguments and StaticArrays matrix macros (Issue #7733)

macro 引数の matrix literal は quoted AST 変換で
`Expr(:vcat, Expr(:row, ...), ...)` として渡る。StaticArraysCore / StaticArrays の
`@SMatrix` と matrix-form `@SArray` は、この AST から `(M,N,args)` を取り出して
現在の MVP row-major tuple layout の `SMatrix{M,N}(args...)` へ展開する。
回帰: `tests/fixtures/macros/macro_arg_matrix_literal_7733.jl`、
`tests/fixtures/static_arrays/static_arrays_matrix_literal_macros_7733.jl`。

### StaticArraysCore static type and trait foundation (Issue #7458)

`StaticArray` / `StaticVector` / `StaticMatrix` / `StaticVecOrMat` / `StaticScalar`
と、tuple-backed `SArray` / `SVector` / `SMatrix` の Phase 2 基礎を pure Julia package
側に追加した。`SVector(1,2,3)`、`SVector{3,Int64}((1,2,3))`、
`SMatrix{2,2,Int64}((1,2,3,4))` が構築でき、`Size` / `Length` / `size` / `length` /
`eltype` / `ndims` / `Tuple` / tuple-size utility を static_arrays fixture で固定した。
`StaticArray{S,T,N}` の upstream 形 `AbstractArray{T,N}` は sjulia の既存 subtype gap
(#7728) に当たるため、`AbstractArray{Any,N}` parent と明示 `eltype` に留めた。

### StaticArrays package skeleton loads through @packages (Issue #7457)

`StaticArraysCore` / `StaticArrays` / `PrecompileTools` を bundled package として追加し、
`using StaticArrays` と `using StaticArraysCore` が default `@stdlib:@packages` load path から
解決できるようにした。各 package source は `include(...)` 構造を保ったまま
`packages/mod.rs` に個別登録し、loader cache hash が included source を見る経路も固定した。
`PrecompileTools` は sjulia が package precompile hook を実行しないため、pure-Julia no-op
macro shim として提供する。`SVector` constructor / `@SMatrix` / indexing などの seed API は
後続 phase の対象なので Phase 0 fixture は skip のまま、ロード専用 fixture を追加した。

### StaticArrays upstream audit and seed baseline (Issue #7456)

StaticArrays.jl 1.9.18 / StaticArraysCore を参照し、MVP 対象ファイル、
dependency 方針、明示 deferral、Phase 1-5 の対応表を `STATICARRAYS.md` に整理した。
`static_arrays/` に `using StaticArrays`、`SVector`、`@SMatrix`、`Size`、indexing/shape の
seed baseline fixture を追加した。Phase 0 では未 bundle が期待状態なので fixtures は
`skip = true` とし、Phase 1 以降で有効化する。

### MacroTools core matcher works across module-local imports (Issue #7451)

`module M; using MacroTools: @capture; ... @capture(...) ... end` のような module-local
selective import を lowering context に反映し、別 module から imported `@capture` を使えるようにした。
同じ fixture で top-level `using MacroTools: @match` の matcher branch も固定した。
`macrotools::` は upstream `match.jl` smoke を含めて通る。

### MacroTools AST and macro-expansion substrate is covered (Issue #7450)

MacroTools が要求する `Expr` / `QuoteNode` / `LineNumberNode` / `GlobalRef`、
quoted `Expr(:block, ...)` / `Expr(:try, ...)` / `Expr(:macrocall, ...)`、
`Base.isexpr` / `Meta.isexpr`、`@q` / macro-definition 内 `@qq` の基礎動作を
既存 fixture 群で固定した。`macrotools::` fixture は bundled MacroTools の upstream
`match` / `split` / `destruct` / `utils` / `flatten_try` smoke を含めて通る。
残る full upstream parity gap は #7634/#7647 に分離済み。

### @__DIR__ works as a call argument (Issue #7494)

`joinpath(@__DIR__, "file_dir_macros.jl")` のように source-location macro を
別 call の argument position で使う形が parse/lower できることを既存 fixture で固定した。
MacroTools の `joinpath(@__DIR__, "..", "animals.txt")` 型の package data path construction を塞がない。

### Unicode ≤ and ≥ comparisons lower as standard comparisons (Issue #7500)

`2 ≥ 1` / `1 ≤ 2` が `UnsupportedOperator` にならず、ASCII `>=` / `<=`
と同じ `BinaryOp::Ge` / `BinaryOp::Le` として lower/execution されることを既存 fixture で固定した。
MacroTools matcher sources の Unicode comparison operator を書き換えずに扱える。

### Ternary branch macro calls expand with context (Issue #7503)

`true ? @m() : false` / `false ? @m() : false` のような ternary branch
position の macro call が active macro context で展開されることを既存 fixture で固定した。
MacroTools `isslurp(p) ? @trymatch(...) : @nomatch(...)` 型の matcher branch を塞がない。

### Unary operands keep macro context for ternary branches (Issue #7505)

`!(false ? @m() : false)` のように unary expression の operand 配下にある
ternary branch macro call が active macro context で展開されることを fixture で固定した。
MacroTools validation 中の nested expression-position macro expansion を塞がない。

### Nested quote interpolation in call arguments parses (Issue #7507)

`Expr(:$, :($TypeBind($(Expr(:quote, name)), Set{Any}([$(ts...)]))))` のような
nested quote interpolation が parse error にならず、call argument と nested vector splat
interpolation を含む Expr tree を構築できることを fixture で固定した。
MacroTools TypeBind matcher construction を塞がない。

### Short-form function bodies keep macro context (Issue #7509)

`f(x) = x ? true : @m()` のような short-form function body 内の expression-position
macro call が active macro context で展開されることを既存 fixture で固定した。
MacroTools `match_inner(...)= ... : @nomatch(...)` 型の short-form helper を塞がない。

### var"name" identifiers passed to macros are Symbols, not Strings (Issue #7676)

`@showarg var"@q", var"@qq", postwalk` のように `var"..."` を macro 引数 tuple に
渡したとき、quote lowering (`cst_to_expr_constructor`) が prefixed-string `var"@q"` を
`String` literal `"@q"` ではなく `Symbol("@q")` として AST に積むよう修正した
(upstream は identifier symbol を保持する)。あわせて Julia-source 整形
(`format_symbol_name` / `value_to_julia_code`) が valid identifier でも operator でも
ない symbol を `var"name"` として出力するようにし、`string(ex)` が
`(var"@q", var"@qq", postwalk)` と upstream 一致するようにした。`Expr(:tuple, ...)` の
comma grouping (#7526) はそのまま。

### Quoted semicolon blocks construct Expr(:block) (Issue #7511)

`:($line;$yes)` が parse error にならず、interpolated values を含む `Expr(:block, ...)`
として構築されることを fixture で固定した。MacroTools match macro の quoted block
construction を塞がない。

### Quoted let expressions construct Expr(:let) (Issue #7512)

`:(let x = 1; x end)` が quote lowering で拒否されず、upstream Julia と同じ
`Expr(:let, binding, body)` shape になることを fixture で固定した。MacroTools match macro の
quoted `let` construction を塞がない。

### Prime-suffixed identifiers work in ternary branches (Issue #7513)

`s′` のような prime-suffixed identifier が ternary branch position で parse/lower できることを
fixture で固定した。MacroTools `replace(ex, s, s′)` 型の helper 定義を塞がない。

### 子モジュールの無修飾メソッドが親モジュールの型付きメソッドに負けるのを修正 (Issue #7575)

sjulia は各モジュール関数を bare 名 (全モジュール共有のフラットプール) と
module-qualified 名の両方の method table に登録する。共有 bare プール上の多重ディスパッチで
親モジュールの同名・より具体的な型付きメソッドが、子モジュール内からの無修飾呼び出しを奪っていた
(`A.B.g(1)` が子の `A.B.f(x)` ではなく `A.f(::Number)` を選び `:outer` を返す)。
子モジュール内で無修飾呼び出しをコンパイルする際、現モジュールが自前で所有する generic 関数は
module-qualified table (`A.B.f`) へ解決するようにした (`module_owned_function_table_name`)。
`using`/`import` で取り込んだ名前 (同一 generic を共有) は除外し、Base/prelude generic の拡張
(`Base.:*(::Diagonal, ...)` 等、qualified table が部分シャードにすぎないもの) と、bare/qualified が
同一メソッド集合の単一モジュール generic (builtin へ forward する `LinearAlgebra.det` 等) も除外する。
これにより #7468 の `LinearAlgebra.BLAS.dot` vs `LinearAlgebra.dot` や行列積の dispatch を壊さない。

### Quoted single tuple interpolation parses (Issue #7514)

`:($arg,)` が parse error にならず、upstream Julia と同じ single-element
`Expr(:tuple, value)` として構築されることを fixture で固定した。MacroTools `longdef1`
の `:($arg,)` signature construction を塞がない。

### eval handles Expr(:try) basics (Issue #7683)

`eval(:(try ... catch ... finally ... end))` が `unsupported Expr head 'try'` にならないよう、
eval interpreter に `Expr(:try, try_block, catch_var_or_false, catch_block_or_false[, finally])`
の基本実行を追加した。catch return と finally 実行後の try value 維持を fixture で固定した。
catch/finally 内の外側配列 mutation は #7687 に分離。

### eval Expr(:try) preserves else branch values (Issue #7727)

Quoted `try ... catch ... else ... finally ... end` は upstream Julia と同じ
`Expr(:try, try_block, catch_var_or_false, catch_block_or_false, finally_or_false, else_block)`
shape を構築する。`eval` は try body が例外を投げなかった場合に else block を評価して返し、
例外時は catch value を返す。これにより MacroTools upstream `flatten_try.jl` の
`eval(flatten(... else ...))` cases が restored fixture として通る。

### eval catch path unwinds nested frames before caller stores (Issue #7730)

`eval(:(try error() catch; value ... end))` が catch value を返した後、nested dispatch の
callee frame を target depth まで巻き戻す。これにより `x = eval(...)` や `@test eval(...) == ...`
の caller-side `StoreSlot` が callee frame 上で実行される slot bounds regression を防ぐ。

### Quoted function-definition Pair expressions parse (Issue #7517)

`:(begin function f_(args__) body_ end => rhs end)` が parse error にならず、
`Expr(:call, :=>, Expr(:function, ...), :rhs)` として構築されることを fixture で固定した。
MacroTools `@match` clause の function-definition Pair pattern を塞がない。

### Anonymous block function expressions are callable values (Issue #7518)

`function (x) ... end` を expression position で拒否せず、匿名名つき nested
`FunctionDef` と function value へ lower するようにした。代入、直接呼び出し、
outer variable capture を fixture で固定し、MacroTools の value-position anonymous
function pattern を塞がない。

### Quoted parenthesized operator function heads parse (Issue #7519)

`:(function (fcall_ | fcall_) body_ end)` が parse error にならず、
operator function signature `Expr(:call, :|, :fcall_, :fcall_)` として構築されることを
fixture で固定した。MacroTools capture pattern の operator function head を塞がない。

### Quoted function definitions interpolate names (Issue #7520)

`:(function $fname(x) ... end)` が parse error にならず、interpolated function name を
signature の callee に splice できることを fixture で固定した。MacroTools `combinedef`
形に合わせ、`$fname($(args...); $(kwargs...))` の parameter splat 併用も確認する。

### Quoted function definitions splice parameter lists (Issue #7522)

`:(function f($(args...)) ... end)` と
`:(function g($(args...); $(kwargs...)) ... end)` が parse error にならず、
interpolated positional/keyword parameter list を function signature の
`Expr(:call, ...)` に splice できることを fixture で固定した。MacroTools `combinedef` の
quoted function reconstruction を塞がない。

### Quoted interpolated field assignments parse (Issue #7523)

`:($x.$f += $v)` / `:($x.$f = $v)` のように receiver と field name の両方を
interpolation した quoted field assignment が、parse error にならず
`Expr(:+=, Expr(:., obj, QuoteNode(field)), ...)` /
`Expr(:(=), Expr(:., obj, QuoteNode(field)), ...)` として構築されることを fixture で固定した。
MacroTools `resyntax` の field rewrite branch を塞がない。

### Whitespace macro comma arguments stay one tuple arg (Issue #7526)

`@m alpha, beta` のような whitespace macro call が、macro varargs 2個ではなく
upstream Julia と同じ `Expr(:tuple, :alpha, :beta)` 1引数として渡ることを fixture で固定した。
MacroTools `@public a, b, ...` 型の single-argument macro を
`macro @m not found (with 2 args)` に落とさない。

### VersionNumber comparison operators are covered (Issue #7529)

`VersionNumber` 同士の `<` / `<=` / `>` / `>=` が `MethodError` にならず、
Julia と同じ lexicographic major/minor/patch comparison として動くことを
`version/version_comparison.jl` で固定した。MacroTools `@public` の
`VERSION >= v"..."` 形も直接 fixture で確認する。

### MacroTools TypeBind splats Set head patterns (Issue #7670)

`MacroTools.match_inner(TypeBind(:x, Set{Any}([:call])), :(f(1)), env)` が
`b.ts...` を `isexpr` の可変長引数として展開できず、`Set` そのものを 1 引数に渡して
`MatchError` になっていた。VM の splat 展開で pure-Julia `Set{T}` wrapper から
backing `Dict{T,Nothing}` の filled key を読み、Set の iteration 順と同じ head set を
可変長引数へ展開する。

### MacroTools upstream utils fixture restored (Issue #7647)

MacroTools upstream `test/utils.jl` v0.5.16 の `animals` / `isdef` /
`flatten` / `flatten try` / `@qq` checks を restored fixture として通す。
bare function values carry resolved method candidates so HOF/runtime calls do
not confuse `MacroTools.flatten` with `Base.Iterators.flatten`; quoted
assignment LHS lowering preserves `where` expressions; macro expansion
`LineNumberNode` values set nested definition spans so `which(...).line`
matches upstream.

### Top-level begin assignment RHS keeps global bindings (Issue #7667)

`x = begin ... end` の RHS block を `LetBlock` local scope として閉じず、
assignment statement lowering で inline block に展開する。top-level では block 内の
代入が surrounding global binding として残り、function 内では同じ local scope に残る。
最後の単純代入も Julia と同じく代入値を RHS value として返す。

### Any-typed infix equality reaches user struct methods (Issue #7643)

`Any` 経由の infix `==` が `CallDynamicBinaryBoth` の候補から user-defined
`==(::S, ::S)` を落とし、同じ値でも `==(s, S(...))` だけが正しく dispatch していた。
user-written binary method を runtime signature resolver の候補に残すことで、macro-generated
nested block assignment 後の `s == S("foo")` も user equality に到達する。

### MacroTools selective `striplines` import visibility (Issue #7645)

`using MacroTools: striplines` で selective import した helper が、unqualified
`striplines(ex)` として呼べることを fixture で固定した。MacroTools upstream utils fixture の
`MacroTools.striplines(...)` workaround を upstream-compatible な bare call に戻した。

### MacroTools animals module-qualified constant lookup (Issue #7646)

`MacroTools.animals` が `function MacroTools.animals` ではなく、package data 由来の
`Vector{Symbol}` constant として解決されるようにした。`Module.name` の値参照で、
using visibility 用に `module_functions` へ混ぜている module constants を function ref より
先に `LoadGlobalAny("Module.name")` として扱う。

### MacroTools destruct_key handles QuoteNode key patterns directly (Issue #7637)

`MacroTools.destruct_key(QuoteNode(:a), :tmp, MacroTools.getkeym)` が
`atoms(i -> getm(val, i), pat)` の closure 経路に入り、captured callable argument
`getm` を `Unknown function: getm` として失敗していた。bundled MacroTools の
`destruct_key` で atomic pattern を `getm(val, pat)` へ直接流す path を使い、
QuoteNode key pattern が postwalk closure を経由しないことを fixture で固定した。

### MacroTools @destruct captures array patterns (Issue #7636)

`MacroTools.@destruct [a, b] = Dict(:a => 1, :b => 2)` が upstream MacroTools と同様に
array-style destructuring pattern を capture し、`a` と `b` へ key lookup 結果を
束縛できることを fixture で固定した。bundled MacroTools の structural
`Expr(:vect, ...)` path により、`Unrecognised destructuring syntax [a, b]` を回避する。

### Macro expansions can return function definitions (Issue #7634)

macro expansion が返す `Expr(:function, Expr(:call, :foo, :x), body)` を
statement-position の `Stmt::FunctionDef` に戻せるようにした。これにより
MacroTools `combinedef` / `@splitcombine` 系の macro-generated function definition が
`macro expansion returned unsupported Expr head :function` で止まらず、通常の関数定義として
登録される。

### Typed full-form methods keep nested arrow lambdas callable (Issue #7545)

`function rmlines(x::Expr)` のような typed full-form method 内で
`filter(x -> !isline(x), ...)` に渡す nested arrow lambda が、nested-function qualification
後も runtime で解決できることを既存 fixture で固定した。MacroTools `utils.jl` の
`rmlines(x::Expr)` 相当の形で `Function '...#__lambda_nested_...' not found` を回避する。

### Nested MacroTools-style quote macros preserve function locals (Issue #7542)

関数内で `esc(Expr(:quote, ex))` 型の nested quote macro を呼び、`@q $y` 相当の
interpolation が function-local `y` を macro expansion 時ではなく runtime/caller context
で解決することを既存 fixture で固定した。MacroTools `@match` branch template 内の
nested `@q` が `UndefVarError: f not defined` / `args not defined` になる早期評価を回避する。

### MacroTools shortdef typed branches avoid nested @q splat failure (Issue #7541)

MacroTools `shortdef1` の `function f_(args__)::rtype_ body_ end` 系 branch が
`@q $f($(args...))::$rtype = ...` を load/lowering 時に評価して function-local capture を
見に行かないよう、upstream と同じ `Expr(:(=), Expr(:(::), Expr(:call, f, args...), rtype), ...)`
shape を明示構築する。`shortdef(:(function f(x)::Int ... end))` が `Expected Function or
Closure, got Symbol(:f)` で失敗しないことを fixture で固定した。

### Quoted typed/where function signatures construct Expr(:function) (Issue #7540)

`:(function f(x::T) where T ... end)` が upstream Julia と同様に
`Expr(:function, Expr(:where, ...), body)` を構築できることを fixture で固定した。
quote constructor が typed parameter / where signature を拒否して MacroTools `utils.jl`
の `shortdef` / `combinedef` 系 helper を止める問題を回避する。

### MacroTools @capture quotes runtime Any payloads (Issue #7539)

`using MacroTools` と basic `@capture(ex, f_(args__))` が、heterogeneous runtime payload を
含む macro return value の quote conversion で `macro expansion cannot quote value type Any`
にならないことを fixture で固定した。MacroTools package load が `utils.jl` の
`@capture` / matcher helper expansion を通過できる。

### Quoted typed expressions construct Expr(:(::), ...) (Issue #7537)

`:(x::Int)` が upstream Julia と同様に `Expr(:(::), :x, :Int)` を構築できることを
fixture で固定した。MacroTools `@capture` / matcher patterns が typed expression を含む
quoted AST を扱う際に `quote for typed_expression not yet supported` で止まらない。

### MacroTools @capture splats generated binding assignments (Issue #7536)

MacroTools `@capture` の `quote ... $([:($(esc(b)) = nothing) for b in bs]...) ... end`
形が、binding 名の comprehension を評価して複数 assignment を quoted block に splice
できることを fixture で固定した。match 成功時の captured values と、match 失敗時の
`nothing` 初期化の両方を確認し、`Undefined variable: [:($(esc(b)) = nothing) for b in bs]`
にならないことを押さえる。

### MacroTools allbindings handles QuoteNode.value guard (Issue #7535)

MacroTools `allbindings(QuoteNode(:x_), bs)` が `isa(pat, QuoteNode)` guard 後の
`pat.value` を動的 field access として扱い、`type Expr has no field value` で
macro helper compilation を止めないことを fixture で固定した。`@capture` expansion が
`allbindings` dependency を含んでも package load を通過できる。

### MacroTools TypeBind @nomatch fallback lowers (Issue #7534)

MacroTools `match_inner(b::TypeBind, ex, env)` の mismatch branch が `@nomatch(b, ex)` を
short-form method body から展開し、`return MatchError(...)` として実行できることを
fixture で固定した。`match/types.jl` lowering が `type Expr has no field value` で
止まらず、package load が `@nomatch` dependency を通過する。

### Package-internal MacroTools @capture resolves defining module (Issue #7533)

`using MacroTools` 中の package-internal `@capture` expansion が、macro 定義元 module
`MacroTools` の helper を expansion-time compilation で解決できることを既存 fixture で
固定した。`Unknown module: MacroTools` で `utils.jl` の local macro expansion が止まらず、
bundled package path から basic `@capture` call を実行できる。

### Short-form arrow closures capture outer parameters (Issue #7531)

`makepred(x) = y -> y == x` のような short-form function body から返る arrow closure が、
outer parameter `x` を capture して runtime で callable になることを既存 fixture で固定した。
VersionNumber comparison helper などの `isequal(x) = y -> isequal(y, x)` 形が
`Undefined variable: x` で compile 失敗しない。

### Macro-expanded Pair calls restore to Pair expressions (Issue #7639)

macro expansion が返す quoted code 内の `:a => 1` が
`Expr(:call, :=>, :a, 1)` のまま generic call として実行され、
`Unknown function: =>` になる問題を修正した。macro result conversion の
`call_expr_from_values` で callee が `=>` の通常 2 引数 call を `Expr::Pair` に戻し、
source の `Dict(:a => 1)` lowering と同じ path に通す。

### MacroTools @match block macro arguments keep clause shape (Issue #7547)

`@match ex begin ... end` の macro argument が、clauses 全体を包む余分な
`Expr(:block, ...)` として渡らず、MacroTools `clauses(lines)` が各 clause を期待形で
処理できることを existing regression fixture で固定した。これにより
`Invalid match clause Expr(:block, ...)` を回避できる。

### MacroTools @match helper splat dependencies are available (Issue #7548)

MacroTools `@match` expansion の `foldr((clause, body) -> makeclause(clause..., body), ...)`
形で、lifted lambda 内の splat call から必要になる `makeclause` などの helper が
expansion-time program に含まれることを existing regression fixture で固定した。
`macro_dependency_functions` の full dependency retry path により、`Cannot find function
'makeclause' for splat call` を回避できる。

### Quote interpolation splats generator values into Expr construction (Issue #7549)

quote 内の `$((expr for x in xs)...)` が runtime `Expr` construction 時に generator
値を splat できることを existing regression fixture で固定した。MacroTools `bindinglet`
が capture bindings を quoted body に展開する形で `Cannot splat value of type Generator`
にならず、`Expr(:block, ..., Expr(:(=), ...))` を構築できる。

### Macro-returned Expr(:let, ...) lowers to LetBlock (Issue #7550)

macro expansion が `Expr(:let, bindings, body)` を返したときに `LetBlock` として
lowering できることを existing regression fixture で固定した。single binding と
multi binding の両方を扱い、MacroTools `@match` の `bindinglet` が返す let AST を
expression position で評価できる。

### MacroTools nested macro dependencies avoid unrelated include helpers (Issue #7551)

`using MacroTools` の package load が、nested `@q` expansion 時に同じ include file の
無関係な helper（`MacroTools.*` を参照するもの）まで compile-time program に含めて
`Unknown module: MacroTools` で失敗しないことを regression fixture で固定した。
`macro_dependency_functions` による transitive dependency filtering の範囲で
MacroTools `utils.jl` の local macro expansion を通過できる。

### Macro expansion dependencies include higher-order function arguments (Issue #7552)

macro expansion 用の compile-time dependency filtering が、`prewalk(rmlines, ex)` の
ように関数値として渡される helper も含めて展開できることを regression fixture で固定した。
MacroTools `@q` の `striplines(ex) = prewalk(rmlines, ex)` 形で `rmlines` が欠落せず、
expansion-time program を構築できる。

### Quoted where expressions construct Expr(:where) values (Issue #7553)

`:(x where {T})` の quote construction が `Expr(:where, :x, :T)` を生成できることを
regression fixture で固定した。MacroTools `@q` helper が function/arrow signature を
再構築するときに使う `where` expression AST を upstream Julia と同じ head/args で扱える。

### Quoted adjoint expressions construct Expr(:') values (Issue #7554)

`:(x')` の quote construction が `Expr(:', :x)` を生成できることを regression
fixture で固定した。MacroTools `resyntax` の `adjoint(x_) => :($x')` 形で必要になる
adjoint expression AST を、quote constructor が upstream Julia と同じ head/args で扱える。

### Macro-generated Expr(:public, ...) lowers as a declaration no-op (Issue #7625)

macro expansion が `esc(Expr(:public, :foo))` を返すと、statement context の
`esc` が expression lowering に落ち、`macro expansion returned unsupported Expr head :public`
で module lowering が失敗していた。macro runtime の statement conversion で
`escape` / `hygienic-scope` を unwrap し、`Expr(:public, symbols...)` を source の
`public` statement と同じ compile-time-only no-op として lower するようにした。

### Quoted field assignment from macro expansion is lowerable (Issue #7630)

macro expansion が `QuoteNode(:($x.$f = $v))` のような quoted field assignment
を返す場合に、`Expr(:(=), Expr(:., ...), value)` の assignment target を
Symbol 前提で拒否しないことを regression fixture で固定した。これにより
MacroTools `resyntax` が生成する `:($x.$f = $v)` 形を Expr object として扱える。

### Macro expansion Expr(::) values lower as typeassert (Issue #7628)

macro expansion が value position に返す `Expr(:(::), value, type)` を
`macro expansion returned unsupported Expr head :::` で拒否していた問題を修正した。
macro runtime の `expr_value_to_expr` が 2 引数の `::` Expr を通常 CST の typed
expression と同じ `typeassert(value, type)` call に変換する。これにより
`esc(:(x::Int))` のような macro 返却値や、MacroTools が生成する typed expression
AST を lowering できる。

### index/field 代入の値に現れるネストした lambda を登録 (Issue #7615)

`xs[1] = map(x -> x + 1, xs[1])` のように **index 代入の RHS** に lambda が現れると、
その lambda は関数値としてコンパイルされるのに生成関数が登録されず、実行時に
`Function 'f#__lambda_nested_...' not found` で落ちていた。原因は
`compile/collect.rs` の `collect_stmt_functions` が `Stmt::Assign` の value は再帰する
のに、`Stmt::IndexAssign` / `Stmt::FieldAssign` / `Stmt::DictAssign` に対応する分岐が
無く `_ => {}` に落ちていたこと（同種の走査は AoT の `call_graph` には存在）。
`IndexAssign`（indices + value）・`FieldAssign`（value）・`DictAssign`（key + value）の
分岐を追加し、これらの位置に現れる lambda を収集するようにした。index 位置の lambda
（`xs[findfirst(x -> x < 0, ys)] = ...`）や `d[:a] = map(...)`（IndexAssign に lowering）も
解決。MacroTools upstream `split.jl` の `dict[:args] = map(arg -> ..., dict[:args])` を
unblock。fixture: `closures/nested_lambda_index_assign_7615.jl`（julia とパリティ一致、4/4）。

### eval handles Expr dotted callees (Issue #7616)

`eval(Expr(:call, Expr(:., :MacroTools, QuoteNode(:trymatch)), ...))` のように
関数位置が dotted callee `Expr(:., module, QuoteNode(name))` になっている AST を、
macro runtime の `eval` が `call expression function must be Symbol or GlobalRef`
で拒否していた問題を修正した。`eval_expr_ast` の call callee 解決で dotted callee
を `module.name` の `GlobalRef` 相当として扱い、既存の module-qualified dispatch
path へ流す。

### Quoted typed assignment LHS preserves Expr(::) (Issue #7622)

`:(x::Int = nothing)` の quoted assignment で、LHS が `Symbol("x::Int")` に潰れて
`MacroTools.splitarg` などの AST 解析が typed binding として扱えない問題を修正した。
quote constructor の `NodeKind::Assignment` で `TypedExpression` LHS を
`FieldExpression` / `IndexExpression` と同じく再帰変換し、upstream Julia と同じ
`Expr(:(=), Expr(:(::), :x, :Int), :nothing)` 形を作る。plain identifier LHS
（`:(x = nothing)`）は従来どおり `:x` のまま。

### Expr 値に対する getfield/getproperty を Expr 対応 (Issue #7614)

`getfield(ex, :head)` / `getfield(ex, :args)` / `getproperty(ex, :head)`（および
`getfield(ex, 1)`=head, `getfield(ex, 2)`=args の整数添字版）が、upstream julia では
正常に `Expr` のフィールドを返すのに、sjulia では
`Type error: getfield with Symbol requires a struct, NamedTuple, Module, or DataType, got Expr(...)`
で実行時エラーになっていた。原因は、`.head`/`.args` の**プロパティ構文**だけが
コンパイル時に専用命令 `GetExprField` へ特殊化される一方、明示的な
`getfield`/`getproperty` 呼び出し（MacroTools ヘルパが受け取り値を `Any` 型の引数で
持ち回ったときに発生。例: `MacroTools.splitdef`）は汎用リフレクション getfield
（`vm/builtins_reflection/mod.rs`）に流れ、そこに `Value::Expr` の分岐が無かったこと。
Symbol 添字（`head`→`ExprValue::get_head`, `args`→`get_args`）・整数添字
（0→head, 1→args）の両分岐と out-of-bounds 用 `field_count`=2 を追加。`get_args` は
共有バッキング配列を返すため `getfield(ex, :args) === ex.args` の参照同一性も upstream と
一致する。fixture: `reflection/expr_getfield_through_any_7614.jl`（julia と pass/fail
パリティ一致、10/10）。

## 最新対応 (2026-06-24)

### Complex/Real の順序比較をエラー化 (Issue #7605)

`complex(1.0, 2.0) < 3` のような `Complex` と `Real` の順序比較（`<`, `<=`, `>`, `>=`,
左右どちらの並びでも）が、upstream julia では `MethodError`（複素数は順序付け不可）に
なるのに、sjulia では**エラーにならず実部だけを比較した `Bool`** を返していた
（`< 3` が `true`）。原因は `promotion.jl` の総称 `<(x::Real, y::Real)` が specialization
下で `Complex` オペランドに緩マッチし、実部同士を比較していたこと（CLAUDE.md の
「bare abstract annotation が Complex 値に loosely match する」既知挙動と同種）。
`==(Complex, Real)` の除外（Issue #5966）と同様に、`base/complex.jl` へ parametric
`Complex{T} where {T<:Real}` の `< <= > >=`（両方向）を明示追加し
「Complex numbers are not orderable」エラーを送出するようにした（bare `::Complex` では
なく parametric 形にして非 Complex 値への緩マッチを防止）。`Complex × Complex` は
どちらのオペランドも `Real` に一致しないため従来どおり `MethodError`（変更なし）。
これにより `test_complex_ordering_error`（main で赤だった）が緑化。fixture:
`complex/complex_ordering_error_7605.jl`（`@test_throws Exception` で upstream julia と
pass/fail パリティ一致）。

### MacroTools splitarg macro-expanded unbound refs (Issue #7556)

MacroTools の `@match` 展開が生成する `let` clause で、前の clause が導入した
binding 名が compiler locals に残り、後続 clause が未実行 binding を shadow-save
して `UndefVarError` に落ちる問題を修正した。`let` が新規導入した locals は body
コンパイル後に compiler state から外し、Julia と同じく clause-local に扱う。

また macro 展開由来の `esc(quote ... end)` が `let` の末尾値として現れる場合に、
ネストした `Stmt::Block` の値を捨てて `nothing` を返していたため、block tail を
再帰的に値-producing としてコンパイルするようにした。これにより `@match` clause
body の capture 値が返る。

`MacroTools.splitarg` は default 引数の分解を `@capture(... = default_)` の
false-path 初期化に依存せず、`Expr(:(=), ...)` を直接分解する同値実装にした。
`MacroTools.splitarg(:(x::Int))` / `:(::Int)` / `:(x)` / `:(args...)` が upstream
Julia と同じ tuple を返す。

### Float64/Float32 × Int64 混合算術の高速化 (Issue #7587)

`x .^ 2` や `x .+ 2`（`Float64` 配列 × スカラー `Int`）が同型版 (`x .^ 2.0`) より
~8–14倍、スカラー `s + 2` に至っては ~240倍遅かった。原因は、ブロードキャストが
要素ごとに**第一級関数**として `^(::Float64, ::Int64)` を呼ぶと、混合型の専用
メソッドが無いため `promotion.jl` の総称 `^(::Number, ::Number)` に落ち、要素ごとに
`promote(x, y)` 連鎖をインタプリタ実行していたこと（no-JIT VM では JIT が promote を
消せない）。

`base/float.jl` に `+ - * / ^` の混合 `Float64×Int64` / `Float32×Int64`（両方向）の
concrete メソッドを復活させ、整数オペランドを一度だけ `Float64` 化して同型 intrinsic
を直接呼ぶようにした（specificity で `::Number, ::Number` に勝つ）。結果値・型は
promote 経路と一致。Issue の式 `exp.(-(x .- t) .^ 2)` は 12.25s→3.30s（50反復、結果
同一）。スコープは concrete `Int64` に限定し、`BigInt`（→ `BigFloat` に promote）や
`Bool` strong-zero、他の整数幅は従来どおり promote 経路に残す（型を変えない）。

なお調査中に `Float64 ^ BigInt`（例 `2.0 ^ big(3)`）が upstream では動くのに sjulia では
エラーになる既存の不具合を発見（本件とは独立、別 Issue #7602 として起票。PR #7609 で修正済み）。

### BigInt ^ BigInt exponent support (Issue #7608)

`BigInt` base の `^` が `BigInt` exponent を受け付けるようになった。static
compile が使う `PowBigInt` intrinsic は exponent を `pop_bigint()` で受け取り、
function argument などから `DynamicPow` に落ちる場合も `BigInt` base + integer
exponent を inline VM path で処理する。

負の integer exponent は従来どおり DomainError とし、`big(2)^big(3)` /
`big(2)^big(64)` / `big(-2)^big(3)` / `big(0)^big(0)` が upstream Julia と同じ
BigInt 結果を返す。

### JSXGraph 3D view rotates on single-finger drag (Issue #7592)

`board(...) do b; v = view3d(...) ...; push!(b, v); end` のように View3D を載せた
board を iOS/タッチ環境でドラッグすると、座標軸が回転せず scene が平行移動して
しまう不具合を修正。原因は JSXGraph board の既定 `pan.needTwoFingers = false`:
単指ドラッグが `initMoveOrigin` を起動して `BOARD_MODE_MOVE_ORIGIN` に入り、
`board.mode === BOARD_MODE_NONE` でしか発火しない View3D の回転ハンドラを
ブロックしていた。

`plotting/jsxgraph.rs` の emitter で、board が(再帰的に)View3D を含み、かつ
ユーザが `pan` を明示指定していない場合に board options へ `pan.needTwoFingers =
true` を注入するようにした。これで単指ドラッグは回転、二指ドラッグで pan、pinch で
zoom となり、マウスは JSXGraph の属性マージで `needShift` を維持する。emitter は
web / iOS / Flutter 共通の JSON spec を生成するため、1 箇所の修正で全フロントエンドに
反映される。2D board(View3D 無し)には注入しないので単指 pan は従来どおり。

回帰テスト: `plot_artifact_mime_tests::test_jsxgraph_view3d_board_requires_two_finger_pan_for_rotation_7592`
と `..._2d_board_keeps_default_single_finger_pan_7592`。

なお、Issue MWE の `[0, 2*π]` は二段ネスト do-block 内で global 定数 `π` を参照する
ため、別の既存不具合(global/const が二段目ネストクロージャで mis-capture される)を
踏む。これは #7600 (bug, #7591 と同根の可能性)として分離起票済み。

### AbstractFloat ^ BigInt type-preserving power (Issue #7602)

`Float64` / `Float32` / `Float16` base に `BigInt` exponent を渡す `^` が、
BigFloat promotion や unsupported BigInt power path に落ちず、base 側の
float 型を保って実行されるようになった。`Float64` は既存の #7308 補正済み
integer-power helper を再利用し、`Float32` / `Float16` は既存の同幅 float pow
runtime path と同じく計算後に元の幅へ戻す。

型推論側の `tfunc_pow` も `AbstractFloat ^ BigInt` を addition promotion から分離し、
base float 型を返す。

### MacroTools @capture/@match expansion helper visibility (Issues #7569/#7603/#7604)

Bundled package macro registry が macro 定義だけでなく、同じ package module の
helper functions / structs / hygiene member set を保持するようになった。
`using MacroTools: @capture` のように caller 側 `LambdaContext` が package internals を
持たない場合でも、expansion-time VM は `allbindings`, `TypeBind`, `trymatch` などを
解決できる。

macro-produced `Expr(:call, :===, ...)` / `Expr(:call, :!==, ...)` は identity
operator の `BinaryOp` に戻し、multi-statement `Expr(:block)` を value position に
戻す際は prefix を statement、tail を value expression として lower する。これにより
MacroTools `@capture` の `if env === nothing ... else ... end` tail value が `nothing`
に潰れない。

### MacroTools @forward quoted generator support (Issues #7572/#7599)

MacroTools `examples/forward.jl` の `:($([:(...) for f in fs]...); nothing)` を
parser/lowering が扱えるようになった。array comprehension の `for` 前改行、
quoted comprehension の `Expr(:comprehension, Expr(:generator, ...))` 構築、
quoted call の keyword/splat argument ordering を upstream Julia の Expr shape に
合わせた。

パッケージ root source の lowering でも `@__FILE__` / `@__DIR__` 用 current file を
設定し、include 内では caller context の current file を一時的に差し替える。
bundled package の `/embedded_packages/.../../animals.txt` は registry-backed file I/O
から読めるため、default bundled `using MacroTools` でも `animals.txt` 初期化を通過する。
module cache version は lowering/file-path semantics 変更に合わせて更新した。

### Nested closure module scope propagation (Issue #7591)

Module function から lifted された closure の中でさらに lifted された nested closure も、
親 module scope を保持するようになった。inline function universe の構築時に
`parent#child` の qualified parent 名へ module path を伝播するため、2段目以降の
closure から module-private helper を unqualified に呼び出せる。

### Base.eachline(filename) package initialization support (Issue #7593)

`eachline(filename)` を read-only file I/O builtin として追加し、`collect(eachline(path))`
や `map(Symbol, eachline(path))` のような package initialization pattern で使える
`Vector{String}` を返すようにした。MacroTools の `animals.txt` loader が必要とする
file-line enumeration を、既存の `readlines(filename)` と同じファイル読み込み経路で扱う。

### Nested module relative named imports (Issues #7574/#7594)

`import ..Parent: name` / `import ..Sibling: name` の relative named import が
module-local import resolver で解決されるようになった。lowering は leading dot count
を `UsingImport` に保持し、compile 側は現在 module path からの相対候補と parent
module self-reference 候補を順に解決する。

`LinearAlgebra.LAPACK` は workaround の parent-qualified call をやめ、
upstream-compatible な `import ..LinearAlgebra: inv, lu, LU` と bare `inv` / `lu` /
`LU` 参照に戻した。

併せて、user module の `x() = ...` が Base closure capture `x` を method-table
function として shadow し、Base/prelude compile 中に `function 'x' is not imported`
になる regression を防いだ。Base/prelude function compile では user top-level /
module function names を hidden user globals として扱う。

### MacroTools macro-runtime and closure lowering progress (Issues #7566/#7569/#7541/#7542/#7554)

`origin/codex/macrotools-support` の MacroTools load 継続作業として、do-block 内
`if` expression の assignment value が outer capture を mutate する経路を ctx 付き
lowering / closure boxing / free-variable analysis に接続した。macro body 内の user
macro call は macro registry context を保ったまま lowering し、MacroTools `@q` /
`@match` が返す `Expr(:$)`, `Expr(:...)`, `Expr(:')`, `Expr(:curly, ...)`、
expression/DataType call target を caller-side IR に戻す経路も追加した。

`using MacroTools` は `utils.jl` と `examples/destruct.jl` の macro expansion blocker を
越え、既知の `examples/forward.jl` parser blocker (Issue #7572) まで進む。

### Macro body lifted lambda visibility (Issue #7584)

ctx-aware lowering が必要な macro definition body 内で生成された lifted arrow helper を、
macro 定義直後に compile-time function registry へ登録するようにした。同一 source/include 内で
直後にその macro を展開しても、`foldr((clause, body) -> ...)` のような helper lambda が
`__lambda_0` MethodError にならない。

### LinearAlgebra Sylvester array unary minus workaround removal (Issue #7577)

`LinearAlgebra.sylvester` now constructs the vectorized RHS with `-_colvec(C)`
instead of an explicit negation loop. Plain array unary minus already compiles
through the broadcast materialization path, so the active workaround W-13 moved
to the resolved workaround table.

The linalg regression fixture checks direct vector unary minus and the dense
Sylvester equation identity `A * X + X * B == -C`.

### MacroTools forward.jl, @capture, and package data load path (Issues #7494/#7535/#7572/#7591/#7593)

MacroTools `examples/forward.jl` の `:($([:($f(...; kwargs...) = ...) for f in fs]...);
nothing)` 形を parser / quote lowering が扱えるようにした。array comprehension の
`for` が改行後に来る場合を comprehension として認識し、quote constructor lowering は
interpolated call argument list / block 内の semicolon token を構文要素として落とす。

`using MacroTools` の残り blocker として、module function 由来の second-level nested
closure が module-private helper を参照できるよう lifted function の module path を親チェーンへ
伝播した。さらに `eachline(filename)` / `EachLine` の vector-backed public surface、
`@__DIR__` の source-file context、embedded package data file (`animals.txt`) の registry
read を追加し、MacroTools の animals 初期化が通る。

MacroTools `@capture` macro expansion では、bundled package macro の実行時に同 package の
compile-time helper functions / hygiene context を登録する。macro が返す quoted block は末尾式の
値を保持し、展開結果の `Expr(:call, :===, ...)` / `Expr(:call, :!==, ...)` は caller-side
`BinaryOp` に戻すため、`package_load_capture_basic` が `Bool` を返す。

### MacroTools upstream fixture smoke expansion (Issues #7614/#7615/#7617/#7621/#7625/#7636/#7637/#7639/#7641)

MacroTools upstream smoke は `destruct.jl` / `utils.jl` / `flatten_try.jl` まで到達した。
macro-result lowering は Pair call (`=>`) と quoted vector assignment LHS を caller-side IR に
戻し、`Expr(:public, ...)` は no-op として扱う。compile-time `Expr` field access は
`Any` slot 経由でも `head` / `args` を読める。

`@destruct` は、現行 sjulia の `@match` array/ref capture gap と captured-callable
closure gapを避ける shim 付きで upstream destructuring smoke を通す。upstream
`flatten_try.jl` eval coverage は Issue #7683 の eval support により復元済み。

### Function-local `size(A)` tuple comparison (Issue #7578)

`size(A) == size(B)` and `size(A) != size(B)` inside function bodies now route
through tuple comparison rather than the primitive numeric fallback. The binary
operator compiler recognizes one-argument `size` / `Base.size` / builtin `Size`
calls as tuple-producing expressions before selecting an `I64` comparison path.

The comparison fixture covers `==` and `!=`, equal and unequal matrix shapes,
an `if size(A) != size(B)` guard, and qualified `Base.size`.

### Base matrix array addition/subtraction (Issue #7579)

`base/arraymath.jl` now dispatches dense `Matrix + Matrix` and `Matrix - Matrix`
through Pure Julia methods instead of falling through to a missing generic
operator method. The implementation checks row/column shape compatibility,
allocates a result with `size(A)`, and fills it elementwise by linear index.

The regression fixture covers the original Float64 matrix addition, shape
preservation, mixed `Int64`/`Float64` numeric results, subtraction, and a
dimension-mismatch path.

### JSXGraph 3D/do-block artifact integration (Issues #7373, #7374, #7375)

`JSXGraph` package に `JSFunction` / `View3D` / `view3d` / `curve3d` /
`point3d` / `line3d` と `board(...) do` / `view3d(...) do` construction を追加した。
`curve3d` の raw JS coordinate expression は Julia 側で `JSFunction(code, :t)` として保持し、
Rust artifact writer は `{"jsfunc": code, "var": "t"}` に変換する。

`application/vnd.jsxgraph+json` は `view3d` element の nested `elements` を表現できるようになり、
web `renderJsxgraph` と iOS `JSXGraphView` は `view.create(...)` で子要素を再帰的に生成する。
2D MVP と同じ MIME / artifact pipeline で 3D Lissajous sample を描画できる。

### Persistent prelude Program cache compiler fingerprint (Issue #7544)

persistent prelude Program cache の compatibility key に、prelude source hash だけでなく
build-time compiler/VM source fingerprint を含めるようにした。lowering が変わった後に古い
lowered Program cache を再利用し、Base 内クロージャ (`isequal(x) = y -> ...`) の capture
情報が壊れたまま `sjulia -e '42'` まで `Undefined variable: x` で失敗する状態を防ぐ。

また、macro context sharing のために `LambdaContext` を関数定義へ渡す経路は、関数 body に
macro call を含む場合だけに限定した。通常の関数内 arrow closure は従来どおり nested
function として lower され、親関数パラメータを capture できる。

### LinearAlgebra factorization result objects (Issue #7463)

`lu`, `qr`, `cholesky`, `eigen`, and `svd` now wrap the existing numeric builtin
results in first-class `Factorization` subtypes (`LU`, `QR`, `Cholesky`, `Eigen`,
`SVD`) instead of exposing raw tuples / NamedTuples for the stdlib surface.
The existing public fields (`L`, `U`, `p`, `Q`, `R`, `values`, `vectors`, `S`,
`V`, `Vt`) remain available, while `LU` and `SVD` keep tuple-style destructuring
compatibility used by existing fixtures.

The wrappers only apply to the builtin tuple / NamedTuple result shapes, so
user-defined dispatch-first overrides such as `LinearAlgebra.lu(A) = :custom`
continue returning the user value unchanged.

### LinearAlgebra in-place and values-only factorization APIs (Issue #7464)

`LinearAlgebra` now exports `lu!`, `qr!`, `cholesky!`, `eigen!`, `eigvals!`,
`svd!`, `svdvals`, `svdvals!`, and `isposdef!` on top of the existing
factorization wrappers. The values-only path returns `svd(A).S`, while the
in-place names return the matching factorization or value vector and write a
supported factorization work form back into the input matrix.

The write-back paths intentionally reuse the public `LU`, `QR`, `Cholesky`,
`Eigen`, and `SVD` fields from the stdlib wrappers, so user-defined
dispatch-first overrides that return non-wrapper values are still returned
unchanged.

### LinearAlgebra diagonal/copy and mutating transpose helpers (Issue #7466)

`LinearAlgebra` now provides dense-array helpers for `diagind`, `diagview`,
`transpose!`, `adjoint!`, `triu!`, `tril!`, `copy_transpose!`,
`copy_adjoint!`, `copytrito!`, and `copyto!` from `Diagonal` into dense
matrices. `diagview` is a lightweight aliasing view over the parent matrix
diagonal, so `setindex!` on the view mutates the source matrix.

The mutating transpose and adjoint paths validate destination shape and use the
same explicit dense loops as the existing `permutedims!`/`copyto!` surface,
keeping unsupported shapes on clear `DimensionMismatch` errors.

### LinearAlgebra matrix division operator calls (Issue #7467)

Operator call lowering now accepts `\` as a regular callable operator target,
so `\(A, b)` reaches the same method-dispatch path as infix `A \ b`.
`LinearAlgebra` exports `\` and `/`, adds dense `AbstractMatrix \`
wrappers through `inv(A) * rhs`, and adds dense matrix right division as
`A * inv(B)`.

The stdlib factorization wrappers also support left division for `LU`, `QR`,
`Cholesky`, and `SVD` vector right-hand sides using their public fields.

### LinearAlgebra Givens rotations and reflection helpers (Issue #7469)

`LinearAlgebra` now exposes the upstream-shaped `LinearAlgebra.Givens` type and
exports `givens`, `rotate!`, and `reflect!`. `givens(f, g, i1, i2)` returns a `Givens{T}` plus the
computed radius, and vector/matrix overloads derive the scalar pair from the
requested entries.

`lmul!(G::Givens, A)` and `G * A` apply the rotation to supported dense
vectors and matrices. `rotate!` and `reflect!` implement the dense vector helper
semantics used by upstream decomposition code.

### LinearAlgebra BLAS/LAPACK module subset (Issue #7468)

`LinearAlgebra` now exports upstream-compatible `BLAS` and `LAPACK` module
bindings. The supported sjulia subset is intentionally explicit and Pure Julia:
`BLAS.dot` / `dotu` / `dotc` / `axpy!` / `scal!` / `gemv!` / `gemm!`, plus
`LAPACK.gesv!` and `LAPACK.getrf!`.

These routines use dense array loops and the existing `inv` / `lu` wrapper
surface instead of native BLAS/LAPACK bindings, keeping the stdlib usable on the
no-JIT VM and iOS path while making the module names available to decomposition
code.

The compiler also resolves exported submodule names imported by `using
LinearAlgebra`, so `BLAS.dot(...)`, `LAPACK.getrf!(...)`, and bare
`typeof(BLAS) === Module` match the upstream module-availability surface.

### LinearAlgebra matrix equations and low-rank updates (Issue #7470)

`LinearAlgebra` now exports dense subset implementations for `condskeel`,
`lyap`, `sylvester`, `lowrankupdate`, `lowrankupdate!`, `lowrankdowndate`, and
`lowrankdowndate!`. The matrix equation solvers use explicit dense
vectorization and `kron`/`\` for the small matrices supported by the VM.

The low-rank update/downdate APIs operate on the existing `Cholesky` wrapper
fields (`L`/`U`) and return `Cholesky` objects, re-factorizing the dense
`L*U ± v*v'` subset instead of exposing tuple-shaped work arrays.

### LinearAlgebra structured matrix wrappers and UniformScaling (Issue #7462)

`LinearAlgebra` now exports `UniformScaling` with the constant `I`, plus the
core structured wrapper names `Symmetric`, `Hermitian`, triangular wrappers,
`UpperHessenberg`, `Bidiagonal`, `Tridiagonal`, `SymTridiagonal`, `Transpose`,
and `Adjoint`.

The initial VM subset stores the upstream-shaped wrapper fields, supports
`size` / `getindex`, and materializes wrappers for dense matrix multiplication
and `I * A` / `A * I` interactions. Specialized packed storage and factorization
fast paths remain outside this issue.

### LinearAlgebra remaining decomposition family objects (Issue #7465)

`LinearAlgebra` now exports the remaining upstream decomposition result object
names: `Schur`, `GeneralizedSchur`, `Hessenberg`, `LQ`, `LDLt`,
`BunchKaufman`, `GeneralizedEigen`, and `GeneralizedSVD`, with constructors and
small dense subset functions for `schur` / `hessenberg` / `lq` / `ldlt` /
`bunchkaufman` plus their `!` variants.

The initial `schur` path reuses the existing `eigen` wrapper for symmetric
dense matrices and preserves the `Z*T*transpose(Z)` reconstruction identity.
Other decomposition families expose upstream-shaped fields and stable small
dense wrapper behavior while deferring specialized LAPACK storage algorithms.

### Module-local macro visibility and qualified Base.isexpr (Issues #7525, #7527)

Module body statements that contain macro calls now lower with the active macro
context, so a macro defined earlier in the same module body is visible to later
calls such as `@m println(...)`. Non-macro module statements stay on the existing
path to avoid disturbing module-local closure/helper behavior.

Qualified `Base.isexpr(...)` calls now dispatch through the Base method table
without applying the unqualified function import guard. This keeps MacroTools-style
macro bodies that call `Base.isexpr(ex, :tuple)` on the qualified call path.

### Include macro context sharing (Issue #7510)

Sequential `include(...)` lowering now reuses the caller's macro/lowering context
for macro lookup, so a macro defined by one included file remains available while
lowering later includes in the same module/package scope. Function-body lowering
has context-aware variants for macro expansion, and ternary expressions propagate
that context to each branch.

Module function lowering stays on the established no-context path when a
definition contains no macro call, preserving module-local closure/helper
resolution such as Issue #7180.

### Persistent Base cache compiler fingerprint (Issue #7515)

persistent / embedded Base bytecode cache の staleness key に、Base source hash だけでなく
build-time compiler/VM Rust source fingerprint を含めるようにした。同じ Base source でも
compiler handler や VM instruction semantics が変わった場合は別 cache path になり、
古い `occursin(::Regex, ...)` body のような stale bytecode を再利用しない。

`source_hash` header validation も同じ combined hash を使うため、embedded cache でも
別 compiler build 由来の snapshot は cleanly reject される。

### VM regex match arity dispatch guard (Issue #7502)

compile-time regex `match` handler が `match` という名前の call を arity に関係なく
先取りしていたため、user-defined `match(a, b, c)` が通常 dispatch に届かず
`match requires exactly 2 arguments` で失敗していた。regex builtin は 2-arg call のみ
扱い、それ以外の arity は generic method dispatch へ fall through するようにした。

### Plots Aizawa attractor push! hot path (Issue #7431)

Aizawa attractor animation の `push!(plt, x, y, z)` hot path を高速化した。
従来は 1 点追加ごとに `collect(s.x)` / `collect(s.y)` / `collect(s.z)` で系列全体を
コピーしていたため、3000-6000 step の Plotly animation が O(n²) になっていた。

Plot 作成時に series data を plot-owned buffer として一度だけコピーし、その後の
`push!(plt, ...)` はその内部 buffer へ直接 append する。これによりユーザーが
`plot(xs, ys)` に渡した元配列は変更せず、animation 用 live series だけを O(1)
append で伸ばせる。

### AoT call/control-flow contract sync (Issues #7032, #7043, #7047, #7053, #7054, #7055)

Milestone 29 の残り AoT call/control-flow 親 issue を
`docs/aot/CALL_CONTROL_FLOW_CONTRACTS.md` に同期した。`try` / `catch` / `finally`
は Rust unwinding ではなく status-bearing Julia exception boundary へ lower する方針にし、
catch variable、finally ordering、rethrow state が保持できるまで gate する。

varargs / splatting は fixed tuple splat と fixed-count `Vararg{T,N}` を static
signature へ展開し、open `args...` tail と dynamic `f(xs...)` は runtime tuple
packing / call adapter boundary に送る。broadcast fusion は static shape の fused
element loop と runtime broadcast helper gate を分ける。

first-class function、do-block、closures / lambdas は known monomorphic callee、
non-capturing lambda、capturing closure environment、runtime callable handle の境界を固定した。
mutable capture を by-value copy に潰すことや、unsupported exception/broadcast path を
success-only Rust として出すことは禁止し、span diagnostic gate を維持する。

### AoT let-binding and HOF inference regression fixes (Issue #7495)

full `aot_e2e_tests` で見つかった AoT let-binding type-instability regression を修正した。
`convert(Int64, x)` の target `Int64` を constructor function value ではなく型名として
扱うようにし、typed local slot が `Function{(Any)->Int64}` に化ける問題を解消した。

あわせて `reduce(f, arr)` の reducer return type が `Any` の場合、static collection の
element type を fallback として使う。これにより inline lambda `reduce((a,b)->a+b, arr)`
が `Any` から `Int64` slot へ入る形にならず、既存の HOF inference fallback と一致する。

### AoT lowered operator-call inference fix (Issue #7504)

lowered `%` / `mod` と `÷` / `div` calls を infix binary operator と同じ
AoT result typing に通すようにした。Collatz pipeline の `n % 2 == 0` は `Bool` に、
`n ÷ 2` は `Int64` に推論され、control-flow condition が `Any` として拒否される問題を
解消した。

### AoT fresh let binding slot type fix (Issue #7506)

fresh local declaration は、先に変換した value の concrete type を slot type として優先する。
これにより `let b = Box{Int64}(41)` が同名の古い global/inference env entry
`Set{Any}` を拾ってしまう問題を避け、parametric struct constructor の local slot は
`Box{Int64}` / `Box{Float64}` として生成される。

### AoT top-level for-loop compound assignment fix (Issue #7416)

AoT DCE が nested loop body を outer block の後続 read を知らない状態で最適化し、
top-level `for x in a; total += x; end` の body を削除していた問題を修正した。
nested branch / loop block では unreachable / constant-condition cleanup だけを行い、
dead-store elimination は outer block の conservative liveness 収集後に適用する。

配列 `for` codegen は loop variable の AoT element type と Rust binding type を合わせるため
`.iter().cloned()` を使う。これにより `total += x` は borrowed element ではなく owned
`Int64` を使って `wrapping_add` に lower される。

### AoT C ABI and runtime numeric contracts (Issues #7077, #7056)

AoT の C ABI export 拡張と runtime numeric family の contract を
`docs/aot/ABI_AND_NUMERIC_CONTRACTS.md` に追加した。`String` / `Array` は borrowed
view または owned runtime handle、scalar-field immutable `struct` は caller-provided
out-param、heap/runtime `struct` / `Any` / multi-variant `Union` は opaque
`SjuliaValue*` handle とする。

`BigInt` / `BigFloat` / `Rational` / `Irrational` は silent primitive lowering を禁止し、
runtime-backed numeric handle として扱う方針にした。`BigInt` を fixed integer、
`BigFloat` / `Irrational` を Float64 へ暗黙変換することは AoT では diagnostic gate とし、
`--pure-rust` / C ABI export でも helper 実装まで拒否する。

### AoT map/filter generated Rust expectation refresh (Issue #7421)

#7070 の AoT map/filter regression tests を、現在の non-Copy-safe filter
codegen shape に合わせて更新した。`filter(f, arr)` は array element を先に
owned iterator 化するため、生成 Rust は
`.iter().cloned().filter(|x| f((*x).clone())).collect::<Vec<_>>()` になる。
typed `Vec<T>` 出力、HOF callee の DCE 保持、predicate への cloned argument は
既存どおり fixture で固定する。

### Pure Julia exp(::Real) VM hot-loop fix (Issue #7455)

`exp(::Float64)` の Pure Julia 実装で、range scale-back の `2.0 ^ k` を
整数指数専用の bit reinterpret helper に置き換えた。VM hot loop では generic `^`
dispatch が消え、Issue #7455 の `loop_exp` は `sin` / `cos` と同じコスト帯に戻った。

subnormal 境界も修正し、`exp(-745.0)` が upstream と同じ最小 subnormal
`5.0e-324` を返す。`exp(::Bool)` は upstream の Real forwarding に合わせて
`Float64` 経由にした (Issue #7484)。`Float32` / `Float16` / `Int64` /
`Rational` の既存 forwarding は維持した。

追加検証: `math_exp_real_upstream_shape_7455` fixture と
`vm_exp_real_benchmark` Criterion benchmark。VM-only quick benchmark では
10k calls の `exp` が約 49-51 ms、`sin` が約 41 ms、`cos` が約 45 ms。

### Cranelift milestone-29 parent surface sync (Issues #7081, #7080, #7079)

Milestone 29 の Cranelift 親 issue を、既に完了した下位 Cranelift work と同期した。
`--emit-binary --backend cranelift` は object emission から reusable system linker へ
接続済みで、scalar/native stack aggregate subset の native executable を生成できる。
`docs/aot/README.md` の古い「Cranelift `--emit-binary` は未対応」記述も現状へ更新した。

runtime `Value` rooting / safepoint は `CRANELIFT_GC_ROOTING_CONTRACT.md` に
`SjuliaGcContext*`、root slots、safepoint IDs、allocation hooks、`SjuliaValue*`
boundary として定義済み。globals / struct / enum は scalar initialized globals、
non-parametric scalar-field struct stack layout、Int32-backed enum metadata/member
lowering として Cranelift support matrix に親 issue #7079 を紐付けた。

### Cranelift varargs / kwargs call adapter contract (Issue #7118)

Cranelift GC/rooting contract に varargs / kwargs call adapter の境界を追加した。
Cranelift の low-level `Call` / `CallMulti` は固定 signature のまま維持し、adapter
lowering が Julia の splat、varargs tail、keyword canonicalization、keyword default、
keyword splat を固定 native signature または runtime tuple / NamedTuple boundary へ
正規化する。

static tuple splat と static `NamedTuple` keyword splat は allocation なしで展開できる。
true varargs tail は `__sjulia_tuple_pack`、dynamic keyword splat は
`__sjulia_namedtuple_pack` を使う managed runtime value とし、allocation / duplicate
keyword / unexpected keyword / default evaluation failure は #7108 の
`SjuliaCallStatus` exception path に接続する。

### Cranelift Array / Vector heap lowering contract (Issue #7098)

Cranelift GC/rooting contract に Array / Vector の heap layout と lowering rule を
追加した。`Vector{T}` は `Array{T,1}` とし、`SjuliaArray*` は runtime-owned
header、shape metadata、data buffer を持つ managed handle として扱う。Cranelift は
Rust `ArrayValue` layout を読まず、runtime ABI table が提供する header offset だけを
memory op の対象にする。

`length(A)` は `len` header load、`size(A,d)` は `dims_ptr[d-1]` load、`getindex` /
`setindex!` は Julia の 1-based / column-major index を zero-based linear index に
変換して typed load/store する形に固定した。allocation overflow、bounds failure、
null allocation は #7108 の `SjuliaCallStatus` exception path に接続する。

### Cranelift exception / unwinding model (Issue #7108)

Cranelift exception propagation contract を
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に追加した。Cranelift generated code は
generated frame / Rust frame / C ABI export wrapper を native unwind しない。
throw 可能な内部関数は hidden `SjuliaGcContext*` と `SjuliaCallStatus` を使い、
pending exception は `SjuliaGcContext` に保持する Result-style ABI とする。

`try` / `catch` は status check から catch block への明示分岐として lower し、
catch 変数は `__sjulia_exception_take(ctx)` で取得する。`finally` は normal /
exceptional exit の両方を post-dominate する cleanup block とし、cleanup 自体が
throw した場合は新しい pending exception が優先される。

### Cranelift runtime Value / Any / Union boundary (Issue #7102)

Cranelift runtime boundary contract に `SjuliaValue*` を追加した。`Any` と
multi-variant `Union` は opaque GC-managed runtime object handle として扱い、
Cranelift は VM の Rust `Value` enum layout を直接読まない。tag check、boxing、
checked unboxing、dynamic dispatch、display は runtime helpers へ委譲する。

single-variant `Union{T}` は `T` が native-representable なら `T` と同じ carrier に
畳める。multi-variant `Union` は boxed のまま流すか、runtime tag branch で unbox し、
join で union value が必要な場合は `SjuliaValue*` に re-box する。`SjuliaValue*` は
managed pointer なので allocation / boxing / dispatch / checked unboxing /
exception transition / safepoint poll をまたぐ場合は root slot が必要になる。

### Cranelift String / Array ownership model (Issue #7107)

Cranelift GC/rooting contract に non-Copy heap value ownership model を追加した。
heap `String` / `Array` は GC-managed pointer handle とし、handle copy は object
ownership を複製しない参照 copy として扱う。read-only String literal payload は
object/JIT data section 所有の immutable data pointer なので GC root 不要の例外とする。

Array handle は element tag、rank、dims、linear length、buffer capacity、initialized
length を runtime ownership に置く。mutating operation は array handle と buffer pointer
を root し、bounds/shape check 後にのみ direct scalar load/store を許可する。managed
element mutation は future write barrier が入るまで gate する。

### Cranelift stack map / precise safepoint contract (Issue #7106)

Cranelift GC/rooting contract に safepoint metadata の shape を追加した。各
safepoint は function-scoped ordinal を持ち、live managed values は
`__sjulia_gc_root_push` に渡した root slot order と frame-base offset を持つ
`RootSlot` descriptor で表す。

managed-value lowering は、Cranelift stack map が safepoint に emit されるか、
すべての live managed pointer が explicit root stack に入っている場合にだけ許可する。
Cranelift 0.115 の現行 JIT/object path で stack map API が直接使えない場合は、
root-stack fallback を safety baseline とする。heap paths は ownership /
runtime-value / array lowering の後続 work が入るまで gate する。

### Cranelift heap allocation hook ABI (Issue #7105)

Cranelift から runtime heap allocation を呼ぶ ABI を
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に定義した。`__sjulia_gc_alloc`、
`__sjulia_array_alloc`、`__sjulia_string_alloc` は platform C calling convention の
import symbol とし、`SjuliaGcContext*`、fixed-width size/alignment/tag/shape
carriers、null-on-failure contract を持つ。

すべての allocation hook は allocating safepoint として扱う。null failure path は
runtime exception transition (#7108) に委譲するため、現行 Cranelift backend は
runtime symbol binding、ownership、exception path が揃うまで heap allocation call
emission を gate する。

### Cranelift GC/rooting and safepoint contract (Issue #7104)

Cranelift backend の managed runtime value contract を
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に定義した。native scalar、scalar
field だけの native stack aggregate、read-only data-section pointer は root 不要とし、
heap string / array / heap struct / `Any` / multi-variant `Union` / exception object
は managed runtime pointer として扱う。

managed runtime pointer を扱う Cranelift function は hidden `SjuliaGcContext*` を
受け取り、allocation hook、root push/pop、safepoint、stack map metadata を通じて
GC と接続する。現行 backend は allocation hook の runtime 実体と heap lowering
(#7098 など) が入るまで heap-shaped values を gate し続ける。

### Cranelift Complex aggregate arithmetic lowering (Issue #7099)

Cranelift backend は local `Complex` / `ComplexF64` / `Complex{Float64}` と
`ComplexF32` / `Complex{Float32}` を `re` / `im` scalar field の stack aggregate
として lower する。`real` / `imag` は byte offset field load、`abs2` は
`re*re + im*im`、同一 element type の `+` / `-` / `*` は field arithmetic +
aggregate reconstruction へ展開する。

この slice は local Complex aggregate と scalar field arithmetic に限定する。
Complex parameter / return ABI、heap/runtime `Value` object、GC/rooting、non-Float
element layout は runtime/aggregate boundary work の範囲として引き続き gate する。

### Cranelift String constant data payload lowering (Issue #7094)

Cranelift backend は local `String` literals を read-only data section の payload
へ lower する。payload は `u64` byte length、UTF-8 bytes、NUL terminator の順で
保持し、low-level Cranelift function 内では payload 先頭への pointer carrier として
`LoadConst(String)` を扱う。`length(::String)` は payload 先頭の byte length を
native `Int64` として load する。

この slice は local String literal / binding に限定する。String parameter / return ABI、
allocating String operations、runtime `Value::Str` への boxing、GC/rooting と ownership
model は runtime boundary / ownership work の範囲として引き続き gate する。

### Cranelift struct field layout lowering (Issue #7095)

Cranelift backend は non-parametric scalar-field struct definitions を layout table
へ変換し、field alignment に合わせた byte offset を計算する。local struct construction
は stack slot に lowering し、field load/store は precomputed byte offset 付きの
low-level IR から Cranelift `load` / `store` へ変換する。

この slice は local stack-allocated struct values に限定する。struct parameter /
return ABI、nested struct / heap-shaped fields、parametric struct layout、runtime object
identity と GC/rooting は後続の runtime/aggregate work の範囲として gate する。

### Cranelift multiple return / destructuring lowering (Issue #7117)

Cranelift backend は scalar field だけからなる tuple return を low-level IR の
`ReturnMany` / `CallMulti` と Cranelift multi-result signature へ lower する。
`f() = (1, 2); a, b = f()` のような既存 destructuring lowering は temp tuple
binding + constant tuple index の形を保ったまま、callee 側では heap tuple を作らず
複数戻り値として受け渡せる。

この slice は tuple-returning static calls を tuple binding、tuple field access、または
tuple-returning function return の文脈に限定する。tuple parameters、runtime `Value` /
`Any` boundary、heap tuple object、out-param ABI は runtime/rooting/aggregate work の
範囲として引き続き gate する。

### Cranelift DWARF debug info output (Issue #7090)

`juliars --backend cranelift --debug-info` を Cranelift native artifact output
(`--emit-object` / `--emit-binary` / `--emit-library`) 向けに追加した。Cranelift
object path は `gimli` で DWARF compile unit、subprogram DIE、line table section を
生成し、Core IR の function/main span から得られる function-level source line を
`.debug_line` と subprogram declaration line へ入れる。

低レベル Cranelift lowering では `IrFunction::debug_line` を `SourceLoc` として
FunctionBuilder に設定する。現時点では AoT IR / low-level IR が命令ごとの source span
を保持していないため、debug info の精度は function-level 行に限定する。per-instruction
line mapping は span-carrying IR の拡張が入った段階で再訪する。

### Cranelift static/shared library output (Issue #7085)

`juliars --backend cranelift --emit-library <path>` now packages Cranelift
object output as a native library artifact. `--library-kind static` is the
default and archives the object with `ar crs`; `--library-kind shared` reuses
the `aot::linker` driver with shared-library output flags for C-driver,
Unix `ld`/`ld.lld`, and MSVC `link.exe`/`lld-link` style linkers.

The library path is mutually exclusive with `--emit-binary`, `--emit-object`,
`--check`, `--jit-run`, and Rust source `-o/--output`. External callable symbols
use the existing Cranelift `--export-c-abi` wrapper surface, so visibility is
controlled by the emitted C ABI export symbols for the current scalar/object
subset. Cross-target shared-library packaging remains dependent on an available
system linker for the requested target.

### Cranelift `--emit-binary` object-to-link path (Issue #7083)

`juliars --backend cranelift --emit-binary <path>` now compiles through the
Cranelift object path, writes the object to a temporary file, and invokes the
shared `aot::linker` driver to produce a native executable. The Cranelift binary
path reuses `--target` for object target selection and linker target-family
selection, and reports linker diagnostics as codegen failures.

This path intentionally does not write Rust source with `-o`; Cranelift binary
output rejects `-o/--output` and `--check` combinations as usage errors. The
initial support covers the current scalar/object subset and depends on a local
system linker capable of linking the emitted object format.

### Cranelift standalone executable entry wrapper (Issue #7084)

Cranelift module lowering now emits a C ABI `main() -> Int32` wrapper alongside
the existing `__juliars_main` top-level entry. The wrapper calls
`__juliars_main()`, then returns `0`, giving object output a conventional
executable entry symbol for the linker path.

This does not yet enable `juliars --backend cranelift --emit-binary`; #7083
will connect object emission, the #7089 linker driver, and output packaging.
Runtime initialization remains a no-op for the current scalar subset and can be
expanded when runtime/rooting hooks land.

### Cranelift system linker / lld driver planning (Issue #7089)

Cranelift object output now has a reusable `aot::linker` boundary for
object-to-native linking. The linker planner selects host/target families,
classifies explicit or discovered linkers as C driver, Unix `ld`/`ld.lld`, or
MSVC `link.exe`/`lld-link`, and fixes object/runtime/system library ordering for
Linux, Darwin, and Windows MSVC style invocations.

The helper can also execute the planned command and returns structured
diagnostics for missing linkers, launch failures, and non-zero linker exit
statuses. CLI `--emit-binary --backend cranelift` remains gated until the
entry-point and binary-output issues connect object emission to this linker
boundary.

### AoT `Dict` construction / lookup / iteration codegen (Issue #7034)

Rust backend は `Dict("a" => 1)` 形式の静的 Pair 引数と `Dict{K,V}()` を
native `std::collections::HashMap<K,V>` carrier へ lower する。`d[k]` は
checked `get(...).cloned()` lookup、`get(d, k, default)` は default 付き
lookup、`haskey(d, k)` は `contains_key`、`d[k] = v` は `insert` へ投影する。
`length` / `isempty` は既存 collection helper を使い、`collect(d)` と `for kv in d`
は key/value clone の tuple iterator として扱う。

AoT の Rust `HashMap<K,V>` carrier は現時点で hashable primitive key subset
(integer / bool / char / string) と fully static value type に限定する。iteration
order には依存せず、Julia `Pair` runtime object/display surface は導入せず AoT 内部の
静的 `(K, V)` tuple として扱う。

検証: upstream Julia / VM MWE stdout 比較、AoT analyze/type/builtin unit、AoT E2E
generated Rust warning-deny check。

### Cranelift ELF / Mach-O / COFF object smoke coverage (Issue #7088)

Cranelift object output now has representative object-format coverage for
`x86_64-unknown-linux-gnu` (ELF), `x86_64-apple-darwin` (Mach-O), and
`x86_64-pc-windows-msvc` (COFF). Each test drives `cranelift-object` through the
explicit target path and checks both the object format magic and exported symbol
presence.

This verifies object-format emission, not full platform linking. Linker
discovery, link order, runtime libraries, and executable packaging remain in the
linker/binary-output issues.

### AoT `Set` construction / membership / iteration codegen (Issue #7035)

Rust backend は `Set([iterable])` と `Set{T}()` を native
`std::collections::HashSet<T>` carrier へ lower する。重複排除は Rust
`HashSet` の `collect` に任せ、`push!(s, x)` は `insert`、`x in s` は
`contains(&x)`、`length` / `isempty` / `collect(s)` は既存 collection helper
経路へ接続した。`for x in s` は `s.iter().cloned()` を使うため、Julia の
`Set` と同様に iteration order へ依存しない。

AoT の Rust `HashSet<T>` carrier は現時点で hashable primitive subset
(integer / bool / char / string) に限定する。実装中に AoT top-level `for`
body mutation が生成 Rust で落ちる既存バグを Issue #7416 として切り出した。

検証: VM / generated native binary MWE stdout 比較、`juliars --minimal-prelude
--check`、AoT analyze/type/builtin unit、AoT E2E generated Rust warning-deny check。

### Cranelift object target triple selection (Issue #7087)

Cranelift object output now accepts `--target <triple>` together with
`--emit-object --backend cranelift`. The CLI passes the requested target into the
Cranelift object generator, which uses `target-lexicon` parsing and Cranelift ISA
lookup instead of always selecting `Triple::host()`.

This selects the object target for emission only. Linker selection, executable
packaging, and platform runtime link details remain owned by the linker and
binary-output issues.

### Cranelift C ABI object export symbols (Issue #7086)

Cranelift object output now honors `--export-c-abi` for C-stable scalar and
`Nothing` signatures. The object pipeline resolves the requested Julia function,
keeps it through AoT DCE, validates the native ABI surface, and emits an
exported Cranelift wrapper symbol that forwards to the lowered function.

This is scoped to object output. Runtime `Value`, aggregate, heap-shaped, and
platform linker/export packaging concerns remain gated by the existing
Cranelift runtime/rooting and linker issues.

### Cranelift relocatable object output path (Issue #7082)

Cranelift backend now has a link-less relocatable object output route for the
current scalar subset. `juliars --backend cranelift --emit-object <path>` runs
the shared AoT preparation pipeline, lowers to the existing Cranelift low-level
IR, compiles with `cranelift-object::ObjectModule`, and writes the emitted object
bytes directly.

This intentionally stops before linker/lld integration, standalone executable
packaging, cross-target object selection, and C ABI export symbol rewriting.
Those remain owned by Issues #7083, #7084, #7087, #7089, and #7086.

### Cranelift scalar global constant lowering (Issue #7103)

Cranelift AoT lowering now accepts initialized scalar top-level globals. The
lowerer carries the `AotGlobal` initializer map into each lowered function and
resolves global `Var` references by lowering the initializer as a read-only
constant at the use site. This covers native scalar globals used from functions
or `__juliars_main` without introducing heap/runtime `Value` state into the
current Cranelift scalar path.

Uninitialized globals and non-scalar/global heap initializers remain explicit
gates. A future object/data-section path can replace the current JIT constant
projection without changing the source-level AoT surface.

### Cranelift tuple local field projection (Issue #7097)

Cranelift AoT lowering now handles the scalar subset of tuple construction and
field access by splitting local tuple literals into per-field scalar carriers.
For `t = (x, y)`, the lowerer records synthetic fields such as `t#1` / `t#2`;
constant one-based tuple indexing (`t[2]` or `(x, y)[1]`) projects directly to
the selected scalar field before Cranelift codegen.

This intentionally avoids introducing a runtime tuple object or tuple ABI
surface in the current scalar Cranelift path. Tuple parameters/returns,
destructuring, and multiple-return representation remain owned by Issue #7117.

### AoT parametric struct definition/codegen (Issue #7040)

Rust backend は使用された user parametric struct を generic Rust struct として
生成する。`struct Box{T}; x::T; end` は `pub struct Box<T>` に下り、
`Box{Int64}(41)` は `Box::<i64>::new(41i64)`、bare default constructor
`Box(1.5)` は field type variable から `Box::<f64>::new(...)` へ推定される。
field access は instantiated type (`Box{Int64}` の `x::Int64` など) を保持する。

DCE は parametric constructor 名から bare struct name を参照として拾うため、
`Box{Int64}` が `filter_program` 後も定義を失わない。一方で未使用の reachable Base
parametric structs は引き続き skip され、#7251 の unrelated compile 回避を保つ。

検証: upstream Julia / VM / generated native binary MWE stdout 比較、`juliars
--minimal-prelude --check`、AoT analyze/call-graph unit、AoT E2E generated Rust
warning-deny check。

### Cranelift `@enum` Int32-backed scalar lowering (Issue #7096)

Cranelift backend now accepts AoT enum definitions as metadata instead of
rejecting `aot_program.enums` at the backend gate. `@enum` members are registered
with both their `Int32` carrier type and backing value during inference/conversion,
so member references such as `green` fold to `AotExpr::LitI32(1)` before
Cranelift lowering.

This matches the existing Rust backend representation (`pub type Color = i32`;
member constants) for the native scalar subset. Runtime enum object/display
parity, `instances(Color)`, and constructor-style enum reflection remain outside
the current Cranelift scalar carrier surface.

### Cranelift short-circuit `&&` / `||` CFG lowering (Issue #7115)

Cranelift AoT lowering now treats Bool `&&` / `||` as control flow instead of an
eager logical `band` / `bor`. The lowerer emits a branch after the left operand,
lowers the RHS only on the path where Julia would evaluate it, materializes the
short-circuit constant on the other path, and merges the Bool result through a
join-block phi.

This keeps RHS side-effect order visible in the low-level IR while reusing the
existing Cranelift branch/block-parameter phi machinery. The implementation is
intentionally limited to Bool operands and Bool result; value-position
non-Bool final operand preservation remains outside the current Cranelift scalar
subset.

### Cranelift Float16 widened scalar lowering (Issue #7093)

Cranelift backend は `StaticType::F16` を Rust backend と同じ widened `F32`
carrier として扱う。Cranelift 自体には `F16` 型があるが、現行 AoT IR は
`Float16` literal を `AotExpr::LitF32` に widen しており、`StaticType::F16` の
`to_rust_type()` も `f32` を返すため、backend 間の ABI surface を F32 carrier に揃えた。

`static_type_to_cranelift(F16)` は Cranelift `F32` を返し、AOT lowering gate は F16
parameter / return / simple scalar binop を受理する。Cranelift の float 判定、
unary neg、comparison、`sqrt` / `sin` / `cos` / `exp` / `log` / `abs` の libm
経路も F16 を F32-family として扱う。Float16 固有の丸め・literal carrier・conversion
parity は引き続き既存の literal/conversion 設計側のスコープとする。

## 最新対応 (2026-06-23)

### AoT parameterized `Complex{T}` arithmetic (Issue #7041)

Rust backend の `Complex` carrier を default type parameter 付き `Complex<T = f64>` へ
広げ、既存の Float64 `Complex` surface を保ったまま `Complex{Float32}` /
`Complex{Int64}` などの primitive numeric parameterized Complex を生成できるようにした。
`Complex{T}(re, im)` constructor は T への static conversion を挟んで
`Complex::<T>::new(...)` へ下ろし、`+` / `-` / `*`、`real` / `imag` / `abs2` は
element type を保持する generic helper を使う。

`Complex{T}` だけを使うプログラムでも synthetic `Complex<T>` definition と dependent
prelude を出すよう、AoT program 内の型参照から Complex 使用を検出する。汎用 parametric
struct constructor/codegen は #7040 / #6975 の範囲に残す。

検証: upstream Julia / VM / generated native binary MWE stdout 比較、`juliars
--minimal-prelude --check`、AoT codegen unit、AoT E2E、既存 mandelbrot complex/broadcast
regression。

### AoT `rand` / `randn` RNG codegen (Issue #7036)

Rust backend の `rand()` / `randn()` と次元付き `rand(dims...)` /
`randn(dims...)` を VM と同じ RNG contract へ接続した。生成 Rust は
`subset_julia_vm_runtime::rng::StableRng::new(42)` を thread-local に保持し、
scalar call は `__sjulia_aot_rand()` / `__sjulia_aot_randn()`、array form は同じ
stream を進める nested `Vec` 生成へ下ろす。

これにより `sjulia` CLI の bare RNG stream と AoT generated binary の stdout が
同じ MWE で一致する。明示 RNG object や `Random.seed!` の AoT builtin surface は
別 issue の範囲として扱う。

検証: VM/AoT MWE stdout 比較、`juliars --minimal-prelude --check`、生成 native binary、
AoT codegen unit、AoT E2E。

### Cranelift I128 / U128 scalar lowering (Issue #7092)

Cranelift backend で `StaticType::I128` / `StaticType::U128` を Cranelift `I128`
carrier へ投影するようにした。AOT lowering gate も 128bit signed/unsigned
integer を scalar subset として許可し、U128 は既存の unsigned integer 判定に含めて
`udiv` / `urem` / logical right shift の経路へ流す。

x64 JIT では i128 args/return の ABI lowering に Cranelift の LLVM ABI extension が
必要なため、ISA flags で `enable_llvm_abi_extensions` を有効化する。回帰テストでは
`i128` add の wrapping result と `u128` logical shift を JIT 実行し、AOT lowerer が
I128/U128 parameter / return / simple binop を Cranelift verifier/codegen まで通すことを
確認する。128bit literal IR 拡張や Julia conversion parity はこの issue では扱わず、
既存の conversion gate / literal carrier 設計に残す。

### Cranelift Char scalar lowering (Issue #7101)

Cranelift backend は `Char` を i32 codepoint carrier として扱う。`StaticType::Char`
は Cranelift `I32` に投影され、`AotExpr::LitChar` は `ConstValue::Char` のまま
low-level IR に渡って `iconst.i32` として codegen される。

回帰テストでは `Char` parameter / return を持つ static call と non-ASCII scalar
literal (`'λ'`) の local binding を Cranelift lowering へ通し、Cranelift
verifier/codegen が受理することを確認する。`print` / `string` の display runtime
境界は #7121、`Char` と整数の non-identity conversion は #7123、invalid codepoint
も保持できる Julia full carrier は #6967 の残スコープとして扱う。

### AoT lazy Range / Char range codegen (Issue #7039)

Rust backend の range literal を intermediate `Vec<T>` materialization から
lazy carrier (`SjuliaRange<T>` / `SjuliaCharRange`) へ切り替えた。`collect(r)`、
`sum(r)`、`map` / `filter` / `reduce` / `mapreduce`、array comprehension の iterator
入力は range binding を clone して `into_iter()` するため、同じ range を複数回使っても
binding を消費しない。

`Char` の unit-step range (`'a':'c'`) は `SjuliaCharRange` へ下ろし、comprehension
や `for` の iterator として使える。step 付き Char range は Julia `Char` の invalid
codepoint 表現を Rust `char` に写す設計が必要なため、引き続き明示 gate にしている。

検証: upstream Julia MWE、`juliars --minimal-prelude --check`、生成 native binary、
AoT codegen unit、AoT E2E。

### AoT generator expression codegen (Issue #7046)

AoT の generator expression (`f(x) for x in xs`) を `StaticType::Generator` /
`AotExpr::Generator` として保持し、Rust backend では `Box<dyn Iterator<Item = T>>`
へ下ろすようにした。`collect(generator)` と `sum(generator)` は lazy iterator を直接消費し、
filtered generator は `filter_map` へ変換する。

range source は lazy range のまま `into_iter()` し、array binding source は
`iter().cloned()` で走査するため、既存の array comprehension / lazy range materialization
回帰を増やさない。generator 内の closure capture や first-class function carrier は
#7055 / #7053 の範囲として残す。

検証: upstream Julia MWE、`juliars --minimal-prelude --check`、生成 native binary、
AoT E2E。

### AoT 3D+ array codegen (Issue #7033)

Rust backend の array carrier を既存の nested `Vec` 表現のまま 3D 以上へ拡張した。
`zeros(dims...)` / `ones(dims...)` は rank に応じた nested `Vec` を生成し、array literal
の 3D+ gate も解除して既存の column-major nested builder を使う。

`length(A)`、`size(A)`、`size(A, dim)`、`ndims(A)` は static rank から shape を
生成し、`A[i,j,k,...]` と `A[linear]` は 1-based bounds check と column-major linear
index decomposition を行って nested `Vec` にアクセスする。未知 rank / dynamic rank の
一般 array object は引き続き runtime carrier 整備後の範囲。

検証: upstream Julia MWE、`juliars --minimal-prelude --check`、生成 native binary、
AoT E2E。

### Cranelift unsupported gate diagnostics (Issue #7129)

低レベル Cranelift codegen の `CraneliftError::Unsupported(String)` が backend 境界で
`CodegenError` に潰れないよう、`AotError::UnsupportedInstruction` へ変換する
`into_aot_error` 経路を追加した。これにより CLI の human/json diagnostic では
unsupported kind と workaround が Rust/AoT 側 gate と同じ形で表示される。

AoT lowering 側の Cranelift gate は既に `UnsupportedInstructionDiagnostic` を使っている。
source span については、現在の AoT IR / low-level Cranelift IR が元 span を保持しない
箇所があるため、span-carrying IR の整備後に追加で埋める。

### Cranelift backend benchmark helper (Issue #7127)

`scripts/aot_cranelift_backend_benchmark.sh` を追加し、同一 fixture について Rust backend
と Cranelift backend の開発者向け timing を TSV で比較できるようにした。Rust 側は
`--check` の compile/codegen probe、`--emit-binary` の生成・link 時間、生成バイナリ
runtime 平均、生成バイナリ size を測る。Cranelift 側は `--check` と #7131 の
`--jit-run` 平均時間を測る。

Cranelift standalone binary size/runtime は object/linker path (#7082/#7083/#7084/#7089)
完了後に同 script へ列を増やす前提とし、現時点では in-process JIT の compile+run
surface を比較対象にする。

### Cranelift differential stdout harness (Issue #7126)

`scripts/aot_cranelift_fixture_differential.sh` を追加し、同一 fixture を upstream
Julia、Rust backend generated binary、Cranelift JIT (`juliars --backend cranelift
--jit-run`) の 3 経路で実行して stdout を `diff -u` する developer helper にした。

Rust backend は既存 parity script と同じく `--emit-binary` で生成バイナリを作り、
Cranelift は #7131 の opt-in JIT entry point を使う。現時点では Cranelift が
support している scalar subset の fixture が対象で、display/runtime/heap feature は
従来通り明示 gate で失敗する。

### Cranelift desktop opt-in JIT execution path (Issue #7131)

Cranelift backend の既存 in-process JIT compile path を、`juliars --backend cranelift
--jit-run` として明示的な desktop/REPL 向け実行 surface に分離した。`--jit-run` は
`--check` / `--emit-binary` / `-o` と排他で、Rust backend では使用できない。

実行時は通常の AoT pipeline と同じ DCE、type inference、AoT IR conversion、
optimization、pass verifier を通した後、Cranelift module を JIT compile して
`__juliars_main` を呼ぶ。成功時は harness や future REPL integration が stdout を
扱いやすいよう、追加の成功メッセージを出さない。Cranelift の object/binary output は
引き続き #7082/#7083/#7084/#7089 が所有する。

### AoT array comprehension codegen (Issue #7045)

AoT の `Expr::Comprehension` / `Expr::MultiComprehension` を専用 `AotExpr` として
保持し、Rust backend で block expression の `Vec<T>` build に下ろすようにした。
単一 clause `[f(x) for x in xs]`、filtered comprehension `if cond`、複数 clause の
cartesian product は静的 element type を推論し、`Value::from(())` placeholder へ
落ちずに concrete `Vec` を生成する。

検証: upstream Julia MWE、`juliars --minimal-prelude --check`、AoT E2E。

### Cranelift lowering property / fuzz regression (Issue #7128)

Cranelift backend の lowering 品質 gate として、deterministic な pseudo-random
AoT IR 生成テストを追加した。現在の Cranelift scalar subset に合わせ、`Int64`
の算術、bit 演算、shift、unary `-` / `~`、`abs` builtin、単純な static call、
比較式を組み合わせた `AotProgram` を seed ごとに生成する。

各 seed では同じ AoT IR を Rust backend の `AotCodeGenerator` が受理すること、
Cranelift lowerer が `__juliars_main` を含む低レベル module を作ること、
Cranelift verifier/codegen が invalid CLIF なしで完走することを確認する。
standalone Cranelift binary / stdout differential は引き続き Issue #7126 と
object/linker 系 issue の完了後に扱う。

### AoT NamedTuple construction / field access (Issue #7049)

AoT の NamedTuple literal `(a=1, b=2)` は `StaticType::NamedTuple` と
`AotExpr::NamedTupleLit` として field 名と順序を保持し、Rust backend では
field-ordered tuple carrier `(i64, i64)` へ投影する。`.a` / `.b` access は
AoT IR 変換時に対応する tuple index へ静的変換するため、`nt.a + nt.b` は
dynamic dispatch なしで codegen される。

検証: upstream Julia MWE、`juliars --minimal-prelude --check`、AoT E2E。

### AoT tuple destructuring rest/splat tail (Issue #7391)

AoT の tuple destructuring は top-level final rest target `a, rest... = xs` を
`#__sjulia_tuple_tail__` 内部 lowering で表現し、AoT 変換時に RHS が静的 tuple 型なら
残り field reads の `TupleLiteral` へ展開する。これにより `rest` は concrete Rust tuple
として型付けされ、後続の `rest[1]` / `rest[2]` も tuple field access になる。
nested rest target や middle-position rest target は引き続き明示エラー。

### AoT tuple return / nested destructuring codegen (Issue #7048)

AoT の tuple return と destructuring assignment は、basic な `(a, b) = f()` に加えて
nested pattern `a, (b, c) = f()` も lowering で indexed tuple reads へ展開する。
RHS は一度 `__tuple_tmp_*` に束縛し、nested target は `tmp[2][1]` / `tmp[2][2]`
相当の Core IR `Index` へ再帰的に展開するため、Rust backend では tuple field access
として codegen される。rest/splat target `(a, rest...) = xs` は tuple-tail/slice
contract が必要なため Issue #7391 に切り出した。

### Cranelift bit operations / shift lowering (Issue #7120)

Cranelift backend の整数 bit operation coverage を固定した。`&` / `|` / `xor`
は Cranelift の `band` / `bor` / `bxor`、`~` は `bnot` へ下ろす。shift は
右辺 count を左辺/result の整数幅へ明示的に extend/reduce し、`UInt* >> n` は
logical shift (`ushr`)、`Int* >> n` は arithmetic shift (`sshr`) を使うようにした。

回帰テストでは `Int64` bitwise 3 種、`UInt8(0x80) >> 1 == 0x40`、
`Int8(-2) >> 1 == -1`、mixed-width count の `Int8(1) << 2 == 4`、`~Int64`
を Cranelift JIT 実行で確認する。

### AoT typed overload source signature projection (Issue #7387)

同一 arity の typed overload を Core IR -> AoT IR に変換するとき、`IrConverter` が
typed signature を arity だけで選び、常に最初の method を使っていた。そのため
`add(::Int64, ::Int64)` と `add(::Float64, ::Float64)` が両方 `add(::Int64, ::Int64)` に
潰れ、main 側の Float64 call と generated method signature が不一致になっていた。
明示 annotation がある関数では、宣言側の static parameter signature に一致する
`TypedFunction` を優先して選ぶようにした。

### AoT C ABI export overload signature resolution (Issue #7078)

`--export-c-abi` は従来 `symbol=function` か generated method 名指定だけだったため、
overload された Julia 関数を original name で export すると ambiguous になっていた。
`symbol=function(Int64,Float64)` 形式で C-stable scalar 引数型を明示できるようにし、
resolver が distinct AoT method を Julia 関数名 + signature から自動選択する。top-level
comma 区切りの bulk specs も受け付け、複数 entry export を 1 option にまとめられる。
Julia source typed overload が AoT IR で同一 signature に潰れる別 bug は Issue #7387 で修正済み。

### Cranelift libm 数学組込みを拡張 (Issue #7122)

Cranelift backend の libm 宣言が `pow`/`fmod` 専用だったため、`sqrt` / `sin` /
`cos` / `exp` / `log` / `abs` を低レベル `Instruction::Call` から libm 経路へ
lowering できるようにした。JIT symbol 登録と Cranelift import signature は arity
付きで管理し、`Float64` は `sqrt` など、`Float32` は `sqrtf` などへ分岐する。
整数引数の単項 math は既存 `pow` 同様に浮動小数へ変換し、`abs` は浮動小数なら
`fabs`/`fabsf`、符号付き整数なら compare/select、符号なし整数なら no-op として扱う。

AoT lowering 側では `AotExpr::CallBuiltin` の対象 math builtin を
`Instruction::Call` へ変換し、未知の runtime-checked call gate は Issue #7111 のまま
維持した。回帰テストは libm 実行、`abs` の float/int lowering、AoT builtin lowering
を追加した。

### Cranelift IR verifier を compile 前に統合 (Issue #7125)

Cranelift backend の各関数で `FunctionBuilder::finalize()` 後、`JITModule::define_function`
へ渡す前に `Context::verify(self.module.isa())` を実行するようにした。verifier が
不正な CLIF を検出した場合は、ネイティブ compile へ進まず
`Cranelift verifier failed before compile for ...` と関数表示を含む
`FunctionCompilation` diagnostic を返す。回帰テスト
`cranelift_verifier_runs_before_compile_issue_7125` で、壊れた Cranelift `Function` が
compile 前 verifier error になることを固定した。

### AoT global state / redefinition policy gate (Issue #7061)

AoT は静的に閉じた program を前提にするため、top-level `const` marker / const 再代入、
関数内 `global` による mutable global state、同一 signature の関数再定義(world-age 依存)
を Core IR -> AoT IR 変換時点で `UnsupportedInstruction` として拒否する方針にした。
従来は `--check` が通った後、生成 Rust が未定義 `___sjulia_declare_const__` や未定義
global 参照で `cargo build` 失敗、または同一 signature 再定義で古い method を選ぶ
silent mismatch になっていた。

### `examples/test_intrinsic.rs` の CompiledProgram 初期化漏れを修正 (Issue #7383)

`CompiledProgram` に `runtime_specialization_map` が追加された後、手動構築している
`subset_julia_vm/examples/test_intrinsic.rs` が同フィールドを埋めておらず、
`cargo nextest run --release -p subset_julia_vm --features cranelift -E 'test(/cranelift/)'`
の example build で `missing field runtime_specialization_map` になっていた。example の
initializer に空 map を追加し、現行 `CompiledProgram` 形状に追随させた。

### Cranelift opt-level mapping を settings に反映 (Issue #7091)

`juliars -O0..-O3 --backend cranelift` の最適化レベルを、AoT IR optimizer だけでなく
Cranelift ISA settings にも渡すようにした。`CodegenConfig` に `OptLevel` を保持し、
`compile_program` から backend codegen へ伝搬、Cranelift generator では `O0 -> none`、
`O1/O2 -> speed`、`O3 -> speed_and_size` に変換して `settings::builder()` へ設定する。
これにより Cranelift backend の compile-speed/debug 重視と speed/size 重視の切替が CLI
指定と一致する。mapping regression は `cranelift_opt_level_maps_to_settings_issue_7091`。

### AoT 相互再帰 codegen と TCO 境界を固定 (Issue #7060)

AoT Rust backend の相互再帰は Rust の通常 `fn` 間 static call として生成できるため、
forward declaration 用の追加 carrier は不要と整理した。`is_even ⇄ is_odd` 形の generated
Rust と `juliars --emit-binary` smoke で実行可能性を確認し、optimizer では direct
self-tail recursion のみ loop 化し、mutual tail call は通常 call のまま保持する方針を
regression test に固定した。

### AoT timing macros の実行時測定方針を固定 (Issue #7059)

AoT の `@time` / `@elapsed` は no-op ではなく、Base macro 展開後の `time_ns()` を
generated Rust の `std::time::SystemTime::now()` 経路へ下ろし、生成バイナリ実行時に
wall-clock time を測る方針にした。`time_ns` builtin を Core IR -> AoT IR 変換へ接続し、
AoT 型推論では `time_ns()::Int64` として扱う。さらに `elapsed = @elapsed ...` のような
macro-lowered `target = let ... end` と CLI の top-level 複数文を statement 位置へ正規化し、
`@elapsed` の戻り値(`Float64` 秒)と `@time` の出力副作用を保持する。

### Web/WASM Apollonian Gasket の compile freeze を修正 (Issue #7357)

Web playground の Apollonian Gasket が `run_from_source` の compile 中に約105〜108秒
メインスレッドをブロックしていた。原因は JSXGraph 描画や Complex 演算ではなく、abstract
inference の再帰 cycle guard が full `(method, arg types)` key にだけ効き、`recurse!` が
`Top` 引数で解析中に `c5::Circ` を渡す自己再帰を別 key として再解析して分岐的に膨らませて
いたこと。`InferenceEngine` に method identity 単位の `active_function_estimates` を追加し、
同じ method が解析中なら refined arg key でも現在の estimate を返すようにした。既存の
exact-key `analyzing_functions` は mutual recursion の cache 昇格 semantics を保つため分離。
併せて Base cache に `specializable_functions` と `runtime_specialization_map` を保持し、warm
compile で cached Base の `CallSpecialize` metadata を復元して Base 関数再走査を避ける。
WASM MWE は `gasket(15.0)` で約113s→約1.5s、`gasket(120.0)` で約108s級→約1.6s、出力
61/889 を確認。

### Cranelift backend 専用 support matrix を追加 (Issue #7130)

`docs/aot/SUPPORT_MATRIX.md` から Cranelift 固有の対応範囲・gate・milestone
roadmap を `docs/aot/CRANELIFT_SUPPORT_MATRIX.md` へ分離した。現行実装に合わせ、
`--backend cranelift` は feature build の experimental in-process JIT path、
`--emit-binary --backend cranelift` は object/linker work まで明示 gate、runtime
`Value` / heap-shaped carrier は rooting/safepoint contract 未充足として gate、
`I128` / `U128` / `F16` / `Missing` は型 mapper の gate として整理。既存
`README.md` と `SUPPORT_MATRIX.md` から新 matrix へリンクし、JuliaC.jl 代替へ向けた
issue 順ロードマップを docs 上で追えるようにした。

### AoT generated Rust の `rustc -D warnings` clean gate を nextest 化 (Issue #7076)

generated Rust の warning clean 保証を、ヘッダ文字列検査だけでなく実際の downstream
Cargo crate として検証するようにした。`aot_e2e_tests` に代表 source
(`a = 3.0; b = 2; println(a + b)`)を Rust へ生成し、temp crate の `main.rs` として
`RUSTFLAGS=-Dwarnings cargo check` に通す回帰テストを追加。これにより unused
variables/mut/imports/must_use、redundant parens/braces、non-snake/global naming などの
generated-Rust warning が再発すると nextest 上で失敗する。`scripts/test_aot.sh` の
generated-Rust clippy smoke も同じ warning 誘発 source に更新し、`cargo clippy -- -D
warnings` のローカル AoT gate で #7076 を継続監視する。

### マクロ展開まわりの複数バグを修正 (Issue #7350)

`@manipulate` 実装中に発見したマクロまわりの 4 件を修正(本家 julia では正常動作)。
(A1) マクロが返す `if`/三項式が式位置で `nothing` を返す → 値を生む `Expr::Ternary` へ
lowering(`macro_runtime.rs`)。(A2) 添字代入 `a[i]=v` 無効 → quote 構築
(`cst_to_constructor.rs`)が LHS を不正 `Symbol("a[i]")` にしていたのを `Expr(:ref,...)` へ、
`value_to_stmt` で `IndexAssign`/`FieldAssign` を処理。(A3) 修飾呼び出し `Mod.f(...)` を
呼び出しターゲットにできない → `:.` callee を module 付き呼び出しへ振り分け。(B5)
`acc=nothing` をループ内で異種型三項に再代入すると `acc===nothing` が常時 true に
const-fold され値が捨てられる → `inference.rs` で「具象非数値型→`Any` widening」も
`mixed_type_vars` にマークし動的スロット化。(A4) は #7355 として切り出し本日対応済み(下記)。

### モジュール内マクロの解決＋ハイジーン (Issue #7355 / #7350 A4)

`module M ... macro m ... end end` のマクロが `using .M` 後の `@m(...)` でも修飾
`M.@m(...)` でも `unknown macro @m` で解決できず、解決後も非 esc 識別子が呼び出し側
スコープで解決され未 export ヘルパ参照(`:(helper($v))`)が失敗していた(本家は両方動作)。
(1) `lower_module_definition` に `macro_ctx: Option<&LambdaContext>` を追加しモジュール内
マクロを呼び出し側レジストリへ登録(パーサは `M.` を落とすため両形が bare 名で解決;関数は
従来からフラット hoist)。(2) `LambdaContext` にハイジーンフレーム(定義モジュール名＋メンバ
集合＋esc 深度)を持たせ、`macro_runtime.rs` の `call_expr_from_values` で esc 外のメンバ
呼び出しターゲットを `M.name` に修飾(修飾アクセスは visibility ゲートを迂回)。`esc(...)`
内は深度カウンタで修飾を抑止し呼び出し側解決を維持。fixture 3 本追加、本家 julia と一致、
フルスイート 3929 passed / 0 failed。

### inner constructor 本体が無視されるバグを修正 (Issue #7345)

ユーザー定義 struct の inner constructor 本体(バリデーション・`new(...)` への引数加工)が
実行されず、常に合成 default フィールド構築へフォールバックしていた。例: `struct Bar; x;
Bar(x) = new(x*10); end` で `Bar(5).x` が `50` でなく `5` を返し、`Foo(x) = (x>0 ||
error(...); new(x))` のバリデーションも黙って無視されていた。原因は
`try_compile_struct_table_constructor_call`(`compile/expr/call/constructors.rs`)の
「引数が宣言フィールド数・型に一致したら default constructor を使う」高速パスが、inner
constructor を持つ struct でも発動していたこと。本家 Julia は inner constructor を1つでも
宣言すると default フィールド constructor を**合成しない**ため、この fallback は誤り。修正=
高速パスを「**宣言された constructor がこの呼び出しにマッチしない場合のみ**」に限定。inner
constructor がマッチすればメソッドディスパッチへ回して本体を実行し、マッチしない場合のみ合成
default を使う。後者は REPL のグローバル再構築が依存している(`@gif` の `Animation` を
`Animation(frames)` の全フィールド位置引数で再注入するが inner ctor は `Animation()` のみ、
Issue #7151 経路)。inner constructor を持たない struct では条件が常に false となり、
outer constructor の再帰終端を担う既存高速パス(`Year(v::Int)=Year(Int64(v))`)は不変。
fixture `struct/inner_constructor_untyped_field_body_7345.jl` を追加。これにより Issue #7338
の `@manipulate` 非プロット検証を本来の inner constructor で行えるようになった。

### Interact `@manipulate` の複数同時コントロール対応 (Issue #7344)

`@manipulate for a = …, b = … end`(複数バインディング)が従来は実行時に
`UndefVarError` になっていた(#7343 で quote の複数 for-binding は通るようになったが、
マクロ側が単一前提だった)。本家は各変数に独立した reactive コントロールを与え、いずれかの
変更で本体を再評価するが、リアクティブ実行が無い sjulia では **選択肢の直積を 1 つの結合
ドロップダウン**で近似する(全組合せが選択可能、ラベル `a=<va>, b=<vb>, …`、最内変数が最速で
変化)。実装=`@manipulate` マクロが `forloop.args[1].head == :block` を検出してネストループに
展開し、直積の各組合せで本体を評価して結合ラベルを生成、`Manipulate(…, :dropdown)` を構築。
既存の dropdown レンダラをそのまま再利用(Rust 変更不要)。fixture
`packages/interact_manipulate_multi_control_7344.jl` と
`plot_artifact_mime_tests::test_manipulate_multiple_controls_emit_combined_dropdown_7344`
を追加。**本家の N 独立コントロールとは異なり 1 結合コントロールに畳む**意図的な近似
(UNIMPLEMENTED.md に明記)。実装中、マクロ展開時 VM が `?:`/`=== nothing` を誤評価する
ことが判明したため、ラベル合成は明示的な fold で回避。


### quote の複数 for-binding 対応 (Issue #7343)

`quote`/`:( … )` 内の `for a = …, b = …`(カンマ区切りの複数バインディング)が従来は
lowering エラー `quote of for: multiple bindings not yet supported` になっていた。本家は
これを `Expr(:for, Expr(:block, :(a=…), :(b=…)), body)` で表現するため、quote→constructor
パス(`src/lowering/expr/quote/cst_to_constructor.rs` の `ForStatement`)を全 `ForBinding`
を収集し、2 つ以上なら `Expr(:block, …)` でラップするよう変更(単一はインラインのまま)。
これは Interact 非依存の汎用 `quote` 機能で、`@manipulate for a = …, b = …`(複数コントロール,
#7344)の前提でもある。fixture `metaprogramming/quote_for_multiple_bindings_7343.jl` を追加
(本家とパリティ)。なお `eval` での `for` Expr 実行は単一/複数いずれも未対応の別制約で本件の
範囲外。

### Interact `@manipulate` のレンジをスライダー描画 (Issue #7338)

本家 Interact の `widget()` ディスパッチ(`AbstractRange → slider`, それ以外 →
`togglebuttons`)に倣い、`@manipulate for k = 1:5 …` のように **`AbstractRange` を
選択肢にした場合は Plotly スライダー**で、配列など離散選択肢は従来どおりドロップダウンで
描画する。リアクティブ実行は無いまま、全選択肢の trace を事前生成し可視性をスライダー
ステップ/ドロップダウンボタンで切替える静的図(MVP 思想を維持)。実装=`Manipulate` に
第3フィールド `control::Symbol`(`:slider`/`:dropdown`)を追加(2 引数の後方互換 outer
constructor 併設)、`@manipulate` 展開で `manipulate_control(choices)` により種別を判定、
Rust 側 `generate_plotly_manipulate_json` が `:slider` のとき `sliders` を、そうでなければ
`updatemenus` を出力。fixture / `plot_artifact_mime_tests` を更新。なお `widget()` の
`Bool→checkbox` / `String→textbox` / `Number→spinbox` / `Dict` / `Date` / `Color` は
双方向入力が必要で静的図に載らないため引き続き Phase 3 据え置き(#7275)。

実装中に判明した sjulia マクロ展開の制約(回避済み): 展開後コードで (1) 三項演算子が
引数位置だと `nothing` に評価される、(2) 添字代入 `a[i] = x` は代入先 Symbol 必須で不可、
(3) 修飾呼び出し `Mod.f(...)` の呼び出しターゲット不可、(4) 非 esc 識別子は caller スコープ
解決のため未エクスポートのモジュール関数は見えない。そのため種別判定を **export した実関数
`manipulate_control` の bare 呼び出し**に閉じ込めた。

### Interact `@manipulate` の非プロット本体を明確にエラー化 (Issue #7338)

`@manipulate for x = 1:3; x^2; end` のように本体がプロット以外の値を返すと、
従来は `Manipulate(Any[1,4,9], …)` を黙って構築し exit 0 で**何も描画しない**無言の
失敗になっていた。本家 Interact はリアクティブ widget で値を表示するが、no-JIT・静的
Plotly の MVP では範囲外(#7275)。修正=`@manipulate` 展開時に各キャプチャ値が `Plot`
かを検証し、違反時に `@manipulate body must return a Plots.Plot (got <型>); …(Issue
#7338)` を投げる。検証は本来 `Manipulate` の inner constructor が自然だが、sjulia は
inner constructor 本体を無視する(Issue #7345)ため展開側で実施。fixture
`packages/interact_manipulate_nonplot_errors_7338.jl` を追加。本家とは挙動が異なる
(本家は値を表示)ため意図的な非パリティ。

### 2D `plot`/`scatter` が `legend` キーワードを受け付ける (Issue #7337)

bundled Plots の 2D `plot` / `plot!` / `scatter` / `scatter!` が `legend` を含む
表示専用キーワードを `MethodError: ... unsupported keyword argument "legend"` で
拒否していた。本家 Plots.jl では `legend` は 2D/3D 共通の普遍属性だが、sjulia 側は
2D パスのメソッドが `aspect_ratio` 系と `title` の特定キーワードしか宣言しておらず
`kwargs...` のキャッチオールを欠いていた(3D の `plot3d`/`plot3d!` は既に `kwargs...`
を持ち `legend` を受理して無視していた)。修正=`packages/Plots/src/api.jl` の 2D
`plot`/`plot!`/`scatter`/`scatter!` 全オーバーロードに `kwargs...` を追加し、未モデル化の
表示専用キーワードを 3D パスと同様に「受理して無視」する。fixture
`packages/plots_plot_legend_kwarg_7337.jl` を追加。

## 最新対応 (2026-06-22)

### `::AbstractMatrix` が `Function` を緩マッチしないよう修正 (Issue #7334)

`::AbstractMatrix`(= `AbstractArray{T,2}`)パラメータが関数シングルトン `typeof(sin)` を
緩マッチし、しかも具体的な `::Function` メソッドより優先される(`h(sin)` が
`h(::AbstractMatrix)` を選び、本家は `h(::Function)`)バグを修正。根因はコンパイル時
`struct_parents_fallback_match` → `struct_is_subtype_of_abstract` の保守的 accept
(`None => return true`)で、`typeof(...)` という struct 名(宣言済みユーザ struct でも
組み込みファミリでもない)を任意の抽象型のサブタイプと誤判定していたこと(#7266 と同クラス)。
修正=関数シングルトン名を既知の `Function` 上位型(`typeof(f) <: Function <: Any`)として
扱い、`Function`/`Any` 以外には `false` を返す。これにより #7322 で導入した Plots の
`::Matrix` 回避を解消し、本家通りの `scatter(m::AbstractMatrix)` / `scatter!(m::AbstractMatrix)`
に復元。`scatter(sin)` は `scatter(f::Function)` に正しく到達。fixture
`dispatch/dispatch_abstractmatrix_no_loose_match_function_7334.jl`(8 テスト, julia parity 一致)。
full 3928/0。

### Plots `scatter(::Matrix)` + 行列スライス dispatch (Issue #7322 / #7333)

#7275 の iOS サンプル `scatter(rand(10,2))` が `MethodError: scatter(::Matrix{Float64})`
で落ちる問題を2点で解消:

- **#7322**: bundled Plots に `scatter(m::AbstractMatrix)` / `scatter!(m::AbstractMatrix)`
  を追加(列ごと1系列、本家 Plots.jl 同等)。当初は #7334 回避のため具象 `::Matrix` だったが
  #7334 修正後に本家通り `::AbstractMatrix` へ復元(上記 #7334 エントリ参照)。
- **#7333**: `m[:, 1]` などの行列スライスが推論で rank 不明の bare `Array` に落ち
  `::Vector` メソッドに dispatch できなかったのを、`Expr::Index` 推論でスライス次元数から
  rank を復元するよう修正(JuliaType/ValueType 両チャネル)。#7307/#7317 の rand rank
  復元と同系統。

作業中に **#7334(bug)** を発見・起票し、本セッションで修正済み(上記エントリ)。

### wasm32 で `::Int` パラメータが dispatch できない問題を修正 (Issue #7310)

`::Int` を持つユーザ関数(末尾デフォルト引数を含む)が wasm32 ターゲットでのみ
`MethodError: no method matching f(::Float64, …, ::Int64, …)` になり、ネイティブ
CLI では `206.0000000001` を返していた。根因は `types/native_word.rs` の
`native_int_julia_type` / `native_int_type_name`(`UInt` 版含む)が `usize::BITS`
を見て 32-bit ターゲット(wasm32)では `Int`→`Int32` を返していた点。VM の整数
キャリアは `compile/utils.rs` で常に `Value::I64` / `ValueType::I64`(整数リテラルは
ターゲット非依存で Int64)なので、`::Int` だけが `Int32` に解決されると Int64 の
リテラル/実引数と決して一致せず、`::Int` を持つあらゆる関数が wasm32 で dispatch
失敗していた。修正は `Int`/`UInt` を常に `Int64`/`UInt64` に解決(ポインタ幅非依存)。
プラットフォーム依存の `Sys.WORD_SIZE` は `usize::BITS` から別途算出しており影響なし。
fixture: `tests/fixtures/dispatch/int_alias_param_dispatch_7310.jl`。wasm32 実機
(node 経由 `run_from_source`)で MWE が `206.0000000001` を返すことを確認。

### `Vector{Any}` の `show` で `Any[` 接頭辞が落ちる (Issue #7303)

`println(Any[1, 2, 3])` が sjulia では `[1, 2, 3]` を出力していた(本家 Julia は
`Any[1, 2, 3]`)。本家の `typeinfo_prefix`(`base/arrayshow.jl`)は型駆動で、
`Vector{Any}` は `typeinfo_implicit(Any) == false` のため常に `Any[...]` 接頭辞を付ける。

sjulia は `Pair`/`Tuple`/ネスト配列のように **本家なら精密な eltype を推論するが
sjulia が `Any` に widen してしまう** ケース(`[1 => 2]` が `Vector{Any}` 等、
`docs/vm/UNIMPLEMENTED.md`)を bare 表示にするため `Any` eltype の接頭辞を要素値から
導出していた。この値駆動が行き過ぎており、同型のスカラ要素を持つ真の `Vector{Any}`
(`Any[1, 2, 3]`)まで接頭辞を落としていた。

- 修正: `_array_show_prefix`(`base/io.jl`)と `array_show_prefix`(`vm/formatting/mod.rs`)で、
  `Any` タグの同型 implicit ランは **要素がすべて推論 widen 対象の複合型
  (`Pair`/`Tuple`/`AbstractArray`)のときのみ** 接頭辞を落とす。スカラ implicit 型
  (`Int64`/`Float64`/`Char`/`String`/`Symbol`)の `Any` 配列はユーザが明示的に
  `Any[...]` と書いたものなので接頭辞を保持する(sjulia はスカラ配列リテラルを `Any` に
  widen しない)。
- 不変: `Int[1,2]`→`[1, 2]` / `Real[1,2]`→`Real[1, 2]` / `Any[1,"x"]`→`Any[1, "x"]` /
  `[1 => 1, 2 => 4]`→bare(本家 1.12.6 一致、`scripts/fixture_julia_parity.sh` でパリティ確認)。

### `Vector{T}(undef, n)` / `T[...]` がユーザ struct eltype を `Any` に widen (Issue #7304)

`Vector{PP}(undef, 1)` / `PP[PP(1)]`(`PP` はユーザ struct)が `Vector{Any}` になっていた
(本家は `Vector{PP}`)。組み込みプリミティブ型は保持されるがユーザ struct/パラメトリック型は
`Any` に落ちていた。

- 根因: 型名→`ArrayElementType` 変換がプリミティブ名のみ認識しユーザ struct を
  `ArrayElementType::Any` に落とし(`exec/array_basic.rs::array_element_type_from_julia_type` と
  `compile/expr/builtin_array.rs::heap_julia_type_array_element_type`)、かつ `StructOf(type_id)`
  タグを持てても表示時の `julia_type_name()` / `array_element_type_to_julia_type` が
  **struct レジストリを持たないため** `Any` を返していた。
- 修正(局所): (1) 構築時にユーザ struct 名を `struct_defs`/`shared_ctx` 経由で
  `StructOf(type_id)` へ解決(`Memory{T}(n)` の動的経路 `NewMemoryDynamicTyped` と
  typed リテラル compile 経路)、(2) `typeof`/`eltype` 表示の境界で `StructOf` を
  `struct_defs` で struct 名へ逆引きする `self` メソッド(`memory_element_type_name` /
  `array_element_type_to_julia_type_resolved` / `array_wrapper_julia_type_resolved`、後者の
  ため `StructInstance::array_wrapper_element_array_type` アクセサ追加)を追加し、Memory・配列
  ラッパの `typeof` 経路をそこへ通す。ホットな `array_wrapper_julia_type`(#6846 でレジストリ
  非依存に最適化済み)は不変のまま、表示境界でのみ解決。
- 不変: 組み込みプリミティブ(`Vector{Int8}`/`Int[...]` 等)と `Vector{Any}`(回帰なし)。
  本家 1.12.6 とパリティ確認。

### 修飾付き型アクセス `Module.Type` (Issue #7302)

モジュールが `export` した型は非修飾名(`Plot`)では参照できる一方、**修飾名**
`Module.Type`(`Plots.Plot` / `M.Circle`)が解決できず
`Compilation error: Msg("Module Plots has no function named Plot")` で失敗していた。
上流 Julia は `Module.Type` を常に許可する。

- **根本原因**: 修飾アクセス `Module.X` を**値**として使うパス
  (`compile/expr/struct_.rs::compile_field_access` → `compile_module_function_ref`)が、
  ユーザ/バンドルパッケージモジュールでは `module_functions`(関数名)しか見ておらず、
  `X` が**型**(struct/parametric/abstract/enum/primitive)の場合に「has no function
  named」エラーへ落ちていた(`Base.<Type>` 分岐には型解決があったがユーザモジュールには
  無かった)。修正は `compile_module_function_ref` で、モジュールに同名関数が無いときに
  型テーブル(`struct_table`/`parametric_structs`/`abstract_type_names`/`enum_types`/
  `is_primitive_type_name`)を**短名**で引いて `PushDataType` を発行する分岐を追加(モジュール
  型は短名で登録される。関数が先にマッチするので関数束縛は従来通り優先)。これで `isa`・`<:`・
  `===`・`Module.T(args)` 構築・素の `Module.T` 参照がすべて動く。
- **副次対応(修飾抽象型注釈)**: メソッドシグネチャの**修飾抽象型注釈**
  `f(s::M.Shape)` は別経路で、`parse_type_name("M.Shape")` が `Struct("M.Shape")` を返し、
  `collect.rs::resolve_abstract_type` が抽象レジストリ(**短名** `Shape` で登録)を
  `M.Shape` のまま引いて失敗 → `AbstractUser` に再分類されず具象 `Struct("M.Shape")` 扱いとなり、
  `M.Circle` 引数がディスパッチで一致せず `MethodError` になっていた。`resolve_abstract_type` で
  ルックアップ前にモジュール接頭辞を除去するよう修正(モジュール修飾は型同一性の一部ではない)。
  非修飾 `f(s::Shape)` と同一にディスパッチする。
- **テスト**: `fixtures/modules/module_qualified_type_access_7302.jl`(13 アサート: isa 修飾
  具象/抽象、`<:`、`===`、`::M.T` 注釈、parametric、mutable、`x isa M.T`、修飾抽象/具象の
  パラメータ注釈、修飾戻り値型注釈、`Type{M.T}` ディスパッチ)を julia とパリティ確認(13/13)。
  `fixtures/packages/plots_qualified_type_access_7302.jl`(バンドル Plots.Plot)。
### `MersenneTwister(seed)` の構築に対応 (Issue #7306)

これまで `MersenneTwister` は型注釈・`isa`・ディスパッチ (#7231) では RNG 扱いだったが、
`MersenneTwister(seed)` の**構築**は `Unknown function: MersenneTwister` で失敗していた。
本対応で Xoshiro / StableRNG と同じ経路で構築可能にした。

- バック実装は決定的な **MT19937-64** エンジン (`rng.rs::MersenneTwister`)。同一 seed →
  同一系列、異なる seed → 異なる系列、`rand`/`randn`/`rand(m,n)`/`randn(m,n)` が有限値を返す。
- `isa AbstractRNG` を満たし、`typeof` は `MersenneTwister` を返す。無型 / `::MersenneTwister` /
  `::AbstractRNG` 引数のユーザ関数を通しても動作。
- **ビット一致性**: upstream Julia の `MersenneTwister` は dSFMT バックなので、生成ストリームは
  **upstream と完全一致しない**(no-JIT VM で dSFMT 再現は非現実的かつ要件外)。fixture は
  完全一致値ではなく構造的・seed 再現性プロパティを検証する。
- 経路: `BuiltinOp::MersenneTwisterRNG` → `Instr::NewMersenne` →
  `Value::Rng(RngInstance::Mersenne(Box<MersenneTwister>))`(状態が ~2.5KiB のため `Value`
  肥大化と `result_large_err` を避けるため Box 化)。`BuiltinOp` 列挙子は serialized IR
  discriminant 互換のため**末尾に追加**(中間挿入は base/prelude キャッシュ bytecode を破壊する)。
- fixture: `subset_julia_vm/tests/fixtures/stdlib/random_mersenne_twister_ctor_7306.jl`(13 件)。
### `rand(n)`/`randn(n)` の native-array carrier ディスパッチ (Issue #7307)

`scatter(rand(5))` / `plot(rand(5))` が `MethodError: no method matching
scatter(::Float64)` で失敗していた(`scatter(collect(rand(5)))` は `scatter(::Array)`)。
`typeof`/`isa Vector` は `Vector{Float64}` を返すのにディスパッチだけ外れる。
根因は `rand`/`randn` の `Expr::Builtin` 推論アーム(`infer_julia_type` /
`infer_expr_type`)が引数付きでも常にスカラ `Float64` / 未パラメータ化 `Array`
(rank 不明)を返しており、バンドル Plots の `scatter(y::Vector)`/`plot(y::Vector)`
に静的マッチしなかったこと。`zeros(n)` は pure-Julia 関数呼び出し経路で
`Vector{Float64}` にランク付けされるため動いていた。

スカラ整数次元引数からランクを復元(`compile/expr/infer/expr_tfuncs.rs` の
`infer_rand_array_julia_type_for`/`infer_rand_array_value_type_for`、`zeros`/`ones`
と同じ `dims_rank_from_args` を流用)し、`rand(n)`→`Vector{Float64}`、
`rand(n,m)`→`Matrix{Float64}`、`randn` 同様。要素型は Float64 固定で、`rand(Int, n)`
は `RandIntArray` ランタイムが現状 `Float64` 配列を返す別バグのため意図的に defer
(推論↔ランタイム不一致回避)。RNG/コレクション形は先頭非整数で従来通り defer。
Vector carrier は完全解決。Matrix carrier(`scatter(rand(n,m))`)はバンドル Plots に
`scatter(::Matrix)` が無い別機能(型は `Matrix{Float64}` に正常化)。
### AoT 生成 Rust の冗長括弧で `clippy -D warnings` が警告 (Issue #7311)

AoT の二項演算エミッタが優先順位保持のため全ての二項演算を括弧で包むため、トップレベル
(および関数の唯一の引数)では冗長括弧となり、生成 Rust に対する `clippy -D warnings` が
`unused_parens`(rustc)と `clippy::double_parens`(clippy)で警告していた(`bug` / `aot`)。
実行と `--emit-binary` は無影響。生成 Rust ヘッダに 3 つの allow 属性を追加して解消(エミッタを
優先順位依存にする選択肢より安全)。詳細は [DONE.md](./DONE.md) を参照。
### `Float64 ^ Integer` を上流の補償付き power-by-squaring に一致 (Issue #7308)

`10^-2` / `10.0^-2` が `0.01` を返していたが、上流 Julia 1.12.6 は
`0.010000000000000002`(~1 ULP 差)を返す。原因は VM の `Float64^Int` が Rust の
`powf`/`powi`(`inv(10.0^2)=inv(100.0)=0.01` 相当)を使っており、上流
`base/special/pow.jl` の `pow_body(x::Float64, n::Integer)`(低位誤差項 `xnlo`/`ynlo`
を追跡する補償付き power-by-squaring。`inv(10.0)^2=0.1^2=0.010000000000000002` 相当)と
リダクション順序が異なっていたこと。`#7233` で整数底の負リテラル指数を `literal_pow`
経由で `Float64(x)^p` に広げたため表面化した。

- `intrinsics_exec.rs` に上流の `pow_body(::Float64,::Integer)` と `two_mul` を移植
  (`pow_body_f64_int` / `two_mul_f64`)し、整数値かつ `|n| < 2^20` の指数を補償経路へ
  ルーティングする `pow_f64(base, exp)` を追加。`n == 0` は上流 `^(x::Float64,n::Integer)`
  に倣い `one(x)` を短絡(`pow_body` は `n != 0` を仮定するため)、非整数・範囲外指数は
  正しく丸められた `powf` にフォールバック(上流 `pow_body(::Float64,::Float64)` の
  log/exp 経路と一致)。
- 既存の `Float64^Int` 経路すべて(`Instr::PowF64` / `Intrinsic::PowFloat` の typed 経路、
  `dynamic_pow` の F64/F32↔F64/F16↔F64/Bool↔F64/Irrational 各アーム)を `pow_f64` に統一。
- 厳密表現できるケース(`2.0^3=8.0`, `2.0^-3=0.125`, `4^-2=0.0625`, `1.5^10`, `7.0^5`)と
  非負指数(`10.0^2=100.0`, `3.0^3=27.0`)は不変。NaN/±Inf/±0.0 底のエッジも上流一致を確認。
- 注: F32/F16 の `^` は上流が別アルゴリズム(`exp2(log2(abs)·y)`)で本件のスコープ外。
  AoT 経路(`aot/optimizer/constant_folding.rs` 等の `powf` 定畳み込み)も別経路で本件未変更
  (VM 優先方針)。
- 検証: `arithmetic/float_pow_int_compensated_7308.jl` を新規追加(38 アサーション、
  全期待値は julia 1.12.6 から取得)。
### quote 内の colon インデックスが未定義 Symbol `:` に往復していた (Issue #7312)

`:(a[:, 1])` のように quote ブロック内で colon インデックス `:` を使うと、quote の
`:ref` アームでは colon が `Symbol(":")` として捕捉され、`eval` 時に `Colon()` へ解決
されず `getindex(::Matrix, ::Symbol, ::Int)` の MethodError(または
`UndefVarError: : not defined`)になっていた。非 colon インデックスは #7275 で対応済み。

- 本家 Julia は大域 `:` を `Colon()` に束縛しているため `eval(Symbol(":"))` は `Colon()`。
  この束縛セマンティクスに合わせ、`vm/builtins_macro/eval.rs` の `eval_expr_value` の
  `Value::Symbol` アームで、同名のローカルが存在しない `Symbol(":")` を
  `Value::SliceAll`(= `Colon()`)へ解決するようにした。これで `eval(:(m[:, j]))` /
  `eval(:(m[i, :]))` / `eval(:(m[:]))` / `eval(Expr(:ref, :m, Symbol(":"), 1))` が本家と
  一致して列・行スライスを返す。直接記述の `m[:, 1]` 経路は従来どおり `SliceAll` に下げて
  おり退行なし。
- 付随して発見(本 PR では対応せず): スタンドアロンの `Colon()` コンストラクタ・`Colon`
  型名は sjulia 未対応(`Colon()` → "Unknown function: Colon"、`Colon` → UndefVarError)。
  `typeof(eval(Symbol(":")))` も本家 `Colon` に対し sjulia は `Any`。fixture では別機能の
  これらに依存しないようにしている。
### `~(::Bool)` ビット否定を追加 (Issue #7305)

`~true` / `~false` が `MethodError: no method matching ~(::Bool)` になっていた。
上流 `base/bool.jl` の `(~)(x::Bool) = !x` を `subset_julia_vm/src/julia/base/bool.jl`
に移植し、`~true === false` / `~false === true` を返すようにした。ブロードキャスト
`.~[true, false]` も `Bool[0, 1]`(= `[false, true]`)を返す。fixture:
`tests/fixtures/bool/bitnot_bool_7305.jl`。

### `inv(::BigInt)` が整数除算で 0 を返す問題を修正 (Issue #7309)

`inv(big(2))` が `0`(整数除算 `1 ÷ 2`)を返していた。汎用フォールバック
`inv(x::Number) = one(x)/x` に届くと、BigInt では `big(1)/big(2)` が整数除算され 0 に
なるため。上流 `base/int.jl` の `inv(x::Integer) = float(one(x))/float(x)` に倣い、
`subset_julia_vm/src/julia/base/gmp.jl` に `inv(x::BigInt) = inv(BigFloat(x))` を追加。
`inv(big(2)) == 0.5 :: BigFloat` となり、`inv(2)` / `inv(2.0)`(いずれも Float64)は不変。
非二進有理数(`inv(big(3))` 等)は astro-float と MPFR で最終 ULP が異なるため fixture
では二進値(`0.5`/`0.25`/`0.125`)のみビット一致を確認。fixture:
`tests/fixtures/bigint/inv_bigint_7309.jl`。

## 最新対応 (2026-06-21)

### Web Playground: iOS 専用サンプルを有効化 (Issue #7286)

これまで `web/samples_ir.js` で `webUnsupported: true` とされ「未対応」通知を出していた
iOS 専用サンプルのうち 5 件を Web Playground で実行・描画できるようにした。

- **バンドルパッケージ系** (`primes_package` / `symbolics_package` / `barnsley_fern`):
  Primes / Symbolics / Distributions は `subset_julia_vm` に `include_str!` で埋め込まれて
  おり、WASM バイナリにそのまま同梱される。WASM のエントリポイント `run_from_source`
  は CLI/iOS と同じ `parse_and_lower` → `PackageLoader` を通るため、`using` だけで解決済み
  だった(別個の base-cache ファイルへの追加は不要)。フラグを落とすだけで動作する。
- **JSXGraph 系** (`jsxgraph_demo` / `apollonian_gasket`): VM は CLI/iOS と同様に
  `application/vnd.jsxgraph+json` アーティファクトを生成していたが、フロントエンドに
  ボード描画が無かった。iOS の `JSXGraphView.swift` の描画ロジックを `web/app.js` の
  `renderJsxgraph` に移植し、`web/jsxgraph.min.js`(iOS と同一の 1.12.2)を同梱。
- **副次的な既存バグ修正**: Plotly / JSXGraph の UMD ラッパは Monaco の RequireJS
  (`define.amd`)が先に読まれるとグローバル `Plotly` / `JXG` を設定せず、プロット/ボードが
  無言で描画されなくなる。`index.html` で両スクリプトを Monaco loader より**前**に読み込む
  ように変更し、これまで Web Playground でプロットが描画されていなかった問題も解消した
  (Plotting 2D/3D・Sinc・Animation・Mandelbrot Heatmap 等)。
- **未対応のまま残す**: `distributions_package` のみ `webUnsupported: true` を維持。`cdf(::Binomial, …)`
  が SpecialFunctions の `_beta_inc_cf`(末尾デフォルト引数あり)を経由するが、その末尾
  デフォルト引数のディスパッチが **wasm32 ターゲットでのみ失敗**する(CLI/native では成功)。
  Normal 系の統計・サンプリング・フィッティングは WASM でも動作する。この wasm32 限定の
  ディスパッチ差異は別途要調査。

検証: `wasm-pack`(cache 埋め込み版 `scripts/wasm_build_with_cache.sh`)ビルド成功、Playwright
ヘッドレス Chrome で `web/test.html` 全 26 サンプル = 25 pass / 1 skip(Distributions)/ 0 fail、
JSXGraph 2 件と Plotly 系が実 DOM で SVG 描画されることを確認。`subset_julia_vm_web` に
サンプルパリティ回帰テスト 6 件追加(`cargo nextest --lib` 2947 pass)。
### 引数を昇格する外部コンストラクタの戻り値型推論 (Issue #7284)

`Foo(3).x` のような **インラインのフィールドアクセス** で、引数を `float()`/`promote()`
で昇格する **ユーザ定義の外部コンストラクタ** を持つパラメトリック構造体が
`Type error: expected I64, got Float64` で失敗していた。

```julia
struct Foo{T<:Real}; x::T; end
function Foo(x::Real)
    v = float(x)
    Foo{typeof(v)}(v)
end
Foo(3).x   # julia: 3.0 / sjulia(修正前): Runtime error: expected I64, got "Float64"
```

`using Distributions` の `Normal(2, 3)`(整数引数)でも、`mean(Normal(2, 3))` や
`Normal(2, 3).μ` のインラインアクセスが同じ症状を示していた。

- **根本原因**: 呼び出し点のコンストラクタ戻り値型推論
  (`compile/expr/infer/mod.rs` の `parametric_structs` 分岐 → `infer_value_parametric_struct_ctor`)
  は、**デフォルト内部コンストラクタ**をモデル化しており、構造体のフィールド型式
  (`x::T`)を生の引数型に直接束縛する(`Foo{Int64}`)。ユーザ外部コンストラクタが
  本体で `float()`/`promote()` を行っていてもそれを無視するため、フィールドロードが
  `Int64` 型付けされ、実行時の `Float64` フィールドがスロット型検査で失敗していた
  (実行時の `typeof` は正しく `Foo{Float64}`)。`mean(...)` 経由は別経路でマスクされて
  いたが、根因は同一(インライン直アクセスで露見)。
- **修正**: `parametric_structs` 分岐の先頭で、その名前に **ユーザ定義の外部
  コンストラクタ**(`function_ir_by_global_index` に本体 IR を持つメソッド)が `arg_types`
  にディスパッチするかを確認する。存在すれば、宣言/推論済みの戻り値型を優先し、なければ
  本体を引数値型で再推論して(`infer_shared_function_return_type_with_arg_types`)昇格後の
  具体型(または動的なら `Any`)を返す。ユーザ外部コンストラクタが無い場合は従来どおり
  `infer_value_parametric_struct_ctor` の精密な型引数推論にフォールバックするので、
  デフォルトコンストラクタのみで構築されるパラメトリック構造体(`Pt(3, 4) :: Pt{Int64}`)
  の整数フィールド型は維持される。
- **付随(本 PR 外, 報告対象)**: `Wrap(x::Integer, y::Integer) = Wrap(x + y)` のように
  外部コンストラクタが **単一引数のデフォルトコンストラクタへ Any 型引数で再帰** する形は、
  修正前の origin/main でも `No method matching Wrap([Any])` で失敗する既存の別問題
  (本修正とは独立で、悪化させていない)。

### Interact `@manipulate` の Plotly ドロップダウン MVP (Issue #7275)

Interact.jl の `@manipulate` マクロの **MVP**(リアクティブ widget ランタイムではなく、
離散選択ごとに本体を 1 回ずつ評価し、Plotly の `updatemenus` ドロップダウンで切替える
静的図を生成)を追加した。iOS/Web には双方向 FFI もリアクティブランタイムも無いため、
既存の表示アーティファクト経路(`plot`/`scatter` と同じ `application/vnd.plotly+json`)を
そのまま利用する。

```julia
using Interact, Plots
datasets = Dict(:some => [1.0, 4.0, 9.0, 16.0], :other => [2.0, 3.0, 5.0, 7.0])
@manipulate for dataset = [:some, :other]
    scatter(datasets[dataset])
end
```

- **新規バンドルパッケージ `Interact`**: `subset_julia_vm/packages/Interact/`(`Manipulate`
  構造体 + `@manipulate` マクロ)。`packages/mod.rs` に登録。マクロは `Plots.@animate`/`@gif`
  と同じ macro_runtime 経路(`ensure_bundled_package_macros_loaded` で自動登録)で `using`
  から到達する。`@manipulate for var = choices … end` は `forloop.args[1]`(`var = choices`
  束縛)を再利用し、本体の値(`Plot`)とラベル(`string(choice)`)を選択ごとに収集して
  `Manipulate(plots, labels)` を返す。
- **Rust アーティファクト生成**: `plotting/plotly.rs::generate_plotly_manipulate_json` を
  `try_value_to_artifact` に配線。`AnimatedGif` の frames 版に対応する設計で、全選択のトレースを
  先頭に出力(選択 0 のみ `visible:true`、残りは `false`)し、`"type":"dropdown"` の
  `updatemenus` の各ボタンが `visible` 配列とタイトルを切替える。
- **対応範囲(MVP)**: 単一の離散コントロール `for var = <vector/range>`、本体は `Plot` を返すもの。
  **Phase 2 として延期**(UNIMPLEMENTED.md): 真のリアクティビティ/双方向 FFI/ネイティブ
  コントロール、連続スライダー、複数同時コントロール、非プロット本体。
- **付随修正(quote→ref 往復)**: マクロ `quote` 本体中の **添字式**(`a[i]`, `a[i,j]`,
  `Any[]`)が `quote for index_expression not yet supported` で失敗していた。上流 Julia と同様
  `Expr(:ref, target, indices...)` に変換する arm を `cst_to_constructor.rs` に追加。これにより
  `@manipulate` の `esc`-ed ループ本体が `datasets[dataset]` 等を索引でき、`Plots.@animate`/
  `@gif` の添字本体(例 `push!(p, 1, d[i])`)も同時に動くようになった。

既知の付随的乖離(本 PR では回避せず lead へ MWE 報告): (1) `scatter(rand(10))` が
upstream は通るが sjulia は `MethodError: no method matching scatter(::Float64)`
(`rand(n)` の native-array carrier が `scatter(y::Vector)` にディスパッチしない)。
(2) `scatter(::Matrix)` / 添字スライス `a[:, 1]` 内のコロンの quote 往復(`:(a[:,1])` の
`:` が `Colon()` でなく未定義変数 `:` になる)。MVP のフィクスチャはベクタデータ形を使用。

### 抽象パラメトリック型まわりのディスパッチ修正 3 件 (Issue #7235)

Distributions.jl の移植中に見つかった、抽象パラメトリック型に絡む 3 つのディスパッチ不具合
(#5966 ファミリ)を修正。いずれも upstream julia 1.12.6 と完全一致するようになった。

- **sub1: `const` エイリアス経由の親型**。`const CUD = Dist{Uni,Cont}; struct Norm{T} <: CUD`
  のように `const` 型エイリアスを親型として宣言すると、`Norm(0.0) isa Dist` / `isa CUD` が
  `false` を返していた(本家 `true`)。根因: struct の親型文字列にエイリアス名 `CUD` がそのまま
  記録され、型階層チェーンが `Norm -> Dist` を辿れなかった。修正: `lowering/struct_.rs` で
  親型をプリスキャンで登録済みのエイリアステーブルに対して `type_alias::expand` するように
  し、`CUD` を `Dist{Uni,Cont}` に解決(本家の挙動に一致)。
- **sub2: パラメトリック抽象スーパータイプを持つ抽象型へのメソッド**。
  `abstract type Dist{F,S} <: Sampleable{F,S} end` のように抽象型がパラメトリック抽象スーパー
  タイプを持つと、`foo(d::Dist, x::Real) = 1; foo(Norm(0.0), 3.0)` が MethodError になり、
  さらに `Dist` 自体が `UndefVarError` になっていた。根因: pure-Rust パーサが
  パラメトリックな親(`Sampleable{F,S}`)をトップレベルの `ParametrizedTypeExpression` として
  出すため、`lowering/abstract_.rs` の `ParametrizedTypeExpression` アームが抽象型自身の名前
  (`Dist`)を親のベース名(`Sampleable`)で**上書き**してしまい、`Dist` が一切登録されていな
  かった。修正: 名前が既に設定済みなら親として記録するよう分岐(struct 側と同じパターン)。
- **sub3 (クロスモジュール qualified 部分)**。`M.onearg(M.Norm(0.0))` のように
  **モジュール修飾コンストラクタ呼び出しを別のモジュール修飾呼び出しの引数に直接**渡すと、
  内側の戻り値型が `Any` と推論され外側ディスパッチが `NoMethodFound` になっていた
  (一旦ローカルに束縛すれば動作した)。根因: `infer/julia_type.rs` の `ModuleCall` アームが
  qualified コンストラクタを認識せず method-table の `-> Any` を経て `Any` に落ちていた。
  修正: 非修飾の `Expr::Call` アームと同様に `resolve_struct_name` /
  `resolve_parametric_struct_name` で構築先の struct 型を推論。

sub3 のモジュールローカル抽象注釈部分は #7265 で既に解決済み(本対応では再修正せず確認のみ)。
fixtures: `abstract/const_alias_parametric_supertype_7235.jl`,
`abstract/parametric_abstract_supertype_dispatch_7235.jl`,
`dispatch/qualified_nested_constructor_arg_7235.jl`。

### パラメトリック型 + ユーザ定義外部コンストラクタの `::Type{Foo}` ディスパッチ修正 (Issue #7247)

`struct Foo{T<:Real}` がユーザ定義の**外部コンストラクタ**(`Foo(a::Real) = ...`)も持つとき、
ベア型 `Foo` を `ff(::Type{Foo}, v)` に渡すと `typeof(Foo)`(コンストラクタ関数)に解決され
MethodError になっていた。パラメトリックでない struct、またはカスタムコンストラクタが無い場合は
動作していた(=トリガはパラメトリック + カスタムコンストラクタの組合せ)。根因:
`infer/julia_type.rs` の識別子推論にパラメトリック struct のアームが無く、パラメトリック struct は
`struct_table` ではなく `parametric_structs` にいるため Priority 4(`struct_table`)を素通りし、
外部コンストラクタが登録した method-table エントリにより Priority 5 でコンストラクタ関数型に
誤推論されていた。修正: `struct_table` アームの直後に `parametric_structs` アームを追加し
`Type{Foo}` を返す(非修飾ベア名解決 `compile/expr/mod.rs` の順序と一致)。
fixtures: `dispatch/type_param_struct_custom_ctor_7247.jl`。

### Aizawa attractor の 3D アニメーションを iOS サンプルとして追加 (Issue #7273)

Lorenz 系で整備した Plots 機能(`plot3d(1)` 空 3D パス・`push!(plt, x, y, z)` で 1 点追加
(#7271)・`@animate ... every N` のフレーム間引き(#7272)・`Base.@kwdef mutable struct`)を
組み合わせ、**Aizawa strange attractor** の 3D アニメーションサンプルを追加した。

- サンプル: `SubsetJuliaVMApp/.../Samples/intermediate/aizawa_attractor.jl`(`samples.json` +
  Swift フォールバック `CodeSamples+Intermediate.swift` の "Aizawa Attractor" エントリ)。
  `plot3d(1, xlim=..., title=..., legend=false, marker=2)` で空の 3D パスを作り、
  3000 ステップの Euler 積分を `@animate ... every 20` で間引いて 150 フレームの GIF を生成する。
- 回帰テスト: `subset_julia_vm/tests/fixtures/packages/plots_aizawa_attractor_7273.jl`。
  `@kwdef` 既定値・`step!` 1 ステップの Float64 厳密値(`x=0.0993`, `y=0.0035000000000000005`,
  `z=0.0059`、upstream julia 1.12.6 とビット一致)・`length(anim.frames) == 150` を検証。
- パリティ: 力学計算と `isa(anim, Plots.Animation)` / `length(anim.frames) == 150` は upstream と一致。
  `.series`(upstream は `.series_list`)・bare `Animation`/`AnimatedGif` の export・filename 無し
  `gif(anim)`・`AnimatedGif.frames` は sjulia Plots サブセット固有の内部 API で、既マージの
  `plots_animate_every_7272.jl` / `plots_push_xy_point_7271.jl` と同じ規約に従う。

### `Random.default_rng()`/`GLOBAL_RNG` と RNG 引数のユーザ関数スレッディング (Issues #7230/#7231)

Distributions.jl ポート (#7178) で詰まった明示 RNG 周りの 2 件。

- **#7230 `Random.default_rng()` / `Random.GLOBAL_RNG`**: VM のグローバル RNG への
  ハンドルを返す `Value::Rng(RngInstance::Global)` を新設。`rand(default_rng())` /
  `randn(default_rng())` は素の `rand()` / `randn()` と **同じストリーム**を進める
  (`seed!` 後の 1 発目が一致、交互呼び出しも整合; upstream julia 1.12.6 と一致確認)。
  `typeof(default_rng()) === TaskLocalRNG`、`default_rng() isa AbstractRNG`、
  `println(default_rng())` → `TaskLocalRNG()` も upstream と一致。
- **#7231 明示 RNG 引数のスレッディング**: `f(rng)=randn(rng)`(無型)/
  `g(rng::Xoshiro)=randn(rng)` / `h(rng::AbstractRNG)=randn(rng)` がいずれも
  RNG からのスカラ `randn`/`rand` を返すように修正。
  - コンパイル経路: RNG 型注釈 (`Xoshiro`/`StableRNG`/`MersenneTwister`/`TaskLocalRNG`/
    `AbstractRNG`) を `julia_type_to_value_type*` で `ValueType::Rng` に対応付け
    (`randn(rng)` が `randn(dims...)` 経路に落ちて `DynamicToI64` する問題を解消)。
  - 無型 param 経路: 単一引数 `rand(x)`/`randn(x)` で `x` が静的 `Any` の場合に
    実行時分岐する `RandArg`/`RandnArg` 命令を追加(`Value::Rng` ならスカラ、整数なら
    `rand(n)`/`randn(n)` のベクトル)。
  - ディスパッチ経路: `Value::Rng` の dispatch 型を具体 RNG 名(`Xoshiro` 等)に揃え、
    `check_subtype` で `Xoshiro`/`StableRNG`/`TaskLocalRNG <: AbstractRNG` を解決
    (`h(rng::AbstractRNG)` の MethodError を解消)。`rand(rng,d)`/`randn(rng,d)` も到達可能に。

検証: 新規 fixture `stdlib/random_default_rng_7230.jl`(7/7)・
`stdlib/random_rng_param_threading_7231.jl`(10/10)が sjulia / upstream julia 双方で
parity 一致 (`fixture_julia_parity.sh`)。lib 2938/2938、full fixtures 153/153 green。
### `using LinearAlgebra` などをユーザモジュール内で使う際の名前衝突修正 (Issue #7245)

ユーザ定義モジュールが `D` / `D1` のように **stdlib のローカル変数名と同名** だと、
`using LinearAlgebra`(や `import LinearAlgebra`)を含むモジュールのロードが失敗していた。
症状: `module D; using LinearAlgebra; ddet(S)=det(S); end` で
`D.ddet([2.0 0.0; 0.0 3.0])` が `Compilation error: Msg("Module D has no function named diag")`
(upstream julia は `6.0`)。

- **根本原因**: LinearAlgebra の `Diagonal` まわりのメソッドは引数 `D::Diagonal` に対して
  `D.diag[i]`(フィールドアクセス)を多用する。コンパイラのフィールドアクセス処理
  (`compile/expr/struct_.rs::compile_field_access`)は、`X.field` の `X` が `Expr::Var` の
  とき `self.module_functions.contains_key(X)` だけでモジュール修飾呼び出しと判定していた。
  ユーザモジュール名が `D`(あるいは `D1`)だと、**ローカル変数 `D` が同名モジュールにシャドウ
  されて** フィールドアクセス `D.diag` がモジュール修飾呼び出し `D.diag(...)` に誤解決され、
  「Module D has no function named diag」で失敗していた。`MyMod` 等の非衝突名や、衝突する
  ローカル変数を持たない `Statistics` の `using` は影響を受けない(=症状の手掛かり)。
- **修正**: Julia のスコープ規則どおり、スコープ内のローカル束縛(関数引数/ローカル変数)が
  **同名モジュールをシャドウ**するように変更。ローカルが実際にモジュール値
  (`ValueType::Module`)を保持している場合のみモジュールアクセスとして扱う。
- **付随ギャップ(本 PR 外, 要報告)**: `inv(S)` を関数経由で得た行列が、要素は全て等しい
  (`all(A .== B)` が `true`、`typeof` も `Matrix{Float64}`)のに `A == B` が `false` を返す
  (`-0.0`/`0.0` を含む全行列 `==` のみ; upstream は `true`)。#7245 とは独立に top-level でも
  再現するため fixture では要素ごと比較で回避。
### AoT: トップレベル global 名衝突 (E0530) / 大きな Float64 の表示 (Issues #7242/#7256)

AoT(Rust への ahead-of-time コード生成、`--features aot`)の 2 件を修正。どちらも
デフォルトのテスト実行では `#[cfg(feature = "aot")]` ゲートのため検出されないので
`bash scripts/test_aot.sh` で確認する。

- **#7242 global 名が prelude/関数の引数名と衝突して E0530**: `a = 3.0` のような
  トップレベルのスカラ global は Rust の `static a: f64` として出力されていた。prelude
  ヘルパ `op_add(a: f64, b: f64)` のように同名の引数を持つ関数があると rustc が
  `error[E0530]: function parameters cannot shadow statics` で失敗(`--emit-binary` 失敗、
  `--check` では静かに通過)。修正=global static に衝突しない接頭辞 `__sjulia_global_`
  を付与し(`mod.rs` の `global_static_ident`)、参照側(`emit_global` / `AotExpr::Var`)も
  同じ名前へ書き換え。同名の関数引数が global を shadow する場合は接頭辞を付けない
  (`current_function_param_names` でスコープ判定)。`im`(lowercase const, #6966)は
  `program.globals` に含まれないため対象外。
- **#7256 大きな整数値 Float64 が指数表記でなく十進展開で出力**: `__sjulia_format_float64`
  が `1e30` を `1000000000000000000000000000000` と出力していた(本家・VM は `1.0e30`)。
  正準アルゴリズムを runtime クレート(`subset_julia_vm_runtime::intrinsics::format_float64_julia`)
  に追加し(VM 側 `vm::formatting::numeric::format_float_julia` と本家 julia 1.12.6 に一致:
  `|x|<1e6` の整数は `.0` 付き fixed、`[1e-4,1e6)` 外は `{:e}` ベースの科学表記、整数仮数は
  `1.0e30` のように `.0` を付与)、prelude の `__sjulia_format_float64` / `_float32` は
  この関数へ委譲する thin wrapper に変更。InexactError メッセージ内の値表示も同経路を通る
  ため `InexactError: Int64(1.0e30)` と本家一致になる。
- テスト: runtime クレートに `format_float64_julia` の executable 単体テスト(30 値、本家
  julia 1.12.6 で採取)、`aot_codegen/tests.rs` に #7242 接頭辞/参照/shadowing と #7256 委譲の
  string-assert、`aot_e2e_tests.rs` に large-float println / InexactError の e2e を追加。
  AoT ゲート full は baseline と同じ 7 件の既存 fail のみ(新規 fail 0)、default lib 2940 green。
- 既知の付随事項(本 PR 対象外): トップレベル binop の生成 Rust に冗長な括弧
  (`(a + (b as f64))`)が出るため、ヘッダに `#![allow(unused_parens)]` を持たない生成物に
  対し clippy `-D warnings` を掛けると警告になる(`--emit-binary`/実行は影響なし)。
  これは binop 出力側の既存挙動(local の `p+q` でも `(p + q)` になる)で、本修正とは独立。
### 型パラメータ波括弧内のネストした関数呼び出し `T{typeof(f(x))}(...)` (Issue #7240)

パラメトリックコンストラクタの型引数をネストした関数呼び出しで計算する形
(`Foo(x::Real) = Foo{typeof(float(x))}(float(x))`)が
`Compilation error: Msg("Undefined variable: float(x)")` でコンパイルできなかった。

- **根本原因**: 実行時型引数 `typeof(float(x))` は `compile_dynamic_parametric_struct`
  から `lower_expr_from_text` で式に戻されるが、この関数が手書きの
  「`Name(args)` を `,` で分割する」簡易パーサだったため、引数 `float(x)`
  (ネストした呼び出し)を 1 個の識別子 `Var("float(x)")` として取り込んでいた
  (コメントにも "nested parens not handled" とあった)。
- **修正**: `lower_expr_from_text` を本物のパーサ(`Parser`)+ 式ロワリング
  (`lower_expr`)経由に書き換え(`try_lower_expr_via_parser`)。これによりネストした
  呼び出し・ブロードキャスト・演算子・文字列リテラルなどが通常の式ロワリングと
  同一経路で処理される。`Symbol(s)` / `Symbol("foo")`(MIME 等)は従来どおり動作。
- 検証: 上流 julia 1.12.6 と一致(MWE は `3.0`)。fixture
  `struct/typeparam_nested_call_7240.jl`(MWE・深いネスト `typeof(g(h(x)))`・
  2 型パラメータ `Pair{typeof(float(x)),typeof(float(y))}`・回帰用 `typeof(var)`)。

### Plots: `plot3d` / `push!(plt, x, y[, z])` / `@animate ... every N` (Issues #7270/#7271/#7272)

upstream Plots.jl の Lorenz アトラクタ・アニメーションサンプルを動かすための 3 件を実装(`packages/Plots/`)。

- **#7270 `plot3d`**: `plot3d`/`plot3d!` を export。`plot3d(x,y,z; kw...)` ≡
  `plot(x,y,z; seriestype=:path3d, kw...)`、`plot3d(n::Integer; kw...)` は `n` 個の
  空 `:path3d` series を持つ `Plot` を初期化(後から `push!` で点を追加)。Lorenz の
  表示専用 kwarg(`xlim`/`ylim`/`zlim`/`legend`/`marker`)は受理して無視、`title` は
  従来どおり保持(#7030)。
- **#7271 `push!(plt, x, y[, z])`**: 第1 series へ 1 点を追加。upstream の
  `extend_series!(series, xi, yi[, zi])` に対応する `push!(plt, x::Number, y::Number)`
  / `push!(plt, x, y, z)`(および index 付き `push!(plt, i, x, y[, z])`)を追加。
  `Series` フィールドは immutable のまま、`plt.series[i]` を差し替えて追記(既存
  `push!(plt, i, y)` の自動 x 拡張と整合; #6355)。
- **#7272 `@animate`/`@gif ... every N` / `... when cond`**: 末尾修飾子に対応。
  捕捉判定は upstream `_animate` 準拠(`every N`→`mod1(counter, N) == 1`、`when c`→`c`)。
  - **パーサ修正** (`subset_julia_vm_parser`): `@m for...end <trailing>` で `for`/`while`
    ブロック引数の後に**同一行の**追加引数(`every 10` 等)を空白区切りマクロ引数として
    収集(従来はブロックで打ち切り→`every` が別文として評価され `UndefVarError`)。改行が
    あれば従来どおり別文(upstream julia と一致)。
  - **マクロ実装**: 単一可変長メソッド `@animate(forloop, args...)` で無修飾/修飾の両方を処理
    (バンドルマクロは name 1 つにつき登録スロット 1 つのため)。判定式は `if` 条件ではなく
    `frame(_anim, should::Bool)` への**呼び出し引数**として splice(マクロランタイムは
    `Expr(:if, cond, …)` の condition 位置に組み立て式を流せない)。
  - **マクロランタイム拡張** (Lorenz の付随ギャップ): マクロ展開後の値→IR 変換で
    フィールドアクセス `obj.field`(`Expr(:.)`)を未対応→対応に。さらにバンドルマクロ展開は
    全 `compile_time_functions` をコンパイルするため、`step!(l::Lorenz)` のようにユーザ struct を
    触る関数が `Unknown field` で落ちていた→展開プログラムにユーザ型定義(struct/abstract/
    primitive)を渡すよう `LambdaContext` に `compile_time_structs` 等を追加。

fixtures: `packages/plots_plot3d_alias_7270.jl`, `packages/plots_push_xy_point_7271.jl`,
`packages/plots_animate_every_7272.jl`。Lorenz サンプル(`plot3d(1)` +
`@animate ... end every 10` + `push!(plt, x, y, z)` + `gif`)が end-to-end で 150 フレーム
生成まで通ることを確認。

### 整数の負リテラル指数 `2^-3` (Issue #7233)

整数ベースに**リテラル**の負指数を与えた `2^-3` が DomainError を投げていた
(本家は `literal_pow` 経由で `0.125`)。本家 `base/intfuncs.jl` 同様、リテラル整数
指数は `Base.literal_pow(^, x, Val(p))` に下げる方針へ。

- **lowering** (`subset_julia_vm/src/lowering/expr/binary.rs`): `^` の右オペランドが
  「`-` の単項に整数リテラルが直接続く形」のときだけ `literal_pow(^, base, Val(p))`
  を生成。正のリテラル指数(`x^2`)は従来どおり高速な `Pow` 経路のまま、**非**リテラル
  指数(`n=-3; 2^n`)も `Pow` 経路に残し本家同様 DomainError を維持。
- **pure Julia** (`base/intfuncs.jl`): `literal_pow` を追加。整数ベースの負指数は
  Float64 へ拡幅(`2^-3 == 0.125`)、それ以外は `inv(x)^(-p)` で型安定
  (`(1//2)^-2 == 4//1`)。
- fixture: `arithmetic/literal_pow_negative_exponent_7233.jl`(リテラル各種 +
  `(1//2)^-2` 保型 + 非リテラル `2^n` の DomainError 回帰)。

### 前置ブロードキャスト単項 `.-v` / `.+v` (Issue #7234)

前置の dotted 単項 `.-v` / `.+v`(本家では `broadcast(-, v)` 相当)がパースエラー
だった。`.!` の前置ブロードキャスト処理に倣って実装。

- **parser** (`subset_julia_vm_parser/src/parser/expressions/mod.rs`): `parse_prefix` /
  `parse_prefix_with_postfix` の先頭で `try_parse_dotted_unary_broadcast_prefix` を呼び、
  `.+`/`.-`(後ろに `(` が来ない単項)と二トークンの `.~` を `BroadcastCallExpression`
  に変換(既存 lowering が `broadcast(op, x)` へ展開)。`.+(x,y)` のブロードキャスト
  関数呼び出しや、単項形を持たない `.*`/`./` の前置はそのまま(本家もエラー)。
  `.^` が前置単項より強く束縛する点(`.-x .^ 2` = `.-(x .^ 2)`)も `.!` 同様に処理。
- fixture: `broadcast/prefix_unary_dot_7234.jl`(`.-v`/`.+v`、`broadcast` 等価、
  優先順位 `.-x .^ 2`、整数ベクトルの `.~`)。

### StatsPlots 分布プロット + comma-form `using` 修正 (Issue #7262)

- 新しい同梱パッケージ `StatsPlots`(`packages/StatsPlots/`)を追加。
  `using Distributions, StatsPlots; plot(Normal(0, 1))` で標準正規分布の pdf 釣鐘
  曲線が既存 Plots アーティファクト経路でレンダリングされる。
  - レシピは「分布 → pdf/pmf を `quantile(d, 0.0001) … quantile(d, 0.9999)` の範囲で
    サンプリング → 既存 `plot(x, y)` / `bar(x, y)` へ委譲」。連続分布
    (`Normal`/`Uniform`/`Exponential`/`Gamma`/`Beta`/`Cauchy`/`LogNormal`/
    `Weibull`)は 100 点の `:line`、離散分布
    (`Bernoulli`/`Binomial`/`Poisson`/`Geometric`/`DiscreteUniform`/`Categorical`)は
    整数台の `:bar`(upstream は `:sticks`、バンドル Plots は `:bar` が最も忠実)。
  - **ディスパッチ方針 (Issue #7235)**: 別モジュール(Distributions)の抽象型
    `d::Distribution` への 1 メソッドは VM で信頼できないため、具象分布型ごとの
    薄い typed wrapper を定義し、untyped ヘルパー
    `_statsplots_continuous_plot` / `_statsplots_discrete_plot` へ委譲する。
  - Rust 登録: `src/julia/packages/mod.rs` に `include_str!` 定数 +
    `get_bundled_package`/`get_package_include`/`bundled_package_names` + 単体テスト
    3 件を追加。
- **comma-form `using A, B` の lowering を修正**: 以前は `using Distributions,
  StatsPlots` がモジュール名 `"Distributions, StatsPlots"` 1 個に潰れて
  「module '…' not found in LOAD_PATH」で失敗していた。CST の
  `import_list > import_path+` を 1 path = 1 `UsingImport` に展開するよう
  `lower_using_statement` を `Vec<UsingImport>` 返しに変更(単一/選択/相対/スコープ
  各形は従来どおり)。回帰 fixture `modules/using_comma_multiple.jl` を追加(upstream
  julia とパリティ確認済み)。
- フィクスチャ `tests/fixtures/statsplots/`(4 件)+ `modules/using_comma_multiple`
  を追加。フル `cargo nextest run --release` = 3909/3909 green。
- スコープ外(別 issue 候補): `@df` / DataFrame 連携 / `corrplot` / `boxplot` /
  `violin` / `density` 等の高機能レシピ、標本ベース `histogram(rand(d, n))`。
  作業中に判明した別の VM 制約(qualified type access `Plots.Plot` 不可)はチームに
  MWE 付きで報告。
### 配列リテラル内の位置スプラット対応 (Issue #7255)

- 配列リテラルが可変長タプル等をスプラットする形(`Any[pts...]`、`[a, xs..., b]`、
  `Float64[pts...]`、`Int[0, pts..., 99]` など)が `unsupported expression:
  splat_expression` で lowering に失敗していた問題を解消。`polygon(pts...)` の
  ような慣用的なコンストラクタが書けるようになった。
- lowering (`lowering/expr/collection.rs`):
  - **untyped `[a, xs..., b]`**: 旧来の `vcat(...)` ベースの降ろし方
    (タプルを正しく展開できず `Any[0, (1,2,3), 99]` を返す、また `vcat((1,2,3))`
    が単一タプルを `[1,2,3]` に展開してしまう upstream 非互換に依存していた)を
    廃止し、スプラット呼び出し `Base._array_splat_literal(vals...)` に降ろす。
    upstream の `Base.vect(X...)`(= `promote_typeof` + `T[X...]`)と一致。
  - **typed `T[a, xs..., b]`**: 型付き配列リテラルは index 式として parse され
    splat を弾いていた。要素に splat があり対象が型(`Any`/`Float64`/ユーザ struct/
    パラメトリック `Complex{Float64}` 等)の場合、`Base._array_splat_literal_typed(T,
    vals...)` へスプラット呼び出しで降ろす。upstream の
    `getindex(::Type{T}, vals...)` と一致。
- Base (`base/array.jl`): スプラット展開後の値から `promote_typeof` 相当で要素型を
  決める `_array_literal_promoted_eltype` と、上記 2 ヘルパー(`_array_splat_literal`
  / `_array_splat_literal_typed`)を追加。各値は 1 要素として配置し `convert(T, ...)`
  で要素型へ変換する。
- 値・型ともに upstream Julia と一致(`splat/splat_array_literal_7255` fixture を
  追加, 25 アサーション、`fixture_julia_parity.sh` で julia と完全一致)。
- 既知の付随的差異(本 PR の範囲外・既存挙動):
  - `Vector{Any}` の `show` が(要素が均質な場合)`Any[` 接頭辞を出さない
    (`Any[1,2,3]` を `[1,2,3]` と表示)。非 splat の `Any[1,2,3]` でも再現する
    既存の表示差異。
  - ユーザ struct の型付き配列(`T[...]` / `Vector{T}(undef,n)`)の宣言要素型が
    `Any` に広がる既存の制限。本 PR の splat 経路も非 splat の `T[...]` と同じ
    挙動(値は正しい)。
### 複合代入を式として使用 (Issue #7269)

- `x += y`(および `-=`/`*=`/`/=`/`^=`/`%=`/`÷=`/`.=`/`.+=` など)を **式の位置**
  ―― `return` 値、別の代入の RHS、関数の引数 ―― で使えるようにした。upstream
  Julia では複合代入は式であり、その値は「新しく代入された値」になる(例:
  `f(p) = (return p.z += 1.0)` は `2.0` を返す)。
- lowering で `NodeKind::CompoundAssignment` を式として降ろす経路を追加
  (`lowering/expr/mod.rs` の `lower_expr` / `lower_expr_with_ctx`)。実装は文形式の
  降ろし(`lowering/stmt/assignment.rs::lower_compound_assignment_impl`)を再利用し、
  得られた `Stmt` を「新しい値を返す `Expr`」へ変換する
  (`compound_assign_stmt_to_value_expr`)。単純変数は `AssignExpr`、フィールド /
  添字 / ネストフィールドは一時変数に新値を束縛して `LetBlock` で返すことで、
  対象の再読み込みや binop の再評価(副作用の二重実行)を避け、upstream の
  `tmp = lhs op rhs; store(tmp); tmp` 降ろしと一致させる。
- 実世界の駆動例 Lorenz アトラクタの `step!`(`return l.z += l.dt * dz`)が動作。
  `control_flow` に式位置の複合代入 fixture(変数 / フィールド / 添字 /
  Lorenz / ネストフィールド)を追加し、sjulia↔julia パリティを確認。
### モジュール内 abstract 型注釈のディスパッチ修正 (Issues #7263 / #7265)

- **根因**: バンドルパッケージ(モジュール)内で宣言された abstract 型
  (`Distributions` の `Distribution`/`VariateForm`/… )が `program.abstract_types`
  に集約されておらず、コンパイラの abstract 型レジストリ
  (`abstract_type_parents` / `abstract_type_names` / struct 階層)に届いていな
  かった。`collect_module_structs` / `collect_module_primitive_types` は存在する
  のに **`collect_module_abstract_types` が存在しなかった**。結果、
  `median(d::Distribution)` の `Distribution` 注釈が `AbstractUser` ではなく
  具象 `Struct("Distribution")` のまま残り、どの具象分布値にもマッチせず
  untyped `Statistics.median(arr)` に流れて `length(::Bernoulli)` で落ちていた。
  `std(d::Distribution)` が「動いて見えた」のは、`std=sqrt(var(d))` の内側
  `var(d)` が具象 `var(d::Bernoulli)` に到達できたため(偶然の迂回)。
- **2 つ目の根因 (#7263)**: パラメトリック package struct の値型は組み込み
  family でないため `CoreType::Named` として像化される。モジュール内呼び出し
  (`mean(d::Categorical)` 本体の `ncategories(d)`)は bare `Named("Categorical")`
  を、メソッド側の引数はモジュール修飾された `Named("Distributions.Categorical")`
  を持ち、`core_match` の `Named(expected)` アームが `actual` を `Struct` 前提と
  していたため **`Named` 対 `Named`(bare↔修飾)が一切マッチしなかった**。
- **修正**:
  1. `compile/mod.rs` に `collect_module_abstract_types` を追加し、
     `pipeline_ctx.rs` の abstract 型収集 (`abstract_types` / `abstract_type_names`
     / `abstract_type_parents`) にモジュール abstract 型を bare 名で合流。
  2. `dispatch_resolver/core_match.rs` の `CoreType::Named(expected)` アームに
     `Named(actual)` 対応を追加(`strip_module_prefix` で family 名比較)。
- これにより `Categorical` を upstream 準拠の **パラメトリック**
  `Categorical{T<:Real}` / `p::Vector{T}` に戻し、`var`/`mean`/`mode`/`quantile`/
  `median`/`std`/`ncategories` が型付きメソッドへ正しくディスパッチする。
  `Categorical(k::Integer)` だけは別不具合 #7266(コンストラクタ内
  Vector→`::Integer` loose-match)を踏むため、パラメトリックデフォルトコンス
  トラクタ `Categorical{Float64}(v)` を直接呼ぶ回避を残置。
- fixtures: `distributions_median_dispatch`(#7265)、
  `distributions_categorical_parametric`(#7263)を追加。両方 upstream julia +
  installed Distributions.jl とパリティ確認。実装中に別不具合
  **#7284**(整数引数の `Normal(2,3)` がコンパイル時に `Normal{Int64}` と誤推論
  → 実行時 `Normal{Float64}` で `expected I64, got Float64`)を発見・起票。

### Distributions.Categorical 追加 (Issue #7260)

- バンドル `Distributions` サブセットの離散分布に `Categorical` を追加
  (`packages/Distributions/src/univariate/discrete.jl`)。`Categorical(p)`(確率
  ベクトル)と `Categorical(k::Integer)`(`1:k` 上の一様)をサポートし、
  `params`/`probs`/`ncategories`/`support`/`mean`/`var`/`mode`/`entropy`/
  `minimum`/`maximum`/`pdf`/`cdf`/`quantile`/`rand` を実装。出力値は upstream
  Distributions.jl と一致(`distributions_discrete` fixture に検証を追加)。
- iOS サンプル `Barnsley Fern` を改修: 累積確率 + `findfirst` での写像選択を
  `Categorical([0.01,0.85,0.07,0.07])` + `rand(picker)` に置き換え(`.jl` 本体・
  `samples.json`・Swift フォールバックを同期)。
- 実装中に判明した VM ディスパッチ不具合を分離して issue 化:
  - **#7263**: パラメトリック `Vector{T}` フィールド struct で、`Statistics.var`
    を拡張した型付きメソッドが untyped generic に負ける(`mean` は勝つ)。
    回避策として `Categorical` は `MvNormal` 同様の非パラメトリック/untyped
    フィールド設計にした。
  - **#7266**: `::Integer` コンストラクタ内から同型の `::AbstractVector` コンス
    トラクタを呼ぶと、ベクトル引数が `::Integer` 側に loose-match する
    (トップレベルでは正しく解決)。回避策として 2 引数デフォルトコンストラクタ
    を直接呼ぶ。
  - **#7265**: `median(d::Distribution)`(抽象注釈)が全分布で `Statistics.median`
    に流れて落ちる(同じ注釈の `std` は動く)。本 PR の範囲外(既存不具合)。

### JSXGraph.jl 統合 backend (Issue #6357)

- **バンドルパッケージ `JSXGraph` 追加**: `packages/JSXGraph/` に
  `Board`/`JSXElement` 型と `board`/`point`/`line`/`segment`/`circle`/
  `polygon`/`text`/`functiongraph`/`push!`/`html` API を Pure Julia で実装。
  `Board` は immutable struct 内の可変 `Vector{Any}` で要素を保持し、
  `push!(b, elems...)` で追加する。
- **artifact 生成**: `src/plotting/jsxgraph.rs` が `Board` 値を検出して
  `application/vnd.jsxgraph+json` を出力。JSON は `options`（board 属性）と
  `elements`（`id`/`type`/`parents`/`attrs`）からなる。要素参照は `{"ref": id}`。
- **パッケージ登録・補完**: `src/julia/packages/mod.rs` と
  `src/repl/completions.rs` に JSXGraph を追加。
- **テスト**: fixture tests 4 件と Rust MIME unit test 1 件を追加。
- **制限**: 配列リテラル内の位置引数 splat (`Any[pts...]`) は lowering 時に
  `unsupported expression: splat_expression` となるため、`polygon(pts...)` は
  `collect(Any, pts)` を回避策として使用（Issue #7255）。
  iOS/Web frontend 描画分岐も未対応。

### Apollonian gasket サンプル追加 (JSXGraph, Issue #6357)

- **iOS サンプル `apollonian_gasket.jl` 追加**: Descartes Circle Theorem の
  線形 swap `b₄′ = 2(b₁+b₂+b₃) − b₄`（中心は複素数 `bz = b·z` で同形に更新）を
  再帰適用してアポロニウスのガスケットを生成し、`JSXGraph` の `circle` で描画。
  根四つ組 `(−1, 2, 2, 3)`、`maxbend=120` で 217 円。AMS Feature Column
  (D. Austin, 2006-03) を典拠とする。
- **座標タプル中心の circle**: 円の中心を point 要素ではなく座標タプル
  `(x, y)` で渡す経路を追加で検証。`value_to_jsx_parent` が `Value::Tuple` を
  JSON 配列にシリアライズし、`parents == [[x,y], r]` となる。
- **登録**: `Resources/Samples/intermediate/apollonian_gasket.jl` +
  `samples.json` + Swift フォールバック (`CodeSamples+Intermediate.swift`)。
- **テスト**: fixture `packages_jsxgraph_apollonian` と MIME unit test
  `test_jsxgraph_circle_with_coordinate_center_emits_array_parent` を追加。

### Distributions.jl サポート Phase 1.5〜5 (Issue #7178)

- **Phase 1.5 — 明示 RNG 配列サンプリング (Issue #7227)**: `rand(rng)` /
  `rand(rng, dims...)` / `rand(rng, Int, dims...)` / `randn(rng, dims...)` が
  壊れていた(`rand(rng)` は RNG を配列次元と誤認して `Cannot convert Rng to
  I64`、`randn(rng, dims...)` は dims を無視してスカラーを返却)。コンパイラ
  (`compile/expr/builtin.rs`)で第1引数の `ValueType::Rng` を検出して
  `RngRand{,n}ArrayF64`/`RngRandArrayI64` を emit し、進んだ RNG 状態を
  `store_rng_back` で書き戻すよう修正。VM(`vm/exec/rng.rs`)に同命令を実装。
- **Phase 2 — 一変量連続分布**: バンドルパッケージ `Distributions` を追加。
  型階層(`VariateForm`/`ValueSupport`/`Distribution{F,S}`)と共通 API
  (`pdf`/`logpdf`/`cdf`/`quantile`/`mean`/`var`/`std`/`mode`/`entropy`/
  `insupport` ほか)、連続分布 `Normal`/`Uniform`/`Exponential`/`Gamma`/`Beta`/
  `Cauchy`/`LogNormal`/`Weibull` を純 Julia で実装。`SpecialFunctions` に
  `gamma_inc`(正則化下側不完全ガンマ)を実装し、`erf`/`erfc` を
  `erf(x)=P(1/2,x²)` 経由で ~1e-13 精度に改善。サンプリングはグローバル RNG
  (`Random.seed!` で決定化)。フィクスチャ:
  `tests/fixtures/distributions/`。
- **Phase 3 — 一変量離散分布**: `Bernoulli`/`Binomial`/`Poisson`/`Geometric`/
  `DiscreteUniform` を追加。pmf(`pdf`)・`cdf`(Binomial は `beta_inc`、
  Poisson は `gamma_inc` 経由)・scan ベース `quantile`・`succprob`/`failprob`/
  `ntrials`/`span` 等。サンプリングは Knuth(Poisson)/ベルヌーイ列(Binomial)。
- **Phase 4 — 多変量正規分布 MvNormal**: `MvNormal(μ, Σ)`(`mean`/`cov`/`var`/
  `pdf`/`logpdf`/`insupport`/`dim`/`rand`)。`using LinearAlgebra` がモジュール
  内で失敗(#7245)・モジュール跨ぎ inner constructor 不可のため、Cholesky 下
  三角分解+前進代入を純 Julia で内製し、外側コンストラクタ(arity 差)で
  precompute。サンプリングは μ + L·randn(k)。
- **Phase 5 — MLE フィッティング**: `fit`/`fit_mle` を `Normal`/`Bernoulli`/
  `Exponential`/`Poisson`/`Geometric`/`Uniform`/`MvNormal` に実装。`::Type{T}`
  ディスパッチがコンストラクタ付きパラメトリック型に効かない(#7247)ため、
  `D === Normal` 等の型同一性分岐で実装。
- **派生して判明したバグ/制限**(別 Issue 化): `Random.default_rng()` 欠如
  (#7230)、rng 型引数のユーザ関数透過不可(#7231)、パラメトリック抽象型の
  ディスパッチ不具合(const エイリアス継承・パラメトリック抽象スーパー
  タイプ・モジュール跨ぎ抽象ディスパッチ)(#7235)、型パラメータ括弧内の関数
  呼び出し `T{typeof(f(x))}(...)` 不可(#7240)、`using LinearAlgebra` がユーザ
  モジュール内で失敗(#7245)、コンストラクタ付きパラメトリック型に対する
  `::Type{T}` ディスパッチ不可(#7247)。単項マイナスと `^` の
  優先順位(`-x^2`)は本家マージで #7232 として修正済みのため、Weibull の
  明示括弧ワークアラウンドは解消済み。

### 単項マイナスが冪乗 `^`/`.^` より強く結合していた (Issue #7232)

- `-x^2` が `(-x)^2` とパースされ、本家の `-(x^2)` と逆符号になっていた。
  `-2^2` も sjulia `4` / 本家 `-4`、`-v .^ 2` も sjulia `[1,4,9]` / 本家
  `[-1,-4,-9]`。実害としてガウシアン `exp.(-(x .- t) .^ 2)` が減衰せず発散していた。
- 原因は `subset_julia_vm_parser` の Pratt パーサが単項オペランドに後続 `^` を
  吸収せず、先に単項を包んでから `^` を適用していたこと(Julia では `^` が前置
  単項より強く結合: `julia/src/julia-parser.scm` "-2^3 is parsed as -(2^3)")。
- 修正: `absorb_power_into_unary_operand` で単項オペランド直後の Power 精度演算子を
  右結合で折り込む。詳細・テストは [DONE.md](./DONE.md) を参照。

### `let` ブロック内の `@test` マクロが展開されない (Issue #7189)

- `using Test; let a = 1; @test a == 1 end` のように `let` ブロック本体へ
  `@test`(および `@testset`/`@test_throws`/`@test_broken`)を書くと、`using Test`
  済みでも `UnsupportedFeature { MacroCall, hint: "@test macro requires \`using Test\`" }`
  で lowering に失敗していた。`begin`/`for`/関数本体/`@testset` 内では動作し、
  `@show` 等は `let` 内でも展開できていたため、`let` × Test 系マクロ固有の不具合。
- 原因: `lower_expr_with_ctx`(コンテキスト付き式 lowering)の `let` 分岐が
  ctx 非伝播の `lower_let_expr` を呼んでおり、本体を lowering する際に `using`
  集合を持つ `LambdaContext` が失われていた。Test 系マクロは context 経由で
  `using Test` を確認するため、誤った "requires `using Test`" エラーになっていた。
- 修正: `let` 分岐を `lower_let_expr_with_ctx` に切り替えて lambda context を
  本体へ伝播(実装済みだが未配線だった `_with_ctx` ヘルパを有効化)。
- 回帰テスト: `subset_julia_vm/tests/fixtures/macros/test_macro_in_let_7189.jl`。

### 内包表記引数が `::Integer` メソッドに loose-match する不具合を修正 (Issue #7266)

- `Foo(p::AbstractVector{<:Real})` と `Foo(k::Integer)` の 2 メソッドを持つ型で、
  `Foo(k::Integer) = Foo([1.0/k for _ in 1:k])` のように内包表記を直接引数に渡すと、
  そのベクトル引数が誤って `::Integer` メソッドに dispatch され
  「Type error: expected numeric value, got Array」になっていた。同じ呼び出しでも
  配列リテラルを渡したトップレベル版(`Foo([0.25,0.25,0.25])`)は正しく動くため
  「呼び出しコンテキスト依存」に見えたが、真因は内包表記固有。
- 真因 (#5966 系の loose-abstract-annotation): 内包表記は要素型が静的に不明なため
  `infer_julia_type` で要素なしの `JuliaType::Struct("Vector")` と推論される
  (`Vector`/`Matrix`/`Array` のベア別名)。ディスパッチの struct-parents フォール
  バック `struct_is_subtype_of_abstract` は、ユーザ宣言階層に無い `Vector` を
  「未知の struct → 保守的に accept」してしまい `Vector <: Integer` を真と判定。
  結果、内包表記引数が `::Integer` メソッドに一致していた(コア matcher
  `core_dispatch_pattern_matches` 自体は false を返しており、フォールバックが原因)。
- 修正:
  1. `struct_is_subtype_of_abstract`(`compile/method_table.rs`): ユーザ宣言階層に
     無い名前でも `Vector`/`Matrix`/`Array`/`Dict`/`Set`/range 系などの**組み込み
     struct ファミリ**は既知の組み込み上位型チェーン(`Vector → DenseArray →
     AbstractArray → Any`)を辿って判定し、保守的 accept を回避。本当に未知の名前
     (未登録のユーザ抽象型)のみ従来どおり保守的 accept。
  2. `is_rank_unknown_array_julia_type`(`compile/expr/call/mod.rs`)を `Vector`/
     `Matrix` のベアファミリにも拡張し、要素型不明の単一配列引数で静的一致が無い
     場合は **runtime dispatch** にルーティング(`compile/expr/call/dispatch.rs` の
     `NoMethodFound` 単一引数アーム追加)。実行時の具体値 `Vector{Float64}` が
     `::AbstractVector{<:Real}` を正しく選ぶ。`::Integer` のみのメソッドには
     upstream どおり MethodError。
- 回帰テスト: `subset_julia_vm/tests/fixtures/dispatch/comprehension_arg_abstract_array_7266.jl`
  (julia 1.12 と 8/8 一致)、`method_table.rs` の
  `builtin_struct_family_does_not_loose_match_scalar_abstract_issue_7266` ユニット
  テスト。バンドル `Distributions.Categorical(k::Integer)` の回避策(2 引数 ctor
  直接構築)を自然形 `Categorical([1.0/k for _ in 1:k])` に戻した。

## 最新対応 (2026-06-20)

### Symbolics 微分のコンパイル推論爆発(~7–17 秒)を解消 (Issue #7215)

- `Differential(x)(cos(x))` / `using Symbolics` の初回コンパイルが ~7–17 秒かかっていた。`SJULIA_COMPILE_PROFILE=1` で計測すると `compile.method_table_setup` 内の `inference_engine.infer_function`(特に `Symbolics._apply_diff`)がほぼ全てを占める。
- 原因: 抽象解釈エンジンの**呼び出し側補間推論**が、呼び出し先に宣言済み戻り値型(`f(...)::T`)があってもボディを毎回再展開していた。相互再帰する `_deriv ⇄ _deriv_*` family では tentative cycle 結果が outer fixpoint iteration ごとに破棄され long-lived cache に届かないため、同じ `(callee, arg_types)` の解析が `depth × iterations × branching` 回繰り返され組合せ爆発する(これが「注釈を付けても改善しない」と Issue で報告された理由)。
- 修正(両面): (1) コンパイラ — `abstract_interp/engine` の補間推論で、呼び出し先が戻り値型を宣言していればボディ再展開せずその型へ短絡(Julia の `convert(T,…)::T` 保証により健全。top-level `infer_function` の挙動と一致)。(2) Symbolics — 再帰ハブ `_deriv(node, x)` に `::Any` 注釈(`convert(Any,…)` は恒等で実行時無変更。`_apply_diff = Num(_deriv(…))` は `Num` 精度を維持)。
- 結果: `Differential(x)(cos(x))` 初回実行 7.18s → **0.28s**(`method_table_setup` 6967ms → 95ms)。受け入れ条件「初回 1 秒未満」を達成。
- テスト: unit `interprocedural::test_issue_7215_declared_return_type_short_circuits_call_site`(宣言 `::Float64` がボディ推論 `Int64` を呼び出し側で上書きすることを検証)。既存 fixture `packages/symbolics_derivative.jl` が実行時挙動を維持。
### モジュール内クロージャから module-private ヘルパへの名前解決 (Issue #7180)

- `module M; help(a,b)=a==b; find2(v)=findfirst(x->help(x,2), v); end` のように、
  module 内の Base HOF(`findfirst`/`reduce`/`sort(by=)` 等)へ渡したクロージャ
  /関数値から module-level ヘルパを参照すると
  `function 'help' is not imported` で失敗していた。直接呼び出しやトップレベルでは動作。
- 原因: `compile/pipeline_ctx.rs` の `all_functions` 構築で、module 関数本体から
  lift された inline/nested 関数(クロージャ)が常に `module_path = None` で登録され、
  その関数の `function_imports` に module の関数集合(`help` 等)が含まれなかった。
- 修正: module 関数名→module_path のマップを作り、inline 関数の親が module 関数なら
  その module_path を継承させる。これにより lift されたクロージャが module スコープで
  名前解決される。
- テスト: fixture `modules/module_closure_hof_helper_7180.jl`(julia と 3/3 パリティ:
  `findfirst` クロージャ・`reduce` 関数値・`sort(by=)` 関数値)。

### モジュール内 callable struct (functor) のディスパッチ (Issue #7185)

- モジュール内で定義した `(obj::T)(args...)`(callable struct/functor)が呼べず `Function '__callable_M.Foo' not found` で失敗していた。トップレベルでは動作。
- 原因: `vm/exec/call_function_variable.rs` の `callable_method_name` がモジュール修飾名(`M.Foo{Int}`)を `__callable_M.Foo` として登録名と不一致にしていた。`{` で head を取った後 `rsplit('.')` で module 接頭辞を落とし `__callable_<bare>` に解決するよう修正(#7171/#7172 の show 名整合と同方針)。
- テスト: fixture `module/module_callable_struct_7185.jl`(julia 1.12.6 と 7/7 パリティ: 内部/外部呼び出し・parametric `Scale{T}`・匿名 functor・converting ctor)。
### Broadcasted unary minus on array values (Issue #7212)

- `-A` / `Base.:-(A)` where `A` is an Array now compiles through the existing
  `materialize(Broadcasted(-, (A,)))` path, matching upstream Julia's
  `-(A::AbstractArray) = broadcast_preserving_zero_d(-, A)` shape.
- `DynamicNeg` now also maps array-like runtime values elementwise, so unary
  minus applied to nested broadcast results such as `-((x .- t) .^ 2)` no longer
  fails when the materialized value is carried as `Array{Any, Any}`.
- Fixture `broadcast_unary_minus_array_7212` covers Float64/Int64 arrays,
  broadcast-result negation, and the `Base.:-` qualified unary-call form.

### Context-aware `let` lowering re-export build fix (Issue #7218)

- Current `main` re-exported and called `lower_let_expr_with_ctx` from expression
  lowering, but `misc.rs` only defined the context-free `lower_let_expr`, so the
  release build failed with unresolved import `misc::lower_let_expr_with_ctx`.
- Added the context-aware `let` lowering entry point and propagated
  `LambdaContext` through let bindings, block statements, and single-expression
  let bodies, matching the existing call-site intent for Test macro lowering.

### 行列/hcat/vcat リテラル中の配列・範囲要素のフラット化 (Issue #7203)

- 行列リテラルの行要素が**スカラーではなく配列や範囲**のとき、本家 Julia のように列/行方向へ
  フラット化されず、`Any` 行のボックス化要素として残っていた(または行列×行列 hcat でクラッシュ)。
  例: `g=[1 2 3]` で `[g 4]` が `Any[[1 2 3] 4]`、`[1:2 3:4]` が `Any[1:2 3:4]`、`[[1 2] [3 4]]` が
  `BoundsError`。`#7196`(パーサ空白修正)とは独立した、連結セマンティクスのギャップ。
- 原因: 行列リテラルの lowering(`lowering/expr/collection.rs`)が要素を直接 `ArrayLiteral` の
  固定 shape に配置しており、`hcat`/`vcat`/`hvcat` を経由しなかった。全要素がスカラーのときは正しいが、
  配列/範囲要素はそのままボックス化される。`[[1 2] [3 4]]` は tree-sitter が `typed_expression`
  (2 つの行列を「型」位置でグルーピング)として誤パースし、最初の行列を型として添字付け→`BoundsError`。
- 修正(2 層):
  - **lowering**: 行要素のいずれかがランタイムで配列/範囲になり得る場合のみ、`[a b c]`→`hcat`、
    各行 1 要素の `[a; b; c]`→`vcat`、複数列の `[A B; C D]`→`hvcat((c1,c2,...), ...)` へ振り分ける。
    全スカラーのリテラルは従来の `ArrayLiteral` 高速パスを維持。`[[1 2] [3 4]]` / 3 個以上の
    `typed_expression` 誤パースは `hcat` へ復元(外側 `[...]` の 1 要素ボックス化も解除)。型付き行列
    `T[...]` は `lower_matrix_expr_raw` で従来どおりフラット要素列を生成(連結振り分けを回避)。
  - **base** (`julia/base/array.jl`): スカラー/範囲/ベクトル/行列を一様にブロック形状
    (スカラー=1×1, ベクトル/範囲=n×1, 行列=size)で扱う `_block_hcat`/`_block_vcat` と `hvcat` を追加。
    既存の 1 次元ベクトル型保存高速パス(#3588)はそのまま維持し、汎用フォールバックのみブロック対応へ。
- 検証: フィクスチャ `arrays/hcat_vcat_flatten_elements_7203.jl`(julia 1.12.6 と sjulia で 28/28 パリティ
  一致)。コンマ形 `[[1,2] [3,4]]`(`index_expression` 誤パースで本来の添字付けと曖昧)はパーサ層の
  別問題として未対応。全スカラー `[1;2;3]` が `Vector` でなく 3×1 行列になる既存挙動は本 PR の範囲外。

### インライン `Dict(...)[key]` が `<: Real` 構造体キーで数値添字へ誤ルーティング (Issue #7173)

- 変数に束縛せず生成した `Dict(...)` をそのまま添字アクセスし、キーがモジュール修飾された
  ユーザ構造体（`<: Real`、例: `Symbolics.Num` や `Dict{M.R, Int64}`）の場合、`getindex` が
  numeric array-index 経路（`IndexLoad`）に落ちて `Type error: expected I64 or CartesianIndex, got M.R`
  で失敗していた（束縛してからの `d[key]` は正常）。
- 根本原因: `is_dict_struct_name`（`compile/expr/mod.rs`・`compile/stmt.rs` の2か所）が
  モジュール接頭辞を剥がすのにパラメトリック名全体へ `rsplit('.')` を適用していたため、型パラメータ内に
  ドットを含む `Dict{M.R, Int64}` が `R, Int64}` に誤分割され Dict 判定に失敗。インライン `Dict(...)` の
  レシーバが compile 時に Dict と認識されず、`<: Real` キーで数値添字フォールバックが選ばれていた。
- 修正: 先に `{` で base を切り出してからモジュール接頭辞を `rsplit('.')` で剥がす（`Base.Dict` → `Dict`）。
  これで束縛/インラインのどちらも `getindex`（`CallSpecialize`）へディスパッチする。
- fixture `dict_inline_dict_real_struct_key_7173`（依存のないモジュール内 `<: Real` 構造体で再現）。
  通常の数値配列添字・インライン Int/String キー Dict 添字は不変。upstream julia 1.12.6 とパリティ。
### マクロが注入した `QuoteNode(:sym)` の `::Symbol` フィールド代入 (Issue #7163)

- マクロが `QuoteNode(x)` でシンボルリテラルを生成コードへ差し込み、それが `::Symbol` 型フィールドを
  持つ構造体コンストラクタや `::Symbol` 仮引数に渡ると、コンパイル時に `Cannot convert Any to Symbol`
  で失敗していた。値自体は本物の `Symbol`(未型付けフィールドや `x.field === :name` は成立)。
- 原因: マクロ展開後の `Literal::Symbol` は `compile_expr` で `PushSymbol`(本物の `Value::Symbol`)を
  emit しつつ、静的型を `ValueType::Any` と返していた。一方ソース直書きの `:sym` は
  `QuoteLiteral(SymbolNew)` 経路で既に `ValueType::Symbol` を返すため `Named(:a)` は通っていた。
  コンストラクタのフィールド coercion が `actual=Any`/`target=Symbol` を見て、対応する受理アームが無く
  エラーになっていた。
- 修正: `compile/expr/mod.rs` の `Literal::Symbol` アームを `ValueType::Symbol` を返すよう変更
  (emit 内容は不変)。型推論関数 (`infer_expr_type`/`literal_rhs_value_type`/`infer_default_type`) と整合。
- 検証: 上流 `julia` 1.12.6 とバイト一致(`Named(:alpha)`/`alpha`/`true`/`Symbol`/...)。
  fixture `macros/quotenode_symbol_typed_field_7163.jl`。

### Macro `Expr(head, args...)` splat in no-context lowering (Issue #7162)

- Macro definition bodies are lowered through the no-context call lowering path. That path mapped `Expr`
  to `BuiltinOp::ExprNew` but dropped the positional `splat_mask`, so `Expr(:vect, names...)` kept the
  macro-local Vector as one AST argument instead of expanding its elements.
- The no-context path now wraps splatted `Expr` constructor arguments in the same `SplatInterpolation`
  marker used by the context-aware path. `ExprNewWithSplat` then expands tuple/native-array/Array-wrapper
  values at macro expansion time.
- Fixture `macros_expr_splat_macro_7162` covers a macro-local Vector of escaped symbols splatted into
  `Expr(:vect, names...)`, matching upstream Julia's `[7, 8]` expansion result.

### AoT generic `::Any` method dispatcher integration (Issue #7158)

- AoT IR converter が同名 overload の型情報を毎回 `TypedProgram` の先頭 signature から取っていたため、
  `pick(::Int64, ::Any)` / `pick(::Any, ::Int64)` のような generic overload set が同一 signature に潰れ、
  Rust backend の generated dispatcher に載らない問題を修正した。
- converter は関数名ごとの method occurrence を追跡し、Core IR の各 method 定義に対応する typed signature を選ぶ。
  codegen は single-method call でも method table に登録された user function なら `resolve_dispatch` を通すため、
  `only_string(1)` のような no-method が invalid Rust call ではなく AoT diagnostic になる。
- E2E regression は `::Any` overload set が `pick_i64_any` / `pick_any_i64` と runtime dispatcher を生成すること、
  `pick(1, 2)` を ambiguous として拒否すること、single-method no-method call を拒否することを固定する。

### Plots plot(p::Plot) copy semantics (Issue #7149)

- bundled Plots の `plot(p::Plot)` は、source plot の `series` 配列を current plot と戻り値にそのまま共有していたため、
  直後の `plot!` / `scatter!` が保存済みの `p.series` にも追記されていた。
- `plot(p::Plot)` は `Series` と x/y/z データを snapshot 化した独立 series list を current に登録する。`frame(anim, plt)`
  も同じ series-copy helper を使うようにし、animation snapshot と replot snapshot のコピー規則を揃えた。
- fixture `packages_plots_existing_plot_7026` は `plot!` 追記と `push!(plt, i, y)` のどちらでも元 plot に波及しないことを検証する。
### 再帰コンストラクタ walker の load 時 PartialStruct 推論ハング (Issue #7186)

- `using Symbolics` が、再帰 `_deriv(node, x)` に「第2引数 (`b`) で再帰しつつ複数の `_mk*`
  ヘルパをネストした一般冪則 (`(a^b)' = a^b·(b'·log a + b·a'/a)`)」分岐を持つだけで load 時推論が
  終わらなくなっていた(関数は実行されず、定義/推論時にハング)。
- 根本原因: PartialStruct-return 推論 (`infer_function_partial_struct_return`) に①再入ガードと
  ②負キャッシュが無かった。コンストラクタ引数 (`Term(:*, ...)`) の精密な struct 形状を復元する際に
  callee 本体を都度再解析し、各 `_mk*` が `Union{Number, Term}` を返す(=クリーンな partial 無し→
  `None`)ため結果がキャッシュされず、ネスト呼び出しサイト毎に深さ上限(10)まで指数的に再解析していた
  (`infer_block_with_fixpoint` が 30 秒で 60 万回超、`infer_expr` 3,600 万回超)。通常の return-type
  経路にある in-flight ガード (`analyzing_functions`) が partial 経路には無かった。
- 修正(`compile/abstract_interp/engine/mod.rs`): (1) `analyzing_partial_structs` 再入ガード(再入は
  `None`=保守的 widening を返すだけで健全)、(2) `CachedConstructorPartial.partial` を
  `Option<ConstructorPartial>` 化して**負の結果も world-stamp 付きでキャッシュ**。PartialStruct は精度の
  最適化に過ぎず、`None` は通常の推論型へ widening されるだけなので負キャッシュは常に健全。
- これにより workaround で外していた一般冪則を `_deriv_genpow` として復活(`x^x`/`2^x` の導関数)。
- 検証: `using Symbolics` が即時 load(以前 >35s でハング)、`derivative(x^x, x)`/`derivative(2^x, x)`
  が数値一致、unit test `test_recursive_constructor_walker_partial_struct_terminates_7186`、
  fixture `packages/symbolics_derivative.jl`、フルスイート。

### Cranelift float comparison NaN parity (Issue #7124)

- Cranelift Float64 comparisons use `FloatCC::Equal` / `NotEqual` / ordered comparison predicates. JIT regression で
  `NaN == NaN == false`、`NaN != NaN == true`、`NaN < 1.0` / `<=` / `>` / `>=` が false になることを固定した。
- comparison predicate result は `bmask & 1` で `I8` 0/1 に正規化する。これにより `Bool` return signature へ
  float comparison の predicate value を直接返して verifier error になる経路も防ぐ。
- normal comparison の sanity check も同じ regression に含め、NaN parity のために通常比較を壊していないことを確認する。
- 検証: `cranelift_float_comparison_nan_semantics_issue_7124`、Cranelift release nextest。

### 行列リテラルの空白依存 `-`/`+` 要素分割 (Issue #7196)

- `[0.20 -0.26; 0.23 0.22]` のような行列/`hcat` 行で、パーサが `0.20 -0.26` を二項減算
  `0.20 - 0.26` と解釈し、列数不一致で lowering の `MalformedMatrix`("inconsistent column
  count: expected 1, got 2")になっていた。
- 上流 Julia の規則に合わせて修正: 行列行の中では、**前に空白があり後ろに空白が無い** `-`/`+`
  は新しい(単項符号付き)要素を開始する。`[1 -2]` は2要素、`[1 - 2]`(両側に空白)は二項減算で
  1要素、`[1+2]`(空白無し)は二項、`[1 *2]`(`*` は対象外)は `1*2`。カンマ配列 `[1, -2]` や
  通常の減算 `x - y`・呼び出し引数 `f(1 -2)` は影響を受けない。
- 実装: パーサに `in_matrix_row` フラグを追加(`macro_arg_space_sensitive` と同じスコープ規則で、
  `(...)`/`[...]`/`{...}`/呼び出し・添字の引数に入ると解除)。`parse_expression_with_precedence`
  で `in_matrix_row` かつ `+`/`-` かつ「前に空白・後ろに空白無し」のとき二項演算子化を中断する
  (空白判定は span の隙間と新規 `peek_next_start` で行う)。行列行・型付き行列(`T[...]`)の各
  要素ループと最初の要素に対してフラグを設定。
- テスト: フィクスチャ `arrays/matrix_literal_negative_element_7196.jl`(julia と sjulia で 41/41
  パリティ一致)、パーサ corpus テスト 4 件(列数を直接検証)。

### Cranelift numeric conversion parity gate (Issue #7123)

- Cranelift low-level IR にはまだ Rust backend と同等の numeric conversion instruction がなく、float→int は
  Julia の `InexactError` / range check、int→float は signed/unsigned carrier と rounding policy を揃える必要がある。
- そのため non-identity `AotExpr::Convert` と `sitofp` / `fptosi` builtin は、placeholder や Rust `as` 相当の
  unchecked lowering ではなく `Issue #7123` diagnostic として止める。
- 既存の low-level `TypeAssert` conversion gate は #7111 に加えて #7123 も示し、runtime check と numeric
  conversion parity の両方に属する境界として扱う。
- 検証: `cranelift_numeric_conversions_are_gated_issue_7123` /
  `cranelift_typeassert_conversion_requires_runtime_check_issue_7111`、Cranelift release nextest。

### Cranelift display runtime parity gate (Issue #7121)

- Cranelift は現時点で VM/Rust backend の Julia display formatter や `show(io, x)` dispatch runtime に接続していない。
  そのため `print` / `println` / `string` は Float64 whole-value suffix、`Inf`/`NaN`、user `show` などの parity を
  保証できるまで compile-time diagnostic として止める。
- これにより Cranelift generated binary が Rust の default `Display` 経由で `1` などを出す silent mismatch を防ぐ。
- 検証: `cranelift_display_builtins_are_gated_issue_7121`、Cranelift release nextest。

### REPL/FFI 結果エコーを user `show` 経由に (Issue #7168)

- 対話 REPL の結果エコーと iOS/Web FFI の結果表示は Rust の `format_value_*` フォーマッタで user
  struct を struct dump していた(`string`/`print`/`println`/`show(io,·)` は user `show` 経由なのに不一致)。
  Symbolics の `x^2+2x+1` が `Symbolics.Num(Symbolics.Term(...))` と表示される問題。
- 修正: VM に `render_value_via_user_show(value)` を追加。eval 後に `user_show_method_for` で登録済み
  show を引き、`start_sprint_call` + 既存の再入ドライバ `run_until_frame_return` で show を実行して
  文字列を得る。`REPLSession::eval` がこれを `REPLResult.value_display` に載せ、CLI(`format_value_with_vm`)
  と FFI(`format_value_with_struct_heap`)がそれを優先する。
- 除外ガード: Complex/Rational/LinRange/array-wrapper は専用 Rust フォーマッタ(上流正準形)を維持する
  ため `value_display=None`(LinRange の `show` は struct 形で `a:step:b` にならない回帰を回避)。`repr` は
  別経路で今回未対応。
- テスト: `repl::tests::test_repl_value_display_uses_user_show_7168`(Symbolics/Complex/LinRange/Int/
  show-less struct を網羅)。フルスイート 3889 green。

### Cranelift integer division/remainder parity gates (Issue #7119)

- Low-level Cranelift `BinOpKind::Div` / `Rem` は integer zero divisor を明示 `TrapCode::INTEGER_DIVISION_BY_ZERO`
  にし、signed carrier では `sdiv`/`srem` の truncated semantics を使う。`(-5,3)` / `(5,-3)` /
  `(-5,-3)` で Julia `div` / `rem` / `%` と一致することを確認した。
- Unsigned carrier は `udiv`/`urem` に分岐し、`UInt64::MAX / 2` など high-bit 値で signed 解釈に落ちない。
- `mod` / `fld` / `cld` / builtin `div` / `rem` は AoT `CallBuiltin` として来るため、Cranelift adapter は
  floored/ceiled/divisor-sign semantics の実装まで `Issue #7119` diagnostic で gate する。
- 検証: `cranelift_signed_integer_div_rem_match_julia_issue_7119` /
  `cranelift_unsigned_integer_div_rem_use_unsigned_ops_issue_7119` /
  `cranelift_division_family_builtins_are_gated_issue_7119`、Cranelift release nextest。

### Cranelift nested break/continue target coverage (Issue #7116)

- Cranelift IR では `break` / `continue` は target block への `Branch` / `Jump` として表現される。continue は
  loop latch を通って induction variable を更新し、body skip 時も stale header state に戻らないことを確認した。
- nested loop の inner break は outer loop exit ではなく inner exit / outer latch へ分岐し、outer iteration を継続する。
  `nested_break_count(3, 5, 2) == 6` を JIT 実行で固定した。
- 検証: `cranelift_continue_targets_loop_latch_issue_7116` /
  `cranelift_nested_break_targets_inner_exit_issue_7116`、Cranelift release nextest。

### Cranelift switch coverage and type gate (Issue #7114)

- Cranelift `Terminator::Switch` lowering は chained `icmp` branch として実装される。case/default 直 return に加えて、
  empty case set が default へ jump する経路、Bool key、switch target から phi merge へ流れる block args を
  JIT regression で固定した。
- Float64 など非整数 key は Cranelift `icmp` へ渡さず、`Issue #7114` 付き unsupported diagnostic とする。
  float equality / NaN semantics は比較 parity の個別 issue で扱う。
- 検証: `cranelift_switch_empty_cases_jump_default_issue_7114` /
  `cranelift_switch_bool_key_issue_7114` / `cranelift_switch_targets_phi_merge_issue_7114` /
  `cranelift_switch_float_key_is_gated_issue_7114`、Cranelift release nextest。

### Cranelift phi placeholder removal (Issue #7113)

- Cranelift phi lowering は block parameter を唯一の SSA phi 実体として扱う。`Instruction::Phi` 実行時に
  destination が `compile_function_body` で block parameter へ map されていなければ、typed zero placeholder
  を作らず `FunctionCompilation` error にする。
- `get_phi_args` は phi destination を持つ target への edge で incoming が欠落している場合、または incoming
  個数が destination 個数と一致しない場合に明示 diagnostic を返す。
- 検証: `cranelift_entry_phi_without_block_param_is_rejected_issue_7113` /
  `cranelift_missing_phi_incoming_is_rejected_issue_7113`、既存 phi/back-edge regression、Cranelift release nextest。

### Symbolics サブセット: 微分(`Differential` / `derivative`) — 中核セット完成 (Issue #6572)

- `diff.jl` を追加。`derivative(expr, var)` が **eager** に微分(和/積/商/冪[x 非依存指数]/連鎖律 +
  初等関数 sin/cos/tan/exp/log/sqrt の導関数表)。`derivative(x^2, x)==2x`、
  `Differential(x)(sin(x))==cos(x)`、`derivative(x^2+sin(x),x)==2x+cos(x)` を実機確認。連鎖/商則は
  substitute 評価で検証。結果は非簡約(上流 `simplify=false` 既定と同じ)なので 2 階微分等は
  `simplify(derivative(...))` で collection。
- **これで Issue #6572 の中核セットが完成**: `@variables` / `Num`・`Sym`・`Term` / 四則・冪・初等関数 /
  `show` / `substitute` / `simplify`・`expand` / `Differential`+`derivative` が `using Symbolics` で動作。
- 実装中に subset-VM バグ 2 件を起票・回避: **#7185**(モジュール内 struct call operator が未ディスパッチ
  → `Differential(x)` はクロージャを返す設計に)、**#7186**(複雑な再帰関数の一般冪則分岐が VM ロード時
  コンパイルをハング → 各規則を小ヘルパに分割 + x 依存指数を除外)。
- テスト: fixture `packages/symbolics_derivative.jl`(manifest 登録、21 アサーション)。

### Cranelift CFG loop/back-edge coverage (Issue #7112)

- Cranelift `compile_function_body` は phi node を block parameter として作り、`Terminator::Jump` /
  `Branch` の lowering 時に predecessor ごとの incoming value を block args として渡す。この経路を
  loop header への back-edge、nested loop、複数 latch から同じ header へ戻る multi-back-edge で検証した。
- 追加した JIT regression は `sum_to_n`、nested loop iteration count、multi-back-edge count を実行し、
  loop header phi が entry 初期値と latch 更新値を正しく切り替えることを確認する。
- 検証: `cranelift_loop_backedge_phi_sums_i64_issue_7112` /
  `cranelift_nested_loop_backedge_phi_issue_7112` /
  `cranelift_multiple_backedges_phi_issue_7112`、Cranelift release nextest。

### Cranelift runtime-checked call/conversion gates (Issue #7111)

- Cranelift low-level `Instruction::Call` が未解決関数を typed placeholder `0` として生成していた経路を
  明示 unsupported diagnostic に変更した。`sqrt`/`log` など DomainError check が必要な runtime call を
  silently zero にしない。
- `TypeAssert` は source/destination/target 型が同じ場合だけ copy として扱い、`Float64 -> Int64` など
  InexactError check が必要な変換は `Issue #7111` 付き diagnostic で止める。
- 検証: `cranelift_unknown_runtime_checked_call_is_gated_issue_7111` /
  `cranelift_typeassert_conversion_requires_runtime_check_issue_7111`、Cranelift release nextest。

### Symbolics サブセット: `simplify` / `expand` (Issue #6572)

- `simplify.jl` を追加。`simplify` は単一ボトムアップパスで `+`/`*` flatten・定数畳み込み・同類項
  (`x+x→2x`, `2x+3x→5x`)/同因子(`x*x→x^2`, `x^2*x→x^3`)結合を行い、可換オペランドを**正準順序**
  (`_canonkey` 構造キーでソート)に並べて `x*y+y*x→2*x*y`、`(x+y)^2→x^2+2*x*y+y^2` まで結合する。
  `expand` は積/小整数冪(≤8)を分配してから `simplify`。本家ルールベース不動点の縮約版。
- 検証は **substitute 数値評価**(順序非依存): `expand((x+y)^2)` を `x=2,y=3` で評価して 25、
  `(x+y)^3` で 125 等を実機(sjulia)で確認。simplify の単一結果は正準形が決定的なので `==` で検証。
- **subset-VM モジュールスコープ制約を発見・回避**: モジュール private ヘルパ(`_structeq`/`_mkmul`/
  `_canonkey`)を Base HOF に関数値として渡す(`findfirst(lambda)`/`reduce(_mkmul,…)`/`sort(by=…)`)と
  `function '_structeq' is not imported` で失敗する。→ 明示ループ(`_findbase`/`_foldmul`/`_foldadd`/
  `sortperm`)で実装。直接呼び出しは別ファイルのヘルパでも解決される(**Issue #7180** 起票)。
- テスト: fixture `packages/symbolics_simplify_expand.jl`(manifest 登録、21 アサーション)。

### Cranelift integer overflow wrapping parity (Issue #7110)

- Cranelift の scalar integer `iadd` / `isub` / `imul` が Julia と同じ wrapping semantics を満たすことを
  JIT 実行 regression として固定した。
- `i64::MAX + 1`、`i64::MIN - 1`、`i64::MAX * 2` の結果を upstream Julia の
  `typemax(Int64)+1` / `typemin(Int64)-1` / `typemax(Int64)*2` と合わせて確認。
- 検証: `cranelift_integer_{add,sub,mul}_wraps_on_overflow_issue_7110`、upstream Julia smoke、
  Cranelift release nextest。

### Cranelift array indexing bounds metadata gate (Issue #7109)

- Cranelift low-level `Instruction::GetIndex` / `SetIndex` は array length / shape を持たず、Julia
  `BoundsError` parity に必要な上限チェックを生成できないため、unchecked load/store lowering を削除した。
- bounds metadata を持つ array carrier が実装されるまで、Cranelift は array indexing/mutation を
  `Issue #7109` 付き unsupported diagnostic として止める。高レベル adapter の配列型 gate と合わせ、
  unsafe な out-of-bounds JIT を生成しない。
- 検証: `cranelift_getindex_requires_bounds_metadata_issue_7109` /
  `cranelift_setindex_requires_bounds_metadata_issue_7109`、Cranelift release nextest。

### Symbolics サブセット: `substitute` + 構造的 hash + rebuild 共有化 (Issue #6572)

- `substitute.jl` を追加。`substitute(expr, dict)` / `substitute(expr, pair)` が記号変数を値で置換し、
  完全に数値化された部分式を畳み込む(`substitute(x^2+1, x=>3) == 10`、部分置換 `x+y, x=>3` は記号維持、
  記号値の置換 `x^2, x=>y+1` も可)。
- 置換後の再正規化のため `arithmetic.jl` に共有ヘルパ `_rebuild`(演算子+引数ベクトルから `_mk*` を再適用)・
  `_applyelem`/`_iselementary`(初等関数の数値畳み込み)を追加。既存の `Base.sin`…`sqrt` を `_applyelem`
  経由に DRY 化。これらは後続の `simplify`/`expand`/微分でも再利用する。
- `Base.hash(x::Num)` を**構造的**(`isequal`/`==` と整合)に定義。これにより `Num <: Real` を `Dict` の
  キーにでき、`d = Dict(x=>3); d[x]` / `haskey(d, x)` が動く(Step 2 で structural `isequal` のみ定義され
  hash が default だった潜在的不整合も解消)。
- 制約: `substitute` は Dict を**反復**するので `dict[key]` には依存しない。インライン連鎖
  `Dict(x=>3)[x]` は呼び出し側 Dict 型推論が効かず数値 getindex に誤ルートして落ちる(**Issue #7173** 起票)。
  → Dict を変数に束縛してからインデックスする。
- テスト: fixture `packages/symbolics_substitute.jl`(manifest 登録、18 アサーション)。

### Cranelift Bool result / Bool-as-integer operand parity (Issue #7100)

- Cranelift scalar binary op lowering が `Bool` を ABI 上の `I8` carrier として保持しつつ、
  mixed numeric/comparison では相手型または result 型へ zero-extend / float convert するようにした。
- これにより `Bool + Int64` と `Bool < Int64` の Cranelift verifier error を防ぎ、
  `Bool * Bool` は `Bool` result のまま実行できる。
- 検証: `cranelift_bool_*_issue_7100` JIT unit tests、`juliars --backend cranelift --minimal-prelude`
  smoke、`timeout 1800 cargo nextest run --release -p subset_julia_vm --features cranelift cranelift`。

### AoT type-unstable local Value boxing boundary (Issue #7075)

- type-unstable local (`x = 1; x = "s"`) の slot 型を RHS の最後の型へ潰さず、代入全体の join 型として収集するようにした。
  converter は typed locals の slot 型を env に投入し、最初の `let` と再代入の両方で同じ `Value` boundary を使う。
- これにより `let mut x: i64 = 1i64; x = "s".to_string();` のような invalid Rust を生成せず、
  `let mut x: Value = Value::from(1i64); x = Value::from("s".to_string());` を生成する。
- `typeof(x)` も multi-variant `Union` slot では static `Union{...}` 名ではなく runtime `Value::type_name()` を使うようにし、
  値が `"s"` なら `String` を返す。
- 検証: upstream Julia smoke、`juliars --minimal-prelude --emit-binary` sequential/branch reassignment smoke、
  `--pure-rust --check` runtime boundary diagnostic、AoT release nextest、`cargo check -p subset_julia_vm --features aot`。

### Symbolics サブセット: `show`(中置プリティプリント) (Issue #6572)

- `show.jl` を追加。`Sym`/`Term`/`Num` を演算子優先順位でカッコ付けしながら中置表示する
  (左結合は右オペランド、`^` は左オペランドを同優先順位でカッコ化)。`x^2 + 2*x + 1`、
  `x*(y + 1)`、`(x + y)*2`、`x - (y - x)`、`x^(y + 1)`、`sin(x)` 等を実機(sjulia)で確認。
- `Term` 検査は `operation`/`arguments` アクセサ経由(動的型値への `t.args` 直アクセスが builtin
  `Expr.args` に誤ルートする問題 #7162 を回避)。`complex.jl` と同じ単一 2-arg `Base.show(io, ·)`。
- **動作経路の制約**: `string`/`print`/`println`/`show(io, ·)` は VM の `user_show_method_for` を
  経由して本 `show` を使う(✅)が、**bare REPL エコー / iOS・Web 結果表示** と `repr` は Rust
  フォーマッタ(`format_value_*`)を使い user `show` を経由せず struct dump になる(全 user struct
  共通、Complex/Rational のみ Rust 再実装で回避)。汎用修正を **Issue #7168** で起票。当面は
  `println(ex)`/`string(ex)` で整形表示。
- テスト: fixture `packages/symbolics_show.jl`(manifest 登録、`string` ベースで表示文字列を pin)。

### AoT abstract return Any boundary validation (Issue #7074)

- 抽象 return annotation (`::Real` など) が AoT ABI 上 `Any`/`Value` boundary へ退避する経路を検証し、
  generated Rust の未定義 `Real` 識別子や `Value + i64` のような不正な静的演算に流れないようにした。
- lowered `convert(Real, value)` は、戻り値の静的型が対象抽象型の subtype と分かる場合に
  `Value::from(value)` へ変換する。静的に保証できない抽象変換は unsupported diagnostic にする。
- `Any`/`Value` を含む dynamic binary operation は Rust 演算子ではなく
  `subset_julia_vm_runtime::dynamic_binop` へ接続し、通常 AoT は runtime dispatch、`--pure-rust --check` は
  dynamic operation diagnostic として検出する。
- 検証: upstream Julia smoke、`juliars --minimal-prelude --emit-binary` abstract return smoke、
  `--pure-rust --check` diagnostic、AoT converter/codegen release nextest。

### AoT Any-boxed ternary branch boxing (Issue #7166)

- #7074 検証中に、`flag ? 1 : 2.5` を `Any`/`Value` boundary へ box すると
  `Value::from(if flag { 1i64 } else { 2.5_f64 })` という Rust branch 型不一致が出る既存バグを発見。
- `emit_expr_as_value` が ternary を branch ごとに boxing するようにし、`Convert(..., Any)` も同じ helper を使う。
  これにより mixed branch の抽象 return でも generated Rust が型検査を通る。
- 検証: Issue #7166 codegen regression と #7074 integrated regression、`juliars --emit-binary` smoke。

### AoT Bool power DomainError / Float64 boundary parity (Issue #7073)

- AoT inference が `Bool ^ signed integer` を `Any` として扱い、Rust backend が `true^-1` を
  `Value::from(1.0_f64)` にしていた境界を修正。
- `Bool ^ Int` は Julia と同じく `Bool` を返し、`true^-1 == true`、`false^0 == true`、
  `false^positive == false` になる。`false^-1` は既存の Julia-compatible DomainError message を維持。
- `Bool ^ Float64` は Float64 `powf` 経路のまま、`true^-1.0 == 1.0` / `false^-1.0 == Inf` を生成バイナリで確認。
- 検証: upstream Julia smoke、`juliars --minimal-prelude --emit-binary` Bool power smoke、
  AoT inference/codegen/E2E release nextest。

### Symbolics サブセット: パッケージ基盤と `@variables` (Issue #6572)

- 記号計算 (Symbolics.jl / SymbolicUtils.jl) の**中核セット**を sjulia のバンドルパッケージ
  `Symbolics` として Pure Julia で新設開始。本 PR はその基盤フェーズ。
- 追加: `packages/Symbolics/`(`Project.toml` + `src/{Symbolics,types,variables}.jl`)。
  `src/julia/packages/mod.rs` に const + `get_bundled_package` + `get_package_include` +
  `bundled_package_names` を登録 (Primes 同形)。
- 型 (`types.jl`): `Sym`(記号変数) / `Term`(`op(args...)`) / `Num <: Real`(ラッパ) と
  `unwrap`/`value`。本家 `BasicSymbolic` uni-type + hashconsing を 2 構造体 + `Num` に簡約。
- `@variables x y`(`variables.jl`): caller スコープへ `Num(Sym(:name))` を束縛し、生成した
  `Num` のベクトルを返す (`Plots.@animate`/`@gif` と同じ `esc` パターン)。
- subset-VM 制約 2 点を回避して構築 (起票済み): ① マクロ内で `Expr(head, args...)`
  へ Vector をスプラットすると壊れる (Issue #7162) → `:block`/`:vect` を `push!` で構築。
  ② マクロ注入の `QuoteNode` 値が `Any` 型化し `::Symbol` フィールドに入らない (Issue #7163)
  → `Sym.name` を未型付け (格納値は実 `Symbol`)。
- `show`・`substitute`・`simplify`/`expand`・微分は後続 PR。本 PR は構造的事実のみ検証で
  `Num <: Real` の promote 再帰トラップ (Issue #5966) を踏まないようにしている。
- テスト: fixture `packages/symbolics_variables.jl`(manifest 登録)、unit
  `packages::tests::test_symbolics_package_exists` / `test_symbolics_includes`。
- 詳細は [SYMBOLICS.md](./SYMBOLICS.md)。

### Symbolics サブセット: 算術・初等関数・等価性 (Issue #6572)

- `arithmetic.jl` を追加。`Num` の四則 `+ - * /`・冪 `^`・単項 `-` を**混合型メソッド
  (`Num⊗Num` / `Num⊗Real` / `Real⊗Num`)網羅**で定義し、`Num <: Real` の promote 再帰トラップ
  (Issue #5966) を回避。`x + 1` / `1 + x` 等が無限再帰しないことを実機(sjulia)で確認。
- 浅い正規化コンストラクタ `_mkadd`/`_mksub`/`_mkmul`/`_mkdiv`/`_mkpow`(定数畳み込み + 0/1 恒等)、
  初等関数 sin/cos/tan/exp/log/sqrt(定数引数は数値に畳み、記号引数は `Term` を作る)。
- 等価性: `==`(数値なら数値比較、それ以外は構造的 `Bool`) / `isequal`(順序依存の浅い構造比較) /
  `zero`/`one`/`iszero`/`isone`。混合メソッドで promote fallback を回避。
- TermInterface 風アクセサ `operation`/`arguments`/`iscall`/`issym`/`isterm` を `types.jl` に追加し
  export。動的(`Any`)型値への `t.args` 直接アクセスが builtin `Expr.args` に誤ルートする問題
  (Issue #7162 と同根)を、`::Term` ディスパッチのアクセサ経由に統一して回避。
- `^(::Num, ::Integer)` は Base `^(::Number, ::Integer)` との曖昧性解消のため別途定義(上流 num.jl 同様)。
- テスト: fixture `packages/symbolics_arithmetic.jl`(manifest 登録)。
### AoT print / println collection display parity (Issue #7072)

- AoT Rust backend の `print`/`println`/`string(...)` 表示境界で、配列・タプルを Rust の未実装 `Display`
  や Debug 表示に流さず、型情報から Julia `show` 風の文字列を生成するようにした。
- `Vector{T}` は `[a, b]`、`Vector{String}` は `["a", "b"]`、nested vector は `[[...], [...]]`、
  `Array{T,2}` は `[1 2; 3 4]` 形式で表示する。
- タプルは `(1, "x")` / singleton comma を保持し、内部の String/Char は引用、Float は既存の
  `__sjulia_format_float*` helper を再利用する。
- 検証: upstream Julia smoke、`juliars --minimal-prelude --emit-binary` collection display smoke、
  AoT codegen unit。

### AoT static dispatch ambiguity / no-method diagnostics (Issue #7071)

- AoT Rust backend の static dispatch resolver が同スコアの method candidates を先勝ちで選び、
  `f(::Int64, ::Any)` / `f(::Any, ::Int64)` へ `f(1, 2)` したとき Julia なら ambiguous MethodError になる
  ケースを silent mismatch にしていた問題を修正。
- `resolve_dispatch` を `AotResult<String>` にし、unique most-specific method がない場合は
  `f(::T, ...) is ambiguous`、候補がない場合は `no method matching f(::T, ...)` の codegen diagnostic を返す。
- 明示 `::Any` parameter は call-site specialization で concrete type に潰さず、fallback method として保持する。
- generated runtime dispatcher も overlapping fallback arms より前に ambiguity guard を出すようにし、
  dynamic `Value` 経路でも先勝ちしない。
- 同一 signature の重複は method table 構築時に dedupe し、既存の通常 multiple-dispatch E2E は維持。
- user-level `::Any` generic methods が dispatcher を bypass する高位 pipeline gap は Issue #7158 へ分離。
- 検証: upstream Julia ambiguity/no-method smoke、AoT codegen unit、AoT multiple-dispatch E2E。

### REPL: 空配列グローバルが eval 間で消える / `@gif` の `push!(ps, …)` が落ちる (Issue #7151)

- REPL で `ps = []`(空配列)をグローバルに束縛し、別の eval で使うと `UndefVarError` になっていた。
  非空配列(`[1,2,3]` 等)は永続化されるのに、空配列(`[]`/`Any[]`/`Int[]`/`Float64[]`)だけが落ちる。
  これが `ps = []; @gif for …; push!(ps, p); end` を REPL で実行したときの `ps` 未定義の原因
  (Editor は全文を1プログラムで実行するため再現しない)。
- 原因は再注入側: 空配列も REPLGlobals には保存されているが、`value_to_init_expr` が空配列に対し
  `None` を返す(モジュール状態初期化子を優先する意図、Issue #5296)ため、ユーザーグローバルの init 文が
  生成されず束縛が落ちていた。struct-backed array のストレージが `Value::MemoryRef` で
  `array_wrapper_struct_to_literal` が非対応だったことも `value_to_literal` の `None` を招いていた。
- 修正: ユーザーグローバルの注入経路にのみ空配列フォールバックを追加。`struct_name`(`Array{Any,1}` →
  `Any`、`Array{Int64,1}` → `Int64`)から要素型を取り出し `Expr::TypedEmptyArray` を生成する
  (`empty_array_init_expr`)。モジュール状態経路(`session.rs:1095`)は従来どおり `None` 維持。
- iOS/Web の REPL は同じ Rust `REPLSession` を FFI 経由で使うため自動的に修正される(Swift 変更なし)。
- 回帰テスト: `repl::tests::test_repl_empty_array_persists_across_evals_7151` /
  `test_repl_push_to_empty_array_across_evals_7151` / `test_repl_gif_with_global_accumulator_7151`。

### AoT HOF function-value codegen and non-Copy element parity (Issue #7070)

- AoT Core call graph と AoT prune が `map(double, xs)` のような裸の関数値参照を call edge として扱わず、
  HOF callee を削除して generated Rust が `cannot find function double` になる問題を修正。
- `map`/`filter`/`reduce`/`foldl`/`sum(f, xs)`/`mapreduce(f, op, xs)` の型推論と Rust backend codegen を拡張し、
  named function / operator function value を配列要素型から静的解決するようにした。
- `String` など非 Copy 要素は `.iter().cloned()` と predicate/result clone で扱い、
  `Vector{String}` 結果が `Vec<Value>` に崩れないことを regression 化した。
- 検証中に見つかった generated `Complex::new(..., im)` と global `const im` の Rust pattern 衝突を
  constructor parameter prefix 化で修正(Issue #7154)。古い array index 文字列を期待していた
  AoT E2E assertion も checked BoundsError codegen に合わせて更新(Issue #7155)。
- 検証: upstream Julia smoke、`juliars --minimal-prelude --emit-binary` Int/String HOF smoke、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm --features aot --test aot_e2e_tests`。

### AoT `zeros`/`ones` type-argument dimension handling (Issue #7069)

- AoT inference/conversion で `zeros(T, dims...)` / `ones(T, dims...)` の先頭 `T` を次元として扱っていたため、
  `zeros(Int64, 3)` が `Array{Float64,2}` と推論され generated Rust で `Int64 as usize` を出していた。
- 先頭が既知の Julia 型名の場合は element type として消費し、残りの引数だけを dims として
  `AotBuiltinOp::Zeros` / `Ones` に渡すようにした。
- literal tuple dims (`zeros(Int64, (2, 3))`) は converter で次元引数へ展開し、既存 2D codegen 経路に載せる。
- 検証: upstream Julia smoke、`juliars --minimal-prelude` 生成バイナリ smoke、
  AoT inference/converter unit。

### AoT DataType field access gate (Issue #7068)

- AoT の `typeof(x)` は display/name 用の compact `Value::DataType(String)` carrier を返すが、
  full Julia `DataType` identity / `parameters` / reflection object model はまだ持たない。
- `typeof(x).parameters` のような `DataType` receiver の field access が generated Rust の
  `Value::DataType(...).parameters` に流れないよう、AoT codegen で明示的な unsupported diagnostic にした。
- 単純な `typeof(1)` 表示は従来どおり `Int64` を生成バイナリで出力する。
- 検証: `juliars --minimal-prelude --check` unsupported smoke、
  `juliars --minimal-prelude --emit-binary` typeof smoke、AoT codegen unit。

### AoT integer division/remainder sign parity (Issue #7067)

- AoT codegen の integer `÷`/`div`/`%`/`rem` に DivideError guard を追加し、`0` 除算と
  `typemin(Int64) / -1` が Rust overflow panic ではなく Julia の `DivideError` になるようにした。
- `mod` は `%`/`rem` と同じ truncated remainder として扱っていた alias をやめ、builtin 経路で
  Julia の floored remainder(結果の符号は divisor 側)を生成するように分離した。
- `fld`/`cld` を AoT builtin として追加し、整数では truncating quotient と remainder から floor/ceil
  division を計算する。Bool mixed 引数は Julia と同じく整数へ cast して処理する。
- 検証: upstream Julia smoke、`juliars --minimal-prelude` 生成バイナリ smoke、
  AoT analyzer/codegen unit。

### AoT integer power/abs overflow parity (Issue #7065)

- AoT integer `^` が Rust の `.pow` を生成していた箇所を `.wrapping_pow` に変更し、Julia の fixed-width
  integer overflow wrapping と明示的に揃えた。
- `abs(::Signed)` は `.wrapping_abs()`、`abs(::Unsigned)` は identity を生成するようにし、
  `abs(typemin(Int64)) == typemin(Int64)` の Julia parity を保つ。
- `AotBuiltinOp::Abs.return_type` がコメントに反して常に `Float64` を返していたため、引数型を返すよう修正。
  これにより `println(abs(x::Int64))` が integer formatting 経路を使う。
- `div`/`fld`/`cld` の特殊ケースは Issue #7067 で別途解消済み。

### Plots.jl: `plot(...; title=...)` とフレーム毎タイトル (Issue #7030) / quote のブロードキャスト・kw・補間 (Issue #7029)

- 全 Plots 公開メソッドに `title=""` キーワードを追加し `Plot` に `title` フィールドを追加。
  `_CURRENT_TITLE` で current plot を追跡、`current()`/`frame` がフレームへ伝播。Rust 側は静的・
  アニメーション両レイアウトに `layout.title` を出力(アニメは各フレーム個別)。
- `@gif`/`@animate` の本体を quote→code で往復させる経路がブロードキャスト(`f.(x)` / `x .- y`)、
  キーワード引数(カンマ/セミコロン)、文字列補間(`"…$x…"`)を扱えるようになった(#7029)。
  これで `@gif for t in -π:0.1:π; plot(x, sin.(x .- t), title="t=$t"); end` が iOS/Web で動く。
- fixtures: `packages/plots_gif_title_7030.jl`、`metaprogramming/quote_broadcast_kwarg_interp_7029.jl`。
  iOS/Web の Animation サンプルをタイトル付きに更新。

### AoT 2D array indexing column-major parity (Issue #7063)

- AoT codegen で `Array{T,2}` に single index `A[k]` を使うと、nested `Vec<Vec<T>>` の row を返してしまい、
  `println(A[k])` が generated Rust compile error (`Vec<T>: Display` 未実装)になる問題を修正。
- `Array{T,2}` の linear indexing は runtime shape から `rows`/`cols` を求め、Julia の column-major
  linear order に合わせて `row = (k - 1) % rows`, `col = (k - 1) / rows` へ変換する。
- `A[i,j]` は 2 つの index 式を先に評価し、row/column bounds guard 後に nested Vec へアクセスする形へ整理。
  `A=[1 2; 3 4]` で `A[1,1] A[2,1] A[1,2] A[2,2]` と `A[1]...A[4]` がともに
  Julia と同じ `1,3,2,4` になることを確認。

### AoT 1D array indexing BoundsError parity (Issue #7062)

- AoT codegen の 1D 配列 indexing が `arr[(i) as usize - 1]` を直接生成し、
  範囲外アクセスで Rust の `index out of bounds` panic になっていた問題を修正。
- 生成式は配列式と index 式を一度だけ評価し、`index < 1 || index > length` を明示的に検査して
  `BoundsError([1, 2], (3,))` 形式の Julia-compatible error text で `aot_throw` する。
- runtime array helper も 1-based index を保持するように `RuntimeError::bounds_error` を signed index 化し、
  `index == 0` などを `usize` underflow へ流さないようにした。
- 2D indexing も direct nested Rust indexing から bounds guard 付き生成へ寄せた。2D 表示の完全 parity は
  既存の Issue #7063 で継続管理。

### Generator reduction `init` keyword forwarding (Issue #7133)

- full release nextest で `generator_arg_with_kwargs_5763` が
  `MethodError: no method matching sum(; init::...): unsupported keyword argument "init"`
  により失敗していたため、`Base.Generator` 向け `sum`/`prod`/`maximum`/`minimum`
  に `init` keyword を追加した。
- 原因: parser は `sum(x for x in itr; init=v)` を正しく generator positional arg + trailing keyword
  として lower していたが、Pure Julia Base 側の `sum(g::Generator)` が keyword を受けず、
  runtime keyword rejection に到達していた。
- `prod(g::Generator)` は `prod(collect(g))` へ単純委譲すると VM の keyword method dispatch が
  `prod(::Array)` を曖昧扱いするため、配列版と同じ empty identity helper を使いつつ generator を直接 reduce。
- 検証: upstream Julia fixture smoke、direct `target/release/sjulia` fixture smoke、
  focused generator fixture nextest、full release nextest pass。

### iOS: エディタ／REPL のフォントサイズ変更(ピンチ + Cmd ショートカット) (Issue #7008)

- iOS アプリのコードエディタと REPL のフォントサイズを動的に変更可能にした。エディタと REPL は
  `@AppStorage("editorFontSize")` で同一設定を共有し、再起動後も保持される。
- 操作: **2 本指ピンチ**(エディタ=`UIPinchGestureRecognizer`、REPL ログ=SwiftUI `MagnifyGesture`)、
  物理キーボードの **Cmd `+`/`=`**(拡大)・**Cmd `-`**(縮小)・**Cmd `0`**(デフォルトに戻す)。
- フォントサイズ方針(整数 pt 丸め + `10...24` クランプ + 前回値と異なる時のみ反映)を
  `AppConfiguration.Editor.clampFontSize(_:)` に集約。エディタ/REPL 双方の入力欄
  (`EditorTextView` / `REPLTextField` / `REPLTextView` の `keyCommands`)へ Cmd ショートカットを追加。
- 変更ファイル: `AppConfiguration.swift` / `MonospacedTextEditor.swift` / `ContentView.swift` /
  `REPLView.swift` / `REPLInputView.swift` / `REPLEntryView.swift` / `SyntaxHighlightedInput.swift`。
  単体テスト `FontSizeConfigurationTests` を追加。

### iOS: REPL 入力欄のフォントサイズ変更を即時反映 (Issue #7025)

- #7008 のフォローアップ。REPL 入力欄に既にテキストがある状態で Cmd `+`/`-`/`0` を押すと、
  プロンプト/履歴は再描画されるのに編集中テキストだけ旧サイズのまま(次の編集まで反映されない)
  というバグを修正。
- 原因: `SyntaxHighlightedTextField` / `SyntaxHighlightedTextEditor` の `updateUIView` が
  `lastText == text` のときに再ハイライトを丸ごとスキップし、フォントのみの変更を適用しなかった。
- 修正: エディタ(`MonospacedTextEditor`)と同じく `uiView.font?.pointSize != fontSize` で
  フォントのみの変更を検出し、`applyFontSizeChange(to:text:fontSize:)`(ベースフォント更新 +
  テキストありなら再ハイライト)を呼ぶ分岐を、メイン／バックグラウンド両スレッド経路に追加。
- 変更ファイル: `SyntaxHighlightedInput.swift`。単体テスト `REPLFontSizeUpdateTests`(6 件)を追加。

### Plots.jl アニメーション (`@animate` / `@gif` / `Animation` / `gif`) (Issue #6355)

- iOS/Web 前提で Plots.jl のアニメーション機構を実装。MVP:
  ```julia
  using Plots
  p = plot(1)
  @gif for x = 0:0.1:5
      push!(p, 1, sin(x))
  end
  ```
  本家は PNG を一時ディレクトリへ書き出し FFmpeg で GIF 化するが、iOS/WASM には FS も
  FFmpeg も無いため、**ファイル I/O を使わず** 各フレームを Plot スナップショット(in-memory)で
  蓄積し、**Plotly ネイティブ `frames` アニメーション JSON** を生成して既存 artifact パイプライン
  で描画する(自動再生+ループ+スライダー)。MIME は既存 `application/vnd.plotly+json` を流用し、
  JSON の `frames` キー有無でフロント(iOS `REPLEntryView`/Web `app.js`)が分岐(C ABI 変更なし)。
- Julia 側 (`packages/Plots/`): `Animation`/`AnimatedGif` 型、`current()`、`plot(::Number)`、
  `Base.push!(::Plot,i,y)`(x を +1 自動延長)、`frame`、`gif`、`@animate`/`@gif` マクロを追加・export。
- Rust 側: `plotting/plotly.rs::generate_plotly_animation_json`(各フレームを既存 `extract_series`/
  `render_trace` で再利用)を追加し `try_value_to_artifact` に分岐。

### バンドルパッケージのマクロを `using` で公開 + include 内マクロの取りこぼし修正 (Issue #6355)

- バンドルパッケージ(Plots)の `include("api.jl")` 内で定義したマクロが `Module.macros` に
  集約されていなかった(`IncludedContent::merge_into` が macros を merge していなかった)不具合を修正。
- バンドルパッケージのマクロを専用レジストリへ登録する `ensure_bundled_package_macros_loaded` を追加し、
  `using <Pkg>` 低位化時に登録。式位置(`anim = @animate ...`)・文位置(`@gif for ... end`)の両ディスパッチに
  バンドルマクロ分岐を追加し、ユーザ定義マクロと同じ完全展開器(`macro_runtime`)経由で展開する
  (テンプレート展開では `Expr(:for,...)` 構築等ができないため)。stdlib(`@testset` 等)の経路は不変。

### `esc` した 3 引数(ステップ付き)レンジの平坦化 (Issue #7020)

- パーサが `a:b:c` を入れ子 `(a:b):c` にするため、マクロで `esc` したステップレンジが quote→code 再構築で
  平坦化されず、実行時に `expected numeric value, got Range` で落ちていた(`@animate`/`@gif` で発覚)。
- `macro_runtime.rs::call_named_expr`(および `quote/handlers.rs`)の `":"` 2 引数ケースで、第 1 オペランドが
  step なし `Expr::Range` のとき 3 引数ステップレンジへ平坦化(`collection.rs::lower_range_expr` と同じ挙動)。

### 既知の VM キーワードディスパッチ不具合を回避 (Issue #7021)

- 同名関数に単一位置引数+同一キーワード集合のメソッドが 3 つ以上あると、選択されたメソッドが
  キーワードを取りこぼす(`plot(sin, aspect_ratio=:equal)` が `:auto` になる)不具合を発見・起票。
- `#6355` で `plot(1)` を足す際に踏んだため、`plot(y::Vector)`/`plot(y::Number)` を**単一の untyped
  `plot(y)`** に畳んで単一引数 `plot` メソッドを 2 つに保つ回避を採用(`WORKAROUNDS.md` 参照)。根本修正は #7021。

### VM キーワードディスパッチ根本修正 (Issue #7021)

- runtime-dispatched calls with keyword arguments now use the keyword/splat call
  opcode instead of falling back to positional typed dispatch.
- ambiguous single-argument overload sets and dynamic no-method fallback paths
  preserve keyword values for the method selected at runtime.
- 検証: upstream Julia fixture smoke、direct `target/release/sjulia` fixture smoke、
  `fixture_tests kwargs::` pass。

### Float step range endpoint length (Issue #7024)

- `RangeValue` length calculation now tolerates floating-point roundoff at the
  inclusive endpoint, so `0:0.1:0.3` has length 4 and exposes the final element.
- collection helpers, range indexing, and `last(range)` use the centralized
  range length/last logic instead of duplicating the old floor calculation.
- 検証: upstream Julia fixture smoke、direct `target/release/sjulia` fixture smoke、
  focused range unit test、`fixture_tests range::` pass。

### AoT regression coverage for implicit returns and boxed returns (Issues #7010, #7012)

- Added regression coverage that implicit full-form function body results convert
  to AoT expressions instead of `Undef`.
- Added inliner coverage that runtime-boxed return calls are not inlined into
  unit-returning main bodies.
- 検証: focused AoT converter/inliner tests pass。

### Plots existing Plot replot support (Issue #7026)

- bundled Plots now supports `plot(p::Plot)` by restoring the existing plot's
  series/current state instead of falling through to `plot(y)` and calling
  `length(::Plot)`.
- keyword runtime dispatch coverage now also preserves kwargs on dynamic
  no-method fallback paths, so `plot(p; aspect_ratio=...)` can override the
  restored plot aspect.
- 検証: upstream Julia with local package load path、direct `target/release/sjulia`
  fixture smoke、`fixture_tests packages::` pass。

### REPL inline suggestion font refresh (Issue #7028)

- single-line and multi-line REPL inputs refresh completion state after
  font-size-only changes, recreating the inline suggestion label with the new
  monospaced font.
- 検証: source-level review; local environment lacks `xcodebuild`, so iOS
  simulator build is deferred to CI。

### AoT throw helper Display error text (Issue #7018)

- AoT runtime `aot_throw` を `Debug` formatting から `Display` formatting へ変更し、
  `RuntimeError::DivisionByZero` が generated binary stderr で
  `DivisionByZero` ではなく `DivideError: integer division error` と表示されるようにした。
- generated prelude の local `throw` wrapper も `Display` bound に揃え、
  `ErrorException` には message を出す `Display` impl を追加した。
- 検証: runtime panic-message regression test、release `juliars --emit-binary`
  divide-by-false smoke pass。

### AoT `typeof` DataType display parity (Issues #6973, #7015)

- AoT runtime `Value` に Julia type object 用の `DataType(String)` carrier を追加し、
  display / equality / `type_name()` を固定した。
- Rust backend の `typeof(x)` は Rust reflection (`std::any::type_name_of_val`) ではなく、
  static 型なら inferred Julia type name、`Any` / runtime `Value` なら `Value::type_name()`
  から `Value::DataType(...)` を生成する。
- 検証: focused codegen/runtime regression tests、upstream Julia vs generated binary
  stdout diff pass。

### juliars Cranelift backend CLI reachability (Issue #6927)

- `--backend cranelift` を early usage error で止めず、`CompileConfig` の backend
  selection として canonical AoT pipeline の codegen boundary まで渡すようにした。
- `cranelift` feature 付き build では、高レベル `AotProgram` から低レベル
  `IrModule` への conservative scalar / straight-line adapter を通して既存
  `CraneliftCodeGenerator` を実際に呼び出す。feature なし build では span なしの
  `UnsupportedInstruction` で rebuild 手順を返す。
- 検証: focused no-feature / feature-gated Cranelift regression tests、release
  `juliars --backend cranelift --check` / `-o -` smoke pass。

### AoT parametric struct constructor gate (Issue #6975)

- `struct Box{T}; x::T; end; Box(1)` のような parametric struct が AoT IR で
  unresolved constructor-like static call として残り、generated Rust の `Box(1i64)`
  compile error まで漏れる経路を停止した。
- `Complex` の既存 special-case を除き、parametric struct definition と未解決の
  uppercase constructor-like call は span 付き `UnsupportedInstruction` として拒否する。
- 検証: focused converter regression tests、release `juliars --check` repro smoke pass。

### AoT converter/inference panic-free cleanup (Issue #6933)

- AoT function conversion の parameter env setup で、既に持っている `(name, ty)` から
  `ty` を直接挿入し、同じ `params` を再検索して `.unwrap()` する経路を削除した。
- multi-argument operator unfold の初期値取得は、到達不能前提の `.unwrap()` ではなく
  explicit `InternalError` にした。call-site specialization / single return type inference の
  single-element vector も panic しない `remove(0)` へ置き換えた。
- 検証: focused function conversion / inference regression tests pass。

### AoT expression-position begin/let side-effect gate (Issue #7014)

- `println(begin println("side"); 1 end)` のような expression-position
  `begin` / `let` block が最後の値だけに変換され、前段の side effect を落とす経路を停止した。
- bindings なし・単一 expression の block は従来どおり値として変換し、bindings あり /
  multi-statement / side-effecting block は sequence expression support が入るまで span 付き
  `UnsupportedInstruction` として拒否する。
- 検証: focused converter regression tests、release `juliars --check` repro smoke pass。

### AoT Float print/string display parity (Issues #7013, #7017)

- static `Float64` / `Float32` values printed by generated Rust now route through a
  Julia-style display helper at `print` / `println` / `string(...)` boundaries, so
  whole values keep a decimal point (`3.0`, `-0.0`) instead of Rust's `3` / `-0`.
- unshadowed global `Inf` / `Inf32` / `Inf64` / `NaN` / `NaN32` / `NaN64`
  references are converted to float literals before codegen, avoiding invalid bare
  Rust identifiers while preserving local binding shadowing.
- 検証: upstream Julia float display smoke、focused converter/codegen regression
  tests、release `juliars --emit-binary` stdout diff pass。

### AoT local `Dict` / `Set` construction gate (Issue #7016)

- local `Dict(...)` / `Set(...)` construction が Pure Julia collection body の `Any`
  condition codegen まで進み、無関係な exit 6 codegen error になる経路を停止した。
- Rust backend の collection representation が入るまでは、constructor call span 付きの
  `UnsupportedInstruction` として exit 5 で拒否する。full `Dict` / `Set` codegen は
  Issue #6971 / #6972 の残スコープ。
- 検証: focused converter regression test、release `juliars --check` local Dict/Set
  repro smoke pass。

### AoT Complex arithmetic Float64 layout gate (Issue #6965)

- Rust backend の monomorphic `Complex` layout は `Float64` field 前提であることを codegen boundary に固定し、`Complex{Float64}` / `ComplexF64` / legacy `Complex64` は既存 `Complex` Rust type へ投影する。
- `Complex{Float32}` / `Complex{Int64}` など non-`Float64` parameterized Complex の static `+` / `-` / `*` arithmetic は、誤った Rust code を出さず parameterized Complex codegen が入るまで diagnostic で拒否する。
- 検証: focused Complex arithmetic codegen regression test pass。

### AoT Char literal Unicode boundary (Issue #6967)

- Rust backend の `AotExpr::LitChar` は Rust `char` で表現できる valid Unicode scalar を `escape_default()` 付き char literal として生成することを regression test 化した。
- Julia `Char` は invalid Unicode code point も保持できるため、`Char(0xd800)` のような conversion-to-Char path は Rust `char` carrier へ誤投影せず diagnostic で拒否する。
- 検証: focused Char literal/codepoint boundary codegen regression test pass。

### AoT Complex `im` lexical shadowing (Issue #6966)

- Julia global `im` を uppercase internal alias `IM` へ強制変換する codegen workaround をやめ、generated Rust でも lowercase `const im: Complex` を emit するようにした。
- local / parameter binding named `im` は Rust の通常 lexical scoping で global `im` を shadow するため、Julia 側の名前解決に近い挙動になる。
- 検証: focused Complex `im` shadowing codegen regression test pass。

### AoT random builtin codegen gate (Issue #6964)

- `AotBuiltinOp::Rand` が undeclared `rand::random::<f64>()` を出す経路と、`Randn` が `0.0` constant fallback を出す経路を停止した。
- VM-compatible RNG contract / seed control が AoT に無い間は、`rand` / `randn` を codegen diagnostic で拒否する。
- 検証: focused random builtin codegen regression test pass。

### AoT range step / float / empty parity (Issue #6969)

- `AotExpr::Range` の Rust backend 表現を direct `..=` / `step_by(step as usize)` から materialized `Vec<T>` へ変更し、positive step、negative step、方向不一致の empty range、zero-step diagnostic を同じ生成パターンで扱うようにした。
- `Float32` / `Float64` range expression も `+= step` で materialize し、Rust の整数 range 制約に依存しない。
- range element type inference / converter が `step` の型も見るようにし、`1:Float32(0.5):2` は `Float32` element として start/stop を cast して生成する。
- 検証: upstream Julia range smoke、focused codegen/inference/converter regression tests pass。

### AoT checked numeric conversion gates (Issue #6968)

- `AotExpr::Convert` が Rust `as` で float->integer、integer narrowing、符号境界、numeric->Bool を silently lower する経路を停止し、Julia `InexactError` parity 用 runtime check が必要な conversion は codegen error にする。
- lossless integer widening、integer->float、Float32/Float64 間、Bool->numeric、Char->wide integer のように runtime check 不要な範囲だけ Rust cast を生成する。
- Core IR `fptosi` builtin も同じ理由で gate し、別経路から truncate/saturate semantics が混入しないようにした。
- 検証: focused checked-conversion codegen regression test pass。

### AoT tuple `first` / `last` field access (Issue #6963)

- tuple-specific `AotBuiltinOp::TupleFirst` / `TupleLast` codegen を array-style `[0].clone()` / `len()-1` から Rust tuple field access (`.0`, `.N`) へ変更した。
- empty tuple や tuple 以外の argument は誤コード生成せず codegen error にする。
- 検証: focused tuple first/last codegen regression test pass。

### AoT tuple dynamic index gate (Issue #6962)

- tuple index codegen は static tuple type と constant in-range integer index を要求し、対応できる場合だけ Rust tuple field access (`.0`, `.N`) を生成する。
- dynamic `t[i]` や literal out-of-bounds index は、Rust tuple に対する invalid array indexing を出さず diagnostic で拒否する。
- 検証: focused tuple dynamic/out-of-bounds index codegen regression test pass。

### AoT 2D array shape builtins use static rank (Issue #6959)

- `length(::Matrix)` codegen を outer `len()` や row-length sum ではなく、static 2D rank に基づく rows*cols に変更した。
- `size(A)` / `size(A, dim)` / `ndims(A)` も inferred `StaticType::Array.ndims` を使い、2D shape を runtime row discovery ではなく rank-aware generated code にする。
- 3D+ arrays は一般 N-D representation が未整備のため shape builtin でも diagnostic gate にして、2D logic の誤流用を防ぐ。
- 検証: focused array shape builtin codegen regression test pass。

### AoT 3D+ arrays are explicitly gated (Issue #6960)

- 3D+ array literal、3D+ indexing、`zeros` / `ones` の 3 dimension 以上指定を Rust nested `Vec` へ黙って lower しないよう diagnostic gate にした。
- 1D/2D の現行 `Vec` / `Vec<Vec<_>>` surface は維持し、一般 N-D Array carrier は未実装として分離する。
- 検証: focused 3D array gate codegen regression test pass。

### AoT array shape rank selection is static (Issue #6961)

- `length` / `size` / `ndims` の 1D vs 2D codegen branch は `StaticType::Array.ndims` から選び、generated Rust の source spelling や runtime nested-`Vec` probe に依存しない。
- `ndims: None` は従来互換の 1D fallback とし、3D+ は Issue #6960 の diagnostic gate に分離する。
- 検証: focused static-rank shape selection codegen regression test pass。

### AoT Bool/Int arithmetic and condition boundary (Issue #6980)

- `Bool` を含む static `+` / `-` / `*` は inferred result width へ Rust cast して生成し、`true + true == 2` の `Int64` result と `Bool + Int8` の `Int8` result を分ける。
- `Bool * Bool` は Julia と同じ Bool result として `&&` へ lower し、Bool と整数の mixed comparison も Rust 型不一致を避ける cast を入れる。
- `Bool`/`Bool` と mixed `Bool`/integer の `÷` / `%` / `^` は Julia result surface
  へ合わせた。signed integer exponent の `Bool ^ n` は `n < 0` で `Float64` /
  `DomainError` になり得るため `Value` boundary へ退避し、Bool denominator false
  は `DivideError` を投げる。
- strength reduction は Bool operand / Bool result を shift や integer literal へ変換せず、
  `true ÷ 2 -> true >> 1` や `false ^ 0 -> 1` のような invalid/wrong Rust を避ける。
- `if` / `elseif` / `while` / ternary condition は `Bool` のみ許可し、`if 1` は Julia truthy/falsy 不在に合わせ diagnostic にする。
- 検証: upstream Julia Bool arithmetic/condition/error smoke、focused Bool codegen /
  converter / optimizer / inference regression tests pass。

### AoT Nothing and nullable return codegen (Issue #6979)

- `LitNothing` / `Nothing` return function は Rust unit `()` として生成し、explicit `return nothing` も `return ();` になる。
- `Union{T,Nothing}` のように runtime `Value` boundary へ落ちる nullable return では、ternary arms と explicit `return nothing` を `Value::from(...)` へ boxing する。
- 検証: focused Nothing / nullable union return codegen regression test pass。

### AoT Union value representation uses runtime Rust enum (Issue #6977)

- multi-variant `Union{...}` は bespoke per-Union enum を生成せず、`subset_julia_vm_runtime::Value`
  Rust enum boundary として表現する方針を固定した。
- `Union{Int64,String}` return では generated function の return type が `Value` になり、
  ternary/branch arms は `Value::from(...)` で boxing される。
- 検証: focused multi-variant Union return codegen regression test pass。

### AoT struct definitions emit in dependency order (Issue #6974)

- generated Rust の struct definitions は field type dependency を DFS で topological sort して、`Outer(inner::Inner)` のような前方参照が入力順に依存しないようにした。
- nested container / tuple / union / function type 内の `StaticType::Struct` dependency も走査し、循環 struct dependency は diagnostic gate にする。
- 検証: focused struct dependency ordering / cycle diagnostic codegen regression tests pass。

### AoT type-unstable bindings use Value boundary (Issue #6978)

- `Any` / `Union` slot の `let` / assignment は native value を `Value::from(...)` へ boxing し、分岐ごとに `Int64` / `String` などへ変わる local を単一 native Rust 型へ誤 emit しない。
- fixed native slot へ incompatible value を入れる path は、型不安定 local は `Any` / `Union` boundary が必要という diagnostic にする。
- 検証: focused type-unstable binding codegen regression tests pass。

### AoT static dispatch picks the most-specific method (Issue #6976)

- static `CallStatic` resolution が最初に一致した method を返すのをやめ、matching methods を specificity score で比較して concrete/exact method を broad `Any` method より優先する。
- 同 specificity の場合は従来どおり source order を保つ。
- 検証: focused broad-first static dispatch regression test pass。

### AoT inliner tracks pure static callees (Issue #6981)

- inliner の purity analysis は static function call を一律 impure とせず、known-pure function set を fixed point で求めて `CallStatic` callee が純粋な場合は caller も pure として扱う。
- `CallDynamic` は world-age / dispatch side effect safety が未整備なため impure のまま維持する。
- 検証: focused inliner purity regression tests pass。

### AoT LICM uses dominator-based back edges (Issue #6982)

- low-level CFG LICM の back-edge 検出を block order heuristic から dominator analysis に置き換え、target が source を支配する edge だけを natural-loop back edge として扱うようにした。
- entry から到達可能な block の dominator set を fixed point で計算し、unreachable block や earlier-block cross edge は LICM loop として扱わない。
- 検証: focused #6982 regression tests と LICM unit-test filter pass。

### AoT LICM refines loop-control dependency hoisting (Issue #6983)

- low-level CFG LICM の loop-control condition 定義について、blanket skip ではなく通常の loop-invariant dependency check を通すようにした。
- induction-variable / loop-carried state に依存する condition 定義は従来どおり loop 内に残し、loop-invariant scalar condition 定義だけ preheader へ hoist する。
- `GetIndex` / `TypeAssert` など no-alias / no-throw proof が未整備の control condition 定義は引き続き loop 内に残す。
- 検証: focused #6983 regression test と LICM unit-test filter pass。

### AoT DCE removes overwritten dead stores (Issue #6986)

- high-level AoT DCE に backward liveness scan を追加し、plain variable `Assign` が後続 store に上書きされ、読み出し前に死ぬ場合は削除する。
- Rust declaration を壊さないよう `Let` は削除せず、branch / loop boundary は conservative に変数参照を収集して block 内 DSE に留める。
- RHS 評価を消すため、削除対象は literal / variable / no-throw static scalar expression に限定し、dynamic/static call、builtin、indexing、conversion、division/modulo/power、heap-shaped expression は残す。
- 検証: focused #6986 regression tests と DCE unit-test filter pass。

### AoT CSE reuses dominating structured expressions (Issue #6985)

- high-level AoT CSE が `if` branch body を fresh scope で最適化するのをやめ、branch を支配する親 statement list の available expression map を seed するようにした。
- loop body も親 scope の expression を seed するが、loop variable と body 内で modified される variable を使う expression は事前に invalidation して、iteration 間で値が変わる式を pre-loop value に置換しない。
- branch / loop の後方へ availability は merge せず、structured dominator から子 block への片方向 CSE に限定する。
- 検証: focused #6985 regression tests と CSE unit-test filter pass。

### AoT direct self-tail recursion to loop (Issue #6987)

- high-level AoT optimizer に direct self tail-call elimination を追加し、explicit `return f(args...)` が同じ function / specialization を呼ぶ場合は argument temps、parameter assignments、`continue` へ変換する。
- 変換後の function body は constant-true loop で wrap し、codegen は `while true` ではなく Rust `loop { ... }` を出す。既存の reassigned-parameter scan により parameter は `mut` として emit される。
- `if` branch 内の explicit tail return は変換するが、既存 `while` / `for` body 内の return は `continue` target が変わるためこの slice では触らない。
- 検証: focused #6987 optimizer/codegen regression tests と AoT optimizer unit-test filter pass。

### AoT optimizer Criterion benchmark gate (Issue #6945)

- `subset_julia_vm/benches/aot_optimizer_benchmark.rs` を追加し、synthetic high-level AoT IR を clone して各 optimizer pass の mutation cost を Criterion で測る gate を用意した。
- 対象 pass は constant folding、strength reduction、CSE、DCE、loop optimization、inlining、direct self-tail recursion TCO。
- 実行方法: `cargo bench -p subset_julia_vm --features aot --bench aot_optimizer_benchmark`。CI では重い実測ではなく `--no-run` compile gate としても使える。
- 検証: `timeout 1800 cargo bench -p subset_julia_vm --features aot --bench aot_optimizer_benchmark --no-run` pass。

### AoT fixture stdout parity helper (Issue #6954)

- `scripts/aot_fixture_julia_parity.sh <fixture.jl>` を追加し、release `juliars` で `--emit-binary` 生成、generated binary 実行、upstream `julia` 実行、stdout exact diff までを一括化した。
- 既存 `scripts/fixture_julia_parity.sh` と同じ developer helper convention とし、`check_*.sh` CI audit 対象にはしない。
- 前提: `cargo build --release -p subset_julia_vm --features aot --bin juliars` と `julia` on PATH。
- 検証: `bash -n scripts/aot_fixture_julia_parity.sh` と temporary `println(1 + 2)` fixture の parity smoke pass。

### AoT VM-vs-generated-binary differential helper (Issue #6942)

- `scripts/aot_vm_differential.sh <fixture.jl> [...]` を追加し、release `juliars` で生成した native binary stdout と release `sjulia` VM stdout を fixture 単位で exact diff できるようにした。
- upstream Julia parity とは別に、VM と AoT backend の差分を切り分ける developer helper とし、CI `check_*.sh` audit には登録しない。
- 前提: `cargo build --release -p subset_julia_vm --features aot --bin juliars` と `cargo build --release -p subset_julia_vm --features repl --bin sjulia`。
- 検証: `bash -n scripts/aot_vm_differential.sh` と `fixtures/aot/builtin_stdout_parity_6999.jl` の VM-vs-AoT smoke pass。

### AoT supported builtin stdout parity fixture (Issue #6999)

- `subset_julia_vm/tests/fixtures/aot/builtin_stdout_parity_6999.jl` と manifest を追加し、生成 binary stdout と upstream Julia stdout が一致する supported scalar operator / builtin surface を固定した。
- カバー範囲: integer arithmetic、float division、integer division / modulo / power、comparison / equality、Bool+Int arithmetic、`abs`、`min` / `max`、1D array `length`。
- probing 中に `sqrt(9.0)` の generated binary stdout が `3`、upstream Julia が `3.0` になる Float64 whole-value formatting gap を発見し、Issue #7013 として分離した。
- 検証: `scripts/aot_fixture_julia_parity.sh`、`cargo nextest run --release -p subset_julia_vm --test fixture_tests aot::`、`scripts/check_fixture_test_names.sh` pass。

### AoT fixture no-silent-mismatch property harness (Issue #7003)

- `scripts/aot_fixture_no_silent_mismatch.sh [fixture.jl ...]` を追加し、VM-passing fixture について generated AoT binary が original stdout と final value の両方で release `sjulia` と一致するか、`juliars` が exit code 5 (`UnsupportedInstruction`) で明示拒否することを検査できるようにした。
- 引数なしでは category manifest 群から fixture file を列挙する corpus mode とし、個別 fixture 引数で targeted smoke も可能にした。
- final value は `println(begin ... end)` wrapper の最後の行だけを比較し、明示的な stdout side effect は original source の比較で別途固定する。wrapper 内 side effect gap は probing 中に Issue #7014 として分離した。
- 検証: `bash -n scripts/aot_fixture_no_silent_mismatch.sh`、`fixtures/aot/builtin_stdout_parity_6999.jl` の compiled+matched smoke、temporary `<:` fixture の explicit unsupported smoke pass。

### AoT DataType carrier and `typeof` codegen gate (Issues #6973, #7015)

- `StaticType::DataType` を追加し、`typeof(x)` inference と `AotBuiltinOp::TypeOf` return type が `Any` / `String` へ逃げず Julia の first-class type value carrier として分類されるようにした。
- Rust backend の `std::any::type_name_of_val` emission は `i64` のような Rust carrier 名を出して Julia の `Int64` / `DataType` surface とずれるため停止し、DataType runtime representation が入るまで `UnsupportedInstruction` で拒否する。
- `DataType` は現時点では runtime `Value` boundary / rooting-required carrier として ABI 分類し、C ABI/native scalar としては扱わない。
- 検証: focused #6973 inference/type tests、focused #7015 codegen gate test、CLI smoke `juliars -e 'println(typeof(1)); true' --check` が exit code 5。

### AoT `map` / `filter` non-`Copy` element codegen (Issues #6957, #6958)

- named-function `map(f, arr)` codegen を `arr.iter().cloned().map(|x| f(x))` に変更し、`String` など non-`Copy` element で `|&x|` destructuring しないようにした。
- named-function / closure `filter(f, arr)` も predicate へ cloned value を渡し、retained element を `.cloned()` で result `Vec` へ集める形にして Copy 前提を外した。
- 検証: focused `Vector{String}` HOF codegen regression test pass。

### AoT typed `zeros` / `ones` fill literals (Issue #6956)

- `AotExpr::CallBuiltin` の `return_ty` から配列 element type を読み、`zeros` / `ones` の generated Rust fill literal を `i32` / `u8` / `f32` / `bool` などの concrete scalar 幅へ合わせるようにした。
- `zeros(Bool, n)` は `false`、`ones(Bool, n)` は `true` を埋める upstream Julia semantics に合わせた。非 scalar element は誤った `f64` fallback を出さず codegen error にする。
- 検証: upstream Julia typed zeros/ones smoke、focused codegen regression test pass。

### AoT Cranelift string/unsupported type gates (Issues #6948, #6949)

- Cranelift low-level backend の `ConstValue::String` は runtime `Value` / managed string rooting contract が未充足のため、専用 diagnostic で拒否する regression test を追加した。
- Cranelift `StaticType` -> Cranelift type mapper で未対応の scalar carriers (`I128` / `U128` / `F16` / `Missing`) を列挙し、rooting gate とは別の `TypeConversion` unsupported として固定した。
- 検証: focused Cranelift feature tests pass。

### AoT Cranelift rooting/safepoint contract gate (Issue #6947)

- Cranelift backend は runtime `Value` / heap-shaped `StaticType` の rooting/safepoint contract をまだ満たさないため、signature parameter / return type と module generation boundary で明示的に拒否する coverage を追加した。
- Rust backend の高レベル AoT path より対応範囲は狭く、Cranelift feature は native
  scalar / straight-line IR の実験経路に限定する現状を support matrix に反映した。
- 検証: focused Cranelift feature tests pass。

### AoT Union/abstract return inference and fallback coverage (Issue #6939)

- explicit `return` が複数 concrete type を返す関数で `Union{...}` return signature を保持し、inference level 3 として扱う regression test を追加した。
- abstract return annotation (`Real` など) は AoT の runtime `Value` boundary (`Any`) に落とし、static call 自体は dynamic fallback として数えないことを固定した。
- runtime-boxed return 関数の inlining は caller-context return rewrite が未整備なため停止し、`main` へ `return Value::from(...)` を流し込む Rust 型不整合を防いだ (Issue #7012)。
- 検証: focused inference/codegen/optimizer tests pass。

### AoT Float32/Float64 type preservation coverage (Issue #6941)

- Float32 と整数の static arithmetic / division / comparison codegen で、整数側を `f32` へ cast し、`Float64` 混在時のみ `f64` へ widen するようにした。
- `Bool` を float 演算へ落とす場合は Rust の直接 `bool as f32/f64` を避け、Julia numeric conversion に合わせて integer 経由の cast を使う。
- 検証: upstream Julia Float32 promotion smoke、focused inference/codegen regression tests。

### AoT low-level optimizer gaps documented (Issue #6944)

- `optimizer/pass.rs` の low-level `IrFunction` strength reduction / inlining no-op を明示的な未実装 gap として `docs/vm/UNIMPLEMENTED.md` と `docs/aot/DESIGN.md` に記録した。
- 現行 `juliars` の主経路は高レベル `AotProgram` optimizer (`-O0..-O3`) であり、低レベル backend は追加の `IrFunction` inlining / strength reduction が走る前提を置かないことを明記した。
- 検証: documentation diff check、AoT clippy pass。

### AoT integer arithmetic uses wrapping codegen (Issue #6940)

- 同一 concrete integer 型の static `+` / `-` / `*` codegen を Rust の直接演算子から `wrapping_add` / `wrapping_sub` / `wrapping_mul` に変更し、Julia の overflow wrap semantics と一致させた。
- `+=` / `-=` / `*=` の compound assignment も concrete integer variable では wrapping assignment へ展開する。`Bool` や float / mixed numeric path は既存経路を保持する。
- 検証: upstream Julia overflow smoke、focused arithmetic/compound/snapshot codegen tests、AoT clippy pass。

### AoT lambda multi-statement body rejects with diagnostic (Issue #6938)

- Core IR -> AoT IR converter が multi-statement lambda body や single-expression でない lambda body を `LitNothing` へ silently fallback する経路を廃止した。
- 現時点の AoT lambda lowering は single expression / single return expression に限定し、未対応 body shape は body span と workaround 付き `UnsupportedInstruction` で拒否する。
- 検証: focused lambda multi-statement converter test pass。

### AoT CSE temp-var dead scaffold removed (Issue #6943)

- `optimizer/cse.rs` の未配線だった `_cse_*` temp-var counter / helper を削除し、CSE の実装形状を「先行 binding を再利用する」現在の動作に揃えた。
- repeated pure expression は新しい temporary を作らず、後続 binding の value を先行 binding 参照へ置換する regression test で固定した。
- 検証: focused CSE scaffold removal test pass。

### AoT DCE constant-condition simplification boundary (Issue #6984)

- DCE が `if` / `while` の条件として現れる nested foldable constant expression と boolean `!` を直接評価し、constant folding pass に依存しない単体 DCE 実行でも到達不能 branch / loop を削除できるようにした。
- 対象は side-effect free foldable condition に限定し、未知の call / variable / non-literal expression は従来通り保持する。
- 検証: focused DCE constant-condition tests pass。

### AoT loop optimizer boundary and alias correctness tests (Issue #6946)

- loop unrolling の boundary regression として、empty positive range は body を出さずに削除し、zero step は元 loop を保持することを固定した。
- LICM の alias/依存 correctness として、loop 内で mutate される `acc` に依存する `tmp = acc + 1` を hoist しないことを固定した。
- 検証: focused loop optimizer boundary/alias tests pass。

### AoT rooting conservative liveness coverage and cost checks (Issue #6989)

- heap-shaped `StaticType` (`String`/Array/Tuple/Dict/Range/Struct/Function/Union/Any) が low-level backend の rooting model を要求し、primitive scalar は over-root しないことを regression test 化した。
- current Rust backend の conservative liveness collection が native scalar locals を obligation に含めず、runtime `Value` live set だけを safepoint obligation として列挙することを cost guard として固定した。
- 検証: focused rooting coverage/cost tests pass。

### AoT many-function compile-time scaling stress test (Issue #7002)

- 128 個の typed `Int64 -> Int64` 関数を call-chain で全到達可能にした synthetic Core IR program を追加し、DCE / inference / AoT IR conversion / optimizer / codegen の一連の pipeline が現実的な時間に収まることを regression test にした。
- source-level function input は既知の #7010 に阻まれるため、今回の stress は Core IR を直接組み立てて AoT pipeline の scaling を測る形にしている。
- wall-clock assertion は pathological regression 捕捉用に 30 秒の広い閾値とし、通常の release unit runtime では大幅に下回る想定。
- 検証: focused many-function stress test pass。

### AoT native-call boundary design and gate (Issue #6988)

- `ccall` / `llvmcall` / `Core.Intrinsics.llvmcall` を ordinary function call ではなく AoT native-call boundary として分類し、現時点では safe subset 未実装のため span 付き `UnsupportedInstruction` で拒否する設計にした。
- future supported path は `AotNativeCallSupport::Supported { abi: AotCallAbi }` に限定し、static signature validation と ABI lowering なしの ad hoc native call 受理を禁止する。
- pass verifier でも `ccall` / `llvmcall` / qualified `*.ccall` / `*.llvmcall` が backend に ordinary call として到達した場合は `InvalidIR` で拒否する。
- 検証: native-call classifier tests、Core IR converter span tests、pass verifier backstop test pass。

### AoT top-level heap global initializers reject before Rust static codegen (Issue #7011)

- top-level `String` など Rust の `static` const initializer にできない global initializer を、generated Rust ではなく Core IR -> AoT IR 変換時点で span 付き `UnsupportedInstruction` として拒否するようにした。
- manually constructed `AotProgram` でも同じ invalid static emission を避けるため、codegen 側にも scalar primitive global 以外の backstop gate を追加した。
- `let` block の local binding は引き続き generated Rust local として利用できる。
- 検証: focused converter/codegen tests と generated `let` string concat compile smoke pass。

### AoT string concatenation audit (Issue #6970)

- `String` / `Char` の Julia `*` concat を AoT static binary op codegen に接続し、Rust の未定義な `String * String` 生成を避けるようにした。
- literal `*` concat は constant folding で `LitStr` へ畳み込み、`string(...)` / interpolation-shaped concat は既存 `StringConcat` builtin の `format!` 出力を regression test で固定した。
- `repeat` と full Base string semantics は未対応範囲として support matrix に残す。
- 検証: upstream Julia string concat smoke、focused codegen/optimizer unit tests pass。

### juliars C ABI export entry generation (Issue #6990)

- `--export-c-abi <symbol>` / `--export-c-abi <symbol=function>` を追加し、generated Rust に
  `#[no_mangle] pub extern "C" fn ...` entry を出力できるようにした。
- `aot::abi` の call ABI classification を使い、C-stable scalar signature のみ許可する。`String`/配列/struct など
  Rust-native だが C ABI として安定でない型、または runtime `Value` boundary が必要な型は codegen error として拒否する。
- overload された Julia 関数は ambiguous とし、`add_i64_i64` など generated method 名、または `sjulia_add=add_i64_i64` alias を要求する。
- 検証: focused C ABI codegen tests、CLI parser tests pass。

### juliars colored/structured span diagnostics (Issue #6996)

- `--diagnostic-format human|json` と `--color auto|always|never` を追加し、CLI diagnostics の出力形式を選べるようにした。
- `UnsupportedInstructionDiagnostic` が span を持つ場合、human 出力で `--> source:line:column`、source excerpt、caret marker、
  workaround を表示する。JSON 出力では kind/source/message/span/workaround を structured payload として出力する。
- 検証: bin parser/rendering tests (`aot` / `juliars`) pass。

### juliars `--target` for binary helper builds (Issue #6994)

- `--target <triple>` / `--target=<triple>` を CLI に追加し、`--emit-binary` の一時 Cargo build へ target triple を渡す。
- `--target` 単独は Rust source generation には効果がないため usage error とし、`--emit-binary` 併用を必須にした。
- `docs/aot/README.md` から旧未実装記載を削除し、target は事前に Rust toolchain へ追加する必要があることを明記。
- 検証: parser tests、no-target `--emit-binary` smoke、AoT clippy pass。

### juliars `--emit-binary` native build helper (Issue #6928)

- CLI に `--emit-binary <path>` を追加し、generated Rust を一時 Cargo project に包んで `subset_julia_vm_runtime` path dependency
  として link し、native binary を出力できるようにした。
- `-o <file>` を併用した場合は Rust source も保存し、`--emit-binary` 単独では Rust source を temp project 内だけに保持する。
- `docs/aot/README.md` の flag list から旧「未実装」記載を削除し、`--emit-binary` の実装済み動作を追記した。
- 検証: parser tests、release `juliars --emit-binary` smoke、AoT clippy pass。

### Uninitialized AoT globals no longer emit TODO Rust comments (Issue #6937)

- `AotGlobal::new(...)` のような未初期化 global を Rust `// TODO: static ...` コメントとして silently emit する経路を廃止し、
  `UnsupportedInstruction` diagnostic と workaround に置き換えた。
- Julia の未代入 global は default 値ではなく未定義エラー semantics を持つため、AoT が faithful な global storage/access を実装するまでは
  default-initialized Rust static にはしない。
- 検証: `uninitialized_global_is_rejected_issue_6937` と既存 initialized global codegen regression pass。

### Stable generated-Rust link helper (Issue #6951)

- `scripts/juliars_build_generated.sh <generated.rs> <output-binary>` を追加し、`juliars` 出力を一時 Cargo project に包んで
  workspace の `subset_julia_vm_runtime` を path dependency として link する標準手順を用意した。
- `docs/aot/README.md` の generated Rust build 手順を helper-first に更新し、直接 `rustc --extern ... -L ...` は手動 fallback
  として残した。
- 検証: `bash -n scripts/juliars_build_generated.sh` と generated sample の helper build smoke。

### Generated Rust と `subset_julia_vm_runtime` の ABI compatibility check (Issue #6952)

- compiler 側 `aot::abi` と runtime crate root に `AOT_RUNTIME_ABI_VERSION` を追加し、generated Rust prelude が
  `subset_julia_vm_runtime::AOT_RUNTIME_ABI_VERSION` との compile-time equality check を emitted するようにした。
- runtime crate の ABI version が合わない場合は generated Rust の compile 時点で array length mismatch として失敗し、
  incompatible runtime に silent link しない。
- 検証: compiler/runtime version equality test、generated prelude assertion test、generated Cargo project の
  `cargo clippy -- -D warnings` smoke pass。

### Generated Rust snapshot tests for AoT codegen drift (Issue #7000)

- `AotCodeGenerator` の generated Rust に inline snapshot tests を追加し、single function codegen と multi-method program
  (mangled static methods / dynamic dispatcher / emitted `main`) の安定形状を pin した。
- helper prelude 全体は snapshot 対象から外し、drift 検出に必要な `pub fn` sections だけを抽出して noisy な fixture churn を避ける。
- 検証: `timeout 1800 cargo nextest run --release -p subset_julia_vm --features aot --lib generated_rust_function_snapshot_issue_7000 generated_rust_program_snapshot_issue_7000` pass。

### Generated Rust cargo clippy smoke を AoT gate に追加 (Issue #7001)

- `scripts/test_aot.sh` に generated Rust の Cargo project smoke を追加し、`juliars` で生成した代表コードを
  `cargo clippy -- -D warnings` で検証するようにした。
- generated prelude に Clippy 用の `needless_range_loop` / `no_effect` allow を追加し、生成 helper と expression-statement
  codegen の既知形状を明示的に許可した。
- 検証: 手元の generated Cargo project が `cargo clippy -- -D warnings` pass、`bash -n scripts/test_aot.sh` pass。
  `shellcheck` はローカル未インストールで未実行。

### AoT generated Rust を `rustc -D warnings` clean にする (Issue #6950)

- generated Rust prelude に `#![allow(unused_imports)]` と `#![allow(unused_must_use)]` を追加し、静的に inlined された
  main expression や未使用 runtime imports が `-D warnings` で落ちないようにした。
- `subset_julia_vm_runtime::prelude` から `RuntimeResult` も再 export し、generated dispatcher signatures の root import を有効化した。
- 検証: release `juliars` で生成した sample を `rustc -D warnings -O ... --extern subset_julia_vm_runtime=...` で compile。

### AoT 変更時に `scripts/test_aot.sh` を走らせる PR CI gate (Issue #6953)

- `.github/workflows/ci.yml` に `aot-gate` job を追加し、PR/push の changed files が `subset_julia_vm/src/aot/`、
  `subset_julia_vm/src/bin/aot.rs`、`subset_julia_vm_runtime/`、`scripts/test_aot.sh`、`docs/aot/`、CI 自身に触れた場合だけ
  `bash scripts/test_aot.sh` を実行する。
- job 内で `cargo-nextest` と `clippy` component を用意し、既存ローカル AoT gate と同じ nextest + clippy 経路を CI に接続した。
- 検証: workflow YAML parse check pass。

### AoT `missing` literal support (Issue #6935)

- `AotExpr::LitMissing` を追加し、Core IR `Literal::Missing` -> AoT IR -> verifier/rooting/optimizer -> Rust codegen
  (`Value::Missing`) まで接続した。
- `StaticType::Missing` の Rust projection を runtime singleton 用の `Value` に修正し、旧 workaround (#3343) を
  `docs/vm/WORKAROUNDS.md` の resolved table へ移動した。
- 検証: `test_convert_literal_missing_issue_6935` / `test_aot_expr_get_type` / `test_aot_codegen_literal_expressions` pass。

### AoT UnsupportedInstruction diagnostic に span/workaround を付与 (Issue #6992)

- `AotError::UnsupportedInstruction` を plain string から `UnsupportedInstructionDiagnostic` に変更し、message / optional span /
  optional workaround を保持するようにした。
- `ccall` / `llvmcall` native-call boundary rejection は source span と workaround を表示し、`<:` gate も workaround を添えて返す。
- 検証: `reject_error_mentions_span_and_boundary_kind` / subtype gate / exit-code tests pass。

### AoT Rust keyword identifier escaping coverage (Issue #6934)

- AoT Rust codegen の identifier escaping を拡張し、strict/reserved/weak/future keyword は raw identifier (`r#...`)、
  raw identifier としても使えない `self` / `super` / `crate` / `Self` は `_self` 等へ rename するよう整理した。
- 予約語 (`async`, `dyn`, `union`, `gen`, `macro_rules` など) と sanitize/rustc invalid raw identifier の回帰テストを追加。
- 検証: `timeout 1800 cargo nextest run --release -p subset_julia_vm --features aot --lib escape_rust_ident_covers_keywords_issue_6934 ...` pass。

### AoT subtype `<:` placeholder mapping の gate 化 (Issue #6936)

- `BinaryOp::Subtype` を `AotBinOp::Lt` に落とす誤った構造的 placeholder を廃止し、専用 `AotBinOp::Subtype` へ写す。
- Rust codegen は `<:` を値比較として生成せず、Julia 型関係演算の AoT 表現が実装されるまで `UnsupportedInstruction` で拒否する。
- 検証: `subtype_operator_is_gated_issue_6936` / `test_aot_binop_from_core_ir` pass。

### docs/aot README 全フラグ整合性レビュー (Issue #6955)

- `docs/aot/README.md` の CLI flag 記述を現行 `juliars` 実装に合わせ、`--emit-binary` / `--target` が未提供であること、
  当時の `--backend cranelift` reserved gate を明示した。現在の Cranelift CLI reachability は
  Issue #6927 の最新項目を参照。
- 対応サブセット / 既知の制限節を追加し、詳細マトリクスへのリンクを置いた。
- 検証: docs-only change として `rg` で `docs/aot/` と `subset_julia_vm/src/bin/aot.rs` の flag 名を突合。

### AoT 対応サブセット・マトリクス (Issue #7004)

- `docs/aot/SUPPORT_MATRIX.md` を追加し、CLI 入力、型、制御構造、コレクション、数値/組み込み、backend/生成物ごとの
  `対応` / `一部対応` / `gate` / `未対応` を一覧化した。
- `README.md` のドキュメント構成にも matrix を追加。
- 検証: docs-only change として AoT IR/codegen/type surface と既存 milestone issue 群を照合。

### Core IR -> AoT IR 2段変換と feature gate 設計ノート (Issue #7005)

- `docs/aot/DESIGN.md` に Parser/Lowering/Core IR DCE/type inference/Core IR -> AoT IR/named verifier/optimizer/backend/
  `--pure-rust` の各 gate と診断責務を表で追記した。
- 当時の Cranelift は高レベル AoT IR からの CLI 接続が未完了な experimental path と明記した。
  現在の Cranelift CLI reachability は Issue #6927 の最新項目を参照。
- 検証: docs-only change として `compile_program` / `pass_pipeline` / CLI diagnostics の現行責務と突合。

### juliars `--pure-rust` 失敗診断の残存 runtime symbol 列挙 (Issue #6926)

- `compile_program` を共有 pipeline 化し、pure-Rust 出力に `subset_julia_vm_runtime` 参照が残った場合は該当行を列挙する。
  動的 dispatch が残る場合は `AotProgram::diagnose_dynamic_operations()` の説明も `--stats` / `--check` から見える。
- 検証: `scripts/test_aot.sh` 3821/3821 pass + clippy `--features aot --all-targets -D warnings` pass。

### juliars 入力 source の相互排他検証 (Issue #6929)

- `-e` / `--ir` / positional input (`-` stdin 含む) の複数指定を parse 時に拒否し、usage exit code `2` を返す。
- 検証: `subset_julia_vm::bin/aot tests::parse_rejects_conflicting_inputs` / `subset_julia_vm::bin/juliars ...` pass、
  CLI smoke で `juliars -e '1+2' input.jl` が exit code `2`。

### juliars `--stats` の AoT 品質情報を拡張 (Issue #6930)

- 既存の関数数/DCE/命令数/推論数/最適化数に加え、生成 Rust LOC、推定出力 byte 数、残存動的 dispatch site を表示。
- 検証: CLI smoke `juliars -e '1 + 2' --stats --time-passes -o /tmp/juliars_stats_smoke.rs` で新項目表示。

### juliars `--check` dry-run mode (Issue #6931)

- 出力ファイルを書かずに AoT pipeline を通し、fully-static 判定または残存動的 dispatch site を報告する `--check` を追加。
- 検証: CLI smoke `juliars -e '1 + 2' --check` が `OK` を出力し、出力 file なしで成功。

### juliars stdin 入力 / stdout 出力 (Issue #6932)

- positional `-` を Julia source stdin、`-o -` を Rust source stdout として扱う。
- 検証: CLI smoke `printf '1 + 2\n' | juliars - -o /tmp/juliars_stdin_smoke.rs` と
  `juliars -e '1 + 2' -o - >/tmp/juliars_stdout_smoke.rs` が成功。

### `compile_from_ir_bytes` を共有 AoT pipeline に接続 (Issue #6991)

- serialized Core IR bytes を `core_ir_file::load_from_bytes` で復元し、CLI `.sjir` と同じ Core IR -> DCE -> inference
  -> AoT IR -> optimize -> Rust codegen pipeline に渡す。
- `docs/vm/UNIMPLEMENTED.md` から旧 stub 行を削除し、`docs/aot/` の設計/ロードマップも更新。
- 検証: `aot::tests::compile_from_ir_bytes_compiles_serialized_core_ir` / invalid bytes test pass。

### juliars optimization level `-O` / `--opt-level` (Issue #6993)

- `OptLevel::{O0,O1,O2,O3}` と `optimize_aot_program_at_level` を追加し、CLI から最適化段階を選択可能にした。
- 検証: `parse_opt_levels` bin tests pass、AoT full gate pass。

### juliars parse/lowering 診断の debug format 排除 (Issue #6995)

- user source parse errors は parser `Span { ... }` Debug dump を含まない CLI message に整形し、
  structured span から source caret context を表示する。
- lowering errors は `UnsupportedFeature` Display + span context で表示する。
- 検証: `parse_error_keeps_source_context` / `lowering_error_keeps_span_context` pass、CLI smoke `juliars -e 'x = (' --check`
  が exit code `4` と caret context を出力。

### juliars エラー種別別 exit code (Issue #6997)

- usage/io/parse+lowering/unsupported/codegen/internal を分類する exit code table を CLI に追加。
- 検証: `exit_codes_are_classified` bin tests pass、conflicting inputs が `2`、parse failure が `4`。

### juliars `--time-passes` (Issue #6998)

- DCE / type inference / IR conversion / optimization / codegen の wall-clock timing を `CompileResult` に保持し、
  CLI から表示可能にした。
- 検証: CLI smoke `--time-passes` で各 pass と total timing を表示。

### web の WASM size 最適化 profile を有効化 (Issue #6922)

- `subset_julia_vm_web/Cargo.toml` の `[profile.release]`(`opt-level = "s"` / `lto = true`)が
  ワークスペース非ルートのため Cargo に無視され(`profiles for the non root package will be ignored` warning)、
  web/WASM ビルドがデフォルト release で出ていた = size 最適化が未適用だった不具合を修正。
- ルート `Cargo.toml` に専用 `[profile.web-release]`(`inherits = "release"` + `opt-level = "s"` + `lto = true`)を新設し
  web 限定に。非ルートの `[profile.release]` は削除。`scripts/wasm_build_with_cache.sh`・`Makefile`・README/docs の
  `wasm-pack build` を `--profile web-release` へ更新(VM/runtime の release・test の compile time には非波及)。
- 効果(raw `.wasm`): 17,920,826 B → 14,218,760 B(約 20.7% 減)。

### `ValueType` 降格エピック #6916 開始 — 変換面の primitive テーブル重複除去 (Issue #6916, Slice 1–2)

- #6720 クローズに伴い分離した #6916(「`ValueType` を `CoreType`/`LatticeType` の薄い codegen ビューへ」、特大、
  `ValueType::` 4539 uses)の behaviour-preserving な変換面削減スライス群。
- **Slice 1** (PR #6917): `compile/bridge.rs::concrete_type_to_julia_type` の primitive 21 アームを共有ハブ
  `inference_core::core_type_to_julia_type` へ委譲(委譲は `Primitive(_)` 限定、abstract/`Any` の `Any` widen は維持)。
- **Slice 2**: `impl From<&CorePrimitive> for ValueType`(単一ソース、全網羅で compiler が完全性保証)を新設し、
  `From<&LatticeType> for ValueType` 逆ブリッジの 21 ネストアームを `Core(Primitive(p)) => ValueType::from(p)` へ
  collapse(abstract/`Any`/catch-all とは disjoint)。
- 各スライス characterization + 不変条件テストで pin。full `--release` 3866/3866 + AoT green、clippy 0 warnings。
- 知見: forward 方向(`ValueType → X`)の primitive テーブルは early-return 化すると残り match の exhaustiveness を失う
  ため対象外。reverse 方向(`X → ValueType`)は match が `p` を束縛でき clean に collapse 可能。`value_type_to_julia_type`
  の 2 実装は ArrayOf rank/Memory/Union で意図的に分岐し統合不可。

### `ValueType` 降格エピック継続 #6919 Slice 3 — `CoreType → ConcreteType` primitive 恒等 fold (Issue #6919)

- #6916 クローズ後の継続エピック。`From<&CoreType> for ConcreteType`(`compile/lattice/types.rs`、#6599 ダウンエッジ)
  の 21 `CorePrimitive` アームが全て `CorePrimitive::X => ConcreteType::Core(CoreType::Primitive(CorePrimitive::X))` の
  恒等写像だったため、単一束縛アーム `CoreType::Primitive(primitive) => ConcreteType::Core(CoreType::Primitive(primitive.clone()))`
  へ collapse。アップエッジが既に行う `ConcreteType::Core(c) => c.clone()`(#6720)の対称版で挙動不変。
- 全 21 primitive 網羅の pin テスト `coretype_to_concretetype_maps_every_primitive_to_identity_issue_6919` を追加。
  impl doc の stale 記述(「no callers yet」)を `julia_type_to_concrete_type_lossy`(#6599 Slice B)経由の現状へ更新。
- full `--release` 3868/3868 + AoT gate(3808/3808 + clippy aot)green、clippy `-D warnings` 0、fmt clean。
- 残スコープ: 本体(`ValueType::` 4539 uses の薄ビュー化、特大/高リスク)は引き続き多 PR スライス前提。

### `ValueType` 降格エピック継続 #6919 Slice 4 — 重複 `ArrayElementType → ValueType` の単一化 (Issue #6919)

- `vm/specialize/expr.rs::value_type_from_array_element_type` が canonical な `ArrayElementType::to_value_type`
  (`vm/value/array_element.rs`)と全 26 variant 同一の重複だったため削除し、2 caller を `elem_ty.to_value_type()` へ。
  pin テスト `to_value_type_maps_every_variant_issue_6919`(全 26 variant)追加。
- full `--release` 3869/3869 + AoT gate green、clippy `-D warnings` 0、fmt clean。
- 知見: forward `ValueType → X` の他テーブル(名前文字列/AET、`compute_union_display`・`value_type_to_type_name`・
  cache・`runtime_generator_value_type_name` 等)は部分集合・fallback(None vs "Any")・native-int 扱いが意図的に
  異なり安全な dedup 不可。クリーンな exact-dup はこの 1 件のみ。残りは XL per-site 本体。

### hot-loop recognizer を再利用可能な pipeline 化 → Milestone #27 完了 (Issue #6829)

- `vm/executable.rs::predecode_range` の手書き if-else recognizer chain(euclidean / complex-mandelbrot /
  typed-loop)を、順序付き recognizer registry `LOOP_RECOGNIZERS: &[LoopRecognizer]` 駆動に置換。
  `LoopRecognizer = fn(&[Instr], usize, usize) -> Option<ExecutableBlock>` で match→validate→typed-IR build の
  契約を統一。新しい最適化 shape の追加は **registry への 1 行追記**(predecode 制御フロー編集不要)に。
- predecode は program install 時のみ(`from_bytecode`/`append_bytecode`)実行され実行 hot path には乗らないため、
  registry 化は **runtime perf 完全中立**(recognizer/executor 不変・順序保持で認識結果 byte 同一)。
  executor 側は block 種別ごとに既に汎用(`TypedLoopBlock` は `TypedLoopOp` IR を持つ)。pipeline ユニットテスト追加、
  既存 fast-path(calc_pi/mandelbrot/gcd)維持、full `--release` 3865/3865 green。
- → **Milestone #27「sjulia VM 周辺リファクタリング」全 11 issue 解決(#6826–#6836、PR #6904–#6914)**。

### compile↔runtime dispatch 境界の明確化 (Issue #6836, Milestone #27)

- 共有 dispatch ユーティリティの neutral module は既に `inference_core`(#3508: selection/specificity/
  dispatch_resolver/subtype/type_core)として compile/vm 両側で共有済み。型変換は `compile::bridge` が
  documented bridge。本対応で**境界契約を `inference_core/mod.rs` に明文化**(compile=型既知時に静的解決
  `CallResolved`、vm=実行時型のみ既知時に `find_best_method_index`、両者は同じ selection core を通るため
  同一入力で必ず同一メソッドを選ぶ)。
- **compiler↔runtime method-selection parity fixture** を追加
  (`dispatch/compile_runtime_dispatch_parity_6836.jl`): 各シナリオを静的引数(compile path)と Any コンテナ
  要素(runtime path)で 1 回ずつ dispatch し一致を `@test`。single/multi-arg・parametric container を網羅、
  upstream julia 1.12.6 でも 11/11 pass。full `--release` 3864/3864 green。

### `dispatch_instr` を macro 生成化(網羅性・perf 維持) (Issue #6827, Milestone #27)

- 428 行・全 ~200 `Instr` variant の網羅 match だった `dispatch_instr` を `dispatch_instr_match!` macro へ
  退避し、関数本体を **3 行**(`dispatch_instr_match!(self, instr)`)に。
- 検討した `Instr::category()` 間接化方式は bench で +1〜5% の regression(criterion p<0.05)が出たため不採用。
  macro 展開は元の `match instr {...}` と **byte-identical**(jump table 含め codegen 不変)で **regression ゼロ**を
  保証しつつ、生成 match が網羅的なので新 variant を handler 未登録で追加するとコンパイルエラー。
  full `--release` 3864/3864 green、calc_pi/mandelbrot bench は codegen 同一のため不変。

### `vm/exec/call.rs` の keyword-default 評価を簡素化 (Issue #6832, Milestone #27)

- 626 行・8 引数の `eval_simple_kw_default_function` が検証・引数束縛・kwarg デフォルト評価・≤64 step の
  mini interpreter を一手に抱えていた。`global_frame`/`global_slot_map` を `KwDefaultEvalCtx` へ集約、
  呼び出し対象を `KwDefaultCallRequest`(func_index/func/args/kwargs)へ束ねた。
- mini stack-machine を `run_kw_default_body(ctx, func, frame, depth)` へ抽出。`eval_simple` は
  **626 → 47 行**(検証 + 束縛 + run_body 呼び出し)。kw-default ファミリ全関数が **≤5 引数**
  (eval_kw_default_expr/args/kwargs/call=5、eval_simple=3)。mini interpreter に専用ユニットテスト 5 件追加。
  挙動不変、full `--release` 3864/3864 green。

### VM ランタイムの深いネスト削減 (Issue #6833, Milestone #27)

- issue 記載の 6 ファイルすべてで「production 関数のインデント ≤ 40 spaces」を達成
  (`call_dynamic.rs` 68→40、`iteration.rs`/`call_function_variable.rs`/`call.rs`/`array_basic.rs`/
  `builtins_types_conversion.rs` 52→≤40)。
- 手法: 深いブロックを helper(`typed_array_element_push_value`・`merge_kwargs_splat_value`・
  `float_conv_struct_ref`・`dynamic_candidate_arg_mismatch`/`user_metadata_candidate_indices`/
  `base_empty_metadata_candidate_indices`/`resolve_scored_family_fallback`/`resolve_tier_filtered_fallback`・
  `iterate_first_struct_dispatch`/`iterate_next_struct_dispatch`)へ抽出、`if let` 連鎖を `let ... else`/`?` に変換。
  borrow 制約のあるホットパスは disjoint field を渡す自由関数で回避。抽出した純粋ヘルパーにユニットテスト追加
  (7 件)。挙動不変、full `--release` 3859/3859 green。

### `BuiltinId`/`Intrinsic` の name↔ID テーブルを単一ソース化 (Issue #6831, Milestone #27)

- `builtins.rs`/`intrinsics.rs` は `from_name`(string→enum)と `name`(enum→string)を 2 つの巨大な手書き
  `match`(各 ~400/~150 arm)で重複保持し、手動同期が必要だった。`define_builtin_table!` /
  `define_intrinsic_table!` macro を導入し、`Variant: "canonical" => ["from_name", ...]` の 1 テーブルから
  両方向を生成(net −554 行)。
- `enum` 自体は**手書き維持**(豊富なコメント + bincode discriminant 順序の保持)。`name()` の網羅 match が
  全 variant のテーブル登録をコンパイル時に強制。プラットフォーム依存の `Int`/`UInt`(host pointer width 連動、
  guard 付き)は macro template に boilerplate として埋め込み、`Meta.*` の name≠from_name 非対称・name-only・
  alias(`memoryref`|`memoryrefnew`)も忠実に再現。edge case ユニットテスト追加、doctest 維持。
  full `--release` 3852/3852 green、挙動・discriminant 不変。

### `vm/mod.rs` god file を分割 (Issue #6826, Milestone #27)

- 3,851 行の `vm/mod.rs`(単一 `impl Vm<R>` に dispatch / type matching / equality / lifecycle が混在)を
  3 つの `impl Vm<R>` モジュールへ分割:
  - `vm/equality.rs`(523 行): `compare_*` 構造的/egal/`==` 比較 + host-return 配列正規化
  - `vm/dispatch.rs`(1,468 行): `type_matches`/`value_matches_param*`、`find_best_method_index` + dominance/
    specificity 前段、`bind_type_params`/`bind_ntuple_params`
  - `vm/state.rs`(1,179 行): `new`/`new_program`、local/global/output accessor、value 型クエリ、
    error handling/`raise`、call-site inline/dispatch cache、stack/compare 実行ヘルパー
  `Vm` struct 定義は `mod.rs` に残置。**`vm/mod.rs` は 3,851 → 757 行**(< 1,000 達成、thin module root 化)。
- 手法: 各 submodule は `use super::*;` で型を取得、移動した private メソッドは `pub(super)` 化(子→親 private は
  呼べるが逆は不可のため)。公開 API(`pub fn`)・discriminant・挙動は不変。full `--release` 3849/3849 green。

### native Array wrapper carrier の抽象境界を完成 (Issue #6834, Milestone #27)

- Milestone #26 で carrier は `Value::ExprArgs` に confine 済み(layout を知るのは `native_array_*`
  ヘルパー + `array_wrapper.rs` のみ。production で variant を直接 match する箇所は無し)。残っていた
  40+ 箇所の低レベル probe `native_array_value_ref(x).is_some()` を高レベル境界述語
  `is_native_array_value(x)`(carrier 定義の隣 `vm/value/array_value` に新設、`vm/value` 経由で再エクスポート、
  `native_array_compat` でも再公開)に集約。
- `native_array_compat.rs` の module doc を post-#6888 の確定境界(dispatch-fence policy のみを持つ)に更新。
  挙動不変、full `--release` 3849/3849 green。

### `vm/formatting.rs` を category 別に分割 → #6835 完了 (Issue #6835 part B, Milestone #27)

- 2,898 行の `formatting.rs` を `formatting/` ディレクトリ化し、自己完結度の高い category を submodule へ抽出:
  `formatting/numeric.rs`(float/BigFloat 表示)、`formatting/sprintf.rs`(C 風 printf)、
  `formatting/julia_code.rs`(`value_to_julia_code`/`expr_to_julia_string` の Julia ソース化)。
  value 表示の dispatch core(struct/array/value)は密結合のため `mod.rs` に集約維持。
- 外部サーフェス(`format_float_julia`/`format_bigfloat_julia`/`format_sprintf`/`format_printf_float`/
  `expr_to_julia_string` 等)は `mod.rs` の `pub(crate) use` で再エクスポートし呼び出し側無変更。
  `julia_code` のテストは `pub(super)` 経由で参照。挙動不変、full `--release` 3849/3849 green。
  → part A(container 分割)と合わせ **#6835 完了**。

### `vm/value/container.rs` を value type 別モジュールに分割 (Issue #6835 part A, Milestone #27)

- 2,279 行の `container.rs` が `Generator`/`NamedTuple`/`Pairs`/`Dict`/`Set`/`ComposedFunction`/`Expr` を
  一括保持していた。各 value type を専用モジュール(`vm/value/{generator,named_tuple,pairs,dict,set,
  composed_function,expr}.rs`)へ移設し `container.rs` を削除。`mod.rs` の宣言と再エクスポート、
  `value_enum.rs` の import のみ更新(外部 consumer は `crate::vm::value::*` 経由なので無変更)。
- Dict のキー/ハッシュ機構とテスト群(ほぼ Dict 専用)は `dict.rs` に集約。`#![allow(clippy::cast_sign_loss)]`
  (元の SAFETY ガード)を `dict.rs`/`named_tuple.rs` に保持。挙動不変、full `--release` 3849/3849 green。
- 残: `formatting.rs` の category 別分割(part B、#6835 の残スコープ)。

### HOF/broadcast/generator runtime state を frame.rs から分離 (Issue #6828, Milestone #27)

- `vm/frame.rs`(1,230 行)が call-frame/local-slot 機構と HOF/制御フロー実行状態を混在させていた。
  `HofOpKind` / `BroadcastInput` / `BroadcastResults` / `BroadcastState` / `RuntimeCallableResult` /
  `ComposedCallState` / `GeneratorIterateKind` / `GeneratorIterateState` / `SprintState` を新モジュール
  `vm/hof_exec/state.rs` へ移設。`frame.rs` は `Frame`/`LazyLocalMap`/`VarTypeTag`/slot accessor と
  例外ハンドラ `Handler` のみに(1,230 → 995 行)。
- consumers(`vm/mod.rs`・`vm/hof_exec/*`・`vm/exec/*`・`vm/type_ops/iteration.rs`・`vm/builtins_macro/eval.rs`)の
  import を `frame::` → `hof_exec::state::` に切替。挙動不変、full `--release` 3849/3849 green。

### Array wrapper ヘルパーの重複排除 (Issue #6830, Milestone #27)

- Pure Julia `Array{T,N}` wrapper の runtime 表現を覗くヘルパー
  (`is_array_wrapper_struct_name` / `array_wrapper_shape_and_offset` /
  `array_wrapper_shape_from_tuple`)が compile/vm/repl の 9 ファイルに byte-identical
  コピーで散在していた。`vm/value/array_wrapper.rs` を単一ソースとして `pub(crate)` 化し、
  各コピーを共有ヘルパー呼び出しに置換(正味 -68 行、ユニットテストを 1 箇所に集約)。
- `vm/type_ops/iteration.rs` のローカル版は `Array`/`Array{` のみ照合する**別 predicate**(Vector/Matrix を
  含まない)、`builtins_strings.rs`/`builtins_linalg.rs` の版は op 名付き `VmError` を返す**別契約**のため
  意図的に残置(挙動不変を優先)。full `--release` 3849/3849 green。

- epic #5916 §4 Phase 6 の representation flip(`ConcreteType` → `Core(CoreType)` + lattice-only carrier)。
  全 nullary(primitive/abstract/`Any`)を `Core(CoreType)` へ畳む **nullary 半分が完了**。PR #6900/#6901/#6902、
  各 full `--release` 3846/3846 + AoT 3786/3786 green。
- 一括 flip は import 整合がカスケードして revert →「ファイル数で小バッチ化」して green 増分で着地。
  container 系(`Array`/`Tuple`/`Dict`/`Set`/`Range`/`Generator`/`NamedTuple`/`UnionOf`)は子に `ConcreteType` を
  持ち lattice が再帰操作するため **carrier 維持**で確定(Core 化は深い改修で利益薄)。
- doc 終点像「`CoreType` + lattice-only carrier の薄い wrapper」を実質達成。残(`Struct`/`DataType`/`Module`/
  `Named`-nullary の Core 化)は利益薄で deferred。詳細 `docs/vm/CONCRETETYPE_RETIREMENT.md`。

## 最新対応 (2026-06-18)

### `Int(::BigFloat)` / `floor(Int, ::BigFloat)` 整数変換 (Issue #6890)

- `Int(big(2.0))` や型付き丸め `floor(Int,x)`/`round(Int,x)`/`ceil(Int,x)`/`trunc(Int,x)`(= `T(round(x))`、
  `base/floatfuncs.jl`)が `Cannot convert BigFloat(...) to Int64` で失敗していた。`convert_to_iNN`/`convert_to_uNN`
  (`vm/type_ops/conversion.rs`)に BigFloat アームが無かった。
- `RustBigFloat::to_bigint_exact`(整数値チェック: `trunc(x) == x`、その後 astro_float の十進 Display を
  `num_bigint::BigInt` へパース)を追加し、全 10 変換(i8..i128, u8..u128)に BigFloat アームを配線。
  `ToPrimitive` で各幅へ範囲チェックし、非整数値・非有限・範囲外は `InexactError`(本家
  `(::Type{<:Integer})(::BigFloat)` 準拠)。Float64 範囲を超える値(`2^70`、18 桁整数)も正確。
  Rust のみで base cache 再生成不要。本家 1.12.6 一致。
  fixture `bigfloat/bigfloat_int_conversion_6890.jl`(parity OK)。

### 汎用 `div`/`divrem` を trunc 化 + 動的 Float `%` を truncated rem 化 (Issue #6891 / #6895)

- 汎用 `div(x, y)`(`base/math.jl`)が `floor(x / y)` で −∞ 方向に丸めていた。本家 `div` は **0 方向(RoundToZero)**
  なので、異符号の Float64 / BigFloat で乖離(`div(-7.0, 3.0)` が −3.0、本家 −2.0)。`trunc(x / y)` に修正。
  Int 経路は typed sdiv で元から正しく、`fld`/`cld` は本来 floor/ceil なので不変。
- `divrem(x, y) = (div(x, y), rem(x, y))` の rem 部も誤っていた(#6895): 動的 Float `%`(`SremInt` fallback、
  `vm/exec/binary_both.rs` ×2)が `a - floor(a/b)*b` = mod を計算していた。`%`/`rem` は **truncated remainder**
  (被除数の符号)なので `a - trunc(a/b)*b` に修正。typed/特殊化経路と BigFloat(#6796 の `RemBigFloat`)は元から
  trunc で、plain-float の動的 fallback だけが取り残されていた。`mod` は `base/math.jl` で `%` から符号調整して
  導出するため、`%` が真の剰余を返せば従来どおり正しい。
- これで `divrem(-7.0, 3.0) == (-2.0, -1.0)`(Float64 / BigFloat とも)本家 1.12.6 一致。
- fixture: `math/div_trunc_negative_6891.jl`(新規、@testset + gating tail で parity 可能かつマスクされない実ゲート)、
  既存 `math/divrem_fldmod.jl` の stale な誤期待値(`divrem(-7,3)`/`fld1`/`fldmod1`)を本家一致へ修正し gating tail を追加。
  両 fixture parity OK。math.jl 変更のため base cache 再生成。

### tuple `==` の BigFloat 要素比較 (Issue #6892)

- `(big(2.0),) == (2.0,)` 等、要素ごとは `true` なのに tuple `==` が `false` を返していた。tuple/named-tuple の
  `==` は `TupleEquals` builtin(`values_equal_tristate`)で要素比較を畳み込むが、BigFloat ↔ Float64/Int の要素ペアが
  スカラ `==` のように BigFloat へ昇格されず、`Debug` 文字列フォールバック(表現差で不一致)に落ちていた。
  `value_to_bigfloat`(`StackOps::pop_bigfloat` と同じ昇格規則)を追加し、BigFloat を含む数値ペアを BigFloat に
  揃えて値比較するよう tristate fold へ配線。BigFloat 同士・BigFloat↔Float64/Int・混在/ネスト tuple・#6801 の
  `divrem` 結果ケースで本家 1.12.6 一致。Rust のみで base cache 再生成不要。
  fixture `bigfloat/bigfloat_tuple_eq_6892.jl`(parity OK)。

### BigFloat の `floor`/`ceil`/`round`/`trunc` と `div`/`fld`/`cld`/`divrem`/`fldmod` (Issue #6801)

- `floor(big(2.7))` 等が `expected numeric value, got BigFloat` で未対応だった(丸め経路が f64 変換していた)。
  `RustBigFloat` に astro_float ネイティブ丸め(`floor`/`ceil`/`int`/`round(0,ToEven)`)を追加し、共有ヘルパ
  `apply_unary_rounding_op_with_heap` で全丸め実行点(`FloorF64`/`CeilF64` instr、`Round`/`Trunc` builtin、
  `*Llvm` intrinsic、動的 `CallDynamicOrBuiltin`)へ配線。任意精度保持・本家 1.12.6 一致。Rust のみで base cache 再生成不要。
  fixture `bigfloat/bigfloat_rounding_div_6801.jl`(parity OK 36 assert)。
- スコープ外で `Int(::BigFloat)` 変換(→#6890)、汎用 `div`/`divrem` の負値 floor バグ(→#6891)、
  tuple `==` の BigFloat 要素比較(→#6892)を起票。

### `Value::NativeArray` → `Value::ExprArgs` 改名 + 封じ込め監査(accept & confine) (Issue #6807)

- メンテナ判断で **option 1(accept & confine)** を採用。全 general 配列は wrapper 化済みで、native carrier の唯一の
  origin は `expr.args`(可変 `Vector{Any}` AST 引数)。`struct_heap` に GC が無く Expr ノード毎に生成されるため、
  heap StructRef 化はリーク → Rc carrier の auto-free が正しい(削除ではなく封じ込めが正解)。
- variant `Value::NativeArray(ArrayRef)` を **`Value::ExprArgs(ArrayRef)`** に改名(役割を型名に反映)。
  `native_array_*` converter helper(汎用 carrier アクセサ)は名称維持。
- `scripts/check_value_array_allowlist.sh` の Policy 2 を「zero へ ratchet」から **恒久的な封じ込め allowlist**
  (`EXPR_ARGS_ALLOWLIST`、3 ファイル=variant 定義+arm / converter hub / carrier 単体テスト)へ変更。
  acceptance criterion は「`expr.args` 以外に carrier 無し」。`CODE_AUDITS.md` も更新。
- フル 3845/3845(改名は挙動中立)、AoT・clippy・fmt クリーン。
- **完了(confined)**: #6807 はこれをもってクローズ。一般配列は MemoryRef-backed `Array{T,N}` ラッパーに統一済みで、
  `Value::ExprArgs` carrier は `expr.args` 専用に限定保持。完全削除は `struct_heap` GC 前提の別作業として保留。
- 後続クリーンアップ(#6889): #6882 表示修正で PR #6884 が既マージの #6883 と二重対応していたため #6884 を revert し #6883 に一本化。

### 直積 `for x in xs, y in ys ... end` のネストループ脱糖 (Issue #6865)

- カンマ区切りで複数イテレータを回す直積 `for` が lowering で `UnsupportedForBinding` に
  なっていた(`control_for.rs` が `ForBinding` 複数を一律拒否)。
- upstream `expand-for` 同様、複数バインディングを外→内へネストした `for` 文へ脱糖。
  内側イテレータが外側変数を参照する形(`for i in 1:3, j in 1:i`)も動作。内包表記側は
  既に `MultiComprehension` 対応済みで変更不要。
- フィクスチャ `control_flow/cartesian_for_6865.jl`、lowering 単体テスト 2 件追加。

### パラメトリック制約メソッド `f(x::T) where T<:Real` の直接呼び出し特殊化 (Issue #6868)

- where-メソッドが generic/具象より遅い問題。直接呼び出し経路
  `execute_direct_call_with_func_args` が未特殊化の generic 本体(パラメータが `Any` 束縛、
  内部演算が動的ディスパッチ)へジャンプするだけで、specialization が効いていなかった。
- 直接呼び出し経路で `try_specialized_entry_for_runtime_call` を呼び、実引数の具象型で本体を
  特殊化(キャッシュ付き)。bound は静的ディスパッチで検証済み、`T` は `bind_type_params` で
  束縛済み。where ≈ 具象(~1.05倍)に改善、upstream と値・型・MethodError 一致。
- フィクスチャ `where/specialized_direct_call_6868.jl`、ベンチ
  `vm_where_specialization_benchmark` 追加。

### 配列の逐次成長 (内包表記 / `push!`) が O(n²) → O(n) (Issue #6873, via #6846)

- Issue #6846(iOS の surface plot ~1.6s)を系統的にプロファイルした結果、ボトルネックは VM 外(Plotly
  JSON 直列化は ~1.16ms / 204KB)ではなく **VM 内の O(n²) 配列成長**だと判明。`Float64[zf(xi,yi) for ...]`
  の 10000 要素構築だけで ~1.2s を占めていた。
- 原因: 内包表記は結果配列を `emit_empty_array_wrapper`(Memory-backed `Array{T}` ラッパ)で確保し、
  毎要素の `ArrayPush` → `push_array_wrapper`(`vm/exec/array_mutate.rs`)が **`undef_typed(new_len)` で
  ぴったりサイズの Memory を作り直し+全要素コピー**(+1 成長)していた → 1 push が O(n) → 全体 O(n²)。
  事前確保最適化(Issue #5186)の `ReserveArray` は `Value::Memory` にしか効かず、配列が wrapper 化した
  #6649/#6807 以降は no-op で死んでいた。`push!` ループも同経路で O(n²)。
- 修正: `push_array_wrapper` の `Value::Memory` / `Value::MemoryRef` 分岐で、ラッパが親 Memory を先頭から
  論理長ぴったり連続所有する一般ケース(`offset==1`/`memref.offset==0` かつ `mem.len()==len`)では、
  親 Memory の償却 `push()`(Vec 幾何成長)で **in-place 追記**。前方オフセット付きビュー等は安全のため
  従来の realloc にフォールバック。
- 効果: 10000 要素の内包表記/`push!` が **1213ms → ~8ms(~140倍)**、O(n²)→O(n)。surface カーネルは
  ~1.4s → ~0.17s。upstream Julia 1.12 と値・型・順序・エイリアス挙動すべて一致。
- 単体テスト `array_push_grows_wrapper_in_place_amortized`(親 Memory の Rc 同一性=in-place を保証)、
  parity フィクスチャ `arrays/growth_amortized_6873.jl`(37 assert)、ベンチ
  `vm_array_benchmark::growth_comprehension_push_2048` を追加。

### Complex×Real 混在配列リテラルの要素型昇格 (Issue #6867)

- `[1.0 + 0.0im, 2.0]` が upstream の `Vector{ComplexF64}` に対し `Vector{Any}` になり、
  `norm` 等の `Complex{Float64}` 特殊化メソッドに乗らず実行時型エラーになっていた。
- `infer_array_element_type` に Complex×Real 昇格ブランチを追加(`promote_type`/`promote_complex`
  で畳み込み)。実数要素は emit 時に `Complex{T}(x, 0)` で widen。fixture
  `complex/complex_real_mixed_array_literal_6867.jl` 追加。整数 Complex の `[1+0im, 2]` は
  別ギャップ(`1+0im` のコンパイル時推論が `Any`)として残存・スコープ外。

### `sinc(norm([x,y]))` カーネルの norm/sinc 高速化 (Issue #6846)

- PR #6849(配列リテラルのネイティブ確保)後の追加プロファイルで残りコストの最大が `norm`(~48%)と判明。
  `LinearAlgebra.jl` の全 `norm` メソッドの内側ループをインデックス走査から直接イテレーション
  (`for xi in x`)へ変更し norm ~2.2倍速、`sinc(x::Float64)` 具象メソッド追加で sinc ~1.5倍速。
  フルカーネルは 1.055s → 0.603s(−43%)、upstream Julia 1.12 と値一致。
- フィクスチャ `linalg/norm_iter_sinc_6846.jl` 追加。発見した別件: #6865/#6866/#6867/#6868。
- 追補: 同テクニックを cos 系 generic ラッパーにも適用 — `cosc`(~1.4x)/`sincos`(~2.3x)/`tanpi`(~1.1x)
  に `::Float64` 具象メソッドを追加(挙動不変、既存 fixture で parity 担保)。

### キーワード引数の配列/タプルリテラルのデフォルトが `0` になるバグ修正 (Issue #6876)

- 症状: `f(; x=[1,2]) = x; f()` が `0` を返す(本家は `[1, 2]`)。`(1,2)` タプル・`Float64[]`・
  内包表記のデフォルトも同様に `0` 束縛。スカラ/文字列リテラルや `zeros(2)` 等の呼び出しデフォルトは正常。
- 原因: ソースの配列リテラル `[1,2]` は `Expr::ArrayLiteral`(タプルは `Expr::TupleLiteral`)としてパースされ、
  畳み込み済みの `Literal::Array` ではないため、事前評価デフォルトの fast path
  (`compile/utils.rs::eval_literal_default`)が `_ => Value::I64(0)` に落ちていた。さらに
  `lowering/function/kw_defaults.rs::default_needs_body_eval` は配列/タプルリテラルを要素に call を含む時のみ
  body 再評価へ回していた。
- 修正: `Expr::ArrayLiteral` / `Expr::TupleLiteral` / `Expr::TypedEmptyArray` / `Expr::Comprehension` /
  `Expr::MultiComprehension` のデフォルトを無条件で per-call body 再評価へ。本家の「省略時は毎回フレッシュな配列」
  セマンティクスと一致(`push!` するデフォルトが呼び出し間で漏れない)。
- 副次効果(#6807 関連): 配列リテラルデフォルトが実行時 VM コンテキストで materialize されるため、
  コンパイル時のネイティブ配列キャリア注入(`compile/utils.rs` の `literal_array_value`)に到達しなくなり、
  carrier 撤去キャンペーンの compile-time injector を縮退させる。
- フィクスチャ `kwargs/kwarg_literal_default_6876.jl` 追加(本家 1.12.6 とパリティ一致、15 アサーション)。

### `Value::NativeArray` carrier 撤去: HOF value-mode result producer を wrapper 化 (Issue #6807)

- #6882(表示修正)で解錠された最大の fresh-build root injector を flip。
  `hof_exec/value_mode.rs::create_typed_array_from_values` が `Array{T,N}` wrapper を返すように変更
  (`array_value_to_wrapper`)、かつ **ネスト配列ラッパー要素のネイティブ変換(line 855、#5229 対策)を撤去**。
  #6882 でラッパー要素の typeinfo 表示が正しくなったため、ネスト `map` 結果は wrapper-of-wrapper のままで
  bare 表示・indexing・mutation すべて正常。
- フィクスチャ `hof/value_mode_nested_wrapper_result_6807.jl`(11 アサーション、1.12.6 パリティ)。フル 3845/3845、AoT green。
- 残: `value_mode.rs` の他 `array_value` サイト(empty/非 wrap/FindAll)、`expr.args` 表現、FFI 境界。

### 配列ラッパー要素の Vector が誤った `Array{T,N}[...]` typeinfo プレフィックスを出すバグ修正 (Issue #6882)

- 症状: 型付き `T[...]`/`T[]` 形式で作った配列を要素に持つ `Vector` が `Array{Int64, 1}[[1], [2]]` と表示
  (本家 `[[1], [2]]`)。素のリテラル `[[1,2],[3,4]]` は正常 → 表現依存の乖離。
- 原因: `vm/formatting.rs::value_show_type`(上流 `typeinfo_implicit` 相当)のネスト配列アームが
  ネイティブキャリア(`native_array_value_ref`)専用で、inline `Array{T,N}` ラッパー `Value::Struct` 要素は
  汎用 struct アームに落ち `(struct_name, false)` = `("Array{Int64, 1}", false)` → 非 implicit → 外側プレフィックス。
- 修正: `value_show_type` の `Value::Struct` アームでラッパー struct(`array_wrapper_julia_type().is_some()`)を
  検出し、ネイティブアームと同じく(自己完結な Memory ストレージから)要素型と implicit を算出。
- フィクスチャ `arrays/nested_array_wrapper_typeinfo_prefix_6882.jl`(8 アサーション、1.12.6 パリティ)。フル 3843/3843、AoT green。
- **#6807 への寄与**: HOF value-mode injector の flip を阻んでいた表示問題を解消(ネスト `map` 結果がラッパー要素でも
  正しく bare 表示)→ 次に HOF value-mode を flip 可能に。残: 非 implicit 内側型のネスト typeinfo 伝播(`Int8[1]`→`[1]`)は別件。

### `Value::NativeArray` carrier 撤去: 実測 injector マップ + compile-time/slice producer flip (Issue #6807)

- **B方針確定**: `push!(a::Array,item)` が `a._mem=mem; a._size=(new_len,)` と `a` のフィールドを再代入する
  (base/array.jl)ため、ラッパーは参照セマンティクスの heap `StructRef` 必須(上流の可変 `jl_array_t` と一致)。
  → **B1(全 producer を heap-StructRef 化)が正解、B2(共有 Memory から長さ導出)は却下**。
- **実測 live-injector マップ**(2533 フィクスチャを instrumented `sjulia` で sweep): native carrier はレガシーでなく
  **ホットパスで現役**。fresh-build root injector = `hof_exec/value_mode.rs`(HOF value-mode 結果、#5229 のネスト配列
  leak 防止で load-bearing)と `exec/array_index_slice.rs`(slice 結果)。再ラップ伝播 = `locals.rs:691`(typed-slot)、
  `container.rs:1565`(`expr.args` は `ArrayRef` 格納)等。iteration 行列/`value_enum`/`deep_copy` 等はレガシー/テスト専用
  (実プログラムで未発火)。variant 削除は全 root injector を flip 後、再ラップループが死に、~187 consumer の wrapper-arm が
  full suite で全ケースを覆うことが証明された時点で機械的に可能。
- **本コミットの flip 2件**: (1) `compile/utils.rs` injector 除去 — #6876 で配列リテラル kw デフォルトが body 再評価に
  なったため `eval_literal_default` の配列アームは dead → `literal_array_value` と import を削除(compile-time/no-VM
  injector 消滅)。(2) `exec/array_index_slice.rs` の slice 結果(`a[range]`/`a[idxvec]`/`m[rows,cols]`/n-dim)を
  `array_value_to_wrapper` で wrapper 化(`arr` はローカル所有 `ArrayRef` なので `arr_borrow` と `&mut self` は非衝突)。
- フィクスチャ `arrays/slice_producers_wrapper_6807.jl`(18 アサーション、1.12.6 パリティ、slice は独立可変 wrapper)。
  フル 3843/3843、AoT green、clippy/fmt クリーン。残 root injector(HOF value-mode #5229・`expr.args` 表現・FFI 境界)は
  深く multi-session。

### `Value::NativeArray` carrier 撤去: 残り execution-engine producer を wrapper 化 (Issue #6806)

- 背景: メンテナの Slice 4-9 (#6841-6847, #6807) で build buffer de-variant + range/RNG/matrix・
  zeros/ones/undef・io/macro-mod/reflection-mod・linalg producer が wrapper 化済み。本作業はその間隙の
  `&mut self` execution-engine producer を埋めるもの。
- ratchet hygiene (PR #6855): `formatting.rs`/`plotting/mod.rs` はコメント内言及のみで実コードは
  `native_array_value_ref` 集約ヘルパ経由 → コメントを reword し `NATIVE_ARRAY_ALLOWLIST` から除外
  (**5 → 3**; 残りは variant 定義 `value_enum.rs`・converter `array_value/mod.rs`・carrier test `frame.rs`)。
- Slice 10 (PR #6857): F64-mode HOF 戻り値 producer (`vm/hof_exec/dispatch.rs`) の mapreduce/broadcast
  F64 結果・`findall` Int64 index・broadcast/map/filter-in-place `dest` を wrapper 化。`ArrayRef` 用
  companion `push_array_ref_as_wrapper` を追加。
- Slice 11 (PR #6859): Slice 9 が deferred した hot `exec/binary_both.rs` の scalar·array / matmul-fallback
  結果 producer 6 箇所を wrapper 化。dispatch-order 懸念は full suite (3842/3842) + AoT (3782/3782) でクリア。
- Slice 12 (PR #6861): reflection `subtypes` (`builtins_types.rs`) と `@eval` `vect` リテラル
  (`builtins_macro/eval.rs`) を return 位置用 companion `array_value_to_wrapper(&mut self)` で wrapper 化。
- Slice 13 (PR #6862): `Diagonal`-matmul (`try_matrix_diagonal_mul`, `&Vm`→`&mut Vm`) と
  native-array 入力の deep-copy (`type_ops/deep_copy.rs`) を wrapper 化、binary_both の `array_value` helper 撤去。
- Slice 14 (本 PR): dynamic broadcast 算術 fallback (`dynamic_ops::dynamic_add`/`sub`/`mul`/`div`,
  `&self`→`&mut self`) を wrapper 化、`dynamic_array_value` helper 撤去。
- これで**容易に flip 可能な producer は全て完了**。残る carrier 構築は全て #6807 結合 / 単独では
  クリーンに flip 不可: host 境界 `normalize_host_return_value`(意図的 FFI re-materialize + StructRef 解決、
  外向き FFI 契約変更)、compile時 literal (`compile/utils.rs`, heap 無)、formatting/Mark·Reshape、
  consumer-entangled `iteration::extract_matrix`、aliasing 依存 `container.get_args`(`push!(expr.args,...)`)、
  そして構造的ブロッカーである ~130 `native_array_value_ref` **consumer** borrow-site
  (splat の native+wrapper 併設パターンで variant 削除時に一括除去)。詳細は `docs/vm/ARRAY_MEMORY_MIGRATION.md`。

### iOS/Web/Flutter の `bar` プロットが折れ線で表示される問題を修正 (Issue #6850)

- 症状: iOS アプリで `bar`(および `heatmap`)プロットが**折れ線グラフ**として描画される
  (`plotting_2d.jl` サンプルの `bar!([1,2,3],[0.4,0.8,0.6])` が点を結ぶ赤い線になる)。
- 原因切り分け: VM 側(`subset_julia_vm/src/plotting/plotly.rs`)は `:bar` Series を正しく
  `{"type":"bar",...}` の Plotly JSON にしており、`tests/plot_artifact_mime_tests.rs` の end-to-end
  でも確認済み。バグはホスト側の Plotly バンドルにあった。3 ホスト(iOS `Resources/`, `web/`,
  Flutter `mobile/assets/plotly/`)が同梱していた `plotly.min.js` は **`gl3d` 部分バンドル**で、
  3D トレース(`scatter3d`/`surface`)は持つが cartesian の `bar`/`heatmap` モジュールを**含まない**。
  Plotly は未登録のトレース型を無言で `scatter` にフォールバックするため、`bar` が線になっていた。
- 対応: 3 ホストの `plotly.min.js` を**フルバンドル**(`plotly.js v2.35.2`, 約 4.6MB)に差し替え。
  フルバンドルのみが 3D と cartesian の両トレースを同梱する。VM 側のコード変更は不要。
- 退行防止: `scripts/check_plotly_bundle.sh` を追加(CI 登録済み)。同梱 `plotly.min.js` が
  VM の emit する全トレース型(`scatter`/`bar`/`heatmap`/`scatter3d`/`surface`)を登録しているか検証する。

### 虚数算術要素の配列リテラル `[1.0 + 2.0im, ...]` が `Vector{Any}` になる問題を修正 (Issue #6851)

- 型注釈なし配列リテラル `[1.0 + 2.0im, 3.0 + 4.0im]` がコンパイル時の要素型推論で `Any` ストレージに
  フォールバックし、`Vector{Any}` になっていた(コンストラクタ形式 `[Complex(...)]` と型付きリテラル
  `ComplexF64[...]` は既に正しく `Vector{ComplexF64}`)。
- 原因: 要素 `1.0 + 2.0im`(= `1.0 + 2.0 * im`)の **ValueType** 推論
  (`infer_expr_type` の `BinaryOp` struct 分岐)は `*`/`+` の Base メソッドへディスパッチするが、それらの
  宣言戻り型が `Any` なので Complex 要素型が失われていた。**JuliaType** 推論
  (`infer_julia_type`)は `Real op Complex{T} -> Complex{promote(...)}` の Complex 昇格を
  ディスパッチが `Any` を返したときのフォールバックとして既に持っている。
- 対応: ValueType 推論の `BinaryOp` struct 分岐でも、ディスパッチが `Any` を返した場合に
  `infer_julia_type` の結果(`Complex{...}`)を `julia_type_to_value_type_with_ctx` で ValueType に
  変換して回収する。これで `1.0 + 2.0im` が `ComplexF64`(`1.0f0 + 2.0f0im` は `ComplexF32`)に畳まれ、
  配列リテラルが `Vector{ComplexF64}` を確保する。整数 Complex(`[1+2im]`)や Complex+Real 混在配列の
  `Vector{Any}` フォールバックは本 issue 範囲外の既存ギャップ(コンストラクタ形式でも同様)。
- フィクスチャ `complex/complex_imag_arith_array_literal_6851.jl` を追加。

### 動的呼び出し毎の `FunctionInfo` clone を `Rc<FunctionInfo>` で除去 (Issue #6853)

- VM の関数呼び出しパス(`get_function_cloned_or_raise` / `start_function_call` / 直接呼び出しの
  `execute_direct_call_with_func_args`)は、`self.functions[idx]` の借用を解放して `&mut self`
  (frame/struct_heap 変更)を取るために、選択した **`FunctionInfo` 全体を毎回 clone** していた。
  `FunctionInfo` は `name`/`params`/`param_julia_types`/`slot_names`/`type_params`/`kwparams` など
  多数の `Vec`/`String` を持つため、clone は複数の heap 確保を伴っていた。
- 対応: `Vm.functions: Vec<FunctionInfo>` → `Vec<Rc<FunctionInfo>>`。`Vm` 構築時(`vm/mod.rs`)に
  `program.functions.into_iter().map(Rc::new).collect()`。`CompiledProgram.functions` は
  `Vec<FunctionInfo>` のまま(serde 非影響)。clone サイトは `Rc` の refcount bump(O(1))になり、
  読み取り(`self.functions[idx].field`)は deref で素通り。whole-vec を借用するヘルパ
  (`bind_kwargs_*`/`KwDefaultEvalCtx`/i64 predecode 群)は `&[Rc<FunctionInfo>]` にシグネチャ調整。
- `Vm.functions` は構築後 read-only(VM 内に mutation サイト無し)。汎用 VM 高速化で全動的呼び出しが
  恩恵を受ける。ベンチ `benches/vm_dynamic_dispatch_benchmark.rs`(`sinc(norm([x,y]))` 10000 点)を追加。

### 型名パースのメモ化で配列ディスパッチを高速化 (Issue #6846 follow-up)

- `sinc(norm([x,y]))` カーネルの追加高速化。プロファイル(`sample`)で steady-state の支配コストが
  **型名文字列の再パース**(`CoreType::from_julia_name` → `split_trailing_where`/`parse_parametric_name`(×2)
  /`parse_core_value_param` の `format!`)だと特定。`from_julia_name` は `name` の純関数なので thread-local の
  `String → CoreType` キャッシュでメモ化(`inference_core/type_core.rs`)。動的ディスパッチ毎に
  `"Array{Float64, 1}"` を再パースしていた連鎖が 1 回の clone に。
- 併せて配列ラッパの型導出も軽量化(`vm/value/struct_instance.rs`): `is_array_wrapper_name` を
  `split_top_level_params`(Vec 確保)から base 名の直接判定に、`array_wrapper_memory_element_type` を
  `julia_type_name()`→`from_name_or_struct` の文字列往復から `array_element_type_to_julia_type` 直写像に置換
  (`get_type_name` のディスパッチ毎 2 確保を除去)。
- 効果(10000点 `sinc(norm([x,y]))`): 全体 **約 −27%**(本セッション計測 0.56s→0.40s compute)。L2 ディスパッチ
  キャッシュ自体は機能済み(399995/400000 hit)で、残コストはラッパ確保の per-literal alloc churn と
  動的呼び出し毎の `FunctionInfo` clone(`Rc<FunctionInfo>` 化は別 PR 候補)、本質的な 2× は配列ラッパ表現の
  alloc 数削減=#6807/#6723 表現エピック領域。正しさは upstream 1.12.6 parity(fixture 16 assert)。
- 作業中に pre-existing bug #6851 を起票(`[1.0+2.0im, ...]` が `Vector{Any}` に誤推論、コンストラクタ形式は正常)。

## 最新対応 (2026-06-17)

### 配列リテラル構築の per-literal `wrap` 呼び出しを native 化 (Issue #6846)

- 配列リテラル `[...]` がリテラルごとに pure-Julia `wrap(::Type{Array}, mem, dims)`(~5 Julia フレーム)を
  呼んでいた perf 劣化(#6649/#6653 の wrapper 移行で導入)を修正。`emit_array_wrapper_from_memory_on_stack`
  (`compile/expr/mod.rs`)を native `FinalizeArray(shape)` に置換し、`finalize_memory_build_buffer`
  (`vm/exec/array_basic.rs`)で `Memory` の `MemoryRef` を **zero-copy で直接** `Array{T,N}` wrapper に包む
  (`wrap`=`_array_construct(T, memoryref(m), dims)` と同型)。`wrap` 用の `PushDataType("Array")` も除去。
  従来 finalize の `ArrayValue` 再マテリアライズは ComplexF64/struct 要素で length 不整合を起こしていた(直接 wrap で解消)。
- 効果(10000点 `sinc(norm([x,y]))` カーネル): 全体 **−41%**(0.844s→0.499s)、`[x,y]` 確保 −49%。
  標準 `vm_array_benchmark` 非回帰、`literal_alloc_2elem_128` ケース追加。正しさは upstream と ~1ULP 差のみ。

### `const` global の `name[]` 空インデックス読み修正 (Issue #6839)

- `const LOG = Ref(0); LOG[]` が `getindex(LOG)` でなく空 `Vector{Any}` にコンパイルされるバグを修正。
  compiler の `TypedEmptyArray` arm(`compile/expr/mod.rs`)で、未知名が **値バインディング**(locals/global_types/
  global_const_structs/captured_vars、Var arm と同判定)なら typed-empty-array でなく `getindex(Var(name))` に
  ルート。Ref 読み(`getindex(::Ref)`)も型束ね変数(`T=Int; T[]`→`getindex(::Type{Int})`)も正しく解決。
  リテラル型名(`Int[]`/struct)は先行 arm で捕捉され不変。write 側(`LOG[]=v`)は元から正常。
- issue の `setindex!` override は無関係(赤鯡)。fixture `essentials/const_ref_empty_index_6839.jl`(11 assert,
  julia 1.12 parity)。

### `Value::NativeArray` carrier 撤去 Slice 9: linalg result producer の wrapper 化 (Issue #6807)

- 線形代数の結果 producer(`builtins_linalg.rs`)を native carrier → `Array{T,N}` wrapper へ。file-local
  `linalg_array_value` free fn を `Vm::linalg_wrapper(&mut self, ArrayValue)` メソッドに置換し、
  `array_wrapper_value_from_array_value` で MemoryRef-backed wrapper を構築。`lu`/`inv`/`\`/`svd`/`qr`/
  `eigen`/`eigvals`/`cholesky` の 19 サイトが wrapper を返す(入力行列は nalgebra に取り込み済みなので `self` 自由)。
- consumer 側(`with_linalg_array`/`linalg_value_to_array_value`)は既に `linalg_array_wrapper_value` 経由で
  wrapper を受理済み → 新規 consumer 修正なし。blast radius ゼロ: full **3842/3842**・AoT green・
  clippy/fmt clean・allowlist 5 files 不変。linalg は全 bench path 外のため bench 不要。
- fixture `linalg/decomposition_wrapper_producers_6807.jl`(23 assert, julia 1.12 parity)で各分解出力を
  downstream(indexing/size/matmul/equality)再利用して回帰検出。残り carrier: deep_copy(再帰)・formatting
  (FFI境界)・`Mark*`/`Reshape`・hot な binary_both/array_index*。

### `Value::NativeArray` carrier 撤去 Slice 8: scattered non-hot producer の wrapper 化 (Issue #6807)

- 散在する非hotの `native_array_value_from_array` producer を wrapper 化(`push_array_value_as_wrapper`):
  `builtins_io.rs`(readlines/readdir 系のファイル読み)、`builtins_macro/mod.rs`(macro/eval 配列結果)、
  `builtins_reflection/mod.rs`(`return_types`/`methods`)。未使用になった `array_value` alias / import も削除。
- 後回し: `builtins_linalg.rs`(LU 分解の free-fn tuple build = struct_heap 無し)、`type_ops/deep_copy.rs`
  (再帰・`copy`/`deepcopy` 駆動)、`formatting.rs`(display/FFI 境界)。
- blast radius ゼロ: full **3842/3842**・AoT green・clippy/fmt clean・allowlist 5 files 不変、新規 consumer 修正なし。
  bench 経路外のため bench 不要。

### `Value::NativeArray` carrier 撤去 Slice 7: native zeros/ones/undef constructor の wrapper 化 (Issue #6807)

- `builtins_arrays.rs` の純粋 fresh-array コンストラクタ 10 サイト(`zeros`/`zerosF64`/`zerosI64`、
  `ones`/`onesF64`/`onesI64`、`AllocUndef{F64,I64,Bool,Any}`)を native carrier → wrapper へ
  (`push_array_value_as_wrapper`)。`Mark{BitVector,BitArray}`(array_type_override + BitPackedBool)と
  `Reshape`(shared_parent 共有)は copy-free fast path が unpack/detach するため意図的に carrier のまま。
- **blast radius ゼロ**: full **3842/3842** で新規 consumer 修正不要(Slice 5 の `length` fallback +
  実プログラムは Base ロード済みで wrapper consumer を解決)。Slice 6 の copy-free 化が効いて構築 bench は
  copy-free baseline 比 ~+0.3-0.6%(大半ノイズ、#6653 が許容した migration tradeoff 内)、他 neutral。
- AoT green・clippy/fmt clean・allowlist 5 files 不変。

### `Value::NativeArray` carrier 撤去 Slice 6: wrapper 構築の copy-free 化 (Issue #6807)

- `array_wrapper_value_from_array_value`(全 wrapper producer の変換ハブ: build-buffer finalize / range・RNG・
  matrix / undef ctor が経由)が常に `MemoryValue::undef_typed` + 要素ごとの O(n) コピーで wrapper の `Memory`
  を作っていたのを、**単純配列では `ArrayData` を move** する fast path に(コピー除去)。
- 安全性: move するのは `MemoryValue::undef_typed(element_type)` が **同じ storage variant** を選ぶ場合のみ
  = override 無し・`array_type_override` 無し(BitArray 除外)・`shared_parent` 無し(view 除外)・`raw_len==element_count`・
  かつ直接型付き primitive backing(F32/F64/I*/U*/Bool/String/Char)。BitPackedBool(→Bool 展開)/StructRefs(→Any 箱詰め)/
  Any 系は従来コピー経路 → wrapper storage は byte-identical。
- full **3842/3842**・AoT green・clippy/fmt clean・allowlist 5 files 不変。`vm_array_benchmark`:
  `construction_undef_zeros_128` −0.53%(p<0.05、`Vector{T}(undef,k)` の copy 除去)、他 neutral。
- 効用: native constructor flip(zeros/ones/undef を wrapper 化する後続バッチ)を **構築 bench を退行させずに**
  進められるようになった(コピーが消えたため)。

### `Value::NativeArray` carrier 撤去 Slice 5: array constructor producer の wrapper 化 第1バッチ (Issue #6807)

- VM builtin/instruction レベルの array producer 第1バッチを native carrier → `Array{T,N}` wrapper へ
  (`push_array_value_as_wrapper` を `pub(crate)` 化して共有): range 実体化(`MakeRange`/`MakeRangeF64`、
  `exec/range.rs`)、RNG 配列(`RandArray`/`RandIntArray`/`RandnArray`、`exec/rng.rs`)、行列演算結果
  (`exec/matrix.rs`)。fresh な constructor/transform 結果で算術/`getindex` の dispatch hot loop 外、かつ
  既に wrapper を返す公開 `zeros`/`collect`(#6653)と同類なので最初に選択。
- **consumer-readiness 修正**: native `length` builtin(`builtins_collections.rs`)は wrapper `StructRef` を
  `length` メソッド dispatch 経由で処理しており、Base 未ロードの bare VM では解決不能だった。**dispatch-miss
  時のみ** MemoryRef-backed wrapper の要素数を native に数える fallback を追加(ユーザ `length` override は
  dispatch 優先で温存、Base ロード済みプログラムは挙動不変 = `length(::AbstractArray)` が常に解決)。
- full **3842/3842**(露出したのは bare-VM ユニットテスト `test_vm_make_range` のみ → `length` fallback で解消)、
  AoT green、clippy/fmt clean、allowlist 5 files 不変、`vm_array_benchmark` neutral。fixture
  `arrays/constructor_producers_wrapper_6807.jl`(21 assert, julia 1.12 parity)。
- 残り ~36 個の `native_array_value_from_array` producer(多くは binary/index の hot dispatch 経路)と対の
  consumer builtin は後続バッチ。native な `zeros`/`ones`(`builtins_arrays.rs`)は依然 NativeArray で別バッチ。
- 副産物: `Int*Int` 行列積の結果 eltype が sjulia では Float64(upstream は Int64)という pre-existing 乖離を確認。

### `Value::NativeArray` carrier 撤去 Slice 4: build buffer の de-variant (Issue #6807)

- 増分 build buffer(`NewArray`/`NewArrayTyped`/`PushElem`/`PushElemTyped`/`ReserveArray`/
  `FinalizeArray`/`FinalizeArrayTyped`、`exec/array_basic.rs`)を `Value::NativeArray` から
  flat で growable な `Value::Memory`(`NewMemory`/`MemorySet` と同じ表現)へ移行。VM に残っていた
  **最後の生きた `Value::NativeArray` producer** を撤去。build buffer は lazy specializer が型付き配列
  リテラル(`[1,2,3]` → I64/F64/Bool/String/Any)に、空 `Vector{String}` 定数(`ARGS`/`DEPOT_PATH`/
  `LOAD_PATH`)に emit する。
- `ArrayValue::push` の ~150 行の要素ロジック(Complex interleave / Tuple・isbits struct AoS / 通常
  `push_value`)を共有 `push_into_array_data` へ抽出し、新 `MemoryValue::push`/`push_f64`/`reserve`/
  `with_capacity`/`is_struct_ref_array` から再利用 → buffer は同一セマンティクスで growth。
- `Finalize*` は native buffer が持っていた `ArrayValue` を厳密再構成(`memory_first_with_capacity` が
  同じ `struct_type_id`/`element_type_override` を導出 → 育った storage と finalize shape を差し込み)し、
  既存 `array_wrapper_value_from_array_value` で変換 → wrapper 出力は byte-identical。これに伴い唯一の
  caller(build buffer)が消えた共有 converter `native_array_value_mut_ref` も撤去。
- full **3842/3842**・AoT **3782/3782**・clippy(通常/`--features aot`)・fmt clean、allowlist 5 files 不変、
  `vm_array_benchmark` は main 比 neutral(element-push 経路は小さなリテラルで hot loop ではない)。fixture
  `arrays/build_buffer_devariant_6807.jl`(35 assert, julia 1.12 parity)。

### `Value::NativeArray` carrier 撤去 Slice 3c (PR B): IndexStore の write fast path (Issue #6806)

- untyped-param の `a[i]=v` は raw `IndexStore` にコンパイルされ、wrapper では `setindex!` dispatch を
  書き込み毎に実行していた。単一整数 index + numeric `Array{T}` wrapper への書き込みを `Memory` へ直接
  書く native fast path に(`exec/array_index.rs`、stack を peek して非該当は従来パスへ defer)。
- 新フラグ `disable_array_setindex_specialization` を追加(#6657 の getindex フラグを write 側にミラー、
  `compile/cache.rs`+`compile/pipeline_ctx.rs` で算出、cache restore 時に再計算 = CACHE_VERSION bump 不要)。
  ユーザ `setindex!` 配列 override がある時は fast path を拒否し dispatch 経由で override に到達。
- **重要な安全制約**: `ArrayData::set_value` が `setindex!` の `convert(T,v)` と一致する value/element ペア
  のみ(完全一致 + 整数→浮動小数配列の widening)。それ以外(特に **float→int 配列**: `convert` は
  InexactError 検査付きで丸めるが `set_value` は拒否)は dispatch へ defer。型保存テスト4件の赤で発覚し修正。
- 効果: `construction_undef_zeros_128` **−46%**(fill via setindex!)・`hof_broadcast_filter_reduce_128`
  **−49%**(#6805 baseline 比)。full **3842/3842**・AoT **3782/3782**・clippy/fmt clean。fixtures:
  `arrays/setindex_wrapper_untyped_param_6806.jl`(12 assert)+ `dispatch/setindex_any_user_method_6806.jl`
  (gate, 3 assert, julia parity)。副産物: ユーザ `setindex!(::Vector)` override が `Ref` setindex を
  壊す pre-existing バグ #6839 を発見・報告。

### `Value::NativeArray` carrier 撤去 Slice 3 (PR B 起点): IndexLoad の rank-1 wrapper fast path (Issue #6806)

- untyped-param / 動的型の `f(a)=a[i]` は raw `IndexLoad` にコンパイルされ、wrapper 配列に対しては
  `Struct|StructRef` アームで **getindex メソッド dispatch をインデックス毎に**実行していた(per-index で
  find_best_method_index + Julia フレーム)。rank-1 の MemoryRef-backed `Array{T}` を単一整数で引くケースを
  `rank1_memoryref_wrapper_element`(`exec/array_index.rs`)で **Memory から直接読む** native fast path に。
- 安全性: #6657 共有フラグ `disable_array_getindex_specialization` でゲート(ユーザ `getindex` 配列 override が
  ある時は fast path を拒否し dispatch 経由で override に到達)。挙動は materialize→`ArrayValue::get` と同一
  (wrapper Memory は `ArrayValue::get` が返す値をそのまま保持)だが O(1)・dispatch 不要。bounds は論理
  `shape[0]` で確認するので view も正しい。
- 効果: `vm_array_benchmark` `hof_broadcast_filter_reduce_128` **−38%**、`view_subarray_parent_share_64`
  **−57%**(共に wrapper を getindex で走査)。他ケースは中立。full **3842/3842**・AoT **3782/3782**・
  clippy(通常+aot)0・fmt clean。characterization fixture `arrays/rank1_wrapper_index_untyped_param_6806.jl`
  (12 assert, julia 1.12 parity)。#6657 fixture green 維持。
- 次(PR B 継続): 同パターンを `IndexStore`/`IndexLoadInbounds`・多次元へ拡張、~190 の
  `native_array_value_ref` consumer を borrow helper から MemoryRef 直接アクセスへ移行。

### `Value::NativeArray` carrier 撤去 Slice 2: 配列 producer を wrapper 化 (Issue #6806)

- 配列の *producer* を MemoryRef-backed `Array{T,N}` wrapper 直生成へ flip。対象は配列リテラル
  (`PushArrayValue`)・comprehension(`FinalizeArray`/`FinalizeArrayTyped`、finished build buffer を
  変換する新 helper `finalize_top_array_to_wrapper`)・undef ctor(`push_undef_typed_array` →
  `push_array_value_as_wrapper`)。変換は既存の `array_wrapper_value_from_array_value` を再利用
  (`MemoryValue` 経由で `ArrayData` storage を共有)。増分 build buffer(`NewArray`+`PushElem`)は
  variant 撤去(#6807)まで一時的に native carrier のまま。
- consumer は既に両表現対応済み(public ctor は #6653 以降 wrapper を返すため main は既に mixed-representation)。
  本 slice で配列の *出力* が一様に wrapper になる。実コードの indexing(リテラル/comprehension/2D/undef/
  代入/ループ/untyped-param/`Any[]`/ネスト)は全て従来どおり動作(getindex dispatch 経由)。
- `IndexLoad` の wrapper 対応: raw `IndexLoad` は Base の `getindex(::Array,…)` 未ロードの bare VM では
  wrapper を index できなかった(`getindex(Vector{…})` MethodError)。dispatch が method を見つけられない
  ときだけ `array_wrapper_value_to_array_value` で native fallback する(Base ロード時は dispatch が常に勝つので
  実プログラムは挙動不変)。
- 既知の host 境界: `normalize_host_return_value` が `run()` の戻り値を host 向けに NativeArray へ再materialize
  する(FFI 境界、allowlist 済み)。内部表現は wrapper。
- full suite **3842/3842**、AoT gate green、clippy 0、perf は #6805 baseline 同等。allowlist 変更なし
  (これらのファイルは元々 variant をテキストマッチしない)。残: typed-slot fast-path の wrapper 対応 +
  `MemoryRefValue` への typed/inbounds accessor 移植(PR B)、build buffer 脱 variant(#6807)。

### 推論型システムに配列の階数(ndims)を追加し comprehension の rank dispatch を修正 (Issue #6817)

- 多イテレータ comprehension `[f(i,j) for i in r1, j in r2]` は 2次元 `Matrix` だが、推論型システムが
  配列の階数を持たず(`ValueType::ArrayOf`/`ConcreteType::Array` は要素型のみ)、`infer_julia_type` の
  Var/expr アームが常に `VectorOf`(rank 1)へ落としていた。結果 typed dispatch が `::Matrix` ではなく
  `::Vector` を誤選択し、`view(matrix_comprehension, …)` が `view(::Vector,…)` MethodError で失敗。
  (`typeof`/`ndims`/`isa` は実行時値ベースで正しかった。#6814 の `<:` 修正とは別経路。)
- 修正: `ValueType::ArrayOf(ArrayElementType, Option<usize>)` と `ConcreteType::Array { element, ndims }`
  に階数フィールドを追加(default `None`→従来通り Vector 投影で挙動保存)。comprehension lowering と
  `infer_expr_type` が階数=イテレータ節数をセット。`infer_julia_type`(Var アーム + expr アーム)・
  `concrete_type_to_julia_type`・`lattice_to_parametric_julia_type`・bridge を rank 対応化。
  要素未知(`Any`)+階数既知のときは bare alias(`Matrix`/`Vector`)を返し、要素特化メソッド
  (`::Matrix{Int64}`)は実行時の具体値へ dispatch される(`Matrix{Any}` 曖昧化を回避)。
- ~360 箇所の機械的フィールド追加(`ValueType::ArrayOf` は base cache serialize 対象のため cache 再生成)。
  fixture `comprehension/multidim_comprehension_dispatch_6817.jl`(15 assert, julia 1.12 parity)。
  full suite 3841/3841、AoT gate green、clippy 0 warnings。#6806 とは直交(実行時 wrapper 化では直らない)。

### `ConcreteType` retirement Slice 1: 分類 pin + `type_id` reader 監査 (Issue #6720, Phase 6)

- representation flip 前の behaviour-preserving 足場。`ConcreteType ↔ CoreType` round-trip 分類の golden
  characterization test を追加し(faithful / type_id-drop / lattice-only carrier を固定)、Slice 2 の tripwire に。
- `type_id` reader 監査完了: 本番 reader は 3 箇所のみ・hard blocker なし(2 つは resolver を既に保持/容易に注入、
  `convert_concrete_to_array_element` のみ 1 hop 注入が必要)。`CONCRETETYPE_RETIREMENT.md` §3.1/§5 更新。
- 次(Slice 2): representation flip(`ConcreteType = Core(CoreType)` + carriers、~3611 箇所の機械的移行、
  reader を name→table 解決へ)。

### `ConcreteType` retirement の設計確定(型表現削減 Phase 6 設計) (Issue #6720)

- 「LatticeType の `CoreType` payload 化」本体の設計を確定。**CoreType 直接拡張は却下**(1706-use 共有 semantic core +
  `core_signature` serialization 汚染のため)、**wrapper 設計**を採用: `ConcreteType = Core(CoreType)` +
  lattice-only carrier(`Function`/`Closure`/`ComposedFunction`/`Enum`)、struct `type_id` は name から resolve。
- 設計書 `docs/vm/CONCRETETYPE_RETIREMENT.md` を新規作成(variant インベントリ・type_id 解決・ハザード・multi-PR
  移行スライス・検証)。`TYPE_REPRESENTATIONS.md` Phase 6 から参照。docs のみ。
- 次スライス(Slice 1): lossless round-trip の characterization pin + `type_id` reader の struct-table 監査
  + 構築/参照ヘルパー(behaviour-preserving)。その後に representation flip(Slice 2)。
### bare alias `Vector`/`Matrix` が `isa`/`<:` で ndims を無視するバグを修正 (Issue #6814)

- `[1,2,3] isa Matrix` → true、`Matrix{Int64} <: Vector` → true 等(本家はいずれも false)。原因=
  `inference_core/type_core.rs::struct_params_are_subtype_with_lookup` の array-family 経路が、
  supertype が **無パラメータ**(bare alias)なら任意の array ペアを無条件 true にしていた。
  bare `Matrix` は `Array{T,2} where T`、`Vector` は `Array{T,1} where T` で rank を固定するため
  rank-free ではない。
- 修正=`other_params.is_empty()` の近道を rank 対応に変更。supertype 名が rank を固定する
  (`Vector`/`Matrix`/`AbstractVector`/`AbstractMatrix`/`BitVector`/`BitMatrix`/range)場合、subtype の
  rank が一致する時のみ true。rank-free 名(`Array`/`AbstractArray`/`DenseArray`/`BitArray`)は従来通り任意 rank。
- 検証: `isa`/`<:` 24 ケース parity(julia 1.12 一致)、fixture `types/vector_matrix_alias_ndims_6814.jl`、
  lib unit 2888/2888、full suite 3840/3840、AoT gate green、fmt clean。
- 副次発見: 2次元 comprehension が compile-time に rank-free `Array` 推論され typed dispatch で `::Vector` を
  誤選択する別バグ(`view`/メソッド選択)を #6817 に分離起票(本 fix とは別根因。runtime 値・`isa`・`typeof` は正しい)。

### `build.sh` の DerivedData 選択を mtime ベースに修正 (Issue #6821)

- `find_app_path()` が `find | tail -n1` で不定な順序の DerivedData を走査していたため、古いビルド
  （bundle executable 欠落）を選んで `simctl install` が失敗することがあった。
- 最新の更新時刻（mtime）でソートして最も新しい `SubsetJuliaVMApp.app` を選択するように変更。

### `build.sh` の `APP_BUNDLE_ID` を Xcode プロジェクトと一致させる (Issue #6823)

- `APP_BUNDLE_ID` が `jp.satoshiterasaki.SubsetJuliaVMApp` に固定されていたが、
  Xcode の `PRODUCT_BUNDLE_IDENTIFIER` は `jp.atelier-arith.subsetjuliavm`。
- これにより `simctl uninstall` / `launch` が正しい bundle ID を対象にできるように修正。

## 最新対応 (2026-06-16)

### VM 実行 hot path の `Value::NativeArray` 直接 match を共有ヘルパ経由へ(#6806 slice 1, Milestone #26)

- carrier 撤去の VM 実行エンジン移行(#6806)の第 1 スライス。4 つの hot-path 実行ファイル
  (`exec/array_index.rs` の `generator_iter_index` / `exec/call.rs` と `exec/locals.rs` の
  `LoadSlotArray` / `exec/call_dynamic.rs` の iterate スコアリング)の直接 `Value::NativeArray`
  variant match を撤去。
  - `array_index.rs` / `call_dynamic.rs`: 共有 destructure ヘルパ `native_array_value_ref(...)`
    (`.is_some()` / `if let Some`)経由へ。
  - `call.rs` / `locals.rs` の `LoadSlotArray`: 冗長な NativeArray アームを削除。
    `native_array_ref_value(rc.clone())` は `Value::NativeArray` 全体の clone(`Rc` の参照カウント
    bump)と完全に等価なため、catch-all の `v.clone()` に畳み込み。
- **挙動完全保存**(`Value::NativeArray(Rc)` の clone は shallow Rc bump)。監査 allowlist は 9 → 5 に縮小。
  残り 5: variant 定義 + enum メソッドの 1 arm(`value_enum.rs`)、変換ヘルパ hub(`array_value/mod.rs`)、
  carrier の unit test(`frame.rs`)、doc コメント 2 件(`formatting.rs` / `plotting/mod.rs`)。
- full suite 3839/3839、AoT gate 3779/3779、clippy 0 warnings、`vm_array_benchmark` 退行なし(±2%, ノイズ域)。
- 作業中に既存バグ #6814 を発見・起票(bare alias `Vector`/`Matrix` が `isa`/`<:` で ndims を無視、
  `Matrix{Int64} <: Vector` が true。型レベルクエリのため carrier 作業と無関係・pre-existing)。

### `Value::NativeArray` carrier 撤去の性能ベースライン + 監査整備 (Issue #6805, Milestone #26)

- carrier 撤去 epic #6723 を slice 化した Milestone #26 の前提ステップ。撤去前の性能ベースラインを記録し、
  退行検知の足場と監査 ratchet を整備(撤去本体 #6806 / #6807 はこの後)。
- `benches/vm_array_benchmark.rs` に欠けていたケースを追加: 多次元 index `a[i,j]`(32×32)、
  MemoryRef-backed 構築(`Vector{Int64}(undef,k)`+`zeros`)、`view`/`SubArray` 親共有。
  3 ケースとも sjulia↔julia 1.12 で出力一致を確認(33792 / 24768 / 89440)。
  ベースライン値は `benchmarks/results/vm_array_baseline_6805.md` に記録(M2 Max, commit 62a930b0a)。
- `scripts/check_value_array_allowlist.sh` に `Value::NativeArray` の **allowlist ratchet** を追加
  (既存の `Value::Array` zero-match は維持)。variant は明示的な 9 ファイル allowlist に固定され、
  list は縮小のみ可能(allowlist 外の新規使用=失敗、使用が消えた stale エントリ=失敗)。
  #6807 で list が空になったら zero-match へ切替。
- FFI/host 境界往復(REPL converter / plotting)は criterion ではなく `cargo nextest` / `scripts/test_aot.sh`
  gate で担保(`Vm::run()` 単体から到達不能なため)。撤去シーケンスは `docs/vm/ARRAY_MEMORY_MIGRATION.md`。

### `substitute_to_julia_type_lossy` の `CoreType` hub 経由化(#34 残り round-trip 排除) (Issue #6720, 3rd slice)

- 2nd slice の follow-up。`substitute_to_julia_type_lossy` の `Parameterized` 腕を構造化
  `substitute_to_core` + `core_type_to_julia_type` 経由へ rerouting し、置換済み param 名の
  render+reparse を排除。これで変換 #34 は両 projection method とも `TypeExpr` 文字列 round-trip なし
  (意図的な単一名 leaf parse のみ残る)。旧経路と byte-identical(網羅 pin あり)。
- full suite 3840/3840、AoT gate OK、clippy 0 warnings(自クレート)。

### `TypeExpr → CoreType` 構造化 resolver(型表現削減 Phase 4 enabling piece) (Issue #6720, 2nd slice)

- Phase 4 の前提となる `impl From<&TypeExpr> for CoreType` を新設し、`TypeExpr::to_julia_type_lossy` を
  `TypeExpr → CoreType → JuliaType` 構造化経路へ rerouting。§3.3.1 / 変換 #34 の `from_name_or_struct(&to_string())`
  render+reparse(parametric params を `Struct(String)` に潰す)を排除。`Parameterized` は
  `CoreType::Struct{name, params}` 等として構造を保持。lowering 形に対し旧経路と byte-identical(網羅 pin あり)。
- **「LatticeType の `CoreType` payload 化」本体は multi-PR に deferral**と判断。素朴な
  `Concrete(ConcreteType)→Concrete(CoreType)` swap は `type_id`/captures/`Enum` を消失して dispatch/codegen
  (#5085/#2863/closure)を破壊し、設計 end-state(Phase 6: ConcreteType = CoreType + lattice-only variants)とも
  異なる。根拠は Issue #6720 スレッドに記録。
- full suite 3839/3839、AoT gate 3779/3779、clippy 0 warnings。
### 配列フィールドアクセス `a.size`/`a.ref`(関数引数経由)を修正 (Issue #6804)

- 関数引数経由の `f(a)=a.size` が `expected numeric value, got Tuple` で失敗(トップレベルは動作済)。
  原因=遅延 specializer が faithful Array wrapper struct のパラメトリック `size::NTuple{N,Int}` フィールド型を
  誤解決し Tuple 戻り値を数値へ coerce。修正=specializer は array-wrapper struct のフィールドアクセスを
  インタプリタへ委譲(`GetFieldByName` が `a.size`=dims Tuple / `a.ref`=MemoryRef を正しく返す)。
  Issue の fallback 方針。根本の ArrayOf 推論リーク撤去は carrier 撤去 epic #6723。fixture
  `array/array_field_access_6804.jl`(parity OK 12 assert)。full 3834 緑・AoT OK。

### `ArrayElementType::UnionOf` の構造化(型表現削減 Phase 4 サブスライス) (Issue #6720)

- epic #5916(型表現の統合・変換面削減)の Phase 4 から、最も自己完結したスライスを着地。
  VM 配列要素型 `ArrayElementType::UnionOf(String)` を構造化 `UnionOf(Vec<JuliaType>)` へ。
- `compile/bridge.rs` が `Union{...}` body 文字列を毎回 `from_name_or_struct` で再パースしていた
  ワート(`TYPE_REPRESENTATIONS.md` §3.3c / 変換 #40)を解消。bridge と
  `array_element_type_to_julia_type` は構造化メンバを `canonicalize_union` で直接正規化し、
  文字列 round-trip を撤去(挙動は旧経路と byte-identical)。表示はメンバ順を温存。
- 残りの Phase 4 項目(`LatticeType` の `CoreType` payload 化、`TypeExpr → CoreType` 直接
  resolver、`ValueType` 降格本体)と Phase 5/6 は epic #5916 / #6720 配下で継続。
- full suite 3834/3834、AoT gate 3774/3774、clippy 0 warnings。
### 分数 BigFloat 指数のハング解消(astro-float vendor + patch)(Issue #6794)

- `big(2.0)^0.5` 等の分数指数 BigFloat 冪が astro-float 0.3.6 の `exp`/`ln`/`pow` Ziv ループの
  table-maker's dilemma(`big(4.0)^0.5 = 2.0` 等の境界値)で無限ループ。`astro-float-num` を
  `vendor/astro-float-num/` へ vendor し `[patch.crates-io]` で差し替え、3 つの Ziv ループに上限
  (`ZIV_REFINEMENT_EXTRA_BOUND=512`)+ 上限時 `set_precision` 最近接丸めを追加。`dynamic_ops` の
  整数指数限定ゲート(#6790)を撤廃。`big(4.0)^0.5`→`2.0`(従来ハング)。厳密表現可能な結果は本家
  ビット一致、無理数は astro vs MPFR の ~1 ULP 差あり(astro 採用に内在)。fixture
  `bigfloat/bigfloat_fractional_pow_6794.jl`(parity OK 17 assert)。full 緑・AoT OK。

### ユーザ `promote_rule` 追加時の数値 `promote_type` dispatch 破壊を修正 (Issue #6782)

- ユーザ定義 `promote_rule` を 1 つでも追加すると、無関係な数値ペアの `promote_type` が
  specific rule ではなく `typejoin`(`Integer`/`Number`)を返していた runtime dispatch バグを修正。
- 原因は `Vm::find_best_method_index_from_candidates` 先頭の blanket "base/user 混在候補なら `Ok(None)`"
  fence。Base 関数 (`promote_rule`) に user メソッドが加わると候補が混在し、`where` 境界付き
  `Type{T}` parametric メソッドを解決できる唯一のメタデータ scorer が無効化されていた。concrete
  メソッドは string resolver で解決されるため壊れず、parametric ペアのみ catch-all → `typejoin` に落ちていた。
- blanket fence を撤去。安全不変条件は本体既存の surgical guard が担保 — native-array wrapper 境界除外
  (#6202) と origin dominance fence (#5926)。
- Regression: `promotion/user_promote_rule_coexists_6782.jl`(parity OK)。dispatch unit 349/349、
  full suite 3831/3831、AoT 3771/3771、clippy 0 warnings。

### hash の Rust intercept を停止し pure-Julia method dispatch 化 (Issue #6728、CLOSE)

- 調査で判明: `isequal`/`isless` は既に dispatch-first(ユーザ method が尊重される)。
  `hash` だけが `handlers/mod.rs` の `"hash" => misc::compile_hash` で 1-arg を
  `CallBuiltin(BuiltinId::Hash)` に強制 intercept しており、ユーザ定義 `hash(::T)` を
  **無視**していた(本家との実害ある乖離)。`compile_hash` を削除し、`hash` を通常の
  Julia method dispatch(pure `base/hashing.jl`)へ。`isequal` の builtin fallback
  (`BuiltinId::Isequal`)は dispatch 後の fallback として温存(ユーザ method が先に勝つ)。
- `BuiltinId::Hash` と `_Hash` は同一ロジック(確認済み)なので、ユーザ型以外の hash 値は不変。
  唯一の差は pure `hash(x::Float64)=_hash(x==-0.0 ? 0.0 : x)` の -0.0 正規化(contract 適合・
  既存 fixture `hashing/hash_basic.jl` が `hash(0.0)==hash(-0.0)` を assert 済み)。
- **CACHE_VERSION 60→61**: compiler-only 変更(enum discriminant 不変)だが、base bytecode の
  `hash` 呼び出し(Dict/Set の `hashindex` 等)が変わるため base 再コンパイルが必要
  (base 内 hash がユーザ `hash(::T)` を尊重するように)。base cache は source hash + CACHE_VERSION
  でのみ無効化されるため明示 bump。
- fixture `hashing/user_hash_dispatch_6728.jl`(parity OK、17 assertions: ユーザ 1-arg hash 尊重 +
  Dict/Set キーとしての isequal⇒hash contract + NaN/missing/isless 不変)。full suite 3831/3831、AoT OK。
### BigFloat の `%` / `rem` / `mod` を実装 (Issue #6796)

- `big(5.0) % big(3.0)` 等が `Unsupported BigFloat operation: Mod` だった。`RemBigFloat` intrinsic
  (astro `rem`、被除数の符号、`x%0`→NaN)を追加し、型付き `%`(`binary/mod.rs`)と動的 `%` 経路
  (`exec/binary_both.rs` の BigFloat fallback に `SremInt → RemBigFloat`、BigInt と対称)の両方に配線。
  `rem`/`mod`(`base/math.jl` の無型 `x%y`)も通過。本家 1.12.6 一致。fixture
  `bigfloat/bigfloat_rem_mod_6796.jl`(parity OK 19 assert)。full 3831 緑、AoT OK。スコープ外で
  `divrem`/`fldmod`/`floor`/`div` 系(BigFloat 未対応)を #6801 起票。

### `Value::Set` carrier 撤去 — pure Set{T} struct へ一本化 (Issue #6732、CLOSE)

- #6731 と同じ "prove-dead-then-delete" レシピ。#6721 で `Set{T}` は既に
  `Dict{T,Nothing}` ベースの pure-Julia struct(`base/set.jl`、フィールド `:dict`)
  になっており、`SetValue::new`/`with_element_type_name` を instrument して全 2508
  fixture を sweep → `Value::Set` births は **0**(carrier は runtime 到達不能と証明)。
  pure Set メソッドは `_set_*` intrinsic ではなく純粋な Dict 演算
  (`s.dict[x]=nothing`/`delete!(s.dict,x)`/`haskey(s.dict,x)`)を使うため `_set_*` も dead。
- 撤去: `Value::Set` variant + `SetValue` 利用 + `builtins_sets/` モジュール全体
  (mod/set_ops/intrinsics/shared、dispatch 配線も)+ `BuiltinId::SetNew/SetPush/
  SetDelete/SetIn/SetEmpty` + `_SetPush`..`_SetLength`(全 emit 0 / caller 無し)+
  internals.rs `_set_*` intercept + `frame::set_slot_set/slot_set` + 死んだ `Value::Set`
  match arm(~20 ファイル)。Set Instr(`NewSet`/`NewSetTyped`/`LoadSet`/`StoreSet`/
  `ReturnSet`/`SetAdd`/`LoadSlotSet`/`StoreSlotSet`)は emit されるが到達不能なので
  carrier-free error ハンドラとして decodable のまま保持。`DictKey` は維持。
  CACHE_VERSION 59→60。`SetValue`(container.rs)は dead-but-present(follow-up sweep)。
- fixture `sets/set_carrier_removed_pure_struct_6732.jl`(parity OK、20 assertions:
  construction/push!/in/delete!/pop!/length/isempty/empty!/eltype/iteration +
  union/intersect/setdiff/symdiff/issubset + Dict 併存)。full suite 3831/3831、AoT gate OK。

### `Value::Dict` carrier 撤去 — ENV 移行 + dead carrier 削除 (Issue #6731 スライス 3-4/N、CLOSE)

- **スライス 3 (ENV)**: `ENV` が `Value::Dict` carrier を runtime 生成する最後の経路だった
  (全 2505 fixture を `new_dict_ref` で計測 → `reflection/env_constant.jl` のみ 4 births)。
  `Instr::PushEnv` を `(key,value)` 2-tuple のタプル供給に変更し、pure-Julia
  `_env_from_pairs(pairs)=Dict{String,String}(pairs)` 経由で struct 化。以後 carrier birth は
  全 fixture で 0 → carrier は runtime 到達不能と証明。
- **スライス 4 (撤去)**: 死んだ carrier を機械的に削除 — `Value::Dict` variant + `DictRef`/
  `new_dict_ref`/`new_dict_value` + `DictValue` 利用 + `_dict_*` intrinsic (BuiltinId `_DictGet`..
  `_DictPairs` 9個) + `DictNew`/`DictMerge`/`DictLen` BuiltinId (emit 0) + `NewDict`/`NewDictTyped`/
  `NewDictWithPairs` Instr + `exec/dict.rs` carrier 経路 + 非パラメトリック `::Dict` メソッド
  (`haskey`/`get`/`getkey`/`keys`/`values`/`empty!` → `_dict_*` 呼び)。public `BuiltinId::Dict*`
  は `try_dispatch_struct_dict` で pure `Dict{K,V}` メソッドへ飛ぶ薄いトランポリンとして存続。
  `Value::Set` 共有の `LoadDict`/`StoreDict`/`DictLen`/`ReturnDict`/`DictSet` Instr と
  `DictDelete` の Set arm、`DictKey` は維持。CACHE_VERSION 58→59。
- fixture `dict/dict_carrier_removed_pure_struct_6731.jl`(parity OK、21 assertions、#6584 トラップ
  + Set delete!/empty! 含む)。`dict_native_demotion_6621_tests.rs` を更新し legacy boundary 0 を維持。
  これで #6731 受け入れ条件(`Value::Dict`/`NewDict*`/Rust Dict builtin 削除)を満たし CLOSE。

### public Dict API → pure dispatch: 破壊系 delete!/get!/empty!/merge! (Issue #6731 スライス 2/N)

- `delete!`/`get!`/`empty!`/`merge!` の `BASE_FUNCTION_ROUTES` を route()→marker() に変更し
  `Value::Dict` builtin fallback を除去。pure-Julia `Dict{K,V}` メソッド(`base/dict.jl`)へ
  static / dynamic(Any barrier、#6584 トラップ含む)双方で dispatch。`BuiltinOp::DictDelete/
  DictGetBang/DictEmpty/DictMergeBang` を dead_but_kept へ移動。Set の delete!/empty! も不変。
- fixture `dict/dict_mutating_ops_pure_dispatch_6731.jl`(parity OK)。CACHE_VERSION 据置。
  残り: setindex!/pop!(別経路)、dead Dict* BuiltinId + `Value::Dict` carrier 撤去(~38 files、closer)。

### public Dict API → pure dispatch: keys / values / pairs (Issue #6731 スライス 1/N)

- `keys`/`values`/`pairs` の `BASE_FUNCTION_ROUTES` を `route(...,BuiltinOp::Dict*,DispatchFirst)`
  から `marker(...,DispatchFirst)` に変更し、`Value::Dict` builtin fallback を除去。これらは
  pure-Julia `Dict{K,V}` メソッド(`base/dict.jl`)へ static / dynamic(Any-typed barrier 経由)
  双方で dispatch される。`BuiltinOp::DictKeys/DictValues/DictPairs` は既存 dead_but_kept に該当、
  `BuiltinId::DictKeys/DictValues/DictPairs` は emit されなくなり dead-but-kept(本 PR では未削除)。
- `keys((a=1,b=2))` 等の NamedTuple 経路は不変。CACHE_VERSION 据置(discriminant 変更なし)。
  fixture `dict/dict_keys_values_pairs_pure_dispatch_6731.jl`(parity OK)。#6731 は multi-PR:
  本 PR が read-query スライス。残り: get/getkey/haskey(lowering map_builtin_name 経由)、
  破壊系 setindex!/delete!/get!/empty!/merge!/pop!、`Value::Dict` carrier 撤去(~38 files)。

### レガシー reducer HOF VM 命令の除去 (Issue #6733)

- 既に pure-Julia dispatch-first 化済み(#3728/#3731)で dead-but-kept だった reducer HOF VM 命令
  を Instr enum から削除: `FindAllFunc`/`FindFirstFunc`/`FindLastFunc`/`MapReduceFunc(WithInit)`/
  `MapFoldrFunc(WithInit)`/`MapFuncInPlace`/`FilterFuncInPlace`/`SumFunc`/`AnyFunc`/`AllFunc`/
  `CountFunc`(13 命令)。`vm/exec/hof.rs` ハンドラ・`exec/mod.rs` dispatch・`effects/instr.rs` も撤去。
- 連鎖して孤立した死コードも整理: `pop_array_or_values`/`PopArrayResult`(util.rs)、
  reducer 専用 starter(`hof_exec/start.rs`: start_hof_call/mapreduce/mapfoldr/map_inplace/
  filter_inplace/sum/with_accumulator、`value_mode.rs`: start_hof_call_values)を削除。broadcast の
  f64 fast-path(`BroadcastInput::F64`/`BroadcastResults::F64`/`new_f64`)は live な broadcast 基盤に
  織り込まれているため `#[allow(dead_code)]` で温存(Values 経路が現用)。
- 維持: `TupleFirst`(タプル destructuring の live codegen、first/last は #3734 で pure-Julia 化済み)、
  range/LinRange(pure-Julia)。any/all の short-circuit セマンティクス不変。`CACHE_VERSION` 57→58。
  fixture `hof/reducer_legacy_instr_removal_6733.jl`(parity OK 16 assert)。

### promote_type 数値優先度テーブル除去 → promote_rule ネットワーク委譲 (Issue #6735)

- compile-time 型推論の数値昇格ハードコード `type_priority`(`compile/promotion.rs`)を除去。数値
  プロモーションは登録済み `promote_rule` ネットワーク(`base/promotion.jl` 由来の thread-local
  registry、152 ルール)を正本とし、cache-less bootstrap 時のみ共有 `inference_core::
  PrimitiveNumeric` taxonomy + Bool/Complex/Big の明示ルールにフォールバック。
- `type_priority` の 3 用途(Float×Float / int 未知幅 / promote_type 末尾フォールバック)を全削除し、
  Float×Float は共有 taxonomy 経路へ集約。**挙動不変**(全数値ペアの promote_type が除去前と一致、
  本家 1.12 とも一致; 既存 BigFloat バグ #6781 のみ除く)。promotion lib test 17/17 緑。
  fixture `promotion/promote_type_pure_julia_6735.jl`(parity OK 16 assert: int 幅/符号・
  Float×Float・Bool・Complex・Bottom/Union{})。
- 発見バグ: #6781(`promote_type(BigFloat,Float64)`→AbstractFloat)、#6782(ユーザ `promote_rule`
  追加が他ペアの dispatch を破壊 — user 拡張 coexistence test は #6782 解消まで保留)。#6727 子スライス 1/4。

### 空のパラメトリック要素型 Vector が eltype を失う問題を修正 (Issue #6768)

- `Vector{UnitRange{Int64}}()` / `UnitRange{Int64}[]` / `Vector{Vector{Int}}()` のように
  **パラメトリック要素型**の空ベクタを構築すると、`push!` 後も `Vector{Any}` に widening
  されていた(`Int64[]` の具象要素型は保持されていた)。`typeof`/`eltype` が本家不一致。
- 根本原因: パラメトリック要素型のうち専用ストレージタグを持たないもの
  (`UnitRange{Int64}`、`Vector{Int}` 等)が、空配列構築の **3 経路すべて**で `Any` に潰されていた。
  (1) `Vector{T}()` 構築 (`compile/expr/collection.rs::compile_array_constructor` の
  `TypeExpr::Parameterized` catch-all)、(2) `T[]` リテラル (`compile/expr/mod.rs` の
  `TypedEmptyArray` 文字列 catch-all + `infer/mod.rs`)、(3) `T[]` の lowering
  (`lowering/expr/collection.rs`: 全 static パラメトリック型が `TypeOf("...")` に lower される
  経路が `T[]` の型名抽出にマッチしていなかった)。`Complex{Float64}` は専用タグで例外的に保持。
- 修正: 具象パラメトリック要素型を `ArrayElementType::Abstract(<型名>)` として保持
  (boxed Any ストレージ + 論理タグ。`Vector{Real}`/`Pair`/`SubArray` と同じ既存機構)。
  free type variable を含む形(`UnitRange{T}`)は対象外。表示は `typeinfo_implicit` を
  `Abstract` 名で再帰評価(`Array{T,N}` は内側が implicit なら implicit)→ `Vector{Vector{Int64}}`
  は本家どおり `[[1,2]]`、`Vector{UnitRange{Int64}}` は `UnitRange{Int64}[...]` を維持。
- fixture `arrays/parametric_eltype_empty_vector_6768.jl`(parity OK)。full 3835 green、
  AoT gate OK。注: 構造体バックの `Dict{K,V}[]` 等は `StructOf` 経路で別途 widening する既存挙動
  (本 issue のスコープ外)。

### `Union{Type{A}, Type{B}, ...}` 引数への型オブジェクト dispatch 修正 (Issue #6781)

- `promote_type(BigFloat, Float64)` が本家 `BigFloat` でなく `AbstractFloat` を返していた(#6735 で
  発見した既存バグ)。根本原因は **第二引数が `Union{Type{...}, ...}` のメソッド**(BigFloat/BigInt/
  Rational の `promote_rule`、Issue #5070)が型オブジェクト引数(`Type{Float64}`)に対して dispatch で
  マッチせず、汎用 fallback `promote_rule(::Type{T}, ::Type{S}) = Union{}` に落ち、`promote_type` が
  `typejoin` に widening していたこと。BigInt+Int 等の **具象ペア**ルールは動作していたため見落とされていた。
- 実行時・コンパイル時の 2 経路を最小修正(`vm/mod.rs::value_matches_param` に `DataType×Union` アーム、
  `core_match.rs` の型オブジェクト fast-reject ガードに `CoreType::Union(_)` 許可)。BigFloat/BigInt/
  Rational + 数値、ユーザ定義 `Union{Type{A},Type{B}}` メソッドがすべて本家一致。fixture
  `promotion/bigfloat_promote_type_6781.jl`(parity OK 19 assert)。残 #6782(ユーザ `promote_rule`
  拡張の coexistence)は別 dispatch バグとして継続。

### BigFloat 表示を本家 1.12 Base.MPFR 形式に一致 (Issue #6789)

- astro_float の生指数形式(`5.e+0` / `2.5e-1` / `1.0e+6`)が本家(`5.0` / `0.25` / `1.0e+06`)と
  乖離。`vm/formatting.rs::format_bigfloat_julia` + 純粋変換 `prettify_bigfloat_string` を追加し
  本家 `_prettify_bigfloat` を移植(指数 ∈ [-4,5] は位取り 10 進、他は科学表記)。**1.12 固有点**=
  科学表記の指数を符号付き 2 桁ゼロ詰め `e±NN`(parity gold standard = 1.12.6、1.14 の `eN` ではない)。
  表示 3 経路集約(`format_value_slow` / `value_to_string` / `ffi/format.rs`)。astro の桁 = MPFR
  最短往復桁(一致確認)。fixture `bigfloat/bigfloat_display_6789.jl`(parity OK 19 assert)。
  非回帰: `-0.0` は astro が符号を保持せず `0.0`。発見した別バグ #6790(`big(2.0)^n` stack
  overflow)、#6791(BigFloat ÷0 が Inf でなく例外)を起票。

### BigFloat ^ 整数 の無限再帰(stack overflow)を修正 (Issue #6790)

- `big(2.0)^100` が stack overflow(終端する `^(::BigFloat,…)` メソッド無し→ runtime dispatch 無限
  再帰)。`dynamic_pow` に BigFloat 冪インラインアーム(`RustBigFloat::pow`=astro pow)+
  `should_use_inline_dynamic_pow` のルーティング(`is_bigfloat_pow`、**整数値指数限定**
  `is_integer_valued_pow_exponent`)で修正。真分数指数(`^0.5`)は astro 0.3.6 の exp/ln 収束ハングの
  ため除外(従来の再帰のまま=非回帰、#6794 で追跡)。fixture `bigfloat/bigfloat_power_6790.jl`
  (parity OK 15 assert)。BigInt **base** 冪(`big(3)^big(2.0)`)は `PowBigInt` が I64 指数限定の別バグ。

### BigFloat の 0 除算を IEEE (±Inf / NaN) に修正 (Issue #6791)

- `big(1.0)/big(0.0)` が `DivisionByZero` を raise(本家 `Inf`)。`DivBigFloat` intrinsic の
  `is_zero()` ガード除去のみで本家一致(astro `div` が `result_to_ext` 経由で IEEE 結果を直接返す:
  同符号→+Inf、異符号→-Inf、0/0→NaN)。表示は #6789 の Inf/NaN フォーマッタが処理。整数 0 除算は
  従来どおり throw。fixture `bigfloat/bigfloat_div_zero_6791.jl`(parity OK 10 assert)。発見:
  BigFloat `%`/`rem`/`mod` 未実装 → #6796 起票。

## 最新対応 (2026-06-15)

### Pure Julia 化: Printf エンジン @sprintf / sprintf (Issue #6746)

- `@sprintf`/`sprintf` を pure-Julia の C-style Printf エンジン(`base/printf.jl`)化。flags /
  width / .precision / conversion をパースし、整数・hex/oct・文字列・char を Julia 側でレイアウト。
  float 変換(`%f`/`%e`/`%E`/`%g`/`%G`)のみ Rust 境界 `_printf_fmt_float`(Ryu float→string)へ委譲。
- **バグ修正**: 旧実装は width/precision/flags を無視し float の既定精度も落としていた
  (`@sprintf("%f",3.14)` が "3.14"、上流 "3.140000"; `%5.2f` が無視)。pure-Julia 化で上流一致。
  `sprintf` route を `DirectBuiltin`→`DispatchFirst`(Rust `BuiltinId::Sprintf` は no-method fallback)。
  Rust 境界 `BuiltinId::PrintfFmtFloat` 追加、`CACHE_VERSION` 56→57。
- 整数 width/zero-pad/+/space/precision・`%#x`/`%08x`/`%#o`・float precision/width/left/zero/sign・
  `%e`/`%E`/`%g`・`%s` width/precision・`%c`・`%%` を網羅。新 fixture
  `stdlib/printf_pure_julia_6746.jl`(@sprintf、parity OK 36 assert)+ 既存
  `stdlib/printf_sprintf.jl` の旧 %f/%e アサートを上流一致に訂正。#6730 子スライス 1/4。

### Pure Julia 化: 丸め floor / ceil / round / trunc + RoundingMode + digits/sigdigits/base (Issue #6742)

- `floor`/`ceil`/`round`/`trunc` を pure-Julia(`base/floatfuncs.jl`)化。CPU intrinsic
  `floor_llvm`/`ceil_llvm`/`trunc_llvm` + 新規 `rint_llvm`(round-to-nearest-ties-to-even)上に構築。
  `where {T<:AbstractFloat}` で float 型を保持。整数恒等(`floor(5)===5`)・型付き形
  (`round(Int8,3.5)===Int8(4)` 型保持)・`digits`/`sigdigits`/`base` keyword・RoundingMode 全 7
  モード(`base/rounding.jl` の TiesUp/TiesAway/FromZero を整備)を網羅。
- digits/sigdigits/base を pure-Julia 化(compile kwargs handler
  `compile_{floor,ceil,round,trunc}_kwargs` 撤去)。`base` を honor(`round(3.7,digits=2,base=2)==3.75`)。
  CPU 丸め命令は principle #6 通り intrinsic として維持(sqrt と同方針)。`CACHE_VERSION` 55→56
  (`Intrinsic::RintLlvm` 追加で discriminant shift)。
- **バグ修正**: 旧 dynamic-dispatch 経路(`call_dynamic.rs`)が `round` に half-away(`f64::round`)を
  使い `round(2.5)==3.0` だったのを ties-to-even(`round(2.5)==2.0`)に。整数恒等・型付き型保持・
  RoundingMode tie 変種も修正。fixture `floatfuncs/rounding_pure_julia_6742.jl`(parity OK、28 assert)。
  full/AoT green。発見バグ #6775(`floor(::Rational)` が Float64 返し)。#6726 子スライス 1/3。

### Pure Julia 化検証: 配列生成 zeros / ones / similar / reshape dispatch-first (Issue #6744)

- `zeros`/`ones`/`similar`/`reshape` が pure-Julia メソッド(`base/array.jl`)へ dispatch-first
  で解決されることを fixture で検証(#6729 スライス 2/3、#6743 と同方針の verify-only)。
  `zeros`/`ones` は #4036 で pure-Julia allocation dispatch 化済み(`BuiltinOp::Zeros/Ones` は
  dead-but-kept)、`similar`/`reshape` は `where {T,N}` pure メソッド。
- 検証の要: 旧 Rust 生成 builtin は Float64/Int64 しか作れないため、`Float32`/`Int32`/
  `Complex{Float64}` 配列が正しい要素型で得られること = generic pure-Julia `zeros(::Type{T},...)`
  (`_array_undef_from_dims` + `fill!`)が走っている証左。default eltype・tuple-dims・similar
  (eltype/dims 各形)・reshape(column-major + range)も網羅。fixture
  `array/array_gen_dispatch_first_6744.jl`(parity OK)。Rust 変更なし(CACHE_VERSION 据置・AoT 不要)。
- 発見バグ: #6771(`ComplexF64[1,2]` の整数リテラル型付き配列が Int を生格納)。

### Pure Julia 化: regex public 検索ラッパー count / findall (Issue #6749)

- `count(::Regex, s)` と `findall(::Regex, s)` を pure-Julia(`base/strings/search.jl`)で追加。
  regex エンジン実体(`match`/`eachmatch`)は Rust の regex crate 境界として維持し、公開ラッパーを
  その上に構築(issue 方針通り)。`occursin(::Regex, s)` は既に pure-Julia(`match` 経由)。
- `findall(::Regex, s)` は `Vector{UnitRange{Int64}}`(バイト範囲 `m.offset : m.offset +
  ncodeunits(m.match) - 1`)、`count(::Regex, s)` は非重複マッチ数。ASCII/unicode haystack・
  空マッチ/不一致を上流(julia 1.12)一致で検証。fixture `regex/regex_count_findall_6749.jl`
  (parity OK)。BuiltinId 変更なし(純追加)のため CACHE_VERSION 据置・AoT gate 不要。
- 発見バグ: #6768(空 `Vector{UnitRange{Int64}}` が push! 後 `Vector{Any}` に退化。既存の string
  findall も同様。値は上流一致のため eltype は fixture で非アサート)。#6730 子スライス 4/4。

### Pure Julia 化: codepoint / bitstring (Issue #6747)

- `codepoint`(UInt32 を返すよう修正)と `bitstring` を pure-Julia 化(`base/strings/basic.jl` /
  `base/intfuncs.jl`)。`bitstring` は reinterpret-to-unsigned(`_bitstring_bits` per type)+
  `sizeof(typeof(x))`(値版 sizeof バグ #6766 回避)+ Bool 三項で MSB-first にビット列構築。
- Rust 撤去: `BuiltinId::{Codepoint,Bitstring}` + handler + intercept + base_functions。
  **維持(byte列/Char primitive、issue 方針通り)**: `ncodeunits`/`codeunit`/`codeunits` と
  `Char(n)`/`Int(c)`。`CACHE_VERSION` 54→55。
- 全型(Int/UInt/Float/Bool 各幅)+ first-class function value で上流一致。
  **バグ修正**: 旧 builtin の `bitstring(true)` は `"1"`(上流 `"00000001"`、Bool=1byte=8bit)だった
  のを pure-Julia 化で上流一致に(`string_bitstring.jl` fixture も訂正)。
  fixture `strings/codepoint_bitstring_pure_julia_6747.jl`(parity OK)。full/AoT green、clippy clean。
  #6730 子スライス 2/4。発見バグ: #6766(`sizeof(value)` が 8 を返す)。

### Pure Julia 化: float 分解 exponent/significand/frexp/issubnormal/nextfloat/prevfloat (Issue #6740)

- 6 関数を pure-Julia(`base/float.jl`)へ移行。`reinterpret` + per-type IEEE bit-field helper
  (sign_mask/exponent_mask/exponent_one/exponent_half/significand_mask/significand_bits/
  exponent_bits/exponent_bias + `_float_uinttype`/`_float_sinttype`、Float16/32/64)で上流
  (`julia/base/float.jl`/`math.jl`)に整合。Rust 境界は `reinterpret` のみ。
- Rust 撤去: `BuiltinId::{NextFloat,PrevFloat,NextFloatN,PrevFloatN,Exponent,Significand,Frexp,
  Issubnormal}` + handler + intercept + builtin.rs assertion + base_functions(is_base_function/
  all_builtin_names/EXEMPTED)+ is_pure_math。未使用 helper(step_next_prev_float_n 等)も除去。
  `CACHE_VERSION` 53→54。
- **バグ修正/改善**: 旧 builtin は Float64 専用で Float32/Float16 を Float64 に collapse していたが、
  pure-Julia 化で型保存(`significand(Float32)::Float32` 等)。`exponent(::Integer)` も追加(上流一致)。
  subset で `da % U`→`convert(U,da)`、`⊻`→`!=`、`reinterpret(_,T(Inf))`→`exponent_mask(T)` に置換。
- precursor: `reinterpret(Float16↔UInt16/Int16)` 実装(PR #6764)。
- fixture `floatfuncs/float_decomp_pure_julia_6740.jl`(parity OK、全幅+Inf/NaN/±0/subnormal/2-arg)。
  full/AoT green、clippy clean。#6726 子スライス 2/3(残 #6742)。

### Pure Julia 化: リフレクション述語 isbits/ismutable/hasfield (Issue #6738)

- `isbits`/`ismutable`/`hasfield` を pure-Julia public ラッパー(`base/reflection.jl`)化:
  `isbits(x)=isbitstype(typeof(x))`、`ismutable(x)=ismutabletype(typeof(x))`、
  `hasfield(T,n)=n in _fieldnames(T)`。VM メタデータ primitive(`isbitstype` 型フラグ /
  `_ismutabletype` / `_fieldnames`)は Rust 境界に維持(issue 方針通り)。
- Rust 撤去: `BuiltinId`/`BuiltinOp` の `Isbits`/`Ismutable`/`Hasfield`(enum・from_name・name・
  builtin.rs arm・abstract_interp name-map・vm handler・base_functions の marker/is_base_function/
  all_variants・infer 3 箇所(ValueType/JuliaType/StaticType Bool)・test)を整理。`CACHE_VERSION` 52→53。
- **バグ修正**: `ismutable("s")` が旧 Rust では false(上流 true)だったのを ismutabletype 経由で
  true に。`hasfield` は `::Symbol` 注釈を外して symbol リテラルの QuoteNode 変換エラーを回避。
- fixture `type_inference/reflection_predicates_pure_julia_6738.jl`(parity OK)。full/AoT green、clippy clean。#6727 子 4/4。

### Pure Julia 化: parse/tryparse(Float64) を pure-Julia ラッパー化 (Issue #6748)

- `parse(Float64,s)`/`tryparse(Float64,s)` を pure-Julia(`base/parse.jl`)化し、実変換は
  `_tryparse_float64` intrinsic(libc strtod)に委譲。`compile_parse_tryparse` ハンドラの Float64
  分岐を撤去しメソッドディスパッチへ。`BuiltinId::StringToFloat` を撤去(parse=tryparse+error の
  pure 実装で不要に)、`TryparseFloat64` の name を `_tryparse_float64` へ。`CACHE_VERSION` 51→52。
- **バグ修正**: `parse(Float64,"bad")` が generic error を投げていたのを上流同様 `ArgumentError` に。
  `parse(Int;base=)` / `string(x;base=)` は維持。`tryparse(Float64)` は nothing。
- fixture `numeric/parse_float_pure_julia_6748.jl`(parity OK)。full/AoT green、clippy clean。#6730 子 3/4。

### Pure Julia 化: bit CPU 関数を pure-Julia ラッパー化 (Issue #6741)

- `count_ones`/`leading_zeros`/`trailing_zeros`/`bitreverse`/`bswap` を public は pure-Julia
  (`base/int.jl`)、CPU 命令は underscored 低レベル intrinsic `_ctpop_int`/`_ctlz_int`/
  `_cttz_int`/`_bitreverse_int`/`_bswap_int` に分離(上流 `count_ones(x)=ctpop_int(x)%Int`
  構造に整合)。BuiltinId variant は維持しつつ `from_name`/`name()`/intercept/is_base_function/
  all_builtin_names/EXEMPTED の名前を underscored へ更新。`builtin.rs` assertion も更新。
- 挙動不変(全幅で上流一致、first-class function value も維持)。#6722 の派生 helper も新ラッパー
  経由で動作。BuiltinId 増減なしのため `CACHE_VERSION` bump 不要(prelude hash で自動 invalidate)。
- fixture `numeric/bitcount_pure_julia_6741.jl`(parity OK)。full/AoT green、clippy clean。#6726 子 3/3。

### Pure Julia 化: length/size/ndims/eltype の dispatch-first 検証 (Issue #6743)

- 配列クエリ `length`/`size`/`ndims`/`eltype` は既に pure-Julia メソッド(`base/array.jl`)へ
  dispatch-first。**ユーザー定義 `length`/`size`/`eltype`/`ndims` が shadow されず**上流一致で
  動作することを検証(Rust builtin は内部 carrier 用の no-method フォールバックのみ)。
- コード変更なし(検証 + 回帰 fixture)。fixture `array/array_query_dispatch_first_6743.jl`
  (parity OK)。#6729 子スライス 1/3。

### Pure Julia 化: convert / promote の dispatch-first 検証 (Issue #6736)

- `convert` / `promote` の public API は既に pure-Julia メソッド(`base/essentials.jl` /
  `base/promotion.jl`)へ dispatch-first でルーティング済み。Rust 側は pure メソッドが無い型の
  「実変換」フォールバックのみ。**ユーザー定義 `convert(::Type{T}, ::Int)` / `promote_rule` が
  shadow されず上流一致**で動作することを検証(`promote(Money(100), 5)::Tuple{Money,Money}` 等)。
- コード変更なし(検証 + 回帰 fixture のみ)。fixture
  `numeric/convert_promote_dispatch_first_6736.jl`(parity OK)。#6727 子スライス 2/4。
- 補足: 型付き配列リテラル `T[...]` の要素 convert 適用は別機構で未対応(#6736 スコープ外)。

### Pure Julia 化: widemul を pure-Julia へ + 数値変換整理 (Issue #6737)

- `widemul` を上流 `widen(x)*widen(y)`(`base/number.jl`)へ移行。旧 `BuiltinId::Widemul`
  handler は **I64 専用**で `widemul(Int32,Int32)` 等が `widemul: cannot multiply` でエラー
  していた **バグを修正**。intercept(`handlers/misc.rs`/`mod.rs`)と BuiltinId を撤去。
- `signed`/`unsigned`/`float` は既に pure-Julia dispatch、`reinterpret` は raw bit の真 primitive
  として維持。`float` の dead `BuiltinId::FloatConv` はハンドラが大きいため本 PR では据え置き
  (到達不能・既に pure-Julia)。`CACHE_VERSION` 50→51。
- 残課題: `widemul(UInt32,UInt32)` の結果型が UInt64 でなく Int64(`convert(UInt64,::UInt32)`
  の mis-tag、**#6755** で別途追跡)。fixture は値で parity 確認。
- fixture `numeric/numeric_conversions_pure_julia_6737.jl`(parity OK)。full/AoT green、clippy clean。
  #6727 子スライス 3/4。

### Pure Julia 化: 非破壊畳み込み/探索の dead builtin 撤去 (Issue #6745)

- `collect`/`findfirst`/`findall`/`argmin`/`argmax`/`prod`/`minimum`/`maximum` + 配列 iterate は
  既に pure-Julia(`base/array.jl`、dispatch-first)。残存していた vestigial な
  `BuiltinId::{Prod,Minimum,Maximum,Argmin,Argmax,FindFirst,FindAll}`(emission 無し=dead、
  handler も無し)を撤去。`from_name`(findfirst/findall)・name()・all_builtin_names・EXEMPTED
  を整理。`CACHE_VERSION` 49→50。
- fixture `array/reducers_finders_pure_julia_6745.jl`(全 8 関数 + iterate + first-class
  function value、narrow-int reduction が Int 昇格する上流挙動を pin)。full/AoT green、clippy clean。
- #6729 の子スライス(3/3)。

### Pure Julia 化: 残存文字列 builtin (unescape_string/findall/count) の撤去 (Issue #6724)

- `unescape_string` を char ベースの pure-Julia 実装(`base/strings/util.jl`)へ修正し、Rust
  `BuiltinId::UnescapeString`(dead・DispatchFirst で shadow 済み)を撤去。旧 byte/char
  混在実装の multibyte 破損(`café`→`cafÃ©`・末尾欠落)を修正、`café`/`αβγ`/emoji/CJK で
  上流一致。
- dead builtin `StringCount` / `StringFindAll` を撤去(`findall`/`count` の String/Char は既に
  pure-Julia `base/strings/search.jl`)。`occursin` 非regex は既に pure-Julia(regex は
  `Occursin` builtin 維持=本 issue スコープ外)。
- `isnumeric` は utf8proc カテゴリ依存のため #6752 へ分離(`BuiltinId::Isnumeric` 維持)。
- fixture `strings/unescape_string_multibyte_6724.jl`(parity OK)、`CACHE_VERSION` 48→49、
  full suite green、clippy clean。

### Pure Julia 化: bit 演算の派生関数を Rust builtin から移行 (Issue #6722)

- `count_zeros` / `leading_ones` / `trailing_ones` / `bitrotate` を上流 `julia/base/int.jl`
  と同じ pure-Julia 定義(真 intrinsic `count_ones`/`leading_zeros`/`trailing_zeros` の
  bitwise-not ラップ + 上流 `bitrotate` 式)へ移行。`base/int.jl` に追加。
- Rust 側を撤去: `BuiltinId::{CountZeros,LeadingOnes,TrailingOnes,Bitrotate}`、
  `vm/builtins_math.rs` の 4 handler、`compile/expr/builtin_math.rs` の compile-time
  intercept、`builtin.rs`/`base_functions.rs`(is_base_function・all_builtin_names・EXEMPTED
  コメント)を整理。維持する真 intrinsic: `count_ones`/`leading_zeros`/`trailing_zeros`/
  `bswap`/`bitreverse`。
- **バグ修正**: 旧 `Bitrotate` handler は I64 専用(`pop_i64`)で `bitrotate(UInt8(0b10110001), 2)`
  が `Int64 708` を返していた。pure-Julia 化で全 BitInteger 幅の型保存・幅 wrap が上流と
  一致(`UInt8 198` 等)。BigInt は上流同様 MethodError(per-concrete-width dispatch)。
- fixture `numeric/bitrotate_type_preservation_6722.jl`(parity OK)、`CACHE_VERSION` 47→48
  (BuiltinId discriminant 変化)、lib 2878 / full suite green、clippy clean。

### 型表現の統合: `CoreType` を `JuliaType↔ConcreteType` ハブ化(Phase 3) (Issue #6599)

- `JuliaType↔ConcreteType` 変換を正準 `CoreType` 経由に寄せ、乖離面を削減(ロードマップ
  Phase 3、`TYPE_REPRESENTATIONS.md` §4)。
- Slice A: `impl From<&CoreType> for ConcreteType`(欠けていた下向きエッジ)を新設、lossy
  アームを round-trip pin。Slice B: `julia_type_to_concrete_type_lossy` を
  `ConcreteType::from(&CoreType::from(ty))` へ rerouting(旧 `_ => Any` のコンテナ型が
  構造回復する精度向上、audit 8 件・回帰なし)。
- Slice D(braced struct 精度)は先行 #6599 PR で既達のためスキップ。逆方向
  `concrete_type_to_julia_type` の rerouting は reflection 特殊ケース(#4843/#2863/type_id)
  のため Phase 4 deferred。
- full suite green(A 3827、B 3828)、clippy clean。Phase 4(`ValueType` 降格本体・5267 uses)は
  別 follow-up issue。
### Set を pure-Julia `Dict{T,Nothing}` ラッパーへ移行 (Issue #6721)

- `Set{T}` を上流準拠で `struct Set{T} <: AbstractSet{T}; dict::Dict{T,Nothing}; end`
  として pure-Julia 化(`base/set.jl`)。`push!`/`delete!`/`in`/`empty!`/`length`/
  `isempty`/`iterate`/`pop!`/`copy` を backing Dict へ委譲。`_set_*` HashSet intrinsic
  は `.jl` から完全に撤去(Rust intrinsic は cache 互換 residual として残置)。
- **behavioral parity 回復**: `ft(x::Set{T}) where {T} = T; ft(Set([1,2,3]))` が
  上流同様 `Int64` を返す(従来は `MethodError: no method matching ft(::Set)`)。native
  `Value::Set` carrier が `Set{T}` struct dispatch に参加していなかったのが原因。
- コンパイラ: `Set(...)`/`Set([...])`/`Set(x for ...)`/`Set{T}(...)` 構築を Dict 移行
  (#6619)と同様に pure-Julia メソッド dispatch へルーティング(`Set{T}(...)` は
  `try_compile_explicit_public_set_constructor` 経由で eltype を runtime type 値として
  helper `_set_from_eltype`/`_set_with_eltype` へ lift)。`Set([...])` の推論が
  `Struct("Set{T}")` を返すよう infer の Dict adapter を Set にもミラー。
- `push!`/`delete!`/`empty!`/`pop!`/`in`/`∈`/`∉`/`collect` の各 Set arm を native
  builtin から Set struct 用メソッド dispatch へ切替(`BuiltinId::Set*`/`DictDelete`/
  `DictEmpty`/`Pop`/`In` は legacy fallback として残置)。`iterate(::Set)` を
  IterateDynamic 候補に追加(`("Set",0)` 除外を解除)。
- 既知の制限(上流との残差、Set 固有ではない既存ギャップ): `[(1,2),(3,4)]` が
  `Vector{Any}` 推論のため `Set([(1,2),(3,4)])` は `Set{Any}`(membership は正常)。

### Set のタプル/struct 要素キー (Issue #6693 Set 側)

- `Set([(1,2)])` や struct 要素 Set の構築/membership 失敗を、`DictKey::Composite`
  追加 + Set 構築/`in` での heap struct ref 解決で対応(#6624 全面再設計ではなく
  的を絞った修正)。`In` builtin の Set arm を Array arm と同じ共有 `values_equal`
  経由に統一。集合演算(pure-Julia)も含め upstream 一致。
- 既知の制限: `typeof(Set([(1,2)]))` は `Set{Tuple}`(厳密なパラメトリック要素型は
  未復元)。回帰 `sets/struct_tuple_element_membership_6693.jl`。

### `d[k1, k2, ...]` カンマ複数キー Dict 添字 (Issue #6707)

- タプルキー Dict の `d[1,2]` / `d[1,2]=v` が native 多次元配列添字に落ちて
  MethodError だったのを、Dict 系 receiver かつ非 slice index 2+ の場合に
  `TupleLiteral` キーへまとめて単一キー getindex/setindex! へ rewrite(upstream の
  `getindex(::AbstractDict, k1,k2,ks...)` 相当)。配列多次元添字は無影響。
- 回帰 `dict/comma_multikey_getindex_setindex_6707.jl`。

### `===` immutable struct 値比較 + native value-op StructRef 統合 (Issue #6709, #6694)

- `===` on immutable struct(`Pt`/`OneTo`、及び tuple 要素)が heap index 比較で
  `false` だったのを修正(`Egal` 冒頭で immutable のみ inline 解決、mutable は参照
  同一性維持)。`==`/`hash`/`in`/`===` の StructRef 解決を単一ヘルパ群に統合し、
  audit `scripts/check_native_value_ops_resolve_structref.sh` で再発防止。
- 回帰 fixture `operators/egal_immutable_struct_6709.jl`。#6685/#6691/#6693 と同族。

### AoT を `--features aot` でゲート + clippy 債務一掃 (Issue #6679)

- `scripts/test_aot.sh` 追加(nextest+clippy の `--features aot`)、CLAUDE.md に手順。
  ゲートが検出した 43 件の AoT clippy 債務を一掃、`--features aot` 3763/3763 green。
### pre-scan 退役: 関数本体スロット型 pre-scan の二重推論を撤去(legacy 削除、完了) (Issue #6601)

- `assign_rhs_value_type` seam の catch-all をエンジンへ flip。corpus 9 クラス +
  非 corpus 非リテラル変種(array/dict literal、comprehension、ModuleCall、Builtin、
  ternary 等)も全てエンジン経由になり、関数本体/inner ctor/main のスロット型
  pre-scan の二重推論(#5922 主目的)が解消。
- legacy `infer_value_type` / `infer_value_type_with_structs` + 専用ヘルパ 4 本を削除
  (−878 行)。リテラル RHS は driver の新規 struct-table 対応 `literal_rhs_value_type`
  で精密に型付け(engine は deferred literal を `Top`→`Any` に widen するため;
  `im`→`Struct(Complex{Float64})`、`[1,2]`→`ArrayOf(I64)` 等)。
- legacy 比較テストを engine-value assertion に変換、divergence-map スキャフォールドを
  削除。full suite 3823/3823 + AoT gate + clippy clean。事前 probe で非リテラル
  catch-all の engine 化が実 fixture を退行させないことを確認。
- #6601(pre-scan 退役 1/3: 関数本体/inner ctor/main)はこれで完了。残: BinaryOp/Complex
  等の divergence は全て解消済み、divergence map は空。

### pre-scan 退役: `Call` Assign-RHS を共有エンジンへ(最終 corpus クラス、divergence map 空) (Issue #6601)

- `assign_rhs_value_type` seam で最後に残っていた `Expr::Call` を legacy fallback
  から共有 `InferenceEngine` へルーティング。`Call` は *engine-better* クラス。
- 共有 transfer function を 1 箇所修正(ローカル特殊化なし、MAIN にも反映、
  `compile/tfuncs/intrinsics.rs`):`tfunc_sqrt`(`sqrt`/`exp`/`sin`/`cos`/`log`
  が委譲)が `Complex{T}` を保存(`is_float`/`is_numeric` の間に Complex struct arm)。
  従来 Complex struct は `is_numeric` でないため `Top`(→ `Any`)に落ち、`exp(z)` が
  `Any` に divergence していた(`exp(::ComplexF64) === ComplexF64`)。`abs(Complex)
  → F64`・`zeros(n) → ArrayOf(F64)` はエンジンが既に upstream 正確で legacy が
  imprecise(`ComplexF64` / bare `Array`)だった分をそのまま採用。
- 特性化 pin: `prescan_engine_value_call_issue_6601`(value-assertion pin)。`Call`
  を `is_migrated_assign_rhs_class` フィルタへ追加し、divergence map から残る Call
  3 行(abs(c) / exp(z) / zeros(i))を削除 → **map は空**(corpus 全クラス移行完了)。
  fixture `type_inference/prescan_call_6601.jl`。
- 残スコープ: catch-all(`_ => legacy`)へ届く非 corpus `Expr` variant のみ。最終
  legacy 削除スライスで catch-all をエンジンへ通せば pre-scan 退役が完了。

### pre-scan 退役: `BinaryOp` Assign-RHS を共有エンジンへ (Issue #6601)

- `assign_rhs_value_type` seam で `Expr::BinaryOp` を legacy fallback から共有
  `InferenceEngine` へルーティング。`BinaryOp` は *engine-better* クラス。
- 共有 transfer function を 2 箇所修正(ローカル特殊化なし、MAIN にも反映、
  `compile/tfuncs/arithmetic.rs`):新規 `tfunc_pow` を `^` に登録
  (`Int^Int → Int`、`String^Int → String`、他は加算昇格。従来 `^` 未登録で
  `i^i` が `Any`)、`tfunc_mul` で `String*String → String` 連結(従来 `s*s` が
  `Any`)。Complex 演算(`c+f` / `z+f` / `z*z`)はエンジンが既に canonical な
  `ComplexF64` を返す(bridge が `Struct{Complex{Float64}}` → `ComplexF64`)ため
  そのまま採用(legacy は `F64` / `Struct`)。
- 特性化 pin: `prescan_engine_value_binaryop_issue_6601`(value-assertion pin)。
  `BinaryOp` を divergence map から `is_migrated_assign_rhs_class` フィルタで除外し、
  BinaryOp 5 行(Pow / Str*Str / Complex 3 ケース)を削除。fixture
  `type_inference/prescan_binaryop_6601.jl`。
- 残スコープ: Call も本日移行済み(本日の `Call` サブセクション参照)。corpus 全
  クラスが移行完了し divergence map は空。

### pre-scan 退役: `Index` Assign-RHS を共有エンジンへ (Issue #6601)

- `assign_rhs_value_type` seam で `Expr::Index` を legacy fallback から共有
  `InferenceEngine` へルーティング。`Index` は *engine-better* クラス:エンジンは
  `arr[i]` を正確に要素型(`I64`)へ推論する(legacy は `Any`)。legacy 比較では
  なくエンジンの upstream 正答値を直接 pin する。
- 共有側を 2 箇所修正(ローカル特殊化なし、MAIN コンパイルにも反映):
  `getindex` transfer function(`compile/tfuncs/array_ops.rs`)で `String[i]`→`Char`、
  エンジン `Index` arm の単一スライス(`compile/abstract_interp/engine/mod.rs`)で
  `String` スライス `s[1:2]`→`String`(従来は `Array` のみ対応で String は
  `Top`/`Any`)。これで `arr[i]`→要素型、`s[i]`→`Char`、`s[1:2]`→`String`。
- 特性化 pin: `prescan_engine_value_index_issue_6601`(value-assertion pin)。
  `Index` は divergence map から `is_migrated_assign_rhs_class` フィルタで除外
  (初回導入)し、Index 2 行(`arr[i]`: I64↔Any、`s[i]`: Any↔Char)を削除。corpus に
  `s[1:2]` を追加。fixture `type_inference/prescan_index_6601.jl`。
- 残スコープ: Call / BinaryOp も本日移行済み(本日の各サブセクション参照)。
  corpus 全クラスが移行完了し divergence map は空。

### Tuple Dict キー `d[(...)]` 取得 + struct 要素キーの一貫ハッシュ (Issue #6693)

- `d[(1,2)]` が `MethodError`、`haskey(Dict((OneTo(3),)=>10), (OneTo(3),))` が
  `false` だった(upstream は両方成功)。
- (A) lowering: `collect_index_nodes` が括弧付き `TupleExpression` を多次元添字と
  同様に spread → `d[(1,2)]` がタプルキーに届かなかった。`TupleExpression` を
  spread 対象から除外(多次元 `A[1,2]` は direct children なので無影響)。
- (B) hash: `Hash`/`_Hash` が tuple 要素/struct を `Debug` 文字列(heap index)で
  ハッシュ → 等価 struct キーが別ハッシュに。`resolve_structrefs_deep` で構造解決
  してからハッシュ(#6685 の StructRef クラス)。
- fixture `dict/tuple_key_getindex_hash_6693.jl`、unit
  `equal_structs_hash_consistently_after_resolution_6693`。
- 残: `Set([(...)])` 複合要素(native `DictKey` スカラ専用、#6624 と協調)、
  `d[1,2]` カンマ複数キー形式(抽象 Dict getindex 経路)は別 follow-up。

### pre-scan 退役: `FieldAccess` Assign-RHS を共有エンジンへ (Issue #6601)

- `assign_rhs_value_type` seam で `Expr::FieldAccess` を legacy fallback から共有
  `InferenceEngine` へルーティング。divergence map から FieldAccess 2 行
  (`ex.head`: Any→Symbol、`ex.args`: Any→Array)を解消。
- 共有 `compile/abstract_interp/engine/mod.rs` の `FieldAccess` に `Expr` builtin の
  固定フィールド特殊ケースを追加(`head`→Symbol、`args`→`Vector{Any}`):`Expr` は
  user struct table に載らず従来 `Top`/`Any` に fall through していた。legacy
  `infer_value_type_with_structs` も `Expr.args` を bare `Array` から `ArrayOf(Any)`
  (upstream `Vector{Any}`)に揃えて一致。MAIN コンパイルにも反映。
  fixture `type_inference/prescan_fieldaccess_6601.jl`。
- 残スコープ: Call / BinaryOp / Index も本日移行済み(本日の各サブセクション参照)。
  corpus 全クラスが移行完了し divergence map は空。

### pre-scan 退役: `TupleLiteral` Assign-RHS を共有エンジンへ (Issue #6601)

- `assign_rhs_value_type` seam で `Expr::TupleLiteral` を legacy fallback から共有
  `InferenceEngine` へルーティング。divergence map から Tuple 1 行
  (`(i, a)`: Any→Tuple)を解消。
- 共有 `compile/abstract_interp/engine/mod.rs` の `TupleLiteral` 推論を upstream
  一致に修正: tuple リテラルは要素型に関わらず常に `Tuple`(`typeof((1, "x", []))
  == Tuple{...}`)。非 concrete 要素を `ConcreteType::Any` に widen して全体の
  `Top`/`Any` collapse を回避。MAIN コンパイルにも反映。
  fixture `type_inference/prescan_tuple_6601.jl`。
- 残スコープ: Call / BinaryOp / FieldAccess / Index も本日移行済み(本日の各
  サブセクション参照)。corpus 全クラスが移行完了し divergence map は空。

### pre-scan 退役: `UnaryOp` Assign-RHS を共有エンジンへ (Issue #6601)

- `assign_rhs_value_type` seam で `Expr::UnaryOp` を legacy fallback から共有
  `InferenceEngine` へルーティング。divergence map から UnaryOp 2 行
  (`!i`: Any→Bool、`-c`: Any→ComplexF64)を解消。
- 共有 `compile/tfuncs/arithmetic.rs` を upstream 一致に修正: `tfunc_not` は常に
  `Bool`、単項 `tfunc_sub` は concrete 被演算子の型を保存(`Complex{T}` 含む)。
  MAIN コンパイルにも反映。fixture `type_inference/prescan_unaryop_6601.jl`。

### Tuple `==` over heap struct 要素の値比較 (Issue #6685)

- `(OneTo(3),) == (OneTo(3),)` が `false`(upstream は `true`)だった。native
  `TupleEquals` builtin の Rust 畳み込みが heap 参照 `Value::StructRef` を解決せず
  `Debug` 文字列(heap index)で比較していたのが原因。`isequal` は pure-Julia
  dispatch のため無影響。
- `subset_julia_vm/src/vm/builtins_equality.rs` の `TupleEquals` 境界で
  `resolve_structrefs_deep`(`contains_structref` 事前判定 + `visiting` 循環ガード)
  により struct ref を inline struct へ解決してから既存の構造比較に渡す方式で修正。
- fixture `tuple/struct_element_equality_6685.jl` + unit `structref_equality_tests`。
  関連: tuple の `in`/`∈` は別経路で未対応 → #6691 に分離。

### `in` / `∈` over tuple collections の要素比較 (Issue #6691)

- `(1, 2) in [(1, 2)]` が `false`(upstream `true`)だった。native `In` builtin の
  ローカル `values_equal` クロージャに Tuple/Struct arm が無く `_ => false` に
  落ちていたのが原因(primitive tuple でも発生、#6685 とは別経路)。
- `In` の非スカラー fallback を共有
  `builtins_equality::values_equal_for_membership`(#6685 の struct-ref 解決を再利用)
  へルーティングして修正。fixture `operators/in_tuple_struct_membership_6691.jl`。
- 関連: Dict/Set の struct 入り key は別経路で未対応 → #6693、集約は prevention #6694。

### `Memory{T}(undef, dims::Tuple)` の 1-tuple 次元受理 (Issue #6688)

- `Memory{T}(undef, (n,))` が "Cannot convert Tuple to I64" でコンパイルエラー
  だった(`Memory` は 1 次元なので upstream は 1-tuple を受理)。
- `compile_memory_constructor`(`compile/expr/collection.rs`)に
  `compile_memory_dim_to_i64` を追加: literal 1-tuple はコンパイル時 unwrap、
  動的 tuple は `TupleGet`+`DynamicToI64`、multi-tuple は拒否。scalar 形は不変。
- fixture `memory/undef_tuple_dims_6688.jl`。関連: `Memory` compact 表示は別 issue #6697。

### `Memory{T}` compact 表示(print/string/repr) (Issue #6697)

- `print(m)` が verbose 多行形、`repr(m)` が空 `Memory{T}()` を返していた
  (multiple display formatters)。
- `format_value`(`vm/formatting.rs`)の Memory arm を Array 同等の compact
  `format_memory_compact` に変更し、pure-Julia `show(io, ::Memory) =
  _show_vector_compact` を `genericmemory.jl` に追加。verbose `format_memory_value`
  は削除。`[1,2,3]`/`Int64[]`/`Bool[1, 0]`/`Any[...]` が upstream と一致。
- fixture `memory/compact_show_repr_6697.jl`。多行 verbose の display 形は別経路で対象外。

## 最新対応 (2026-06-14)

### Array construction を `Memory{T}` + `Array{T,N}` wrapper へ追加移行 (Issue #6649)

- Untyped array literal (`[1,2,3]`, matrix literal `[1 2; 3 4]`) と typed empty
  literal (`Int64[]` など)の compiler emit を、native array builder
  (`NewArray*` / `PushElem*` / `FinalizeArray*`) から `NewMemory(T,len)` +
  `MemorySet` + `wrap(Array, mem, dims)` へ切り替えた。runtime surface は
  upstream と同じ `ref::MemoryRef{T}` / `size::NTuple{N,Int}` を持つ
  `Array{T,N}` wrapper になる。
- 追加で typed non-empty literal (`T[a,b,...]`)、single / tuple-destructuring /
  multi-iterator comprehension の初期 materialization、`Vector{T}()` /
  `Array{T}()` empty constructor も同じ Memory-backed wrapper route へ移行。
  `StoreArray` / `LoadArray` は wrapper `StructRef` を array-like として扱い、
  `ArrayPushTypejoin` は wrapper を grow しながら element tag を widening できる。
- `array_literal_struct_routing_6649.jl` で `typeof(a.ref) == MemoryRef{T}`、
  `a.size`、linear/matrix indexing、mutation、typed empty literal を upstream
  Julia と固定。`array_construction_remaining_routing_6649.jl` で typed literal、
  comprehension、typed empty constructor、`undef`/`zeros`/`fill`/`similar`/Bool
  constructors の wrapper route を固定。Rust bytecode guard
  `array_construction_routing_6649_tests` で public construction function body に
  native array builder 命令が出ないことを固定。
- 追加で direct `collect`/range materialization boundary (`collect(1:3)`,
  `collect(1:2:7)`, float range, tuple collect, array copy collect) の返り値
  surface も `Array{T,N}` wrapper へ移行。`array_collect_wrapper_routing_6649.jl`
  で `ref::MemoryRef{T}`、element type、copy semantics を upstream と固定。
- さらに non-empty generator/HOF-backed collect (`collect(x + 1 for x in 1:3)`,
  `Base.Generator` runtime/named function callable、filtered generator、
  tuple-splat generator) の返り値 surface も wrapper へ移行。
  `array_generator_collect_wrapper_routing_6649.jl` で `ref::MemoryRef{T}` と
  値/shape parity を固定。`get_array_type_id` は bootstrap 中に `Array` def が
  未登録でも fallback `0` をキャッシュせず、後続の `Array{Any, Any}` def を
  再探索する。
- final native carrier demotion / benchmark は #6653 で完了。

### Array wrapper indexing/shape を `MemoryRef` storage 上で upstream surface に補強 (Issue #6650)

- `Array{T,N}` wrapper は `ref::MemoryRef{T}` + `size::NTuple{N,Int}` を読み、
  `getindex` / `setindex!` / `length` / `size` / `ndims` / `eltype` を Pure Julia
  method で処理する。linear/cartesian indexing と `MemoryRef` offset storage は既存
  `test_array_pure_julia.jl` / `wrap_memoryref.jl` で固定済み。
- 今回、残っていた `axes` surface を upstream 形に寄せ、`axes(A)` /
  `axes(A,d)` が `UnitRange` ではなく `OneTo` を返すよう修正。0-dimensional
  `Array{T,0}` は `axes(A) == ()`、`axes(A,1) == OneTo(1)` になる。
- 0-dimensional wrapper の no-index mutation `setindex!(A, v)` を追加し、
  `getindex(A)` / `setindex!(A,v)` が同じ `MemoryRef` slot を読む/書くことを
  `array_axes_zero_dim_wrapper_6650.jl` で upstream parity として固定。

### Array wrapper mutation/iteration を `MemoryRef` storage 上で upstream parity に補強 (Issue #6651)

- `Array{T,N}` wrapper の vector mutation surface (`push!` / `pop!` /
  `pushfirst!` / `popfirst!` / `insert!` / `deleteat!` / `append!` / `resize!` /
  `empty!`) を `MemoryRef` storage 上で upstream の sharing semantics に寄せた。
- offset `MemoryRef` wrapper で親 `Memory` の head/tail capacity が残っている場合、
  grow/shrink は新規 `Memory` へ即コピーせず同じ parent storage と ref/size 更新で処理する。
  `pushfirst!` / `popfirst!` / `deleteat!(a,1)` は ref offset を動かし、
  `insert!` / middle `deleteat!` は同じ `Memory` 内で shift する。
- `iterate(::Array wrapper)` の state を upstream と同じ「次に読む 1-based index」に修正。
  初回 `iterate(a)` は `(a[1], 2)`、`iterate(a, 2)` は `(a[2], 3)` を返す。
- TDD: `array_memory_mutation_iteration_6651.jl` で upstream Julia parity、
  direct sjulia、offset `MemoryRef` sharing、for-loop iteration を固定。

### Array wrapper HOF/broadcast/materialization surface を固定 (Issue #6652)

- #6649-#6651 の wrapper routing と mutation/iteration 補強の上で、
  `map` / `map!` / `broadcast` / `broadcast!` / `reduce` / `mapreduce` /
  `collect` / `filter` / `filter!` / `sort` / comprehension materialization が
  `Array{T,N}` wrapper (`ref::MemoryRef{T}` + `size`) 上で upstream と同じ
  value/shape を返すことを確認した。
- HOF/broadcast の public result surface は `MemoryRef` backed `Array` として保持される。
  offset `MemoryRef` source、binary `map`、vector/matrix broadcast、filtered
  comprehension を `array_hof_broadcast_wrapper_6652.jl` で固定。

### Array native carrier demotion と VM-only benchmark を完了 (Issue #6653)

- 現行コードでは旧 `Value::Array` は #4568 で退役済みのため、#6653 は残る
  `Value::NativeArray(ArrayRef)` carrier を public route から外す最終固定として扱った。
  public construction / materialization / HOF / broadcast / `similar` / `reshape`
  は `MemoryRef` backed `Array{T,N}` wrapper surface を返し、`NativeArray` は
  precompiled cache、VM instruction fallback、formatting/REPL/host boundary 用に残す。
- `array_native_carrier_demoted_6653.jl` で literal、typed literal、empty/undef
  constructor、tuple-dims constructor、collect(range/tuple/generator)、comprehension、
  `map`/`filter`/`sort`/`broadcast`/`similar`/`zeros`/`ones`/`reshape` の
  `typeof(a.ref) == MemoryRef{T}` surface を固定。Rust bytecode guard は public
  materialization function body に `NewArray*` / `PushArrayValue` / `AllocUndef*`
  が出ないことを固定。
- `vm_array_benchmark` を追加。短縮 VM-only Criterion
  (`--warm-up-time 1 --measurement-time 1 --sample-size 10`)では、#6649 直前 baseline
  `2404f188e` に同 bench を一時適用した値に対し、現行 wrapper route は
  `index_mutation_push_pop_128` が `7.455 ms` → `25.170 ms` (約 3.4x 遅い)、
  `hof_broadcast_filter_reduce_128` が `525.90 ms` → `65.372 ms` (約 8.0x 速い)。
  index/mutation の退行は typed Memory storage / intrinsic hot loops の後続最適化対象で、
  native carrier を public default に戻さない。

### AoT が dead な Base helper を `-> Value` で出力する退行 (Issue #6629)

- `--features aot` の `test_aot_e2e_mandelbrot_broadcast_codegen_regression` が main で red:
  mandelbrot の AoT 生成 Rust に 29個の `-> Value` 関数(collect/channel/exception 系 Base machinery)が
  残存。mandelbrot/broadcast 関数自体は具象型で正しく、問題は **broadcast 特殊化(`__aot_broadcast_*`)+
  inlining 後に dead 化した Base helper を codegen が無条件出力**すること(AoT IR レベルの whole-function
  DCE が欠落)。
- 修正: `AotProgram::prune_unreachable_functions` を新設し `optimize_aot_program_full` 末尾で実行。
  entry から `CallStatic`+関数値参照(`Var`)+`CallDynamic`(全 method)で static 到達閉包を BFS し未到達を prune。
  閉包内に動的ディスパッチ(`CallDynamic`/`BinOpDynamic`)があれば全保持(無退行)、完全 static なら dead web を除去。
- テスト: 回帰ガード green + `prune_tests` 3件。full `--features aot` 3728/3728。AoT 専用 feature のため default 無影響。
### Array 移行 B の地ならし: 構造体バック配列(MemoryRef storage)の表示修正 (Issue #6649 / milestone 20)

- milestone 20 の Array 移行 B 向け地ならし。faithful `Array{T,N}` 構造体(#6648, storage=
  `ref::MemoryRef{T}`)が `println` で構造体フィールドをダンプしていたのを `vm/formatting.rs` で修正
  (`Value::MemoryRef` 分岐 + ndims を除く eltype 名抽出)。公開構築の構造体ルーティング本体は
  const/global 初期化・exports 数等のギャップが判明したため後続増分(#6649 に記録)。詳細は DONE.md。

### 繰り返し匿名型引数が `where` 型束縛を潰す (Issue #6661)

- `f(::Type{K}, ::Type{V}, n) where {K,V}` を `f(String, Int64, 1)` で呼ぶと K も V も Int64
  (第2引数型)に束縛され `Memory{Int64}` を返していた(upstream は K=String/V=Int64)。根本原因は
  `vm/slot.rs` の `build_slot_info` が両匿名 `_` パラメータを `name_to_slot` で1スロットに dedup し
  (`param_slots=[0,0,n]`)、引数束縛で上書き + `where` 型抽出 `infer_type_binding_from_frame_args`
  が両 type var を同じ collapse スロットから読んでいたこと。
- 修正: 匿名 `_` は繰り返し可能・本体から読めない位置パラメータなので各 `_` に独立スロットを割当
  (named は従来通り dedup)。`local_slot_count`/frame サイズ・`build_slot_types` サイズ基準も追従。
- テスト: `dispatch/anonymous_typed_params_where_6661.jl`(julia 1.12 parity 5/5)+ `vm::slot` ユニット2件。
  full suite 3786/3786。

### `filter(::Dict)` 結果の型消失で `empty!` が legacy Dict boundary に降格(dispatch順依存) (Issue #6672)

- `filtered = filter(p->p.second>1, d)` 後の `empty!(filtered)` が native struct-backed dispatch でなく
  legacy `DictEmpty` boundary を emit し、#6621 ガード(`dict_native_demotion_6621_tests`)が isolation で
  決定的に失敗(full-suite では順序依存で通過)する main 由来の既存 RED。根本原因は **`filter` の
  call-site 戻り値型推論が Array しか container 型を保持せず** Dict だと `None` → interprocedural fallback
  (`filter`→`copy(h)`)が depth limit で `Any` に widen し、`filtered` の型が生成元 `d`
  (`Dict{String,Int64}`)と食い違うこと。`Any` 化で collection-mutation routing が runtime 候補 +
  builtin fallback を選び legacy 化していた。
- 修正(upstream 準拠 = filter は container 型を保つ): `infer_filter_call_return_type`(hof.rs, ValueType)
  と `infer_julia_type`(julia_type.rs, JuliaType)で dict/set receiver 型を伝播、エンジンの
  `infer_filter_return_type` も一貫修正。`filtered` が `d` と同一型に推論され generic dispatch へ。
- テスト: `dict/filter_result_native_dict_6672.jl`(julia 1.12 parity 2/2)。full suite 3784/3784。

### `getindex`(`xs[i]`)(::Any) のユーザー array override dispatch (Issue #6657, getindex 部分)

- `fg(xs::Any)=xs[1]` を、ユーザー override `getindex(::Vector{Int64},::Int)` を持つプログラムで
  具体 Vector で呼ぶと native `IndexLoad` が override をバイパスしていた。3 層を協調修正:
  (1) 汎用コンパイラが `Any` 受け手 + **ユーザー(非 Base)override 存在**時のみ新 `BuiltinId::GetIndex`
  fallback 付き `CallTypedDispatchOrBuiltin(GetIndex,…)` を emit、(2) 抽象解釈エンジンが
  dispatch 勝者がユーザー method の時のみその戻り値型で `xs[i]` を推論、(3) ランタイム特殊化器が
  ユーザー array override 存在時に native-indexing fast path を bail。
- 候補から **自由型変数 array(`Array{T,N}` = Base 形)** を除外し、Base array getindex が候補に
  混入して Base body 内 `a[i]` が無限再帰するのを防止。**override の無い一般プログラムは候補空 →
  native fast path 不変**。full suite 3783/3784(唯一の赤 `dict_native_demotion_6621` は本変更前から
  main で失敗する既存問題)。フィクスチャ `dispatch/getindex_any_user_method_6657.jl`(julia 1.12
  パリティ 7/7)、bench 追加。
### AoT IR 型キャリアを `StaticType` へ統一し `aot::JuliaType` enum を削除 (Issue #6598 完遂)

- AoT 低レベル SSA IR(`VarRef`/`IrFunction`/`ConstValue`/`TypeAssert`)のキャリアを
  `aot::JuliaType` → `StaticType` へ移行。消費側(`ir_codegen`/`rooting`/`cranelift`)を更新し、
  rooting は保守的 root(heap/aggregate 全て)へ。enum・impl・`From<&aot::JuliaType> for CoreType`・
  単体テストを削除。VM 側 `crate::types::JuliaType` producer は温存。`--features aot` build 緑、
  aot テスト回帰なし。詳細は DONE.md。
- 注記: aot feature 全体に既存 clippy lint debt(標準ゲート外)。別途整理候補。
### 残 HOF を tfuncs registry / HofLambdaAnalyzer seam へ移行 (Issue #6604, 残スコープ)

- `map` 移行(PR #6644)の `arg_exprs` + `HofLambdaAnalyzer` 機構を残 HOF
  (`broadcast`/`filter`/`reduce`/`foldl`/`foldr`/`mapreduce`/`mapfoldl`/`mapfoldr`)へ適用。
  rule を `compile/tfuncs/hof_ops.rs` の free fn 群に切り出し、`hof.rs` アダプタは
  element 抽出 + rule 呼び出しに整理。analyzer を N 入力 + `reduce_result_type`(`^`/`&`/`|`/`xor`
  被覆)へ拡張。振る舞い保存、`hof::` green、julia 1.12.6 パリティ一致。詳細は DONE.md。
- engine 側並行推論と `match function.as_str()` arm 削除は対象外(後続)。
### `first`/`last`(::Any) のユーザー override dispatch (Issue #6657, first/last 部分)

- `ff(xs::Any)=first(xs)` を具体 Vector で呼ぶと wrapper 戻り値が要素型推論され override の
  非要素値で `StoreSlotI64` crash。call-site 単一関数推論エンジンが他関数の method table を
  持たず element-type tfunc にフォールバックしていたのが原因。`core_compiler.rs` で
  **user override を含む table のみ** seed(`seed_initial_method_tables`)し body 再推論で
  override を解決。non-override は tfunc fast path 維持。full suite 3784 green。
- 残: `getindex`(`xs[1]`)は `IndexLoad` fast path のため別基盤が必要で #6657 に残置。詳細は DONE.md。

### faithful `Array{T,N}`(#6648/#6659)由来の 6 fixture 退行を修正 (Issue #6663)

- #6648 の faithful Array 移行(storage=`ref::MemoryRef`)で native-array 消費側が
  新ストレージ形に未対応となり main が 6 fixture RED 化。`array_wrapper_memory_and_shape`
  と string-collect に `MemoryRef` 対応を追加(collect/`in`/`String(collect)`)、BitArray family の
  `size`/`length` + storage アクセサ(`_array_dims`/`_array_offset`/`_array_memory`)の `BitArray`
  メソッドを追加(BitArray の明示 dispatch + broadcast 結果の `Array{T,N}` body 内 helper 救済)。
- full suite **3784/3784 green**(6→0)。詳細は DONE.md。

### `haskey`/`isempty`/`empty!`(::Any) の非Bool/非コレクション override dispatch (Issue #6610 完了)

- これらの Base op をカスタム型で異なる返り値型に override し `Any` 束縛経由で呼ぶと、
  推論された返却型に強制され `ReturnI64` crash か `==` の compile-time `false` 畳み込みが
  起きていた。関数シグネチャは abstract-interp engine が registry tfunc で算出し、
  `==` の constant-fold がそれを使う。
- `tfunc_haskey`(先行 PR #6658)/ `tfunc_isempty` / `tfunc_empty_bang` を受信側型に応じて
  defer 化。非対称が肝: `isempty` は Bool 分岐条件なので struct/Any/Top で defer 可、
  `empty!` はコレクション自身を返し native-array `_mem` に流れるため **struct のみ** defer
  (Any/Top を広げると #6648 の `collect("abc")` が `_mem=Any` で壊れる、検証済み)。
  `haskey` は加えて value/julia general tfunc + `compile_haskey` も defer 化。
- 具体コレクションは精度・fast-path とも不変。julia 1.12 パリティ一致、full suite green。
  **#6610 クローズ。**

### Array wrapper を upstream 形 `Array{T,N}` へ移行 (Issue #6648)

- `subset_julia_vm/src/julia/base/array.jl` の Pure Julia Array wrapper を
  旧 `mutable struct Array{T}; _mem; _size; end` から、upstream 形に近い
  `mutable struct Array{T,N} <: DenseArray{T,N}; ref::MemoryRef{T};
  size::NTuple{N,Int}; end` へ移行した。`wrap(Array, Memory{T}/MemoryRef{T}, dims)`
  は `N=0..16` の rank-specialized constructor へ流し、wrapper `ndims` は
  size tuple の長さではなく value parameter `N` を返す。
- 互換層として、移行中の native `Value::NativeArray` / 旧 `_mem`・`_size`
  アクセスを VM の `GetFieldByName` / `SetFieldByName` に集約。既存の
  `push!`/`pop!`/`similar`/logical indexing/iteration/linalg などは
  `MemoryRef` storage の wrapper と native carrier の双方を扱う。native
  `reshape(a, dims...)` / `reshape(a, dims::Tuple)` は alias 保持のため VM
  builtin を優先し、runtime dims parser が単一 tuple dims を展開する。
- #6648 当時の残スコープだった public `Array`/`Vector` constructor の
  pure-Julia wrapper ルーティングは #6649、native carrier 降格は #6653 で完了。
  今回の #6648 自体は faithful struct shape と wrapper 実行互換の foundation まで。
- 検証: direct fixture
  `array/test_array_pure_julia.jl`, `array/reshape_range_5758.jl`,
  `array/slice_index_logical_3908.jl`,
  `array/prod_dims_type_preservation_4614.jl`,
  `array/similar_receiver_typevar_binding_4018.jl`。category
  `timeout 1800 cargo nextest run --release --test fixture_tests array::`,
  `memory::`、lib smoke `test_array_functions` と
  `array_wrapper_julia_type_uses_native_array_mem_element_type_issue_4340` が green。

### `Dict{K,V}` storage を upstream Memory-backed shape へ移行 (Issue #6617)

- Pure Julia `Dict{K,V}` struct の内部 storage を旧 untyped
  `Vector{Int64}` / `Vector{Any}` から upstream Julia 1.11+ 形の
  `slots::Memory{UInt8}`, `keys::Memory{K}`, `vals::Memory{V}` へ移行した。
  slot sentinel も `UInt8` 定数化し、`_shorthash7` / probing / deletion /
  `empty!` / `rehash!` は `Memory{UInt8}` 上で動作する。
- `_new_dict_kv(::Type{K}, ::Type{V}, n)` helper を追加し、
  `Dict{Any,Any}` だけでなく typed storage (`Dict{String,Int64}` など)を
  直接構築できるようにした。`rehash!` は `where {K,V}` で受け、resize 後も
  `Memory{K}` / `Memory{V}` を維持する。
- 実装中に、複数の匿名 typed 引数 `(::Type{K}, ::Type{V})` が同じ `_` slot に
  畳まれて `where` binding を壊す既存バグを発見。bug Issue #6661 を作成し、
  helper では未使用引数に名前を付ける documented workaround を置いた。
- スコープ注記: literal / comprehension / public `Dict(...)` の Rust-backed
  `Value::Dict` fast path は維持。今回の #6617 は pure-Julia struct storage の
  faithful foundation で、public routing / native 表現降格は後続 phase。
- 検証: direct `sjulia` で `_new_dict_kv(0)` と
  `_new_dict_kv(String, Int64, 4)` の型・field 型を確認。Julia 1.12 parity
  `dict/test_dict_type_params.jl`、`fixture_tests dict::`、lib
  `test_dict_functions`、workaround sync scripts が green。

### Generic `Dict` constructors を typed Memory-backed struct へ移行 (Issue #6618)

- `Dict(ps::Pair...)` と `Dict(kv)` の ordinary Julia constructor 経路を、
  legacy `Value::Dict` ではなく `_new_dict_kv(K, V, n)` で構築する
  `Dict{K,V}` struct へ移した。Pair splat は各 `p.first` / `p.second` の
  runtime 型を `typejoin` し、iterable constructor は tuple/zip entries を一度
  走査して `K` / `V` / capacity を決める。
- upstream Julia 1.12 と合わせ、`Dict(pa, pb)` の narrow integer values は
  `Dict{String,Signed}`、mixed key family は `Dict{Any,Int64}`、tuple/zip iterable
  は entry value 型から `Dict{String,Int64}` / `Dict{String,Int16}` に狭まる。
  backing fields も `Memory{String}` / `Memory{Signed}` など typed storage を保つ。
- 既存 #6571 fixture は literal `Value::Dict` public surface の回帰網へ戻し、
  #6618 の struct constructor narrowing は専用 fixture
  `dict_constructors_6618.jl` で固定した。full operation parity
  (`keys`/`values`/`pairs`/`copy`/`empty!`/`merge` などの mixed legacy+struct 面)
  は #6620、literal/comprehension/empty/typed public constructor fast path の
  route 変更は #6619 に残す。
- 検証: direct `sjulia` / Julia 1.12 で #6571 parity fixture と
  `dict_constructors_6618.jl` が green。`timeout 1800 cargo nextest run --release
  --test fixture_tests dict::`、`timeout 1800 cargo nextest run --release --lib
  test_dict_functions` が green。

### Public `Dict` construction を pure-Julia struct routing へ移行 (Issue #6619)

- Public `Dict(...)` / `Dict{K,V}(...)` の literal Pair、empty、typed empty、
  comprehension/generator 由来 construction を legacy `Value::Dict` fast path から
  Pure Julia `Dict{K,V}` struct method 経路へ寄せた。`NewDict*` bytecode は
  cache-compatible decode 用に残すが、新規 public construction では生成しない。
- `Pair{K,V}` JuliaType と Dict constructor tfunc を接続し、literal
  `Dict("a"=>1)` などの戻り型を struct-backed `Dict{K,V}` として local slot /
  `getindex` / `setindex!` / `pairs` iteration に伝播するようにした。明示
  `Dict{K,V}(...)` は type object 値を Pure Julia helper に渡す routing で扱う。
- struct-backed Dict の public operation parity も今回の routing に必要な範囲で補強。
  `get!` / `getkey` / `copy` / `merge` / `merge!` / `mergewith` / `mergewith!` と、
  `keys` / `values` / `pairs` / mutating fallback の mixed legacy+struct dispatch が
  user override を潰さず Base `Dict{K,V}` method に解決される。
- 残スコープ: broader mixed legacy `Value::Dict` + struct Dict parity / cleanup は #6620。
  #6619 の public construction routing は完了。
- 検証: direct `sjulia` / Julia 1.12 parity
  `dict_construction_fastpath_6589.jl`, `dict_constructors_6618.jl`。
  `timeout 1800 cargo nextest run --release --test fixture_tests dict::`,
  `timeout 1800 cargo nextest run --release --lib test_dict_functions`,
  `timeout 1800 cargo nextest run --release --lib expr_tfuncs` が green。

### `Dict{K,V}` op/display parity を補強 (Issue #6620)

- struct-backed `Dict{K,V}` の public operation parity を追加補強。
  `keys(d)` は eager `Vector` ではなく lazy `KeySet{K}`、`values(d)` は
  `ValueIterator` を返し、`collect` / `length` / `isempty` / membership が動く。
- `in(::Pair, ::Dict{K,V})`、`filter` / `filter!`、`==` / `isequal` / order-insensitive
  `hash(::Dict{K,V})` を Pure Julia 側に追加。`hash(d)` は 1-arg Rust builtin から
  `Dict{K,V}` receiver の時だけ generic dispatch に逃がす。
- compact display は `repr(d)` と `string(d)` が `Dict("k" => v)` 形で一致するよう、
  concrete `show(::IO, ::Dict{K,V})` を追加。local/Any 経由の `in` は Base candidates
  を runtime dispatch に含め、lazy view local でも builtin type error に落ちない。
- fixture `dict_op_display_parity_6620.jl` で lazy views、Pair membership、
  filter/filter!、equality/hash/display、mutation reference semantics、mixed
  Float/Type/Symbol keys、rehash lookup を upstream Julia 1.12 と比較して固定。
- 残スコープ: native `Value::Dict` / `NewDict*` / Rust Dict builtin の public route
  demotion と cleanup は #6621、performance measurement は #6622。

### native `Value::Dict` / `NewDict*` route を境界用途へ降格 (Issue #6621)

- `Expr::DictLiteral` の compiler path を `NewDict` + `DictSet` 直接 emit から
  Pair 引数つき `Dict(...)` method call へ変更。通常の public `Dict(...)` /
  `Dict{K,V}(...)` と同じく Memory-backed `Dict{K,V}` struct へ流れる。
- Public struct-backed `Dict{K,V}` 操作は `getindex` / `setindex!` / `get` /
  `getkey` / `haskey` / `keys` / `values` / `pairs` / `filter` / mutation helpers
  の user function bytecode に `NewDict*`、`LoadDict` / `StoreDict` / `ReturnDict`、
  public `BuiltinId::Dict*` fallback を出さないことを Rust regression test で固定。
- `Value::Dict`、`DictValue`、`_dict_*` intrinsics、public `BuiltinId::Dict*` handler、
  `NewDict*` decode/exec は旧 bytecode/cache 互換と VM boundary として残置。
  `BUILTIN_REMOVAL.md` と `DICT_INDEXING.md` をこの分類へ更新。
- 残スコープ: #6622 で no-JIT iOS hot Dict performance の測定・docs finalization。

### Pure Julia `Dict{K,V}` の VM-only benchmark を追加 (Issue #6622)

- `subset_julia_vm/benches/vm_dict_benchmark.rs` を追加。compile 済み bytecode の
  `Vm::run()` だけを測り、typed `Dict{Int64,Int64}` / `Dict{String,Int64}` で
  insert、lookup、iterate、delete、post-delete insert(rehash 方向)を実行する。
- 短時間 Criterion 設定
  `timeout 1800 cargo bench -p subset_julia_vm --bench vm_dict_benchmark -- --warm-up-time 1 --measurement-time 1 --sample-size 10`
  で測定。#6619 直前の `2a1691063` に同じ benchmark を一時適用した旧
  `Value::Dict` route は Int keys `10.992 ms`、String keys `12.022 ms`。
  現行 Pure Julia struct route は Int keys `73.876 ms`、String keys `48.476 ms`。
  VM-only regression はそれぞれ **6.7x** / **4.0x**。
- 退行は interpreter 上の Pure Julia hash-table method 実行が native Rust HashMap
  fallback より重いことによる想定内コストとして受け入れる。follow-up は
  `Value::Dict` default 復帰ではなく、typed `Memory` field の slotized access、
  `ht_keyindex` / `_setindex!` hot-loop helper、`Dict{K,V}` method body の
  specialization/call lowering 改善に限定する。
- #6571 Dict migration は #6617–#6622 で閉じ、残スコープは performance follow-up
  issue として分離する。

### `haskey(::Any)` の非Bool override dispatch (Issue #6610, haskey 部分)

- `haskey` をカスタム型で非Bool返却に override し `Any` 束縛経由で呼ぶと、`Bool`
  固定推論により `ReturnI64` が override 値で crash。`tfunc_haskey` / value+julia
  general tfunc / `compile_haskey` の 3 チャネルすべてが受信側型に関係なく `Bool`
  固定だったのが原因。
- 「具体 `Dict`/`NamedTuple` のみ `Bool`、それ以外は defer」へ統一(`sqrt` の
  `ConcreteDeferStructAny` / `compile_keytype_valtype` の user-candidate パターン
  踏襲)。具体 Dict は `Bool` のまま。julia 1.12 パリティ一致、full suite 3782 green。
- 残: #6610 の `isempty`/`empty!` override は multi-channel で範囲が広く #6610 に残置。
  詳細は DONE.md。

### `iterate(::Any)` の native 配列 → ユーザー `iterate` dispatch (Issue #6638)

- `IterateDynamic` が native 配列(`Value::NativeArray`)を候補スコアリングから
  除外していたため、`iterate(xs::Any)` がユーザー定義 `iterate(::Vector{Int64})`
  に dispatch できなかった(`collect` の `CallDynamic` は動作)。
- `can_score_iterate_dynamic_candidates` に `Value::NativeArray` を追加し、配列は
  **ユーザー定義候補のみ**をスコアリングする(`scored_iterate_candidates`)。Base に
  Array/Vector 用 `iterate` は無いため、override が無い限り VM 組み込み iterator が
  既定のまま(#5584 維持)。
- フィクスチャの末尾 bare `true` が `@testset` 失敗をマスクしていた点も是正し、
  実 dispatch 結果の論理積で manifest `expected = true` をゲート。
- julia 1.12 パリティ一致、full suite 3781/3781 green。詳細は DONE.md。

### `LatticeType → JuliaType` 乖離ペアの単一化 (Issue #6599)

- 親 epic #5916 の引き継ぎ残スコープ(2)。`LatticeType → JuliaType` には
  **構造保存版**(`lattice_to_parametric_julia_type`, 変換表 #14)と
  **文字列不透明版**(`lattice_to_julia_type`, #15, 共有コア
  `concrete_type_to_julia_type` #16 経由)の乖離があった: braced な
  `ConcreteType::Struct { name }`(例 `Vector{Int64}` / `Complex{Float64}`)を
  前者は `from_name_or_struct` でパースして構造化 `JuliaType`(`VectorOf` 等)に
  復元する一方、後者は不透明な `JuliaType::Struct("Vector{Int64}")` 文字列のまま
  にしていた。
- **upstream 検証(julia 1.12)**: `Vector{Int64} === Array{Int64,1}` で具体
  パラメトリック `DataType`、`Base.return_types` も構造化型を返す(不透明な名前
  文字列ではない)→ 構造保存版が正しい。共有コア `concrete_type_to_julia_type` の
  `Struct` arm を braced 名のみ `from_name_or_struct` 経由にし、#14/#15 を単一化。
  engine 側 `concrete_type_to_julia` は既に `lattice_to_julia_type` へ委譲済みの
  ため、この 1 点修正で全 lattice→JuliaType 経路が一致する。
- **保守的ゲート**: bare(非 braced)名は不透明 `Struct(name)` のまま据え置き
  (`ComplexF32` 等のエイリアス名/プリミティブ名と同名の user struct を誤って
  プリミティブに再解釈しない)。§3.5 の意図的 widening
  (`LatticeType::Bottom → ValueType::Any`)・#4679 special-case・Top→Any 禁止は
  すべて不変。
- 残スコープ(#6599 として継続): `ValueType` を `LatticeType` の薄いビューへ
  降格する本体構造変更(変換表 §4 Phase 4/6)は未着手のまま。今回は乖離ペアの
  単一化スライスのみを着地。
- 検証: unit `bridge::test_lattice_to_julia_type_pair_agrees_on_braced_struct_issue_6599`
  / `…_unification_preserves_pins_issue_6599`(TDD: 先に red を確認)、bridge/lattice/
  concrete lib 190/190 green、clippy `-D warnings` clean、フルスイートはワークスペース
  ルートから実行。

## 最新対応 (2026-06-13)

### HOF call-site lambda 推論を TransferFn の式参照チャネルへ拡張し `map` を registry 経路へ (Issue #6604)

- `TFuncContext` に式参照チャネル `arg_exprs: Option<&[Expr]>` と、`map` のような
  HOF が lambda の*式*を解析できるようにする narrow trait
  `HofLambdaAnalyzer`(`compile/tfuncs/registry.rs`)を追加。これが #6534 で
  「`TransferFn` は引数 lattice 型しか見えず lambda 式そのものを解析できない」と
  文書化されていたギャップを埋める。`HofLambdaAnalyzer` は構造的に
  `StructInstantiation` の HOF 版(`&mut` seam): 式推論側(`CoreCompiler`)が実装し、
  registry は具体型に依存しない。
- registry 側の `map` ルールを `compile/tfuncs/hof_ops.rs::map_call_result` に新設。
  lambda の戻り型を analyzer 経由で求めて `Array{U}` に sharpen できる場合はそれを、
  できない場合は従来の素の `array_ops::tfunc_map`(named type-converter sharpen +
  要素型保存)へフォールバックする(挙動が悪化しない)。`infer_map_call_return_type`
  は registry ルールを駆動する薄いアダプタに縮小。
- 残スコープ(#6604 として継続): `broadcast` / `filter` / `reduce` / `foldl` /
  `foldr` / `mapreduce` は今回未移行で、まだ `infer/mod.rs` の HOF arm で式推論層に
  直接残る。`map_call_result` は unary `map` 形のみ sharpen(多引数 map は保守的
  フォールバック)。式参照チャネル(`arg_exprs`)と analyzer seam は他 HOF の移行でも
  再利用できる設計。
- 検証: fixture `hof/hof_map_registry_inference_6604.jl`(sjulia / julia 1.12 とも
  14 passed / 0 failed)、unit `compile::tfuncs::hof_ops::tests`(4 件)。
  full fixture 142/142・full lib 2811/2811 green、clippy `-D warnings` clean。
### value/name チャネルの wrapper fence を selection core の policy へ吸収 (Issue #6595)

- 親 epic #6502 の残スコープ(3)。value channel(`find_best_method_index_from_candidates`)と
  name channel(`resolve_typed_runtime_core_candidates_with_subtype_fallback`)の候補集合は
  native-array wrapper fence で非対称だった。fence 判定を `call_dynamic_typed.rs` 私有の
  `typed_dispatch_signature_is_broad_any` から selection core の policy
  `selection::signature_is_broad_wrapper_fence` へ移管し、broad winner repair の制御フローも
  `selection::wrapper_fence_name_channel_repair` へ抽出。ハンドラは core helper の薄アダプタ化。
- ハザード #6528 を保存(broad `::Function` が typed 特殊化を上書きして空 reduce を壊さない)。
  `selection::tests::wrapper_fence_*` 9 unit + `hof::` 空 narrow-int/Bool reduce fixture parity。
### pre-scan 退役 2/3: For/ForEach の内部推論を engine 注入へ (Issue #6602)

- `compile/inference.rs` の `collect_local_types_with_mixed_tracking` 内、`For` 端点
  (start/end/step) と `ForEach` iterable のループ変数型付けを、legacy pre-scan
  (`infer_value_type_with_structs` + 並行 `promote_range_element_value_type`) から
  共有推論エンジン注入へ移行。エンジン自身の式推論 `infer_expr_result` +
  エンジンが `Stmt::For`/`Stmt::ForEach` で使うのと同一のラティス補助
  (`range_element_type` / `loop_analysis::element_type`) を経由し、`bridge::lattice_to_value_type`
  でループ変数型へブリッジする(`inference.rs` の ForEach ラティス挿入と同経路)。
- エンジンは struct table + globals シードのみで関数表なし(置換した legacy pre-scan と
  同等の能力スコープ)。**遅延構築**(最初のループ文到達時のみ)し再帰へ thread するので
  ループ無し関数本体は追加コスト 0、ループ有り本体でも 1 回のみ構築。
- 不要化した `promote_range_element_value_type` を削除。`For`/`ForEach` ループ変数の
  数値レンジ昇格・配列要素型・String→Char を pin する unit テスト 2 本追加
  (`for_loopvar_numeric_ranges_via_engine_issue_6602` /
  `foreach_loopvar_element_type_via_engine_issue_6602`)。
- 残スコープ(#6602 本体外): 関数本体/inner ctor/main のスロット型 2パス化 (#6601)、
  `collect_global_types_for_inference` (#6603) は別スライス。
### CallDynamic family-fallback の `to_julia_name()` 文字列 tier を構造化照合へ (Issue #6593)

- `CallDynamic` / `CallDynamicOrBuiltin` / `IterateDynamic` の structured resolver が使う same-family fallback `runtime_core_family_fallback_matches`(`vm/exec/call_dynamic.rs`)が、`actual`/`expected` の `CoreType` を `to_julia_name()` で Julia 名文字列にレンダーし直してから `extract_base_type` + `strip_module_prefix` で base 名を再パースしていた最後の「文字列 tier」を廃止
- 新 accessor `CoreType::nominal_family_name(&self) -> Option<&str>` を追加。`Struct`/`AbstractUser`/`Named`/`Module` の nominal 変種から module prefix と parametric `{...}` を剥がした bare family 名を構造化表現から直接読む(共有 `nominal_family_name` ヘルパー経由)。非 nominal 変種(`Any`/`Tuple`/`Union` …)は `None`。family fallback はこの accessor 同士の比較に置換され、dispatch 毎の String 確保 + 再パースが消滅。`expected` は `core_type_allows_family_fallback` で bare `Struct`/`Named` に gate 済みなので挙動不変
- t-wada TDD: 移行前に pin テスト `nominal_family_name_strips_module_and_params_issue_6593` を追加。`structured_slice_resolver_uses_family_fallback_issue_6502` の照合クロージャも `to_julia_name()` round-trip から `nominal_family_name()` へ更新
- 残余 string-encoded resolver(`resolve_runtime_type_pattern_candidates*` / `runtime_type_pattern_score*`)は #6543 slice 2(`2cbf489ca`)で既に `#[cfg(test)]` parity oracle 化済みで production 不参照。本 slice で family-fallback hot path から `to_julia_name()` round-trip が完全消滅。`core_signature`/`MethodSig` シリアライズ形状は不変(CACHE_VERSION bump 不要)
### User macro expansion-time execution + mutable `Expr.args` (Issue #6616)

- User-defined macros now follow the upstream Julia invocation shape: a macro
  body is executed during lowering as a synthetic function receiving hidden
  `__source__`, `__module__`, and unevaluated AST arguments. This lets
  Symbolics-style macros delegate to helpers such as
  `parse_vars(:variables, Real, xs)` and use the helper's returned AST as the
  expansion, instead of leaving the helper call for runtime.
- Macro expansion return values are converted structurally from runtime AST
  values back to sjulia IR, including common upstream heads used by existing
  fixtures: `:block`, `:call`, `:=`, `:local`, `:macrocall`, `:if`, `:for`,
  `:const`, tuples, vectors, refs, quotes, and operator/range calls.
- `Expr.args` now has upstream reference semantics (`args::Array{Any,1}`):
  field access returns the shared mutable array owned by the `Expr`, so
  `push!(ex.args, ...)` mutates `ex`.
- Regressions: `macro::macro_expansion_helper_call_6616` and
  `metaprogramming::metaprogramming_expr_args_mutation_6616`.

### `comparison.rs` の `Type{<:Bound}` permissive fallback を StructHierarchy で厳密化 (Issue #6596)

- `types/julia_type/comparison.rs` に hierarchy-aware の
  `JuliaType::is_subtype_of_in(other, &StructHierarchy)` を追加。`Type{<:Bound}` /
  bounded-typevar / `where`-bound UnionAll の bound 名が `from_name` で解決できない
  場合、hierarchy 供給時は `CoreSubtypeEngine::with_hierarchy` で厳密判定し、
  非供給時は従来どおり permissive(既存呼び出し元の挙動を保存)。
- 残スコープ(意図的に未着手、理由付き): 実 dispatch / runtime `<:` `==` 経路は
  既に hierarchy-aware で正しいため、`is_subtype_of_in` を production caller へ
  全面配線する必要はなかった。`is_subtype_of`(hierarchy なし)を使う低トラフィック
  compile-time 経路(`compile/lattice/ops.rs` の `LatticeType` subtype、
  `types/julia_type/parsing.rs` の union 簡約、`vm/type_utils.rs::type_values_subtype`)
  は permissive のまま。これらに hierarchy を引き回すには `is_subtype_of_parametric`
  + `dispatch_resolver::julia_type_pattern_matches` まで配線が波及し、#6596 の
  clean slice 範囲を超える(かつ現状パリティに影響しない latent residue)。
- 検証: fixture `dispatch/typebound_strict_structhierarchy_6596.jl`(sjulia/julia
  1.12.6 とも 14 passed / 0 failed)、unit `typebound_hierarchy_strictening_issue_6596`、
  フルスイート 3751 緑。
### `expr_tfuncs.rs` の pinned adapter divergences 縮小 (Issue #6600)

- `compile/expr/infer/expr_tfuncs.rs` の `julia_type_to_lattice`(tfunc 引数の
  `JuliaType → LatticeType` 変換)は正準 `bridge::julia_type_to_lattice` へ委譲済み
  だが、明示 pin された adapter divergence が残っていた(§3.6 / 変換 #20)。
- **adapter レベル**の pin 監査テスト
  (`pin_audit_load_bearing_arms_diverge_dead_arms_match`)を追加。各 pin arm を
  「local pin」と「canonical 委譲」の両方で全 julia-path adapter entry point に
  通し(`#[cfg(test)]` 限定の委譲フック経由)、最終 adapter 出力が変わるかを比較。
- 結果: 残る pin は **すべて load-bearing**(deferral/type-object/range/
  abstract-string・char・array、および `Module`/`Function`/`IO`/`IOBuffer`/
  `NamedTuple`/メタプログラミングノード/`Pairs`/`Generator`/`Enum` の "legacy
  result" pin は `min`/`max`/`reverse` 等の identity tfunc 出力を変える)。
  **唯一 dead だった `TupleOf(_) → Tuple{}` pin を削除**(canonical へ委譲し
  構造化 `Tuple{…}` を保持)。adapter 出力は不変(全 julia-path で tuple は
  `julia_type_from_concrete_type` により bare `JuliaType::Tuple` へ畳まれ、
  `length → Const(Int64(n))` も `lattice_to_julia_type` で `Int64` に widen される)。
- 検証緑: `type_inference::` fixtures(4 chunk)、`compile::` unit 1309 件、
  `expr_tfuncs::tests` 61 件(新監査含む)。`#5922` の registry 移行と連動。
### `aot::StaticType` 重複射影を共有 `CoreType` へ降格 (Issue #6598)

- 親 epic #5916 / Milestone #19。変換 #7(`From<&vm::JuliaType> for aot::StaticType`)
  の手書き fallback から、先行する `from_vm_julia_type_lossy`(CoreType 経由)が既に
  生成する `Array`(bare)/`MatrixOf` の2アームを削除し、`Array`/`Matrix`/`Vector` 族の
  AoT backend 射影を CoreType 単一経路へ一本化。残アームは CoreType が意図的に射影
  しない形のみ。`aot::JuliaType` enum 本体は AoT IR 型キャリアとして残置(完全削除は
  #6599 領域の構造変更)。pin `test_issue_6598_array_projections_route_through_core_type`。
  詳細は DONE.md / TYPE_REPRESENTATIONS.md(変換 #7 行)を参照。
### Memory{T}-centric collections 基盤 (Milestone #20 / Issue #6624)

- 目標(#6624): `Memory{T}` を唯一の Rust collection 境界とし、Vector/Array/Dict を
  Memory 上の pure-Julia struct に再ベースする(upstream Julia 1.11+ の階層 —
  `Array{T,N}; ref::MemoryRef{T}; size::NTuple{N,Int}`)。
- **基盤3件を実装・マージ**(全フルスイート green):
  - **#6623**(PR #6630): `Memory{K}`/`MemoryRef{K}` を parametric struct
    フィールドにできるよう `context.rs::substitute_field_type` に Memory/MemoryRef
    arm を追加(従来は F64 デフォルトに落ちていた)。
  - **#6626**(PR #6632): `MemoryRef{T}` を struct フィールドに(`Any→Memory`
    coercion)+ `Memory`/`MemoryRef` を `is_builtin_type_name` に追加して型値化
    (isa/`<:`/println)。
  - **#6625**(PR #6633): 整数値型パラメータの**値抽出**を修正
    (`f(::Arr{T,N}) where {T,N}=N` が DataType でなく `2::Int64` を返す)—
    `bind_type_params` + `LoadTypeBinding` で value local を優先。
- **アーキテクチャ実証**(PR #6635): pure-Julia `Array{T,N}` over `Memory{T}` が
  構築・getindex/setindex!・length/size/ndims・iteration までエンドツーエンドで
  動作することを fixture で固定(`pure_julia_array_over_memory_6627.jl`)。
- **#6640**(PR #6642): `obj.field[i] = v` / `+=` を `setindex!(getfield(...))` に
  desugar(従来 `UnsupportedAssignmentTarget`)。pure-Julia collection struct が
  `a.mem[i]=v` を直接書けるようになり、移行のローカル変数回避が不要に。
- **#6627(public Array/Vector を pure-Julia struct に置換し native carrier を降格)**
  は、Array が言語・Base ブートストラップに深く使われるため 6 サブステップ
  #6648–#6653(忠実 struct → 構築ルーティング → indexing → mutation/iterate →
  map/broadcast → native carrier 降格 + ベンチ)へ分解して完了。`Value::Array`
  は #4568 で退役済みで、残る `Value::NativeArray` は cache/VM/host 互換境界として残す。
- **横断状況**: Dict(#6571/#6617–#6622)と Array(#6624/#6627/#6648–#6653)は
  public route の struct 化と native 表現の降格まで完了。今後の性能改善は typed
  Memory storage / intrinsic hot loops の個別 follow-up で扱う。

### `::Function` carve-out (#6512 WORKAROUND) の削除可否を再評価 (Issue #6597)

- #6512 の `::Function` exact-name carve-out は既に PR #6524 で削除され、
  `type_matches` は `check_subtype` 経由でエンジン委譲している。#6597 はこの削除が
  **完全かつ安全**であることを再評価・確定(残る carve-out なし、upstream julia
  1.12 と全パリティ一致)。
- 検証3ケース緑: (a) 直接 callable `+`/`*`、(b) `map(+/*, ...)`、(c) 空 narrow-int /
  Bool `reduce`/`mapreduce`(#6528/#6529 回帰、空集合 throw なし)。回帰は unit
  `runtime_type_matches_function_param_via_core_subtype_issue_6597` + fixture
  `arithmetic/narrow_int_wrapping_5205.jl` の #6597 ブロックで pin。WORKAROUNDS.md
  の #6512 行を実 PR 付き Resolved 化。
### Dict pure-Julia 移行エピック完了 (Issue #6571 / Milestone #18)

- マイルストーン #18 の全子 Issue を解決しエピック #6571 をクローズ。公開 `Dict`
  サーフェスは dispatch-correct かつ user/`Struct` レシーバで method-dispatch-first、
  `Value::Dict` は Public Base Routing Rule に従い primitive fast-path /
  cache-compat fallback としてのみ残置。
- **#6585** 修正(PR #6609): `get!` の `Any` 経路返り値が Float 化していた根本原因
  = bare `ValueType::Dict` の推論ラティス変換が key/value を `Float64` デフォルトに
  していたこと(`compile/bridge.rs`)。`Any` へ修正(`Array` と同様)。フルスイート
  3732 通過。
- **#6586**(PR #6611): ディスパッチ整合を検証・固定(21操作の `Any` ディスパッチ
  マトリクス + 副作用観測の no-shadowing fixture)。user メソッドが Rust fallback に
  勝つことを確認。
- **#6587/#6588**(PR #6612): `getindex`/`setindex!` と `keys`/`values`/`pairs`/
  `merge`/`copy` が user/`Struct` で method-first、`Value::Dict` は fast-path 維持を
  検証・固定。
- **#6589**(PR #6613): `Instr::NewDict*` を構築 fast-path + cache-compat primitive
  として正式分類。literal/comprehension/typed 構築のパリティを固定。
- スコープ注記: `Value::Dict` を完全撤廃する表現スワップは **意図的に対象外**(no-JIT
  iOS の hot Dict パスを退行させ、利益がない)。詳細分類は `BUILTIN_REMOVAL.md` の
  "Dict → Pure Julia Migration Audit (Issue #6571)" 節。
- 副産物の一般推論エッジケース #6610(Bool 返し builtin を異なる戻り型で上書き →
  `Any` 経由で破綻)は移行スコープ外の standalone bug として分離。

### Dict → pure Julia 移行の基盤整備 + `empty!` ディスパッチ修正 (Issues #6571, #6584)

- エピック #6571(公開 `Dict` を pure Julia `Dict{K,V}` へ移行)の基盤 PR。
  全 `BuiltinId::Dict*` / `_Dict*` ハンドラを **公開API / プリミティブfallback /
  VM境界 / キャッシュ互換** の4種に分類した監査を `docs/vm/BUILTIN_REMOVAL.md`
  の "Dict → Pure Julia Migration Audit" 節に記載。`DICT_INDEXING.md` から相互参照。
- 監査中に発見した実バグ #6584 を修正: `Any` 型バインディング(関数引数)経由で
  `Value::Dict` に `empty!` を呼ぶと `MethodError` になっていた。bare
  `empty!(d::Dict) = _dict_empty!(d)` を追加(既存 `haskey`/`get` と同じ
  bare ラッパーパターン)。typed パスは従来通り `CallBuiltin(DictEmpty)`。
- セーフティネット fixture `dict/dict_pure_julia_parity_6571.jl` を追加。
  リテラル/非リテラル4経路で公開 Dict サーフェスを `Any` 型ディスパッチ経由で
  突き合わせ(upstream Julia 1.12 検証済み)。
- `mod.rs` に `test_dict_functions()` シグネチャ smoke-test を追加(従来 Set/Array
  にはあったが Dict には無かった)。bare-vs-parametric メソッド分割を pin。
- 残る深いコード移行は #6586–#6589 に分解し、マイルストーン
  *Dict pure-Julia migration (#6571)* で管理。型保存バグ #6585(`get!` の
  `Any` 経路返り値が Float 化)も同マイルストーンに起票。

### Dict non-literal constructor routing (Issue #6531)

- `Dict(p::Pair)`, `Dict(p, q)`, `Dict(pairs::Vector{Pair})`, and
  `Dict(zip(keys, vals))` now route through ordinary Julia constructor methods
  instead of falling into the internal 8-field `Dict{K,V}` struct constructor
  path.
- Literal pair arguments and Dict comprehensions keep the existing `NewDict*`
  fast path. The larger migration of public Dict semantics fully onto pure
  Julia `Dict{K,V}` is tracked separately as Issue #6571.
- Regression: `collections::collections_dict_nonliteral_constructor_6531`.

### Type/AbstractVector diagonal dispatch via Any (Issue #6573)

- Runtime `CallTypedDispatch` now lets anonymous covariant bounds such as
  `AbstractVector{<:Real}` use the structured subtype fallback while still
  rejecting named diagonal type-variable mismatches such as
  `(::Type{T}, ::AbstractVector{T})` with `Integer` and `Vector{Int64}`.
- This restores upstream-compatible selection for the Any-routed
  `Type{Integer}` / `Vector{Int64}` case that surfaced during the full fixture
  verification for Issue #6531.
- Regression: `dispatch::type_abstract_vector_diagonal_6239` and focused unit
  coverage for Issue #6573.

### Type/AbstractArray rank-TypeVar diagonal dispatch via Any (Issue #6577)

- The same structured typed-dispatch fallback now distinguishes type variables
  already bound by an earlier slot from fresh rank variables such as `N` in
  `AbstractArray{<:Real,N}`. Fresh rank variables may use the subtype fallback;
  previously they blocked the fixed `Type{Integer}` method for Any-routed
  `Integer, Vector{Int64}` calls.
- Regression: `dispatch::type_abstract_array_rank_typevar_diagonal_6249` and
  focused CoreType resolver coverage for Issue #6577.
### Base cache を varint bincode 化して payload を 68% 削減 (Issue #6453)

- profile で支配要因を特定: デフォルト fixint bincode が `Instr` ごとに 4byte u32
  discriminant・`usize` ごとに 8byte を使い、~78k 命令 + ~4.7k functions の Base
  payload(`code` 1.50MB / `functions` 1.08MB)を固定幅が支配。
- Base cache 全 section を varint bincode(`cache_codec`)へ。decode 結果 bit-identical の
  wire-format 変更のみ。`allow_trailing_bytes` で version-gate のストリーミング読み出しを維持、
  `CACHE_VERSION` 47 / persistent namespace v3 で旧キャッシュは graceful miss。
- 実測: persistent Base cache **3.12MB → 0.99MB(68.2%減)**、`code` -70%/`functions` -66%。
  embedded cache(iOS バイナリ埋め込み)では ~2.13MB のバイナリ削減。decode CPU は
  +7.5%(micro-bench)だが読み出しバイト 2.13MB 減 + 通常は warm-prefetch で critical path 外。
  precompile/cache round-trip 37 件 green。#6440/#6449 の follow-up。

### tuple リテラル destructuring swap を tuple 確保なしで lowering (Issue #6569)

- CPython 3.14 実測比較で swap ループが sjulia で約 9 倍遅い(原因=毎イテレーションの
  tuple ヒープ確保)。`lower_tuple_destructuring_impl` の自己参照型 swap `a, b = b, a%b`
  を temp tuple + `IndexLoad` から **各要素を個別 temp へ評価 → 各 target へ代入**
  (`__t0=b; __t1=a%b; a=__t0; b=__t1`)に変更。同時代入セマンティクスを保ちつつ
  `NewTuple`/`IndexLoad` を完全除去。specializer でそのまま型安定(#6561 の tuple-element
  追跡を経由せず; mixed 型 swap も各 target が自型維持)。非 tuple リテラル RHS と
  arity 不一致は従来の temp-tuple 経路維持。
- VM-only ベンチ `vm_swap_accumulate`: ~121.5ms(#6561)→ **~14.6ms(約 8.3x)**。
  CLI swap ループ(2M回)は **0.83s→0.11s で CPython 3.14 と同等**。fixture
  `tuple/swap_no_tuple_alloc_6569.jl` + integration `swap_without_tuple_alloc_6569_tests.rs`
  で pin。#6561/#6346 の follow-up。

### TypeExpr 表示 / simple-name projection の正準化 (Issue #5916)

- `compile::base_functions::type_expr_to_string` を削除し、parametric struct
  instantiate 名、constructor MethodError 表示、field type projection は
  `TypeExpr::Display` (`TypeExpr::to_string`) を直接使うようにした。
- `collection.rs` の `type_expr_to_type_name` 局所 helper を
  `TypeExpr::as_simple_type_name` へ移し、`Dict{K,V}` / `Set{T}` / `Union{...}`
  の simple type-name 解決が `TypeExpr` 側の単一 helper を参照するようにした。
  nested parameterized / runtime expression は従来どおり simple-name なしとして
  generic fallback を維持する。
- `TypeExpr` unit で concrete/typevar/simple-name、nested/runtime rejection、
  nested display を pin。既存 fixture `dict::` / `collections::` /
  `type_inference::` / `array::` も通過。
- compile pipeline の struct field table projection も
  `TypeExpr::to_julia_type_lossy` へ委譲し、`Union` だけを特別扱いする
  `TypeExpr::to_string` + `JuliaType::from_name_or_struct` の局所 arm を削除した。
- `TypeExpr` パラメータ列表示も `TypeExpr::{render_param_list, format_parameterized}` へ
  集約し、compile context / dynamic call / constructor MethodError /
  collection の局所 `iter().map(TypeExpr::to_string).join(", ")` 実装を削除した。

### AoT struct field の `TypeExpr → StaticType` projection 統合 (Issue #5916)

- AoT inference engine の private `type_expr_to_static` match を削除し、
  `StaticType::from_type_expr_lossy` へ移管。`TypeExpr::Display` で得た Julia type name を
  shared `CoreType` parser / `StaticType::from_julia_name_lossy` に通し、`Array{Int64,2}`、
  `Tuple{Int64,String}`、`Union{...}` などを AoT の backend 型へ直接 projection する。
- `Concrete(::Real)` のような abstract VM `JuliaType` は従来どおり `Any` に widen。
  `TypeVar` / runtime expression / projection 不能な user parameterized 型は
  `Struct { name }` fallback を維持し、既存の user-surface 表示を壊さない。
- `--features aot` の targeted unit で `StaticType::from_type_expr_lossy` と
  `TypeInferenceEngine::analyze_struct` の field projection を pin。

### call-site dispatch cache の IP 直参照 L1 化 (Issue #6345)

- VM runtime state に bytecode IP と同長の `call_site_caches` を追加し、`CallDynamic` /
  `CallDynamicOrBuiltin` / `IterateDynamic` / `CallDynamicBinary` の前段に
  monomorphic L1 cache を配置。L1 hit は exact scalar fingerprint の整数比較だけで
  `func_index` / negative sentinel を返し、従来の型名生成・`hash_type_name`・二段
  `HashMap` lookup を通らない
- 既存の `dispatch_cache: HashMap<usize, HashMap<u64, usize>>` は polymorphic /
  parametric site 用の L2 として維持し、L2 miss 後に structured runtime resolver
  (L3) が選んだ結果を L1/L2 の両方へ書き戻す。`Type{T}`、tuple、container、
  parametric struct、function singleton は L1 対象外にして、Julia dispatch identity を
  粗い `ValueType` tag で潰さない
- unit で L1 positive/negative sentinel と unsupported identity skip を pin。
  `hot_paths_benchmark` には `CallDynamic` を確実に出す `String` 単一メソッド +
  `Any` 引数 loop の VM-only benchmark を追加。`origin/main` + 同 benchmark
  34.17 ms に対して current 30.37 ms (criterion mean、約 11% 改善)

### lazy specialization を `FieldAssign` / n-ary 演算子呼び出しへ拡張 (Issue #6346)

- runtime lazy 特殊化エンジンが可変 struct のフィールド読み書き(`obj.field`,
  `obj.field = value`)を typed `GetField`/`SetField` で特殊化するようになった。
  `specialize_function` へ `&[StructDefInfo]` を渡してフィールド index/型を静的解決し、
  代入値はインタプリタの `compile_expr_as` と同一命令で型強制(parity 保証)。
- parser が n-ary `*(k, b.x, dt)` に展開する連鎖積など、演算子関数呼び出しを typed
  binary-op fold で特殊化。これで struct フィールド更新ホットループ全体が typed 命令化。
- VM-only ベンチ `vm_field_update` の A/B で約 22% 高速化(~923ms→~723ms)。
  `DestructuringAssign` は lowering desugar 済みで現状未生成のため別スコープ(#6561)。

### desugar 後の destructuring swap を型安定に特殊化 (Issue #6561)

- 自己参照型 destructuring swap `a, b = b, a % b` は lowering が
  `__tuple_tmp = (b, a % b); a = __tuple_tmp[1]; b = __tuple_tmp[2]` に desugar する。
  従来 `__tuple_tmp[k]` が `Any` を返し swap 先が `Any` へ widen していた。
- 特殊化エンジンに tuple リテラル一時変数の要素型追跡(`tuple_element_types`)を追加。
  `temp[k]` の定数 index を記録済み要素型に解決し、`I64`/`F64` のみ typed `Store*` を
  発行。記録型は specializer 自身が emit した型で `IndexLoad` の `Value` タグと厳密一致
  するため防御的 coercion は不要(入れると A/B で ~3% 遅くなる)。非数値・非定数 index・
  非追跡 tuple・別値再代入は generic `Any` 経路維持。
- 真価は swap 先を downstream 利用するパターン(swap 後の `s += a` 等)。従来 `a` が
  `Any` で `s += a` が `DynamicAdd` に落ち `s` も poison されたが、型追跡で `AddI64` のまま
  typed 化し return も `ReturnI64`。VM-only ベンチ `vm_swap_accumulate` の A/B で
  ~123.5ms→~121.5ms(約 1.6%, CI 非重複)。出力 upstream 一致(`149905498950`)。
  プリミティブ unboxed の本 VM では純粋 swap(gcd 等)は型安定でも実測 neutral。
  fixture `tuple/destructuring_swap_specialization_6561.jl` + specializer unit 4 +
  integration 7 で pin。#6346 の follow-up。

### CoreType array abstract / runtime ReshapedArray dispatch 補強 (Issue #5915 / #6502)

- `CoreType` の `Struct/Named(Vector|Matrix|Array) <: AbstractUser(AbstractVector/
  AbstractMatrix/AbstractArray)` を builtin abstract へ正規化して判定するようにし、
  `AbstractVector` が `AbstractUser` として届く method signature でも
  `Tuple{Vector{T}} where T <: Tuple{AbstractVector}` が core 側で成立するようにした。
- `ReshapedArray{T,1,P,MI}` の runtime `collect` / `map` 経路を補強。
  `collect` の runtime candidate filter に `ReshapedArray` を追加し、runtime
  method 候補でも compile-time と同じ signature-wide strict subtype dominance
  precheck を使うようにした。
- runtime candidate の `where` 埋め込みで、複数文字 type parameter (`MI`) が
  `Named("MI")` のまま残るケースを `TypeVar(MI)` へ昇格。`map(f,
  ::ReshapedArray{T,1,P,MI}) where {T,P,MI}` が generic `map(f, itr)` より前に選ばれる。

### `JuliaType::is_subtype_of` の built-in family arm 削減 (Issue #5915)

- `JuliaType::is_subtype_of` に残っていた `AbstractString` / `AbstractChar` /
  `IO` / `Function` / `Type` の局所 family 判定を削除し、既に先行している
  `CoreSubtypeEngine` 判定を単一の答えにした
- `String <: AbstractString`、`Char <: AbstractChar`、`IOBuffer <: IO`、
  `Type{T} <: Type`、`DataType <: Type` などは core 側の既存 lattice/TypeOf
  arm で維持。subtype/type-object unit と dispatch fixture で確認済み

### engine の `LatticeType → JuliaType` 変換を bridge 委譲 (Issue #5916)

- `compile::abstract_interp::engine` に残っていた local `LatticeType` /
  `ConcreteType` → `JuliaType` 変換コピーを削除し、正準実装
  `compile::bridge::lattice_to_julia_type` へ委譲。戻り型 cache invalidation
  など engine 内の JuliaType 投影が同じ bridge を通るようになった
- `Pair{K,V}` literal 用の型名 helper は残しつつ、型パラメータ名の生成も
  bridge 経由に寄せた。Dict/Pair/return-cache まわりの既存挙動は targeted
  nextest で確認済み

### `TypeExpr → JuliaType` projection helper 化 (Issue #5916)

- `TypeExpr` に `to_julia_type_lossy` と `substitute_to_julia_type_lossy` を追加し、
  compile context / runtime type-object reflection に分散していた `TypeExpr` →
  `JuliaType` のレンダー経由 projection を共有 helper へ移管
- 未束縛 typevar / runtime expression を `Any` へ広げる既存 reflection 方針は維持。
  parametric struct constructor unit と `struct_tests::chunk_000` で既存挙動を確認

### import list の演算子名パース修正 (Issue #6544)

- `import Base: *, ==, +` のように selective import list に複数の演算子名が並ぶ場合、比較演算子 `==` の直後のカンマで parser が停止していた問題を修正。import/export list の name 開始判定を識別子・macro 名だけでなく operator / operator keyword に拡張し、演算子 token を `Identifier` leaf として扱うようにした
- fixture `parse/operator_import_list_6544.jl` と parser corpus `test_import_specific` で `*` / `==` / `+` の同一行・複数行 import を pin

### inner constructor の `where` 上限境界 enforcement (Issue #6548)

- 明示型引数つき parametric constructor (`Pos{String}("x")`) が inner constructor の `where T<:Real` 境界を無視して構築できていた問題を修正。inner constructor 候補の arity が合う場合、明示型引数を `type_params` の upper bound に照合し、全候補が境界不一致なら catch 可能な `MethodError` を投げる
- fixture `struct/inner_constructor_where_bound_6548.jl` で `Pos{Int}` は構築可能、`Pos{String}` は upstream Julia と同様に拒否されることを pin

### Base numeric wrapper 推論 snapshot の過広化を補正 (Issue #6547)

- cached/fresh 共通で `clamp` / `binomial` / `ndigits` / `widen` / `copysign` の wrapper `Base.infer_return_type` が upstream より広い `Any` へ落ちる問題を、既知 Base numeric helper の conservative tfunc で補正。literal `Const` 引数も concrete type に正規化して、`clamp(x, 0.0, 1.0)` のような wrapper を精密化した
- fixture `type_inference/base_numeric_method_snapshot_precision_6547.jl` と tfunc unit `numeric_snapshot_precision_helpers_issue_6547` で Float64/Int64/Int32 の代表ケースを pin

### `map(abs, ::Vector{Any})` の runtime callable dispatch 修正 (Issue #6550)

- runtime `Generator` 作成時に `Vector{Any}` の element type `Any` だけを見て `abs(::Any)` fallback を先に固定し、実値 `Holder6550` へのユーザー `abs` 拡張を bypass して binary `operator(Holder, Int64)` 経路へ落ちていた問題を修正。iterator element type が `Any` の場合は generator HOF specialization を行わず、各要素の runtime 値で通常の callable dispatch を行う
- fixture `hof/map_any_unary_base_extension_6550.jl` で `map(abs, Any[Holder6550(...)])` が unary user extension を呼ぶことを pin

### legacy ディスパッチマッチャの CoreType ネイティブ移植 — stages 1–7c-ii-b 完了 (Issue #6495)

- #6336 構造化シグネチャ移行の最終盤を完了: compile-time ディスパッチパイプライン(arity 展開 → マッチ → スコア → dominance 事前チェック → tie-breaker → 候補ヒューリスティクス)を `core_signature` / `CoreType` ネイティブに段階移植。新サブモジュール `inference_core/dispatch_resolver/core_match.rs` が `julia_signature_match_with_bindings` をアーム単位で `CoreType` 上に再実装し(#4857/#5383/#5314/#5051/#5050 の各回帰アームを保存)、本番マッチ/スコアは `expanded_core_param_types_for_arity` + `core_signature_type_vars` 経由で流れる
- `MethodSig` は `param_names` + `core_signature` のみを保持し、`params: Vec<(String, JuliaType)>` と `type_params` を削除。構築は `MethodSig::from_julia_projections` で eager に `core_signature` を導出して JuliaType 入力を破棄し、Deserialize は wire/CACHE_VERSION を変えず投影を再構成しない
- structured-unavailable な legacy fallback chain、`MethodSig` accessor の legacy アーム、`legacy_pred` 呼び出し、`sig_param_types` / `*_legacy` テストオラクルを撤去。struct-parents declared-ancestry fallback は canonical inverse (`expanded_projected_param_julia_types_for_arity` + `projected_type_params`)を signature source にして親リンク walk だけを維持
- 各ステージは Base 全コーパスのパリティ/恒久ゲートで pin。最終ゲートは stored-projection parity ではなく accessor-vs-canonical (`base_method_signature_accessors_are_canonical_issue_6495`、serde canonical roundtrip、runtime signature canonical derivation)へ更新
- persistent Base cache は wire/CACHE_VERSION 据え置きのままファイル namespace を `sjulia_base_cache_v2_<prelude-hash>.bin` へ分離し、同 version の古い serialized cache に残る inference snapshot はロード時に破棄。手元の stale cache が pre-7c-ii-b の Base bytecode/推論状態を再利用して cached-Base parity を壊す経路を遮断
- criterion ベンチ(origin/main `3be82144b` baseline): `vm_benchmark` / `hot_paths_benchmark` 全項目で >5% 退行なし。最大は再測定した `closure_capture_affine_map_1000` の +4.4% で、`fib_20` −3.4%、`recursive_calls_depth10` −3.7%、`hof_apply_twice_lambda` −3.4% などは改善

### callable-value チャネルの `where` 境界 enforcement + `@test` インライン式の等値ミスフォールド (Issue #6539)

- Issue #6539 の repro を 3 つの真因に分解して全て修正(upstream julia 1.12 parity 検証済み):
  1. callable-value チャネル(`resolve_callable_value_candidates`)が `where` 境界を enforcement せず `f = abs; f(Holder("s"))` が `T<:Real` メソッドを選択 → 明示境界つき候補に #6543 と同じ `core_signature` subtype ゲート(`Tuple{actuals} <: signature`)を追加。無境界 `where T` はゲート対象外(対角規則 #5050 が既存どおり担当)
  2. issue が「メソッド選択ミス」と推測していた `@test` インライン形の真因は **コンパイル時定数フォールド**: 実行トレースで両呼び出しサイトとも正しいメソッド (holder-any) を選択しており、`abs(::Any/::Struct)::Float64` 推論 + String-vs-非String 等値ショートカットが式全体を `PushBool(false)` に畳んでいた → `abs`/`abs2`/`sign` の ValueType フォールバックを Struct/Any/Union で defer(JuliaType チャネルは既に defer 済み、Complex の Float64 はレジストリ tfunc が維持)
  3. ネスト比較サブバグ(`@test (a == b) == c`)は `==` 結果推論の無条件 `Bool` が同じフォールドを誘発(test-macro 特別扱いではない)→ ユーザー定義の非 Bool 戻り 2 引数等値メソッド存在時(`function_ir_by_global_index` で Base/stdlib 起源を除外)に Any オペランドの `==`/`!=` 推論を `Any` へ拡張
- fixture: `dispatch/callable_value_where_bound_test_inline_6539.jl`(parity 8/8)。調査副産物: `map(abs, Any[...])` がユーザー struct で binary `operator` 経路へ誤投入される別件を Issue #6550 起票
### 代入形演算子メソッドの braced `where` 境界脱落を修正 (Issue #6537)

- `*(a::Wrap{T}, b::Wrap{T}) where {T<:Real} = ...` が `where {T}` 相当に lowering され、境界が `type_params` に届かず #6536 の runtime enforcement が効かなかった問題を修正。根本原因は `lower_operator_method`(`lowering/function/short_form.rs`)の手書き where ループが、pure parser の braced 境界(`TypeParameters` 内の `BinaryExpression [T, <:, Real]` / `SubtypeConstraint`)を認識せず黙って捨てていたこと(長形式 `full_form.rs` には正しい near-copy が存在し、経路が乖離していた)
- 長形式の WhereClause 処理を共有ヘルパ `parse_where_clause_type_params`(`where_clause.rs`)に抽出し、長形式・代入形演算子の両経路を一本化。コンストラクタ経由なので live な `bound` フィールドも `upper_bound` と同期(#6518)。`lower_operator_method` は WhereClause を固定 index でなく node kind で探索し、param 注釈の typevar 化(`convert_params_with_type_vars`)も非演算子経路と同様に適用
- **unbraced 形も修正(stretch)**: `*(a,b) where T<:Real = ...` は `parse_where_clause` が一般式パーサで制約を読み `= body` を Assignment として飲み込むため `expected Eq` で parse 失敗していた。`parse_type_constraint`(値位置 where と同じ、`=` の前で停止し `>:`/二重境界も保持)に切替え、連鎖 `where T where S` も単一 WhereClause に折り畳み対応(単純 lookahead のみ、バックトラック不要)
- inner constructor の WhereClause 処理(`lowering/struct_.rs` の第 3 の手書きコピー)も共有ヘルパへ一本化: 旧コードは braced 境界を `BinaryExpression` の `children[1]`(= 裸の `<:` 演算子テキスト、#5374 と同型のバグ)として記録しており、parser の SubtypeConstraint 化で unbraced 境界が脱落するところだった。記録された境界は unit test で pin(braced/unbraced)。inner ctor 境界の runtime enforcement 自体は未実装(julia は `Pos{String}` を MethodError で拒否、sjulia は構築してしまう)— Issue #6548 起票
- テスト: lowering unit 9 本(braced/multi/unbounded/unbraced/連鎖/inner-ctor の `upper_bound`+`bound` を pin)+ fixture `dispatch/assignform_operator_where_bounds_6537.jl`(`*`/`==`/`+`、runtime `Any[]` + compile-time、関数形コントロール、julia parity 15/15)
- 調査中に発見した別件: `import Base: *, ==, +` が比較演算子の直後のカンマで parse 失敗(Issue #6544 起票、fixture は import 行分割で回避)

### runtime 候補解決を `core_signature` ベースの構造化照合へ移行 (slice 2) (Issue #6502)

- `resolve_runtime_type_pattern_candidates` を使っていた 4 経路(`CallDynamicBinaryBoth` / `CallDynamicBinaryNoFallback` / `CallDynamicBinary` / `CallDynamicOrBuiltin`)を、新しい構造化リゾルバ `dispatch_resolver::resolve_runtime_core_signature_candidates` へ移行。候補は `FunctionInfo`(= `MethodSig::core_signature` の runtime 投影)から導出した per-slot `CoreType` + `where` パラメータ込みの `core_signature` ゲートとして照合され、毎 dispatch の文字列再パースが消滅(actual 側は dispatch ごとに 1 回だけパース、候補側は memoize)
- **parity-gated 挙動修正 (Issue #6536)**: (1) パラメトリック struct param 上の `where` 上限境界を enforcement(`Wrap{T} where T<:Real` が `Wrap{String}` を拒否)— lowering は境界を `type_params` にしか持たないため `embed_type_param_bounds` で typevar に再付与; (2) `dispatch_pattern_score_in`(hierarchy-aware tier 採点)でユーザー抽象境界(`Box{T} where T<:Animal`)が構造 tier 3 を維持し bare `Box`(tier 2)に勝つ; (3) `core_signature` ゲート(`Tuple{actuals} <: signature` を共有エンジンで判定)で slot 間 typevar 束縛一貫性を enforcement(`(Holder{T}, Holder{T}) where T` が混合要素型を拒否)。全て upstream julia 1.12 と突き合わせて fixture `dispatch/runtime_where_bound_enforcement_6536.jl` で pin
- 構造化ソースの残余だった `CoreType::from(&JuliaType)` と歴史的レンダー名パースの divergence(`AbstractUser` / `Module`)は、下記 slice で legacy parse fallback を削除済み。callable-value チャネルは残余(境界 enforcement 欠落は Issue #6539)
- 調査中に発見した別件: 代入形演算子メソッドの braced `where` 境界が lowering で脱落(Issue #6537)、`@test` インライン式の callable チャネル評価(Issue #6539)

### `CallDynamic` family fallback の構造化照合移行 (Issue #6502)

- `Instr::CallDynamic` の fallback tier と `IterateDynamic` の fallback tier から、残っていた `resolve_runtime_type_pattern_candidates_with_family_fallback` production 呼び出しを削除。`RuntimeCoreSliceCandidate` + `resolve_runtime_core_signature_slice_candidates_with_family_fallback` を追加し、可変 arity の runtime 候補も per-slot `CoreType` と `core_signature` gate で採点するようにした
- VM-native iterator sentinel(`DynamicCallCandidate::NativeIterator`、旧 `usize::MAX` + 型名文字列)は、候補 idx `usize::MAX` を維持したまま legacy family name を `CoreType` 候補へ投影。same-family tier は bare `Struct` / `Named` 候補のみに限定し、parametric expected を誤って family fallback で通さない unit を追加
- `runtime_candidate_core_type` の `AbstractUser` / `Module` divergence は legacy parse fallback を削除し、`CoreType::from(&JuliaType)` を単一 projection にした。ユーザー抽象注釈と `Module` の exact-name tier は `CoreType` の nominal bridge(`AbstractUser`/`Module` ↔ rendered `Named`)で維持し、子 user struct の `core_signature` gate は `StructHierarchy` 経由の subtype で通す

### `CallTypedDispatch` 候補キャッシュの `core_signature` 化 (Issue #6502)

- `CallTypedDispatch[OrBuiltin*]` の per-arity candidate cache を、rendered type-name `Vec<String>` から `RuntimeCandidateCoreSignature`(rendered + per-slot `CoreType` + optional `core_signature` gate)へ拡張。typed dispatch family は call-site dispatch cache を持たないため、候補側の再レンダーだけでなく `CoreType` 投影も候補・arity ごとに memoize される
- 既存 selection は互換性維持のため `signature.rendered` を通して旧 `resolve_type_name_candidates_with_subtype_fallback` を使う。次 slice はこの structured cache を入力にして、`call_dynamic_typed.rs` の production string resolver 呼び出しを `CoreType` resolver へ置換する

### `CallTypedDispatch` production resolver の構造化照合移行 (Issue #6502)

- `call_dynamic_typed.rs` の production `resolve_type_name_candidates_with_subtype_fallback` 呼び出しを、`RuntimeTypedCoreCandidate` + `resolve_typed_runtime_core_candidates_with_subtype_fallback` へ置換。候補 matching は cached per-slot `CoreType` と optional `core_signature` gate で行う。初期 slice では互換 ordering のため specificity tie-break のみ rendered 名を参照していたが、これは後続の構造化 tie-break slice で解消済み
- `CallTypedDispatch` / `CallTypedDispatchOrBuiltin*` / runtime name-search fallback の全てが structured resolver を通るようになり、candidate 側の毎回 string reparse を production 経路から削除。erased `JuliaType::Array` declaration でも rendered `Vector{T}` / `Vector{<:Real}` が残る候補は structured slot へ復元し、Issue #6229 の repeated `Vector{T}` diagonal と covariant-bound sibling の順位を維持
- pin: unit `typed_core_resolver_matches_legacy_string_order_issue_6502` / `typed_core_resolver_keeps_rendered_array_diagonal_issue_6502`、fixture `dispatch::chunk_002` を含む dispatch/hof/iterators チャンクで確認

### `CallTypedDispatch` specificity tie-break の構造化 (Issue #6502)

- `resolve_typed_runtime_core_candidates_with_subtype_fallback` の final tie-break を
  `type_name_pattern_specificity(candidate.rendered)` から
  `core_type_pattern_specificity(candidate.slots)` へ移行。候補 selection の最後の
  specificity 比較も cached `CoreType` slots から計算するため、production typed
  resolver は rendered type-name specificity helper に依存しなくなった
- 新 helper は既存 policy(構造 specificity + parametric surface bonus + repeated
  typevar bonus)を `CoreType` 形状から復元する。unit
  `typed_core_specificity_matches_rendered_policy_issue_6502` で `Type{T}` /
  `Type{<:Number}` / repeated `Vector{T}` / `Tuple{}` / `Union{}` / `where`
  surface の legacy score parity を pin

### `CallTypedDispatch` covariant-bound bridge の CoreType 化 (Issue #6502)

- `typed_core_candidate_matches_with_subtype_fallback` から
  `typed_core_covariant_rendered_match` と JuliaType/rendered 名への一時変換を削除。
  fallback loop は cached `CoreType` slots だけを `core_pattern_matches` /
  `subtype_matches` に渡すようになり、production typed resolver の matching 判定は
  rendered type-name bridge を通らない
- `CoreType::TypeVar("_")` は上限/下限境界だけ enforcement し、名前付き binding へ
  登録しないようにした。これで `Vector{<:Real}` のような匿名 covariant slot は
  複数スロット間で同一型を要求せず、Issue #6229 の repeated `Vector{T}` diagonal と
  `Vector{<:Real}` sibling の挙動を CoreType matcher だけで表現できる
- string-only helper(`inferred_type_params_from_expected_names` /
  `covariant_bound_matches`)は `#[cfg(test)]` parity oracle へ降格。unit
  `typed_core_resolver_uses_covariant_slots_without_rendered_bridge_issue_6502` で
  rendered 名を意図的にずらしても structured slot が selection authority になることを pin

### `CallTypedDispatch` tier split の構造化 (Issue #6502)

- `resolve_typed_runtime_core_candidates_with_subtype_fallback` の primary/fallback
  tier split が、rendered type-name の `"<:"` scan ではなく cached `CoreType`
  slots の explicit bound 有無を見るようになった。`Type{<:T}` /
  `Vector{<:Real}` / `where T<:...` などの bounded shape は
  `core_type_pattern_has_explicit_bound` で判定する
- unit `typed_core_resolver_tier_split_uses_bounded_slots_issue_6502` で、rendered 名に
  `<:` marker が無い候補でも bounded `CoreType` slot なら fallback tier に残ることを pin。
  これで production typed resolver の matching / specificity / tier split は
  rendered type-name ではなく structured slots を authority とする

### `CallTypedDispatch` 選択 flow の selection helper 化 (Issue #6502)

- `CallTypedDispatch` の最終 winner 選択順序(非 broad name-channel repair → `metadata_best` value-channel → positive compiled name-channel → runtime name-search → fallback index)を `inference_core::selection::select_typed_dispatch_candidate` へ移管。VM handler は候補列挙・signature matching・runtime search closure を供給する薄い adapter になった
- runtime name-search は helper の lazy closure に閉じ込め、compiled/metadata 経路が勝つ hot path では function-name index scan を行わない。従来どおり、runtime search は compiled match がない場合か、compiled specificity を strictly 上回る場合だけ採用される
- unit `typed_dispatch_selection_*` 6 本で順序・fallback・lazy 実行を pin

### legacy string resolver API の test-only 化 (Issue #6502)

- production 参照が消えた旧 string resolver 群(`resolve_type_name_candidates*` / `resolve_runtime_type_pattern_candidates*` / `runtime_type_pattern_score*`)を `#[cfg(test)]` に移し、production API surface から退役。旧 resolver は structured resolver の parity oracle と regression unit 用にのみ残す
- `RuntimeTypedCoreCandidate` の specificity tie-break も structured slots へ移行済み。
  covariant-bound bridge も CoreType 化済み。production で残る rendered 参照は
  `<:` marker による primary/fallback tier split と表示/互換 metadata に限定し、
  旧 production string resolver call site は復活させない

### runtime `where` 境界チェックを `StructHierarchy` aware に統一 (Issue #6502)

- `runtime_value_type_matches_param_with_bindings` が `where T<:UserAbstract` の上限境界を
  `JuliaType::from_name` で解決できる built-in 型に限って enforcement していた非対称を解消。
  VM adapter から共有 `StructHierarchy` を渡し、runtime binding の境界判定を
  `CoreSubtypeEngine::with_hierarchy` へ統一した
- `Dog <: Animal` のようなユーザー定義階層でも value-channel dispatch の
  `where T<:Animal` が compile/core-signature 経路と同じ subtype authority を使う。
  unit `runtime_where_bound_uses_struct_hierarchy_issue_6502` で Dog は受理、Int64 は拒否することを pin

### 常に throw する関数の戻り型を `Union{}` (Bottom) と推論 (Issue #6532)

- `throw(x)` / `rethrow` / `error(...)` を tfunc レジストリで `LatticeType::Bottom` 返しとして登録(upstream `julia/Compiler/src/tfuncs.jl` の `add_tfunc(throw, 1, 1, ->Bottom, 0)` を踏襲)。`tfunc_throw`(`compile/tfuncs/intrinsics.rs`)+ `register_intrinsics` への 3 登録のみで、engine 側の join(`Bottom` は join の単位元)・snapshot 経路(`lattice_to_parametric_julia_type` の `Bottom → JuliaType::Bottom` 保持、#6523 の正準コンバータ)は既存のまま機能
- 観測可能な改善: `f() = error("boom")` の `Base.infer_return_type` が `Any` → `Union{}`(upstream julia 1.12 一致)。throw する枝は join に寄与しない(`x > 0 ? throws() : 1.5` は `Float64`、ループ内 throw は非 throw 戻りの `String` を維持)。常に throw する callee を呼ぶ caller へも `Union{}` snapshot が伝播
- `error` の登録が必要な理由: fresh full compile では `error` の pure Julia body(`throw(ErrorException(s))`)経由で `Union{}` が推移的に出るが、cached-Base 経路では multi-method Base callee が engine の method table(cached Base sig 未登録)にも function table(複数シグネチャ → ambiguous 除外)にも現れず tfunc レジストリへ落ちる。この cached 経路の構造的ギャップ(`error` 以外の全 multi-method Base 関数に影響)は Issue #6538 として起票
- 既知の残余: 素の `rethrow()` だけを呼ぶ関数の reflection は、非 cached 経路では `base/error.jl` のドキュメント用空 stub(`function rethrow(e) end`)の `Nothing` snapshot が method-table 経路で先勝ちするため upstream の `Union{}` と不一致(変更前から同じ挙動; tfunc は table 不在時のみ有効で、cached 経路では `Union{}` になる)
- fixture: `type_inference/bottom_throw_return_6532.jl`(常時 throw 3 形 + join 4 形 + 実行時挙動 8 アサーション、`fixture_julia_parity.sh` で upstream 一致確認)
### legacy pre-scan の縮小 wave 6: 死んだ非wideningモード削除 + capture 解析の型計算退役 (Issue #5922)

- 文単位 pre-scan(`collect_local_types_with_mixed_tracking`)の `use_widening` フラグを削除。
  唯一の公開エントリポイントが `true` を固定で渡しており、`false` 用の「main/REPL exact types」
  分岐(Assign の非widening insert と If の逐次トラバース)は到達不能の死コードだった。
  widening は無条件化し、ラッパー関数も再帰本体に統合。
- モジュールレベル lambda capture 解析(`analyze_module_lambda_captures`)は pre-scan の
  結果から **束縛名集合しか消費していなかった**(`main_locals.keys()` のみ参照、型と
  mixed-type 追跡は全て破棄)。型を一切計算しない名前専用ウォーカー
  `collect_local_binding_names_for_capture` に置換し、typed pre-scan の消費者を 5→4 箇所に削減。
  スコープ規則(testset 本体は escape しない / 非testset LetBlock 本体は escape する)は
  typed pre-scan と完全に同型で、等価性は unit test
  `capture_binding_names_match_typed_prescan_keys_issue_5922` で pin。
- 式位置の LetBlock 探索を単一の `visit_outermost_letblocks` ビジターに共通化し、
  typed 側(`collect_expr_locals`)と名前専用側が構造的に乖離できないようにした。
- 残余 (b)(撤去不可と分類): 関数本体 / inner ctor / main の pre-scan は最初の store の
  コンパイル前に全文 widening 済みスロット型(前方参照 `s = 0; s = s + 1.5` など)と
  `mixed_type_vars` を必要とするため存置。For/ForEach のループ変数要素型もループ内
  代入を同一スキャン内で型付けするために必要。
### cached-Base 経路: multi-method Base callee を推論エンジンから可視化 (Issue #6538)

- cached-Base コンパイル経路(`compile_with_cache`、`sjulia file.jl` の既定)では、`build_method_tables` が cached Base 関数を `is_cached_base_function` で short-circuit して `add_initial_method` を呼ばず、`InferenceEngine::add_function` も複数シグネチャ名を ambiguous として function table から除外するため、multi-method Base callee の呼び出しが tfunc レジストリへ素通り(`Any` 推論)していた。fresh full compile(`SUBSET_JULIA_VM_DISABLE_CACHE=1`)は method-table snapshot 経路で精密に推論しており、cache 有無で推論パリティが割れていた
- 修正: `InferenceEngine::seed_initial_method_tables` を新設し、`build_inference_engine` 直後に cached Base `MethodTable` を丸ごと engine の inference 専用 method table へ移植(gate は `is_cached_base_function` と同一)。`MethodTable::methods` は `Arc<Vec<MethodSig>>` なので O(#tables) のポインタクローンのみ — interleave 実測で warm 起動 (median ~48ms) に回帰なし。serialized shape 変更なし(CACHE_VERSION 据え置き)、fresh-cache での serde roundtrip green を確認
- パリティバッテリー(cached vs uncached vs upstream julia 1.12): `mod1` / `factorial` / `flipsign` が cached 経路で `Any` → `Int64` に改善し、全行で cached == uncached に。残余の両経路共通の不精密(`clamp` / `binomial` / `ndigits` / `widen` / `copysign` が upstream の精密型に対し `Any`)は #4337 系 snapshot widening 由来の既存ギャップとして Issue #6547 に起票
- #6532 の `error` tfunc は本修正で構造的に冗長化(method-table snapshot 経由でも `Union{}` が出る)が、exact かつ無害な fast path として存置
- テスト: `tests/cached_base_inference_parity_6538_tests.rs`(cached vs uncached 出力一致 + upstream 検証済み精密型の pin)、engine unit 2 本(seed 経由の caller 推論解決 / 既存 table 非クロバー)

## 最新対応 (2026-06-12)

### `julia_type_to_lattice` の `JuliaType::Bottom` 落とし穴を修正 (Issue #6523)

- 正準 `compile/bridge.rs::julia_type_to_lattice_with_struct_resolver` で、`Union{}` の正準綴りである `JuliaType::Bottom` が `_ => Top` arm に落ちて最広型へ反転していたのを `LatticeType::Bottom` へ修正(非正準の `Union(vec![])` 綴りとは既に一致していた)。`LatticeType::Bottom → ValueType::Any` のキャリア広化(§3.5)は不変
- 観測可能な改善: multi-method callee の `MethodSig.return_julia_type` snapshot が `Union{}` のとき(`method_return_type_to_lattice` 経由 — single-method 呼び出しは lattice 空間で直接推論され該当しない)、呼び出し側の join が `Any` でなく正しく枝の型になる。upstream julia 1.12 と一致を確認し fixture `type_inference/bottom_return_snapshot_join_6523.jl` + unit `test_julia_type_to_lattice_bottom_variant_is_bottom_issue_6523` で pin
- 調査中に発見した別件: 常に throw する関数(`f() = error("x")`)の `Base.infer_return_type` が sjulia では `Any`(upstream は `Union{}`)— 別 Issue として起票

### `JuliaType::is_subtype_of` の Union/`Type{}` arm を CoreSubtypeEngine へ委譲 (Issue #5915)

- compile-time `types/julia_type/comparison.rs::is_subtype_of` に残っていた local の Union 分解 early-return(∀-members / ∃-member)と `Type{}` invariance arm を削除し、既存の `CoreSubtypeEngine` 呼び出し(`CoreType` solver の Union / `(TypeOf, TypeOf)` arm)に一本化
- 唯一の local 残余は `Type{<:Bound}` の bound 名が `JuliaType::from_name` で解決できない場合の permissive fallback(この enum レベルには struct hierarchy が無いため従来挙動を維持; `Pairs{K,V,I,A}` のような method type-param 込み bound もここに該当)
- 副次修正: legacy invariance 再帰の reverse-parametric quirk(`Vector <: Vector{Int64}` 扱い)により `Type{Vector} <: Type{Vector{Int64}}` が誤って true だったのが upstream 通り false に。upstream 検証済み unit matrix `engine_delegated_union_typeof_arms_issue_5915` を追加
### tfuncs 移行 wave 5: パラメトリック struct ctor 解決 + 変換コピー委譲 (Issue #5922)

- `infer_expr_type` の `&mut SharedCompileContext` 依存コンストラクタ解決 4 系統
  (パラメトリック struct ctor / Dict 非builtin-pattern fallback / Rational ctor /
  `{`-instantiated ctor 名)を `expr_tfuncs` アダプタの `StructInstantiation` trait
  (`SharedCtxInstantiation`)経由へ移行。call site は thin dispatch のみ残し、解決
  順序(exact concrete entry → on-demand instantiation → any instantiation → Any、
  推論失敗時は base-name id)を `StubInstantiation` unit test で pin。「any
  instantiation」fallback は HashMap 順依存だった legacy `.find` から決定的な最小名
  選択(レジストリと同一の `instantiation_of`)に揃えた。
- `expr_tfuncs.rs` の手書き `JuliaType → LatticeType` コピーを削除し正準
  `bridge::julia_type_to_lattice` へ委譲(Issue #5916 cross-credit)。dispatch-deferral
  edge(Struct/Signed/Unsigned/Bottom → Top)と type-object edge(TypeOf →
  `DataType{name}`、typemin/typemax/zeros/ones が依存)、legacy pin
  (AbstractString/AbstractChar/AbstractArray/TupleOf{}/NamedTuple{}/Range{Any}/
  Module/Function/IO/metaprogramming/Generator/Enum)は明示 arm として保持し
  テストで pin。`Bottom → Top` は正準コンバータの Bottom edge(Issue #6523)と
  独立に保証。Union は正準採用(旧コピーは全 union を Top に潰していた —
  TYPE_REPRESENTATIONS.md §3.6 の union-loss 修正)。
- HOF call-site 推論 arm(map/filter/reduce/broadcast/mapreduce/foldl/foldr)は
  lambda 本体の式解析を要するため tfunc(引数 lattice 型のみ可視)では表現不可
  — `infer/mod.rs` にコメントで明文化し移行対象外と確定。
- `infer/mod.rs:130` の `julia_type_to_value_type_resolved` は既に
  `type_helpers::julia_type_to_value_type_with_table` への thin wrapper であることを
  確認(§3.6 の deferred 記載は stale、解消済みに更新)。
- fixture: `type_inference/parametric_ctor_resolution_5922.jl`(julia parity 11/11
  一致)。Dict 非builtin-pattern ctor の compile path gap は Issue #6531 で
  end-to-end fixture 化して解消。

### lazy specialization の `IndexAssign` typed fast path を追加 (Issue #6346)

- runtime specializer が 1D `Vector{Int64}` / `Vector{Float64}` の `a[i] = x`
  を受理し、`LoadArray` + `IndexStoreTyped(1)` + `StoreArray` を発行するようにした。
  index は `Int64`、value は配列要素型と一致する場合のみ fast path 化し、型不一致・多次元
  index は従来通り generic bytecode へ fallback する。
- `ExecutableProgram` の typed loop predecode/execution に `LoadSlotArray` /
  `StoreSlotArray` / `IndexStoreTyped(1)` を追加。ループ開始時に 1D Int64/Float64
  配列を guard し、hot loop 内では array handle を保持して直接 `ArrayValue::set`
  するため、配列書き込みを含む数値ループも `ExecutableBlock::TypedLoop` に乗る。
- VM-only Criterion current:
  `timeout 1800 cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark -- --warm-up-time 1 --measurement-time 1 --sample-size 10`
  で `vm_mandelbrot/run_only` median `35.637 ms`,
  `vm_mandelbrot/clone_new_program_run` median `49.616 ms`。これは precomputed bytecode
  の `Vm::run()` 測定で、cold CLI timing ではない。
- 回帰: specializer 単体で `IndexStoreTyped` 発行と型不一致 fallback を pin。
  executable 単体で runtime-specialized `a[i] = i * 3` が typed loop block を追加することを確認。
  fixture `arrays_index_assign_specialized_6346` / `arrays_index_assign_multidim_fallback_6346` を追加。

### 型表現変換の正準化 wave 4: sibling 所有 `JuliaType → LatticeType` コピーの委譲統合 (Issue #5916)

- wave 3 で確立した正準 `compile/bridge.rs::julia_type_to_lattice` への委譲を §3.6 の deferred call site に適用。`type_stability/analyzer.rs`・`abstract_interp/engine/mod.rs`・`vm/builtins_reflection/mod.rs` の手書き変換本体(+ 各 `…_to_concrete(_or_any)` 射影、計 6 実装)を削除し thin wrapper 化
- 正準側は `julia_type_to_lattice_with_struct_resolver(ty, Option<&dyn Fn(&str) -> Option<usize>>)` に一般化(engine の `StructTypeInfo` テーブルと compiler の `context::StructInfo` テーブルの双方から struct id 解決を注入可能)。要素位置(配列要素・タプル要素・union メンバ)も resolver 対応射影 `julia_type_to_concrete_or_any_with_struct_resolver` で再帰し、`Vector{MyStruct}` の `type_id` 解決と `Vector{Union{...}}` の要素 union 保持を維持。resolver 供給時の未解決名は engine 従来通り `Top` に広げる(`AbstractDict` 等の抽象族が `Struct(name)` 綴りで来るため — `dict_mergewith` fixture が回帰検出)
- 委譲中に正準側の潜在バグを修正: bare `JuliaType::Array` が lossy の `_` fallback で `Concrete(Any)` に潰れ全 sibling コピー(`Array{Any}`)と乖離していたため、lossy に bare-`Array` arm を追加
- 残る手書きコピーは `compile/expr/infer/expr_tfuncs.rs`(並行 infer リファクタの所有領域のため skip)。docs/vm/TYPE_REPRESENTATIONS.md §3.6 を resolved/deferred に更新

### 残存 ad-hoc 戻り値型ゲートの tfuncs レジストリ移行 — LinearAlgebra / Dict / collect / rand 系 (Issue #5922)

- `compile/expr/infer/mod.rs` の ModuleCall 内ネスト LinearAlgebra 結果型 match(13行)を廃止。
  結果形状ルール(det/cond→Float64, rank→Int64, svd/qr/eigen/cholesky→NamedTuple, lu→Tuple,
  inv/eigvals/transpose→Array)を新設 `tfuncs/linear_algebra_ops.rs` に移し、
  **`LinearAlgebra.` 修飾キー**で登録(裸名 `det` 等の builtin/メソッドディスパッチ経路には不干渉。
  fixture `linalg/det_lu_module_dispatch_first_4020.jl` がユーザー多重定義との共存を担保)。
  legacy が返していた無パラメータ `ValueType::Array`(`ArrayOf(Any)` ではない)はアダプタ側で pin。
- `compile/expr/infer/julia_type.rs` の ~46行 `match function.as_str()` ブロック + インライン
  Dict builtin-pattern ゲートを削除。Dict パターンゲートは `infer_julia_dict_builtin_call` で
  ValueType 経路と単一共有化(二重ゲート解消)。裸 `collect` はレジストリの `collect` tfunc 経由
  (UnitRange/StepRange→`Vector{Int64}` pin、`VectorOf` 要素保存、その他 Array。裸名限定で
  `Base.collect` はメソッドディスパッチ経路のまま)。`rand`/`randn` は新設 `tfunc_rand`
  (0引数→Float64、引数あり→Top でアダプタが legacy の無パラメータ Array を pin)。
- 残存(設計上 seam に留まるもの): `infer_expr_type` の `complex`/Dict thin アダプタ呼び出し、
  パラメトリック struct コンストラクタ(`infer_type_args`+`resolve_instantiation` が
  `&mut SharedCompileContext` を要求)、HOF call-site 推論(map/filter/reduce/broadcast/mapreduce —
  ラムダ本体の式レベル解析が必要)、メソッドテーブルディスパッチ、julia_type.rs の
  iterator ラッパ群(enumerate/zip は engine 用 Generator tfunc が同名キーを保有、かつ
  struct 解決順序に依存するため据え置き)。

### runtime バインディングマッチャーの dispatch_resolver 移管 (Issue #5915 / #6502)

- #5915 残件スコープ(2026-06-12 検証コメント)のうち、runtime 側の未委譲マッチャー 2 件を
  `inference_core/dispatch_resolver.rs` へ移管(挙動保存):
  - `Vm::value_matches_param_with_bindings` → `runtime_value_type_matches_param_with_bindings`
    (VM は型導出 + `value_matches_param` fallback クロージャのみ供給)。
    `vm/dispatch_binding.rs` の `where` 型変数 binding ヘルパー群 4 関数を削除。
  - `Vm::check_type_match` → `runtime_type_name_matches_param`(#5314 leaf-struct ガードと
    エンジン裏付け `check_subtype` をクロージャ注入)。
- 現状整理(#6502 向けの所見): callable-value 経路(`dispatch_function_variable`)は既に
  `resolve_callable_value_candidates` の薄いアダプタ、typed dispatch 経路も
  `resolve_type_name_candidates_with_subtype_fallback` + `inference_core/selection` を使用済み。
  残る大物は (1) compile 側 `MethodTable::dispatch` の候補列挙→選択フローの共通コア化、
  (2) `julia_type_pattern_matches`(compile)と
  `runtime_value_type_matches_param_with_bindings`(runtime)の意味統合
  (bound 検査が compile=CoreType 常時検査 / runtime=`from_name` 既知型のみ検査と非対称)、
  (3) 文字列エンコード候補解決の `core_signature` ベース構造化照合への置換。いずれも #6502。

### メソッド選択パイプライン driver の単一コア化 — compile/runtime 双方を thin adapter 化 (Issue #6502)

- `inference_core/selection.rs` に選択パイプライン driver `select_method`
  (空ゲート → morespecific dominance pre-check → conflict/ambiguity ゲート → 最終 scored pick;
  `Selection::{NoMatch, Selected, Ambiguous}`)と、一般化 first-best winnowing
  `pick_best`(caller 供給の strict order; `pick_max_score` は thin wrapper 化)を追加。
  upstream の `jl_lookup_generic` / typemap に相当する選択制御フローの単一所有点。
- compile 側 `MethodTable::dispatch_inner`(`compile/method_table.rs`)と runtime 側
  `Vm::find_best_method_index_from_candidates`(`vm/mod.rs`; `CallDynamic` tier 群・
  `IterateDynamic`・`CallTypedDispatch` の value channel が呼ぶ)を `select_method` への
  thin adapter に縮退(挙動保存)。runtime の 7 ルール dominance ladder は compile 側
  `dominance_precheck_index` をミラーする `runtime_dominance_precheck_index` に整理。
- `vm/mod.rs` に手書きされていた unique-dominant ループ 8 箇所
  (`dominant_method_index_runtime_for_indices` の subtype-definitive ループ +
  tuple_vararg / tuple_diagonal / vector_diagonal / union_actual /
  type_value / type_vector / type_matrix diagonal)を `selection::unique_dominant_index`
  へ委譲し削除(#6509 が compile 側で行った移行の runtime ミラー)。
  `find_best_method_index_uncached` / `find_best_method_index_from_candidates` の
  max-score+vararg-tie fold 2 箇所も `selection::pick_best` へ委譲。
- 残件(#6502 step 3): 文字列エンコード候補解決
  (`resolve_runtime_type_pattern_candidates`)の `core_signature` 構造化照合への置換は
  挙動変更を伴う(runtime 文字列マッチャーは `JuliaType::from_name` が知る bound のみ検査、
  CoreType 照合は常時検査)ため、parity 検証付きの別スライスとして deferred。
  #6528 で顕在化した value/name channel の候補集合非対称(wrapper fence が value channel
  のみを刈る)も同スライスで扱う。

- #6524 マージ直後の main で `reduce(+, Int64[])` 等のリテラル空配列 reduction が throw
  (fixture `hof_mapreduce_identity_plus_type_preservation_4619` red)。`Function` arm の
  エンジン化により broad な `reduce(op::Function, itr)` が value-channel(`metadata_best`)の
  唯一のマッチになり、wrapper fence で value-channel から除外されている typed 特殊化
  (`reduce(::typeof(+), Vector{T})`)への name-channel ランキングを上書きしていた。
- `CallTypedDispatch` の broad-signature ガードを `Function` スロットも broad と数える
  よう拡張して修正(詳細は DONE.md / Issue #6529)。value-channel と name-channel の
  候補集合非対称(native-array wrapper fence)は #6502 のエントリ枠組み統合で
  解消すべき構造的課題として残る。

### Callable-value n-ary `+` / `*` fold の narrow integer 対応 (Issue #6512)

- `try_call_intrinsic` の callable `+` / `*` fold が呼ぶ `dynamic_add` /
  `dynamic_mul` に、binary-both fallback と同じ narrow-int modular wrap
  protocol を共有ヘルパー(`vm/narrow_int_arith.rs`)として導入。same-type
  `Int8` / `Int16` / `Int32` / `UInt8` / `UInt16` / `UInt32` の `+` / `-` /
  `*` は runtime dynamic path でも operand の narrow 型を保って wrap する。
- `Vm::type_matches` の `::Function` exact-name carve-out を削除し、
  singleton function 型名(`typeof(+)`)を他の名義型と同じく
  `CoreSubtypeEngine` 経由で判定するように戻した。これで
  `typeof(+) <: Function` が upstream Julia と同じ true になり、Issue #5915
  の runtime matcher migration に残っていた #6512 blocker は解消。
- 回帰 fixture `arithmetic_narrow_int_wrapping_5205` を拡張し、
  `f = +; f(Int8(...), ...)` / `f = *; ...` と、`map(+, Int8...)` /
  `map(*, UInt8...)` の callable-value path を pin。`convert(Int8, 300)` の
  InexactError(#5192) は引き続き保持。

### `TypeParam.bound` の serde 復元ずれを修正 (`Complex{T} where T<:Real` / isnan・isinf・transpose) (Issue #6518)

- `TypeParam.bound`(`upper_bound` の legacy ミラー)は `#[serde(skip)]` のため、
  prelude `Program` を bincode round-trip するとデフォルトの `None` に落ち、`where T<:Real`
  制約が黙って消えていた。`bound` は dead state ではなく**ライブ**フィールドで、
  パラメトリック束縛ディスパッチ(`types/julia_type/comparison.rs`、`where T<:Bound` を強制)と
  パラメトリック構造体の bound 検査(`compile/context.rs`)が参照する。
- 症状: `base_method_tables_serde_roundtrip_reconstructs_projections_issue_6336` が
  **キャッシュ再生成直後**に `isnan`/`isinf`/`transpose`/`unsafe_rational`(HashMap 反復順依存)で
  `bound: None`(prelude 由来)vs `Some("Real")`(`core_signature` 復元)の不一致で FAIL。
  既存キャッシュが残っている間はマスクされ、`CACHE_VERSION` を上げる全 PR がこれを顕在化させる潜在バグ。
- 修正: `TypeParam` に手書き `Deserialize` を実装し、デシリアライズ時に `bound` を
  `upper_bound` から再構築(`types/type_param.rs`)。これで prelude-program キャッシュと
  method-table の `core_signature` 復元の双方が、全 production コンストラクタ
  (`with_upper_bound` 等)と同じ `bound == upper_bound` 不変量に揃いキャッシュが透過になる。
- julia 1.12 と parity 確認: `isnan`/`isinf`/`transpose`(Float / Int の Complex 要素型)は
  挙動不変。唯一の差分は範囲外の型引数(`Complex{String}` 等)を正しく拒否する点のみで、
  Base/テストにそのような呼び出しは無い。回帰 fixture `complex_isnan_isinf_bound_6518.jl` を追加。

### runtime `<:` (check_subtype) を CoreSubtypeEngine 単一 authority 化し legacy fallback を全廃 (Issue #5915 wave 3)

- `Vm::check_subtype`(`vm/type_ops/comparison.rs`)を**エンジン直結**に書き換え。
  `left==right` / `right=="Any"` / `left=="Union{}"` の安価な早期 return を残し、
  残り全ての `<:` 判定を `CoreSubtypeEngine::with_hierarchy(struct_hierarchy)` に委譲。
  retire した legacy fallback: 名義スーパータイプ鎖 walk (`reflection_supertype_name`)、
  `type_ancestors` walk (`check_abstract_type_hierarchy`)、Union 左右分解、
  Tuple 共変 walk、forall-left / where-right の名義鎖再入、bare-nominal 文字列一致、
  `check_parametric_typevar_match`。付随する「authoritative ゲート」機構
  (`core_type_subtype_is_authoritative` ほか ~40 関数)と文字列ヒューリスティック
  (`has_top_level_where` / `forall_left_nominal_base` / `parse_union_members` ほか)も削除。
- エンジン拡張 2 点(本家 julia 1.12 と parity 確認):
  1. `(Named, Abstract)` アーム(`inference_core/type_core/subtype.rs`)— 非パラメトリック
     なユーザ型(`struct Money <: Real` / `abstract type Currency <: Number`)が宣言した
     親鎖を built-in abstract 数値ラティスまで `struct_is_subtype_of_abstract_in` で辿る。
     従来はエンジンが `_ => false` に落ち、runtime 側の `type_ancestors` fallback でのみ
     復旧していた。
  2. matcher の親 walk(`inference_core/type_core/match.rs` の `Struct` パターンアーム)—
     パラメトリックユーザ構造体を**異名**の existential パターン
     (`MyVec{Int64}` vs `Wrapper{S} where S` の本体 `Wrapper{S}`)に当てる際、宣言親を
     パラメータ置換した `Wrapper{Int64}` へ降りて束縛状態を保ったまま再 match し `S=Int64`
     を束縛。これで `MyVec{Int64} <: (Wrapper{S} where S)` が true、`Wrapper{Real}` は false。
- 残置 carve-out は **SubArray arity-gate のみ**(subtype エンジンのギャップではない)。
  #6512 で `::Function` exact-name 一致は撤去済み。SubArray については、本家では
  `SubArray{Int64}` は UnionAll prefix で 1-D/2-D 両方が
    `<:`(確認済み)だが、VM Base `subarray.jl` が `length(v::SubArray{Int64})` 等を
    **1-D 専用キャリアタグ**として使うため、エンジン意味論に合わせると 2-D view を捕捉して
    Base dispatch を壊す。これは dispatch arity の意図的乖離であり Base 書き換えepic。
- 監査: `check_subtype` の legacy fallback は完全撤去。残る独立 subtype ロジックは
  コンパイル時 `JuliaType::is_subtype_of`(`types/julia_type/comparison.rs`, sibling scope
  #5916)— エンジンに委譲しつつ Union/Type{} 等の薄い局所アームを保持 — と、上記 SubArray carve-out
  のみ。よって本 Issue は **Part of**(runtime 文字列パスは単一 authority 達成、コンパイル時
  パスの完全統合は別 wave)。
### CallDynamic / CallTypedDispatch 候補ペイロードの構造化 (Issue #6496)

- #6336 構造化マイグレーション第6弾(CACHE_VERSION 46)。`Instr` の動的
  ディスパッチ系候補ペイロードから焼き込み型名文字列を全廃: `CallDynamic` は
  `DynamicCallCandidate::Method(usize)` + `NativeIterator(NativeIteratorKind)`
  (旧 `(usize::MAX, "Zip".."Zip7"/"Base.Generator")` sentinel の enum 化)、
  `CallDynamicOrBuiltin` / `CallDynamicBinary[Both|NoFallback]` /
  `CallTypedDispatch[OrBuiltin*]` / `TypedDispatchStoreDict` は候補関数 index
  のみ(`Vec<usize>`)を直列化。ランタイムは各候補の `FunctionInfo` から
  従来文字列を導出(`vm::derived_runtime_signature`)し、既存リゾルバへ渡す
  ため判定は不変。Base 全コーパスのパリティゲートで歴史的焼き込みとの完全
  一致を恒久 pin。call-site cache を持たない系統は導出を memoize
  (`binary_signature_cache` / `typed_signature_cache`)。
- `MethodSig::runtime_type_names_for_arity` は削除(arity ゲートは
  `accepts_arity`、abstract-array 系ヒューリスティクスは
  `param_type_at_call_position`、展開規則はパリティゲートのテストローカル
  `historical_baked_signature` として残置)。詳細は BINARY_DISPATCH.md
  「State of the #6336 structured-signature migration」第6項。

### 型表現変換の正準化 wave 3: `JuliaType → LatticeType` 重複の集約 (Issue #5916)

- `docs/vm/TYPE_REPRESENTATIONS.md` の提案 step 2-3 に沿い、`JuliaType →
  LatticeType` の正準変換 `bridge::julia_type_to_lattice` /
  `julia_type_to_lattice_with_struct_table(ty, Option<&table>)` を
  `compile/bridge.rs`(本 wave の所有ファイル)に新設。歴史的に 4 つの並行実装
  (`analyzer.rs` / `engine/mod.rs` / `expr_tfuncs.rs` /
  `builtins_reflection`)が **3 点で食い違って** いたのを upstream Julia 1.12
  準拠に解決:
  - 空 `Union{}` → `Bottom`(`typeof(Union{}) == Core.TypeofBottom`。reflection
    版の `Union(∅)`・expr_tfuncs 版の `Top` 崩しはいずれも誤り)。
  - 抽象数値スーパータイプ(`Number`/`Real`/`Integer`/`Signed`/`Unsigned`/
    `AbstractFloat`)を対応する `ConcreteType` マーカーとして保持(`Top` への
    広域化を回避)。
  - 構造体解決をパラメータ化(テーブルあり → `type_id` 解決、なし → 構造体
    spelling を保持する `type_id: 0` プレースホルダ)。
- 要素変換は構造化された `julia_type_to_concrete_type_lossy`(`pub(crate)` 化、
  sibling から到達可能に)を再利用し、lattice 方向と ConcreteType 方向が乖離
  しないことを `bridge::test_julia_type_to_lattice_*_issue_5916`(6 テスト)で
  ピン留め。
- sibling 所有ファイル(`compile/abstract_interp/**`・`compile/expr/infer/**`・
  `compile/type_stability/**`・`vm/builtins_reflection/**`)の 4 呼び出し箇所は
  本 PR の scope 外のため、次 wave で委譲するよう
  `TYPE_REPRESENTATIONS.md` §3.6 に deferred として明記。`JuliaType →
  ConcreteType`(1 構造化正準 + 2 lattice 派生ラッパ)・`JuliaType → ValueType`
  (単一ベース実装 + capability ラッパ)も同節で整理。

### tfunc レジストリの構造体コンテキスト拡張と complex/Dict/struct ctor ゲート移行 (Issue #5922)

- `TFuncContext` に `StructIdLookup` トレイト(構造体名 → `type_id` の読み取り
  専用解決)を追加。abstract-interp エンジン側の `StructTypeInfo` テーブルと
  式推論側の `SharedCompileContext::struct_table` の双方が同一トレイトで
  レジストリに渡せるようになり、「現在のレジストリ形状では表現不能」だった
  コンストラクタ系ゲートが contextual tfunc として表現可能に。
- default struct constructor の型規則は `struct_constructor_result`
  (単一 authority)として `TransferFunctions` に追加。ただし汎用ディスパッチ
  (`infer_return_type_with_context`)へのフォールバック適用は**意図的に不採用**:
  エンジンの最終フォールバックが `Top` を返すことに依存する
  「builtin 表現を持つ pure-Julia struct」(例: `Base.Generator` →
  `ValueType::Generator`)を `Struct(id)` に解決すると codegen の coercion が
  壊れる(fixture `generator_runtime_callable_constructor` で検出)。
  適用箇所は式推論アダプタ側のみ(レジストリ単体テストでピン留め)。
- `compile/expr/infer/mod.rs` のインラインゲート 3 種を移行:
  lowercase `complex`(contextual tfunc `tfunc_complex_contextual`)、
  builtin パターン `Dict`(構文ゲートはアダプタ、型規則はレジストリ `Dict`)、
  非パラメトリック struct constructor(レジストリ共有規則)。
  `julia_type.rs` のハードコード `complex` ゲートも同一 tfunc に統一
  (両表現パスのドリフト防止)。
- パラメトリック instantiation(`infer_type_args` + `resolve_instantiation`)は
  `&mut SharedCompileContext` を要するため呼び出し側のシームに残置。
  HOF(map/filter/broadcast/reduce)・LinearAlgebra ゲートは未移行。

### wave 3: pre-scan の literal 局所型を共有 authority へ移行 (Issue #5922)

- レガシー pre-scan(`compile/inference.rs` の
  `collect_local_types_for_inference_with_mixed_tracking`)と共有
  abstract-interp エンジンの二重推論のうち、**literal 右辺の局所型**を共有
  authority へ一本化。新モジュール `compile/abstract_interp/local_authority.rs`
  に `literal_to_lattice`(`Literal -> LatticeType` の単一真実)を置き、
  エンジンの `InferenceEngine::infer_literal` はこれへ委譲。pre-scan は
  `literal_assignment_value_type`(`literal_to_lattice` + `bridge::lattice_to_value_type`)
  経由で局所型を決める。
- 移行対象は格子が忠実表現できる literal クラスのみ:
  `Int / Int128 / BigInt / BigFloat / Float / Float32 / Float16 / Bool /
  String / Char / Nothing / Missing / Symbol`(13 variant)。これらは
  `infer_value_type` の literal arm と round-trip 同値であることを単体テスト
  (`local_authority::tests` + `inference::tests::literal_assignment_prescan_matches_legacy_issue_5922`)
  でピン留め。
- 残置(レガシー pre-scan が引き続き所有): 格子表現が忠実でない literal
  (`Array / ArrayI64 / ArrayBool`(要素幅) / `Struct`(struct ctor 解決) /
  `Module / Regex / Enum / Expr / QuoteNode / LineNumberNode / Undef`)は
  `literal_assignment_value_type` が `None` を返し旧来の struct-aware 推論へ
  フォールバック。**`Top`→`Any` の暗黙ワイドニングは禁止**(誤った広すぎる
  局所型は codegen 特殊化の罠 — wave 2 の `Struct(42)`→Generator 退行参照)
  なので `None` で明示的に委譲する。非 literal 式クラス(call / index /
  field-access / binary など)は全て旧 pre-scan のまま(将来 wave の対象)。
- カバレッジ: 局所型付けの「literal 代入」クラス(`x = <literal>`)が単一
  authority 経由に。literal RHS の中で移行できたのは 13/24 variant(54%、
  残り 11 は格子非忠実につき意図的にレガシー)。式クラス全体ではまだ literal
  クラスのみが authority 経由で、call/index/field/binary 等は pre-scan。

### binary_both.rs の promote-then-same-type 再構成 (Issue #6338)

- `vm/exec/binary_both.rs` の手書き型ペア分岐を upstream Julia の promotion
  構造(`julia/base/promotion.jl`: 異型ペアは promote で共通型へ、同型ペア
  だけが intrinsic 実装を持つ)に沿って再編。`same_type_fast_path`(同型
  ペアのみ `Value::` マッチを許す intrinsic テーブル)と
  `promote_numeric_pair`(異型→共通型変換 + `PromotedPairPolicy`)を分離し、
  Float64 昇格グループ(Float16×Float64 / Float32×Float64 / Int64×Float64)
  と Float32 昇格グループ(Float16×Float32 / Float32×Int64 / Float32×Int128)
  を promote 経路へ畳み込み。到達不能な dead-float16/float32-duplicate arm
  も削除。`Value::` パターン数 509 → 481。
- 「現在の挙動が厳密に promote→同型演算と一致する」ペアだけを畳み込み、
  挙動例外(Bool 結果型、Float16×Int の結果側 narrowing(真の promote とは
  二重丸めで相違)、unsigned 幅、Int128×Int64、BigInt/BigFloat、Char)は
  明示 arm のまま残置(Issue #5966 promote-fallback 再帰トラップ対策)。
  ベンチは全数値ホットパスで回帰なし(fib_20 -5.9% など改善のみ)。構造の
  詳細は BINARY_DISPATCH.md「Promote-then-same-type structure」節と
  PROMOTION.md を参照。

### 選択コア第2スライス: ランタイム dispatch パスが selection.rs を採用 (Issue #6502)

- ランタイム `call_dynamic*` の選択制御フローを共有選択コア
  (`inference_core/selection.rs`)へ。新プリミティブ `pick_max_score`
  (最初勝ち argmax)/ `pick_first_tier`(順序付き tier フォールバック)を
  追加し、`Instr::CallDynamic` の metadata 候補 3 段カスケードと
  `Instr::CallTypedDispatch` のランタイム関数名検索ループを変換
  (挙動保存・ホットパスは単相クロージャのみ、tier リストは遅延構築)。
- 値依存の VM 表現フィルタ(Dict/Range ミスマッチ等)は設計どおり呼び出し
  側の候補プリフィルタとして残置。残ギャップと非対象ファイル
  (`call_dynamic_binary.rs` = #3910 で委譲済み、`dynamic_ops/dispatch.rs`
  = 選択ループなし)の整理は BINARY_DISPATCH.md の選択コア節を更新。
  直列化ペイロード置換(スライス (b))は #6496 のまま据え置き、
  CACHE_VERSION は不変。

### compile_call のテーブル駆動ハンドラ分割 (Issue #6332)

- `compile/expr/call/mod.rs` の `compile_call`(約 3,993 行、関数名文字列比較
  ~50 箇所の直列 if/match チェーン)を、upstream `add_tfunc` 方式に倣った
  テーブル駆動ハンドラへ純粋切り出し。本体は 141 行のディスパッチ列になった
  (挙動・評価順は不変)。
- `handlers/` 配下に責務別モジュール(early / arrays / collections /
  internals / math / misc / strings)。ハンドラは統一シグネチャ
  `fn(&mut CoreCompiler, &CallCtx<'_>) -> Option<CResult<ValueType>>` で、
  `None` = 「特殊ケース非該当 → 汎用パスへフォールスルー」により元の
  if 不成立セマンティクスを厳密保存。ディスパッチ地点は元の判定位置に
  対応する 3 箇所(`early_special_case_handler` = 関数先頭 /
  `special_case_handler` = 旧大 match 直前 /
  `post_struct_special_case_handler` = struct コンストラクタ解決直後)。
- 名前キーでない順序依存ブロックはテーブル化せず、同位置から呼ぶヘルパーへ
  verbatim 抽出: splat 解決 + callable 変数チェーン + enum 統合
  (mod.rs 内ヘルパー)、コンストラクタ解決チェーン(`constructors.rs`)、
  汎用メソッドディスパッチ末尾 ~1,300 行(`dispatch.rs`)。旧 no-op
  `match`(全アーム空のルーティングメモ)はコメントとして本体外へ移設。

### ランタイムマッチャ `type_matches` の subtype 判定を共有エンジンへ委譲 (Issue #5915)

- `value_matches_param_with_bindings` → `value_matches_param` → `type_matches`
  チェーン(ランタイム動的ディスパッチの最後の独立マッチャ)のうち、純粋な
  nominal subtype 判定を `check_subtype`(= `CoreSubtypeEngine` ファサード)
  経由に統一。#5921 で数値系 arm が確立したパターンの拡張。
- 委譲した arm: `::Array` / `::AbstractArray`(手書きの
  `{Array, Vector, Matrix}` ベース名ホワイトリストを削除)、`::Tuple`、
  `::AbstractString` / `::AbstractChar` / `::IO`(従来は `_` フォールバックの
  名前完全一致で upstream に反して不一致だった)、`AbstractUser`(ユーザ宣言
  abstract 型。boot.jl の `AbstractDict` / `AbstractSet` を含む。従来は名前
  完全一致でサブタイプ値に一致せず、`::IO` 委譲後に `repr(Dict)` が generic
  `show(io::IO, x)` へ誤ディスパッチする退行として顕在化したため同時に委譲)、
  および型変数を含まない `TupleOf`(`Tuple{Int64} <: Tuple{Real}` の
  covariance を upstream どおりに判定。従来は要素の完全一致を要求)。
- upstream Julia で全ペアを検証済み(range/SubArray は `AbstractArray`、
  `String <: AbstractString` 等)。TypeVar 要素を含む `TupleOf` と束縛抽出
  (bindings)ロジックはローカルに維持。
- 残りの未委譲経路: `TupleOf`(TypeVar 要素あり)/ `VectorOf` / `MatrixOf` /
  Ref/RefValue の不変要素一致(Julia の invariance に整合)、`Struct(name)`
  arm のパラメトリック文字列比較、`dispatch_resolver.rs` の文字列名マッチャ
  (`type_name_pattern_matches` / `resolve_type_name_candidates`)。

### ランタイムマッチャ移行 第2弾: `Struct(name)` / `_` フォールバック / TypeVar 付き TupleOf (Issue #5915)

- 第1弾(PR #6507)の残り arm を `check_subtype`(= `CoreSubtypeEngine`
  ファサード)経由へ委譲:
  - `Struct(name)` arm のパラメトリック文字列比較: 両辺パラメトリックな
    具象パラメータ対を engine 判定へ(invariance 維持 +
    `struct MyVec{T} <: Wrapper{T}` の宣言親
    `MyVec{Int64} <: Wrapper{Int64}` を upstream どおり一致させる)。
    TypeVar パラメータのワイルドカード一致と「runtime 側パラメータ不明時の
    寛容なベース名一致」はローカルに維持(bindings 抽出は別経路)。
  - `_` フォールバック(残余の nominal 名 variant): 名前完全一致 →
    engine 判定へ(`DataType <: Type`、`Set{Int64} <: Set` 等が upstream
    どおり一致するように)。当時は `::Function` のみ #6512 workaround として
    維持していたが、#6512 で n-ary intrinsic fold 側を修正して撤去済み。
  - TypeVar 要素を含む `TupleOf`: TypeVar 脚はローカルワイルドカードの
    まま、具象要素脚を engine の covariant 判定へ
    (`Tuple{Int64, Int64} <: Tuple{T, Real} where T` を upstream どおり
    一致)。Vararg 末尾の lead 要素も同様。
- `VectorOf` / `MatrixOf` / Ref/RefValue の要素一致は Julia の invariance
  (`Vector{Int64} <: Vector{Real}` は false)に既に整合しており委譲対象外。
  upstream 検証済みの invariance 回帰テストで固定
  (`runtime_type_matches_vector_matrix_ref_params_stay_invariant_issue_5915`)。
- `dispatch_resolver.rs` の `struct_family_matches`: 手書きの
  `Array → {Vector, Matrix, Array}` エイリアス表を
  `CoreSubtypeEngine::is_subtype_by_name` 委譲に置換(accept-set 不変。
  bare 名 `Array <: Vector` は engine が存在量化で緩く true を返すため、
  rank 消去方向のみ従来どおり expected == "Array" ゲートで遮断)。
- 未委譲のまま残る独立 subtype ロジック(継続課題):
  `vm/type_ops/comparison.rs` 内 `check_subtype` 自身の legacy フォール
  バック(`check_nominal_supertype_chain` / `check_abstract_type_hierarchy`
  / `check_parametric_typevar_match` / Union 文字列分解 /
  `check_tuple_covariant_subtype` — engine 判定が authoritative でない
  場合のみ実行)、`dispatch_resolver.rs` の
  `same_invariant_container_family_concrete_miss`(名前同一時の invariance
  ゲート)。

### メソッド選択コア統合 第1スライス (Issue #6502)

- メソッド選択の制御フロー(候補列挙→マッチ→dominance→選択)を
  `inference_core/selection.rs` へ抽出(`unique_dominant_index` /
  `pick_scored_match`)。`MethodTable::dispatch_inner` は意味判定をクロージャ
  注入する薄いアダプタになり、9 個の dominance プレチェックと tie-breaker の
  重複制御フローを集約。
- 挙動保存リファクタ: ディスパッチ意味論・wire format とも不変
  (CACHE_VERSION 45 据え置き)。runtime `call_dynamic*` の同コア採用と
  文字列エンコード候補解決(`resolve_runtime_type_pattern_candidates`)の
  `core_signature` ベース構造化照合への置換が残フォローアップ。
### 型表現の変換インベントリ + 死変換削除 (Issue #5916)

- Issue #5916 提案 step 1 を実施: 6 系統の型表現(`JuliaType` / `CoreType` /
  `LatticeType`+`ConcreteType` / `ValueType` / `TypeExpr` / AoT 射影)の
  全変換(44 エントリ、`file:line` + 損失情報つき)を
  `docs/vm/TYPE_REPRESENTATIONS.md` に集約。TYPE_SYSTEM.md の要約表から
  リンク。
- 主要発見: (a) 不一致ラウンドトリップ —
  `LatticeType::Bottom → ValueType::Any → LatticeType::Top` の格子反転、
  `ConcreteType` 文字列往復の非対称(`String`/`Char` 片方向、`type_id` 0 化)、
  `Range{element} → CoreType` での要素消失; (b) 死変換 1 件; (c) 構造変換が
  あるのに文字列往復している箇所(promotion 経路 / reflection /
  `ArrayElementType::UnionOf(String)`)+ 同一変換の並行実装
  (`JuliaType→LatticeType` ×4 等)。
- 安全な削減 1 件: vm `JuliaType` → AoT `JuliaType` の
  `impl From`(`aot/types.rs:889`、79 行)は外部呼び出しゼロ(自己再帰のみ)
  で削除。`cargo check --all-targets --features "aot repl"` /
  `--features "cranelift repl"` で検証。
- 推奨 canonical 表現は `CoreType`(構造化パラメータ + 両 bound +
  UnionAll/Vararg/値パラメータを持つ唯一の表現、#6336 で直列化の単一情報源
  済み)。段階的移行案(重複実装の統合 → CoreType ハブ化 → view 化)を
  同ドキュメントに記載。

### 型表現ラウンドトリップ不一致の修正 wave 2 (Issue #5916)

- インベントリ(上記)が挙げた不一致ラウンドトリップを修正:
  - **格子反転(部分解消 + 文書化)**: `ValueType::Union(vec![])`(`Union{}`
    の VM 側スペリング、`julia_type_to_value_type(JT::Bottom)` が生成)→
    `LatticeType::Top` だった変換を `Bottom` に修正(`compile/bridge.rs`、
    table-aware 変種も同様)。逆方向 `LatticeType::Bottom → ValueType::Any`
    は**意図的な widening として維持**: 空 union キャリアへの厳密化を実装した
    ところ、再帰関数(`Meta.unblock`)の呼び出し点に in-progress Bottom 推定
    が漏れ、フィールドアクセス等の strict な codegen 消費者が「到達不能だが
    コンパイルは必要」なコードを拒否する退行が発生したため revert。
    ラウンドトリップ `Bottom → Any → Top` はテストでピン留め
    (`test_empty_union_value_type_is_bottom_issue_5916` ほか join/meet 法則)。
  - **`lattice_to_julia_type` の Bottom 保存**: `JuliaType` には `Bottom`
    変種があるため total 変換でも `Union{}` を保存(従来は `Any` に widening)。
    #4679 の `lattice_to_parametric_julia_type` の Bottom arm は引き続き必要
    (ValueType フォールバックが意図的に widening するため)と判定し、
    コメントを実態に合わせ更新。
  - **`Range{element} → CoreType` の要素保存**: `Abstract(AbstractRange)` への
    平坦化をやめ `Struct{"AbstractRange", [element]}`
    (`from_julia_name("AbstractRange{T}")` と同形)へ。`Range{Any}` は不変条件
    の関係で bare abstract のまま。subtype エンジンの range-family 規則で
    後方互換(ユニットテストで `<: Abstract(AbstractRange)` をピン)。
  - **`ValueType → JuliaType` の重複統合(部分)**: `vm/type_objects.rs` の
    コピーを `vm/builtins_reflection/primitives.rs` の canonical 実装への thin
    wrapper 化。不一致だった `Union` 処理は構造保存側(upstream 準拠:
    `Union{...}` 保存、空 union → `Union{}`)を採用。残る重複
    (`compile/expr/infer/julia_type.rs` ほか sibling 担当ファイル内)は
    TYPE_REPRESENTATIONS.md §3.4 に不一致内容を記録して deferred。
- `docs/vm/TYPE_REPRESENTATIONS.md` を更新(§3.5 Resolved 新設、表の
  file:line 更新、残存 divergence の所在を明記)。

- `infer_expr_type` / `infer_julia_type` の name-keyed な戻り値型ゲートのうち、
  レジストリで表現可能な残存分を `compile/tfuncs` + `expr_tfuncs` アダプタへ
  移行: gcd/lcm (BigInt 保持, Issue #2383)、big (Float32/64→BigFloat)、
  IOBuffer (julia 経路の Struct/Any ディスパッチ委譲を維持)、
  typeof/promote_type/promote_rule/eltype/keytype/valtype (DataType)、
  isequal (2 引数 Bool のみ。1 引数カリー形は arity ゲートで非推論のまま,
  Issue #5662)、hash/fld/cld + 日付アクセサ (Int64)、trues/falses
  (BitVector/BitMatrix/BitArray{n})。
- レジストリ規則は証明できる引数形状にのみ型を主張し (例: fld→Int64 は
  Int64 ペアのみ)、レガシーの無条件フォールバックは式アダプタ側の
  `FixedFallback` が保持するため、観測可能な推論結果は不変。
- 未移行 (レジストリで表現不能なため legacy gate を残置): `complex`
  (struct_table 参照が必要)、`Dict` builtin パターン (構文パターン +
  shared_ctx)、struct / パラメトリックコンストラクタ、HOF ハンドラ
  (map/filter/reduce/broadcast/mapreduce: コールサイト特殊化が必要)、
  `collect` (range 要素型 Int64 の知識が lattice 変換で失われる)、
  `rand`/`randn` (引数個数依存フォールバックが FixedFallback で表現不能)、
  `Base.LinearAlgebra` ModuleCall (name-keyed レジストリに修飾名の規約が
  未導入)。
### 数値/Range 並列 subtype テーブルの削除 (Issue #5921)

- `JuliaType::is_subtype_of` の手書き並列テーブル(Number/Real/Integer/
  Signed/Unsigned/AbstractFloat + AbstractRange/UnitRange/StepRange の
  match arm, 約 160 行)を削除し、関数冒頭の `CoreSubtypeEngine` 委譲に
  一本化。Issue #2494 の「compile-time / runtime 両実装を手で同期する」
  duty は解消(両エントリポイントの doc コメントからも除去、
  WORKAROUNDS.md Resolved に記録)。
- 削除前に upstream julia 1.12 検証済みの 22×22 数値マトリクス +
  range 関係テスト(`engine_delegation_matrix_issue_5921`)を追加し、
  `test_check_subtype_parity_with_julia_type` を range ペアへ拡張
  (退行ゲートとして恒久維持)。
- エンジン側修正 1 件: `struct_is_subtype_of_abstract` の
  AbstractRange/AbstractUnitRange arm を既存の
  `range_family_name_subtype_allowed` 格子に委譲。これにより parametric
  abstract スペル(`AbstractUnitRange{Int64} <: AbstractRange` =
  upstream true)が成立し、`LogRange <: AbstractRange` は false のまま。

### Issue #6336 完了: core_signature を直列化の単一情報源へ (CACHE_VERSION 45)

- `MethodSig` の直列化を専用 wire format(`MethodSigWire`)に変更: 型情報は
  canonical な `core_signature` のみが直列化され、引数名は表示用
  `param_names` として運ぶ。legacy `params: Vec<(String, JuliaType)>` /
  `type_params: Vec<TypeParam>` は **非直列化の in-memory 射影** となり、
  デシリアライズ時に canonical 逆変換
  (`inference_core::core_type_to_julia_type` / `core_type_var_to_type_param`,
  convert.rs に新設)から一度だけ再構成される。表現の二重化(スキュー)は
  キャッシュ境界を越えられない。bincode レイアウト変更につき
  `CACHE_VERSION` 44→45。
- 逆変換の正確性: `JuliaType→CoreType` は非単射のため、逆変換は lowering が
  実際に生成する唯一のスペル(`Expr`→専用 variant、`Pairs`→`Struct("Pairs")`
  等、`JuliaType::from_name` の挙動に整合)を選ぶ。Base 全コーパス
  (~9,000 引数)での厳密 round-trip を
  `base_method_params_roundtrip_core_signature_issue_6336`(params +
  type_params)と
  `base_method_tables_serde_roundtrip_reconstructs_projections_issue_6336`
  (全メソッドテーブルの serialize→deserialize)が恒久ゲート。ユーザ形状
  (where 句 / `Vararg{T,N}` / ネスト `Vector{Vector{Int64}}` / `Type{T}` /
  `AbstractVector{<:Integer}`)は method_table.rs の serde テストで検証。
- 残フォローアップ(Issue 起票済み): legacy matcher の CoreType ネイティブ
  移植(in-memory 射影の撤去まで含む)= #6495、
  `CallDynamic`/`CallTypedDispatch` の名前文字列ペイロード = #6496。

## 最新対応 (2026-06-11)

### Bool div result type (Issue #6486)

- Added the upstream `div(::Bool, ::Bool)` method in bundled `base/bool.jl` so
  `div(true, true)` and `div(false, true)` return `Bool` instead of reaching
  the generic Float64 fallback.
- Divide-by-zero Bool cases still throw `DivideError`, matching upstream Julia.
- Added `bool/div_result_6486.jl`.

### signed/unsigned primitive-width fallback conversions (Issue #6494)

- Extended the VM `BuiltinId::Signed` / `BuiltinId::Unsigned` fallback to cover
  Int8/16/32/64/128 and UInt8/16/32/64/128 with same-width bit reinterpretation.
- This keeps early/fallback runtime paths consistent with the Pure Julia
  `signed` / `unsigned` methods used for normal public dispatch.
- Added `conversion/signed_unsigned_widths_6494.jl` plus direct Rust unit
  coverage for the fallback builtin.

### Mixed-width integer div result types (Issue #6477)

- Added Pure Julia mixed `div(::Integer, ::Integer)` routing so same-sign
  mixed-width integer division promotes to the upstream concrete result type
  instead of falling through to `div(x, y) = floor(x / y)` and returning
  `Float64`.
- Mirrored upstream signed/unsigned `div` directionality, including the
  `UInt8 ÷ Int16 -> UInt16` and `Int8 ÷ UInt16 -> Int16` result shapes.
- Added `arithmetic/mixed_width_div_6477.jl` covering `div`, lowered `÷`, and
  BigInt mixed integer pairs.

### BigInt narrow integer promote conversion (Issue #6489)

- `BigInt(...)` now accepts Bool and every primitive signed/unsigned integer
  width, not just `Int64`.
- Value-level `promote(big(10), Int8(3))` and the reverse order now convert both
  operands to `BigInt`, matching the `promote_type(...)=BigInt` result restored
  by Issue #6487.
- Added `promotion/bigint_narrow_promote_6489.jl` to cover direct constructor
  conversion and mixed BigInt/narrow promotion.

### Mixed integer promote_type concrete results (Issue #6487)

- Expanded the bundled integer `promote_rule` table into explicit concrete
  `Type` pairs so value-level `promote_type(Int16, Int8)` returns `Int16`
  instead of falling through to abstract `typejoin` results like `Signed`.
- `promote(...)` now converts mixed signed/unsigned primitive integer pairs to
  the same concrete result type that upstream Julia reports; BigInt/narrow
  value conversion remains tracked separately by Issue #6489.
- Added `promotion/mixed_integer_promote_type_6487.jl` to cover both
  `promote_type` and value-level tuple conversion.

### Legacy native-array carrier compatibility isolation (Issue #6337)

- Added `vm/native_array_compat.rs` as the single VM-side home for transitional
  native-array carrier predicates used by dispatch boundary checks, pointer
  identity, and borrowed carrier access.
- Removed the scattered `is_legacy_array_value` / `legacy_array_*` helper names
  from `vm/mod.rs`, `exec/binary_both.rs`, `exec/array_basic.rs`, and adjacent
  VM call sites; `rg 'legacy_array' subset_julia_vm/src/vm -g '*.rs'` is now
  zero.
- The existing `Base.empty` native-array dispatch exception remains isolated in
  the compatibility module and was not expanded; new Base functions should keep
  using the Memory-first / Pure Julia `Array{T,N}` wrapper path.

### ウォーム起動フェーズ2完了: 1行スクリプト ~40ms (≤50ms 達成) (Issue #6348)

- `println(1+1)` のウォーム起動(永続 Base キャッシュ有効)が ~60ms → **~40ms**
  (Issue 起票時 ~320ms)。フェーズ2 受け入れ条件 **≤50ms を達成**し Issue クローズ。
- 対策(Issue #6348 フェーズ2):
  - **2大デシリアライズの並列化**: Base キャッシュは VM `Value` 定数(`Rc`)を含み
    `Send` 不可のためコンパイルスレッド固定。代わりに prelude `Program`(`Send`)側を
    バックグラウンドスレッドへ移し(`begin_warm_start_prefetch`)、CLI はパース前に
    `warm_base_cache()` でメインスレッド上の Base キャッシュ deserialize(~9ms)を
    prelude ロード(~9ms)と重ねる。`pipeline.prelude_program_load` は実測 0ms に。
  - **推論エンジン用 Base 関数 clone のプリフェッチ**: prefetch スレッドが
    prelude.functions の clone を事前作成し、`compile.inference_functions_clone` を
    ~4.4ms → ~0.2ms に短縮(Base 再定義時は長さ検証で自動フォールバック)。
  - **ワンショット CLI の fast exit**: 実行完了後に stdout/stderr を flush して
    デストラクタ(~14MB の CompiledProgram 解放)をスキップ。
  - 計測点追加: `cli.vm_new` / `cli.vm_run`。
- 残りの内訳 (~40ms): prelude マージ clone ~7ms / compile ~16ms(method_table_setup
  ~7.4、code prefix copy ~2.5、emit ~2.4)/ vm_new ~3.1 / プロセス起動 ~10ms。
  さらなる短縮は `.sjvmbc` 直実行(~25-30ms)経路か Program の zero-copy 化が必要。

### `for outer i` modifier rejection (Issue #6465)

- Lowering now detects the parser marker for `for outer i in itr` and rejects
  it before it can be mis-lowered as `for outer in i`.
- The normal contextual-variable form `for outer in itr` remains accepted and
  continues to bind the variable named `outer`.
- The diagnostic includes the upstream Julia phrase
  `no outer local variable declaration exists for "for outer"` for the
  top-level repro shape.

### MethodSig structured core_signature 移行(第1弾)/ specificity の文字列パース撤去 (Issue #6336)

- `inference_core/specificity.rs` のディスパッチ経路から ad-hoc な型名文字列
  パースを撤去: 抽象コンテナパラメータ(`AbstractVector{T}` /
  `AbstractArray{T,N}` 等)は中央ブリッジ `CoreType::from` で一度だけ構造化
  し、`CoreType` のまま検査する。`parse_diagonal_container_param` /
  `split_diagonal_container_params` / `bound_subtypes(&str,&str)` を削除。
- 対角パターン(tuple/vector/type-value/type-vector/type-matrix)の `where`
  上限は `&str` ではなく構造化 `CoreType`(`type_param_upper_bound_core`)で
  保持・比較する。
- `MethodSig::arg_core_types()` を追加(`core_signature` から `UnionAll` を
  剥がした引数型スライスの射影)。`empty_trailing_vararg_dominant_match_index`
  を legacy `params` 読みから同アクセサへ移行。
- `params` / `type_params` フィールド自体の削除、`IterateDynamic` の文字列
  シグネチャ、`struct_parents` 名前キー階層、`vm/type_objects.rs` の
  リフレクション用名前分解は残課題(Issue #6336 継続)。

### Issue #6336 第5弾: 最終残課題の整理と round-trip ゲートテスト

- `struct_is_subtype_of_abstract` ウォーク内に残っていた最後の ad-hoc
  `split('{')` 2 箇所を共有 `nominal_family_name` に置換(family 名抽出が
  `StructHierarchy` のキー正規化と完全一致)。
- `params` / `type_params` 撤去(最終形)のブロッカーを定量化:
  `base_method_params_roundtrip_core_signature_issue_6336`(cache.rs)が
  Base 全メソッドの `JuliaType → CoreType → JuliaType` round-trip を検証。
  9,051 引数中、不一致は `Pairs` / `Expr` の二重スペル(専用 variant と
  `Struct(name)` の両方が Base に実在し `CoreType::from` が同一像に潰す)
  のみ。撤去には (a) lowering でのスペル正規化 + (b) legacy matcher
  (`dispatch_resolver` ~2,500 行)の CoreType 移植 or デシリアライズ時
  再構成が必要 — 残課題として BINARY_DISPATCH.md / TYPE_SYSTEM.md に明記。

### Issue #6336 第4弾: IterateDynamic ペイロードの構造化 (CACHE_VERSION 44)

- `Instr::IterateDynamic` の候補ペイロードを `Vec<(usize, String)>`
  (関数 index + `\u{1f}` 連結型名シグネチャ)から構造化 `Vec<usize>`
  (候補関数 index のみ)へ変更。直列化レイアウト変更のため
  `CACHE_VERSION` を 43→44 に bump。
- ランタイムの名前パターンフォールバック(#3910)は、焼き込み文字列の
  デシリアライズではなく各候補の `FunctionInfo` から
  `expanded_param_types_for_call` でアリティ別シグネチャを導出する。
  stale キャッシュ互換アーム(collection 型のみの旧候補)は version bump
  により不要となり削除。

### Issue #6336 第3弾: struct_is_subtype_of_abstract の親マップ統合

- `MethodTableProjection` の名前キー親マップ 2 種(`struct_parents` /
  `abstract_parents` — どちらも共有 `StructHierarchy` の制限射影だった)を削除。
  ディスパッチの祖先ウォーク(`struct_is_subtype_of_abstract` と #5605/#5646
  系ヘルパ)は `projection.declared_parent_link(name)` 経由で共有
  `StructHierarchy` を直接参照する(#5614/#5646 の「3 レジストリ」問題の
  method_table スライス解消)。
- 旧マップが持っていた唯一の追加情報「親なし `abstract type` は射影されない
  (subject 位置では保守的 accept)」は `parentless_abstract_names` 集合として
  明示的に保存。`has_parent_links` は旧 `is_empty()` 検査の O(1) 代替。
- ウォークのロジック(保守的 accept / `Any` 打ち切り / builtin 階層フォール
  バック / サイクルガード)は不変。

### Issue #6336 第2弾: type_objects 名前分解の中央パーサ統合 / legacy-array 例外のフラグ化

- `vm/type_objects.rs` のローカル名前分解(`find('{')` / `split_top_level_commas`)
  を削除し、`base_name_without_params` / `parametric_base_name` /
  `split_parametric_name` / `parametric_arg_tokens` / `canonical_typename` を
  type_core の中央トークナイザ `parse_parametric_type_name`(
  `CoreType::from_julia_name` と同一実装)上の `parse_display_type_name` に統合。
  これらは `supertype`/`subtypes`/`TypeName` のリフレクション表示名処理であり
  ディスパッチ経路ではない(レジストリが名前キーのため文字列入出力自体は残る)。
- `base_function_accepts_native_array_value` の名前文字列判定をディスパッチ
  ループから除去: `Vm::new_program` で関数表から一度だけ
  `native_array_exempt_functions: Vec<bool>` を導出し、3 箇所の境界フェンスは
  `is_native_array_exempt_function(idx)` のフラグ参照になった。

### Plots `bar` / `bar!` support (Issue #6358)

- Bundled `Plots` now exports `bar` and `bar!`.
- `bar(y)`, `bar(x, y)`, `bar([(x, y), ...])`, and mutating `bar!` variants
  construct `:bar` Series, reusing the existing Plotly bar trace renderer for
  iOS/Web artifacts.
- Display-only kwargs such as `fillcolor` and `fillalpha` are accepted by the
  shorthand API so common Plots.jl calls run; full per-bar styling remains
  outside this concrete scope.

### dispatch_instr 単一網羅 match 化 (Issue #6343)

- VM の命令ディスパッチが 28 段の `NotHandled` フォールスルーチェーンから、
  全 422 バリアント明示・ワイルドカードなしの単一 match(ジャンプテーブル)になった。
- 中間 enum 21 種を削除し、全ハンドラが `Result<DispatchAction, VmError>` を直接返す。
- 計測: `fib_recursion_25` −5.8%、`vm_mandelbrot/run_only` +1.2%(ホット系は
  #5175 の前方配置で既に低コストだったため中立、コールド命令の固定費は削減)。

### Mixed-width integer `DynamicPow` stack overflow (Issue #6390)

- `DynamicPow` now keeps primitive integer base/exponent pairs on a
  pow-specific inline VM path, so mixed-width calls such as
  `Int8(2)^Int16(3)` no longer dispatch recursively through generic
  `^(::Number, ::Integer)`.
- Runtime integer power preserves the base integer type for nonnegative
  exponents across signed, unsigned, and Bool operands.
- Negative integer exponents now raise catchable `DomainError` instead of
  returning a Float64 fallback.

### Legacy return inference retirement (Issue #6335)

- `infer_function_return_type_v2_with_arg_types` の本番呼び出し元を撤去し、
  HOF / generator / call expression の引数型付き戻り値推論は
  `CoreCompiler::infer_shared_function_return_type_with_arg_types` から
  shared abstract-interp engine の `infer_function_with_arg_types` を使う形へ統一。
- `compile/inference.rs` は shared engine construction adapter に寄せ、legacy
  return-inference helper と旧二層説明を削除。
- `rg 'infer_function_return_type' subset_julia_vm/src` は残存なし。
- Verification: `cargo clippy --all-targets -- -D warnings`;
  `timeout 1800 cargo nextest run --release` (3511 passed, 1 leaky, exit 0).

### メソッド特異性 (diagonal/vararg) 判定のコンパイル時・ランタイム統合 (Issue #6331)

- `compile/method_table.rs` と `vm/mod.rs` にほぼ逐語的に二重実装されていた
  メソッド特異性判定(tuple vararg 展開 / tuple・vector・type-value・
  type-vector・type-matrix diagonal / union-actual dominance / 上限境界・
  コンテナパラメータ解析)を新設の `inference_core/specificity.rs` に一本化。
- 共有コアは `&[JuliaType]` + `&[TypeParam]` を受け取り、`MethodSig` /
  `FunctionInfo` 側は薄いアダプタのみ。`vm/mod.rs` の `runtime_*` 特異性
  ヘルパー(約 50 関数)は削除済み(`rg 'fn runtime_.*diagonal|fn
  runtime_.*vararg' src/vm/mod.rs` は 0 件)。
- 突き合わせで見つかった差分は統合時に解消: コンパイル側は bounded-typevar
  コンテナスロット比較(ランタイム側のみにあった修正)を獲得し、ランタイム側
  の冗長な abstract 除外リスト(`is_concrete()` と等価)と unbounded
  パラメータ名を自分自身の境界として返す quirk を除去。

### Pair expression bare-operator RHS (Issue #6461)

- Lowering now preserves a bare operator node when it is the RHS of `=>`, so
  `:f => +` becomes a Pair whose value is the `+` function.
- The `=>` binary-expression lowering path selects operands adjacent to the
  actual Pair operator before applying the generic operator-child filter.
- Abstract inference now resolves Pair expression return metadata to the real
  `Pair` struct type id, so functions returning `:g => *` keep field access
  usable at call sites.

### `IOContext(io, context)` property inheritance (Issue #6467)

- Direct `IOContext(io, existing_ctx)` construction now inherits properties from
  the source context instead of treating the wrapper itself as the property
  collection.
- Property lookup normalizes the implicit-constructor storage shape for existing
  contexts, matching upstream `get` / `haskey` behavior.

### Frame typed-slot sidecar removal (Issue #6344)

- 関数呼び出しフレームの型別 sidecar `Vec` 19本を削除し、`locals_slots` 1本に
  集約。型別アクセスは `Frame::slot_*` アクセサ(`locals_slots` の match)経由。
- call-heavy ベンチで `fib_recursion_25` −9.3% / `recursive_calls_depth10` −6.9%。

### Empty `IOContext(io)` constructor (Issue #6468)

- Added the one-argument `IOContext(io)` constructor for empty property contexts.
- `IOContext(existing_ctx)` now returns the same context object, matching
  upstream Julia's idempotent constructor behavior.
- `IOContext(io, ctx)` context inheritance remains tracked separately by
  Issue #6467.

### IOContext get/haskey fixture Julia parity (Issue #6408)

- `iocontext_get_haskey.jl` now uses upstream-compatible `IOContext(...)`
  constructors instead of the sjulia-only `iocontext(...)` helper.
- The fixture is directly verifiable with both upstream Julia and sjulia after
  the direct Pair constructor support from Issue #6409.

### Direct `IOContext` pair constructors (Issue #6409)

- `IOContext(io, :key => value)` now normalizes the single stored `Pair` as a
  one-entry property collection, so `get` and `haskey` match upstream Julia.
- Added direct multi-pair constructor overloads used by
  `IOContext(io, :compact => true, :limit => true)`.
- The remaining empty `IOContext(io)` and context-inheritance
  `IOContext(io, ctx)` constructor gaps are tracked separately by Issues #6468
  and #6467.

### Contextual `outer` for-loop variable (Issue #6414)

- The parser now treats `outer` as the optional loop modifier only when another
  binding follows, so `for outer in itr` uses `outer` as a normal variable.
- Existing `for outer i in itr` modifier parsing remains accepted; full
  modifier semantics are tracked separately by Issue #6465.
- Added parser and control-flow fixture coverage for the variable form.
### ウォーム起動コンパイルオーバーヘッド削減 ~135ms → ~70ms (Issue #6348)

- 1 行スクリプト `println(1+1)` のウォーム起動(永続 Base キャッシュ有効)を
  実測 ~135ms → **~65-70ms** に短縮(フェーズ1 受け入れ条件 150ms を大幅クリア)。
- 主な対策:
  - **メソッドテーブル射影の共有** (~37ms 削減): `set_struct_hierarchy_projection`
    が 1100+ テーブルごとに階層 clone + 射影 map を再構築していたのを、
    `MethodTableProjection` を 1 回だけ構築し全テーブルで `Arc` 共有する方式に変更。
  - **ir_opt の Base 全 IR clone 除去** (~5ms): `optimize_pure_expressions_user_only`
    が Base 4577 関数を `extend_from_slice` で deep clone していたのを、
    ユーザー関数・モジュール・main だけを返す `UserSegmentOptimized` に変更。
  - **prelude SHA-256 のメモ化** (~7-10ms): `compute_prelude_hash` /
    `compute_prelude_source_hash` が毎回マルチ MB の prelude 文字列を再構築・再ハッシュ
    していたのをプロセスごとに 1 回へ。
  - **PROGRAM_CACHE の store を 2 回目以降に遅延** (~6ms): ワンショット CLI 実行が
    `CompiledProgram` の deep clone を払わないよう、同一ハッシュの 2 回目の
    コンパイル時にのみ store(3 回目以降はフルヒット)。
  - フェーズ0 計測の拡充: `pipeline.parse_user` / `pipeline.prelude_program_load` /
    `pipeline.merge_prelude`(即時出力)、`compile.struct_tables_build` /
    `compile.method_table_hierarchy_projection` / `compile.inner_ctors_collect` 等を追加。
- シリアライズ形式は不変(`#[serde(skip)]` フィールドの置換のみ)のため
  キャッシュバージョンのバンプは不要。
- フェーズ2(2 セグメントリンク、目標 ≤50ms)は未了。残りの主な内訳:
  prelude Program ロード ~10ms / Base キャッシュ deserialize ~9ms /
  prelude マージ clone ~6ms / 推論エンジン用 Function clone ~4ms。

### `-e` semicolon-separated bare operator statements (Issue #6394)

- The parser now treats a bare operator followed by a statement or delimiter
  boundary as a first-class operator value, so `f = +; f(1, 2)` parses like
  upstream Julia.
- Unary operator expressions still parse when an operand follows, e.g. `+ 1`.
- The `sjulia -e 'f = +; println(f(1,2)); println(reduce(+, [1,2,3]));
  println(foldl(+, [1,2,3]))'` reproducer now prints `3`, `6`, `6`.

### Plots `heatmap` support (Issue #6360)

- Bundled `Plots` now exports `heatmap` and `heatmap!`.
- `heatmap(z)` uses column and row indices as x/y coordinates, while
  `heatmap(x, y, z)` preserves explicit axes. Both construct `:heatmap` series
  with matrix z values using the same row=y, col=x orientation as `surface`.
- Plotly artifact generation renders `:heatmap` as a 2D `"type":"heatmap"`
  trace and keeps `aspect_ratio` axis locking in the 2D layout.

### Plots histogram `weights(...)` wrapper and bar rendering (Issue #6451)

- Bundled `Plots` now exports a lightweight `weights(w)` helper so the
  documented `histogram(data; bins=..., weights=weights([...]))` form works in
  sjulia.
- `histogram` / `histogram!` continue to bin x-only data into `:bar` series, and
  Plotly artifacts render weighted histograms as `"type":"bar"` traces with the
  weighted bin counts preserved.

### Plots `aspect_ratio` keyword (Issue #6353)

- Bundled `Plots` が `plot(sin, aspect_ratio=:equal)` を受け付け、`Plot` 値に
  subplot-level `aspect_ratio` 属性を保持するようになった。
- 本家 Plots.jl に合わせ、`aspectratio` / `axis_ratio` / `axisratio` / `ratio`
  aliases と `:auto` / `:none` / `:equal` / numeric values を扱う。
- Plotly artifact では 2D layout の `yaxis` に `scaleanchor:"x"` と
  `scaleratio` を出し、`:equal` は unit aspect ratio として描画される。
  3D artifact では fixed aspect 指定を Plotly `scene.aspectmode:"data"` に反映する。

### VM eval-breaker-style boundary checks (Issue #6342)

- `run()` / `run_until_frame_return_inner()` の loop top から毎命令の
  `cancel::is_requested()` atomic load と `frames.len() > MAX_CALL_DEPTH`
  比較を外した。
- cancellation は backward jump と call-frame push の境界で確認する。cancel flag は
  他データの同期に使っていない単一 atomic bool なので `Relaxed` ordering にした。
- call-depth overflow は call-frame push 時に pending flag だけを立て、call/return/print/HOF
  handler が callee `ip` を設定し終えた直後に `StackOverflowError` として raise する。
  これにより catch handler の `ip` を call setup が上書きしない。
- generated/eval の一時 push/pop frame は `try_push_temporary_call_frame` に分け、
  pending overflow が通常 VM dispatch loop に漏れないようにした。
- VM-only Criterion: `vm_mandelbrot/run_only` は main `37.467ms` から `36.471ms`
  (median, 約 `2.7%` 改善)。`vm_calc_pi_large/base_gcd_run_only/1000` は main
  `3.9518s`、after `3.9512s` で実質 neutral。

### CompiledProgram Base cache decode profiling and specialization IR omission (Issue #6449)

- Base cache 内の `CompiledProgram` payload をさらに sub-section 化し、
  `code` / `functions` / `struct_defs` / `abstract_types` / `show_methods` /
  `specializable_functions` / global slot metadata を個別に
  `SJULIA_COMPILE_PROFILE=1` で測れるようにした。
- section 化直後の embedded-cache profile では `compiled.code` が `~13.9-14.3ms`
  / `3.83MB`、`compiled.specializable_functions` が `~8.9-10.2ms` / `2.91MB`、
  `compiled.functions` が `~5.2-5.6ms` / `1.11MB` だった。
- persistent/embedded Base cache では `specializable_functions` を保存しない。
  cached Base warm compile は prelude/user Program から specialization registrations
  を再構築して cached `CallSpecialize` index alignment を保つため、cached Base
  `CompiledProgram` 側の specialization IR は decode しても使われない。
- embedded prelude/Base cache 付き `sjulia -e 'println(1+1)'` では、Base cache が
  `8.58MB` → `5.66MB`、`compiled.specializable_functions` section が `2.91MB` →
  `8 bytes`、`cache.deserialize.compiled` が `~28-30ms` → `~18.7-20.7ms`、
  `cache.get_or_init_base_cache` が `~39-41ms` → `~29.8-31.8ms` へ下がった。
- 同じ 3-run profile の CLI wall は `0.34-0.37s` で、主な残り decode cost は
  `compiled.code` (`~13-15ms`) と `compiled.functions` (`~5-6ms`)。

### Base cache section decode profiling and method-table payload trim (Issue #6440)

- embedded/persistent Base cache の外側 format を section envelope にし、各 section
  (`compiled`, `method_tables`, `closure_captures`, `promotion_rules`,
  `inference_results`) は従来どおり bincode payload として保持する。これにより
  `SJULIA_COMPILE_PROFILE=1` で Base cache decode の支配構造を直接測れるようにした。
- `MethodTable` の `struct_parents` / `abstract_parents` は compile setup の
  `set_struct_hierarchy_projection()` で毎回再構築され、cached warm path でも
  `clone_for_reprojection()` が捨てる per-table projection map なので、serialized
  Base cache から除外した。
- embedded prelude/Base cache 付き `sjulia -e 'println(1+1)'` では、section 化直後の
  profile で `cache.deserialize.compiled` が `~28-30ms`、
  `cache.deserialize.method_tables` が `~31-33ms` と判明した。projection map 省略後は
  Base cache が `13.6MB` → `8.58MB`、`method_tables` section が `5.73MB` →
  `0.70MB`、`cache.deserialize.method_tables` が `~31-33ms` → `~4.2-4.6ms`、
  `cache.get_or_init_base_cache` が `~65-69ms` → `~39-41ms` へ下がった。
- 同じ測定で CLI wall は `0.40-0.42s` → `0.34-0.37s`。この時点で残る大きい
  decode cost は `CompiledProgram` section (`~28-30ms`, `7.87MB`) 側に移り、
  #6449 でさらに内訳化・削減した。

### Warm-start compile overhead profiling and cached-prefix peephole skip (Issue #6348)

- `SJULIA_COMPILE_PROFILE=1` + `--features profiling` で
  `compile_with_cache` / `compile_core_program_internal` の warm path phase
  timing を stderr に出せるようにした。通常 build では no-op。
- `ir_inline` は inline candidate が無い場合に merged Program を clone せず借用のまま返し、
  candidate がある場合も Base function slice は変換対象から外す。
- `ir_opt` は Base functions を走査・再最適化せず、user functions / modules / main にだけ
  pure-expression pass を適用する。
- cached Base bytecode が code prefix として保護されている場合、peephole optimizer は
  protected prefix を再走査せず、追加された user/main suffix だけを最適化する。
- shared inference engine の構築では、一度 clone した `Function` vec を owned のまま
  engine に登録し、engine 側の追加 clone を避ける。
- cached Base warm path では、top-level Base function の parametric parameter /
  struct literal scan を cached instantiation table に任せ、user/module/nested function
  だけを再走査する。nested Base function alignment と closure metadata のため、
  inline function collection 自体は安全側で維持する。
- nested closure capture prepass は parent function lookup を毎回全探索せず、
  first-match parent parameter map を一度だけ作る。
- cached method tables は `Arc<Vec<MethodSig>>` で method list を共有し、
  cached Base warm path では hierarchy projection を再構築する前提の
  `clone_for_reprojection` を使う。これにより、Base method signatures と
  古い projection maps を毎回深く clone しない。
- Base cache load profile に `cache.deserialize_body` / `cache.compute_prelude_hash` /
  `cache.restore_base_compile_context` などの sub-phase を追加し、残る load cost が
  embedded cache の bincode decode に集中していることを見える化した。
- persistent/embedded Base cache には inference return snapshot を保存しないようにした。
  cached Base functions は warm path で skip される一方、巨大な seeded return cache は
  user method 追加時の invalidation cost を増やしていたため、同一 process の
  source-compiled Base cache hits だけが in-memory snapshot を保持する。
- `compile.seed_inference_results` profile label を追加し、persisted snapshot replay が
  発生しているかを warm profile 上で確認できるようにした。
- cached Base bytecode prefix は user functions / main suffix の emit・slotize・peephole
  が終わるまで mutable code vector に入れず、最後に1回だけ `CompiledProgram.code` へ
  assemble するようにした。これにより protected-range peephole は warm cached path から
  外れ、Base function metadata も suffix peephole の index mapping から明示的に除外する。
- embedded prelude/Base cache 付き `sjulia -e 'println(1+1)'` は before `0.85-0.90s`
  から after `0.51-0.55s`。profile 上は `compile.peephole_pre_slotize`
  `142.677ms` → `12.527ms`、`compile.peephole_post_slotize`
  `138.460ms` → `8.434ms`。その後の method/inference setup trim で
  `compile_core_program_internal` は `284.789ms` → `258.711ms`、
  `compile.build_inference_engine` は `~11ms` → `3.378ms`、
  `compile.method_table_setup` は `~75ms` → `58.053ms`。method table COW 後は
  `compile.cached_method_tables_clone` が `25-30ms` → `~0.8ms`、
  `compile_core_program_internal` が `260-270ms` 台 → `230-241ms`、
  CLI wall が `0.51-0.55s` → `0.46-0.52s`。persisted inference snapshot 省略後は
  Base cache が `15.3MB` → `13.6MB`、`cache.deserialize_body` が `~72-74ms` →
  `~65-66ms`、`compile.method_table_setup` が `~57-61ms` → `~31-32ms`、
  `compile_core_program_internal` が `230-246ms` → `192-199ms`、
  CLI wall が `0.46-0.52s` → `0.44-0.48s`。cached prefix assemble 化後は
  `compile.peephole_pre_slotize` / `compile.peephole_post_slotize` が
  `~12ms` / `~8ms` から `0.08-0.11ms` / `0.05-0.11ms` へ下がり、
  `cache.compile_core_program_internal` は `168-173ms`、CLI wall は `0.40-0.42s`。
- #6348 時点で残る主な warm compile cost は embedded Base cache bincode decode
  (`~65-66ms`)、inference function clone (`~17ms`)、最終 cached prefix assemble
  (`~11-12ms`) だった。decode 内訳と method-table payload 削減は #6440 で継続。

### Resolved/direct I64 slot-call fusion (Issue #6315)

- Peephole optimizer が `LoadSlotI64(arg)...; CallResolved(func, argc)` と
  `LoadSlotI64(arg)...; CallInbounds(func, argc)` を、それぞれ
  `CallResolvedI64Slots` / `CallInboundsI64Slots` に畳み、resolved/direct call
  sites でも `CallSpecializeI64Slots` と同じく I64 slot sidecar から引数を直接読める
  ようにした。
- VM executor は I64 slot sidecar が揃う場合、stack materialization なしで既存の
  Euclidean modulo loop / generic `I64Function` direct path を試す。shape miss や
  sidecar miss では `LoadSlotI64` 相当の value 読みと通常 direct-call frame path へ戻る。
- profiling に露出していた `ExecutableBlock::GcdI64*` 名は、API 名ではなく認識している
  loop shape を表す `ExecutableBlock::EuclideanModuloI64*` に rename した。Base `gcd`
  の `abs` prefix まで専用認識するような追加の gcd 特化は入れていない。
- VM-only Criterion `vm_calc_pi_large/base_gcd_run_only/1000` は開始時 `396.96ms`
  から `363.76ms`。同じ current run の user `mygcd` は `303.98ms`。
- embedded prelude/Base cache 付き CLI `calc_pi(1000)` は user `mygcd` `1.23s`、
  Base `gcd` `1.27s`。CLI 値は parse/lower/user bytecode compile を含むため
  VM-only とは分けて扱う。
- Regression: Base `gcd` caller bytecode が `CallResolvedI64Slots` を使うこと、
  non-gcd `score6315(i, step)` helper も slot direct-call fusion に乗ることを検証する。

### Generalized resolved-call I64 function blocks (Issue #6314)

- generic `I64Function` decoder が、Base `abs(::I64)::I64` の固定 unary op だけでなく、
  shape guard を通った小さな resolved/direct I64 callee を nested `I64Function`
  block として保持し、frame 作成なしで呼べるようにした。miss 時は従来どおり
  normal frame execution へ戻る。
- Guard は conservative に、非 generated、非 vararg、keyword なし、type parameter
  なし、全 positional parameter が `I64`、戻り値 `I64`、decode 可能 opcode、再帰深さ
  上限、callee 数上限を満たす関数に限定する。Base `abs` の `AbsI64` fast op は
  既存 `gcd` 経路保護のため残す。
- `I64Function` に `LoadAddI64Slot` / `LoadSubI64Slot` / `LoadMulI64Slot` を追加し、
  peephole 済みの小さな integer helper (`x * x + 1` など) を表現できるようにした。
- cached `I64FunctionBlock` は per-call clone せず参照実行するようにし、nested callee
  list 追加後も既存 hot path の overhead を抑えた。
- VM-only Criterion: baseline `vm_calc_pi/base_gcd_run_only/500` `105.94ms`
  から current 再測定 `102.78ms`。`vm_i64_function_calls/run_only/20000` は
  baseline `26.72ms` から `25.98ms`。non-gcd nested helper
  `nested_resolved_helper_run_only/20000` は `10.40ms`。
- embedded prelude/Base cache 付き CLI `calc_pi(1000)` は mygcd 版 `1.21s` のまま、
  Base `gcd` 版は `1.29s` → `1.26s`。
- Regression: `i64_resolved_call_6314_tests` が non-gcd `score6314(i)` helper を
  nested `I64Function` block として実行し、結果 `2890` と
  `ExecutableBlock::I64FunctionNestedCall` 発火を検証する。

### Persisted program file modules split by format (Issue #6328)

- 公開 `bytecode` module を廃止し、persisted Core IR `.sjir` は
  `core_ir_file`、VM bytecode `.sjvmbc` は `vm_bytecode_file` へ分割した。
- Core IR 側の public 型も `CoreIrFileError` / `CoreIrFileFlags` /
  `CoreIrFileHeader` に rename し、VM 側は `VmBytecodeFileError` を公開する。
- CLI / AoT / tests は明示 module を import するよう更新した。iOS 接続面の
  C ABI / FFI (`compile_to_ir`, `run_ir_json_*`, `compile_and_run_*`) は変更しない。

### Core IR AoT test naming cleanup (Issue #6327)

- 歴史的な AoT Core IR integration test file を `core_ir_aot_tests.rs` に rename し、
  persisted Core IR `.sjir` と AoT 変換を扱うテストであることを明確にした。
- テスト関数名の historical bytecode prefix を `test_core_ir_*` / `test_sjir_*` に整理し、
  `.sjir` file format と Core IR roundtrip の用語を分離した。
- Makefile の narrow target も `test-core-ir-aot` に rename した。公開 file-format
  module 名の整理は Issue #6328 で別途扱う。

### AoT Core IR file conversion filters unreachable keyword sentinels (Issue #6324)

- `ir_file_to_aot_ir()` が persisted Core IR 全体をそのまま AoT IR へ変換していたため、
  到達不能な prelude 関数内の body-evaluated keyword default guard
  (`kw === Undef`) まで `Literal::Undef` として変換し、`unsupported literal kind`
  で失敗していた。
- `Literal::Undef` を `nothing` などの実行値へ変換せず、AoT CLI と同じ
  `CallGraph::filter_program()` を file helper にも適用して、到達可能な Core IR
  surface だけを AoT IR 化する。
- Regression: `core_ir_aot_tests` に到達不能な `Undef` keyword default guard を含む
  `.sjir` 入力を追加し、`--features aot` 付きの Core IR→AoT IR 変換が通ることを確認する。

### AoT Core IR API naming cleanup (Issue #6323)

- AoT analyze の公開 Rust API 名から historical な bytecode 用語を外し、
  `load_bytecode_file` / `load_bytecode_bytes` / `bytecode_file_to_aot_ir` を
  `load_ir_file` / `load_ir_bytes` / `ir_file_to_aot_ir` に破壊的 rename した。
- `BytecodeAnalyzer` は `CoreIrAnalyzer` に rename し、module file も
  `core_ir_analyzer.rs` に移した。互換 wrapper は残さない。
- 未実装 stub `compile_from_bytecode` も `compile_from_ir_bytes` に rename した。
- iOS アプリが使う C ABI / FFI (`compile_and_run_detailed`, `run_ir_json_*`,
  `compile_to_ir` など) は変更していない。

### Persisted Core IR `.sjir` rename (Issue #6322)

- `sjulia --compile` の保存形式を `.sjbc` から `.sjir` に改名し、default 出力も
  `<stem>.sjir` に変更した。Core IR 保存形式の magic bytes は `"SJIR"` になり、
  旧 `.sjbc` は互換 alias として扱わない。
- 明示実行 CLI は `--run-bytecode <file.sjbc>` ではなく `--run-ir <file.sjir>` に変更した。
  拡張子自動実行も `.sjir` のみ Core IR として扱う。
- AoT CLI の Core IR 入力も `aot --ir program.sjir` に寄せ、`.sjir` を generic
  bytecode と呼ばないよう help/docs/tests を更新した。公開 Rust API 名の
  Core IR 用語化は #6323 で扱う。
- Regression: `sjulia_cli_vm_bytecode_tests` が `.sjir` の compile、`--run-ir` 実行、
  拡張子自動実行、および `.sjbc` default 非生成を検証する。

### sjulia VM bytecode CLI execution path (Issue #6317)

- `sjulia --compile-vm <file.jl> -o <file.sjvmbc>` を追加し、source を
  parse/lower/compile した後の `CompiledProgram` を永続化できるようにした。
  既存の `--compile` / `.sjir` は Core IR 保存のまま維持する。
- `sjulia --run-vm-bytecode <file.sjvmbc>` と `<file.sjvmbc>` 拡張子の自動実行を追加した。
  実行時は source parse/lower と VM bytecode compile を通らず、保存済み
  `CompiledProgram` を直接 `Vm::run()` に渡す。
- `CompiledProgram.compile_context` は serde で保存しないため、`.sjvmbc` payload には元の
  `Program` も同梱し、ロード時に specializable function 用の runtime compile context を
  復元する。これにより `CallSpecialize*` の既存 runtime specialization を維持する。
- embedded prelude/Base cache 付き CLI `benchmarks/calc_pi_benchmark.jl` は、source run
  `1.41s`、IR `.sjir` run `1.24s`、VM `.sjvmbc` run `0.47s`。`.sjvmbc` ファイルサイズは
  `14M`、IR `.sjir` は `6.0M`。
- Regression: `sjulia_cli_vm_bytecode_tests` が `--compile-vm`、`--run-vm-bytecode`、
  拡張子自動実行の roundtrip を subprocess で検証する。

## 最新対応 (2026-06-10)

### Base gcd resolved-call I64 function blocks (Issue #6312)

- Base `gcd(::Int64, ::Int64)` は caller 側で
  `LoadSlotI64(a); LoadSlotI64(b); CallResolved(gcd, 2)` になり、
  user `mygcd` の `CallSpecializeI64Slots` fast path を通らず、callee frame と
  `ReturnI64` を gcd call ごとに払っていた。
- simple direct `Call` / `CallResolved` callee が I64 stack arguments を受け取り、
  既存の gcd / generic `I64Function` executable block で表現できる場合に、
  frame 作成前に direct 実行する fast path を追加した。miss 時は従来の
  direct frame path に戻る。
- generic `I64Function` block は Base/prelude 由来かつ signature が
  `abs(::I64)::I64` の unary call だけを `AbsI64` op として扱う。これにより
  Base `gcd` 先頭の `abs(a)` / `abs(b)` prefix を decode できるが、ユーザー定義
  `abs` には適用しない。
- direct I64 result 後の `PushI64; JumpIfEqI64/JumpIfNeI64` fused compare branch も
  `try_consume_i64_eq_branch` で消費するようにした。
- profile 付き release CLI `calc_pi(50)` は `123,683` → `21,183` instructions。
  `CallDirectFastHit` / `ReturnI64` は hot path から消え、
  `CallDirectFastI64FunctionHit` と `ExecutableBlock::I64Function` が 2,500 回発火する。
- embedded prelude/Base cache 付き CLI `calc_pi(1000)` は Base `gcd` 版で
  `4.40s` → `1.35s`。user `mygcd` 版は `1.24s` で退行なし。
- VM-only Criterion に Base `gcd` calc_pi cases を追加した。current は
  `vm_calc_pi/base_gcd_run_only/100` `13.436ms..13.733ms`,
  `base_gcd_run_only/500` `104.59ms..105.63ms`。
- Broader resolved-call I64 block coverage was resolved in #6314. The smaller
  residual Base `gcd` vs user `mygcd` delta remains tracked in #6315.

### Generic direct I64 specialized-function blocks (Issue #6308)

- cache hit 済みの simple runtime-specialized function が、local I64 slot 操作・
  I64 arithmetic/comparison・branch・`ReturnI64` だけで表現できる場合に、
  call frame を作らず direct に実行する汎用 `I64Function` executable block を追加した。
  gcd 関数名や特定ユーザー関数には依存しない。
- Guard は conservative に、global/heap/call/keyword/vararg/generated/type-param を含まない
  specialized bytecode に限定する。未対応 opcode、未初期化 slot、zero division などは
  従来の frame 実行へ戻る。既存の gcd 専用 fast path は先に試すため `calc_pi` の
  Euclid loop は現行の最短経路を維持する。
- decode 結果は specialized entry IP ごとに `Vm` 内で cache し、未対応関数の miss も
  cache する。これにより `CallSpecializeI64Slots` hot path で毎回 pattern scan しない。
- 非 gcd regression `advance(i, step)` / `sum_pairs(100000)` の profile 付き release CLI
  aggregate は `900,226` → `700,226` instructions。per-call の `ReturnI64` と
  callee-side `ExecutableBlock::TypedLoop` dispatch が消え、代わりに
  `ExecutableBlock::I64Function` が発火する。
- VM-only Criterion に `vm_i64_function_calls` group を追加した。#6307 baseline と current
  の中央値は `run_only/20000` `35.46ms` → `26.98ms`,
  `clone_new_program_run/20000` `57.91ms` → `49.25ms`。
- `calc_pi(500)` の profile 付き release CLI aggregate は `1,308,198` instructions のまま。
  current Criterion は `vm_calc_pi/run_only/500` median `86.93ms`,
  `clone_new_program_run/500` median `109.96ms` で退行なし。
- Regression: `scalar_hot_loop_6167_tests` は non-gcd `advance(i, step)` が
  `ExecutableBlock::I64Function` direct path を使い、結果 `250` を保つことを
  profiling feature 付きで検証する。

### Cached I64 slot specialized-call fast path (Issue #6301)

- `CallSpecializeI64Slots` が全引数を `slot_i64` sidecar から読める場合、
  specialization cache hit 後の hot path で `Vec<Value>` 引数列、runtime
  `ValueType` 列、巨大な `FunctionInfo` clone を毎回組み直さず、cached
  specialized entry へ直接入る fast path を追加した。
- Guard は conservative に、cache hit 済み・非 generated・非 vararg・keyword
  なし・type parameter なし・param slot 数一致の simple specialized call に限定する。
  sidecar が欠ける場合や複雑な関数形は従来の `execute_call_specialize_with_args`
  経路へ戻る。
- 既存の gcd executable fast path は `Value` 引数版と `i64` 引数版で共通の
  `execute_gcd_i64_values` helper を使うよう整理した。これは gcd 固有 pattern を
  追加する変更ではなく、I64 slot specialized call 全般の cached call entry を
  軽くする変更。
- `calc_pi(500)` の profile 付き release CLI aggregate は `1,308,198`
  instructions のまま変わらない。今回の改善は bytecode dispatch 数ではなく、
  `CallSpecializeI64Slots` 1 命令の内部 allocation / metadata rebuild を避ける
  VM-only runtime 改善。
- VM-only Criterion (`cargo bench -p subset_julia_vm --bench calc_pi_benchmark -- --sample-size 10 --measurement-time 3 --warm-up-time 1`) は、
  main 再測定から `vm_calc_pi/run_only/500` median `484.13ms` → `88.08ms`、
  `clone_new_program_run/500` median `503.82ms` → `108.43ms`。
  `run_only/100` は `37.70ms` → `19.91ms`、`clone_new_program_run/100` は
  `64.04ms` → `42.08ms`。
- Regression: `scalar_hot_loop_6167_tests` に gcd ではない `advance(i, step)` の
  `CallSpecializeI64Slots` regression を追加し、cached I64 slot call path が
  generic user function result を保つことを固定した。

### Positive const-step counted loop backedge fusion (Issue #6305)

- 正の定数 step を持つ `Int64` counted loop の backedge を、
  `AddConstI64Slot(slot, delta); Jump(header)` から
  `AddConstI64SlotAndJumpIfLe(slot, delta, stop_slot, body)` へ fusion する
  VM superinstruction を追加した。`delta > 0` の一般形を扱い、`+1` 専用や
  `calc_pi` / `mygcd` 固有の pattern にはしていない。
- Guard は loop header が
  `JumpIfGtI64Slots(loop_slot, stop_slot, fallthrough_exit)` または peephole 前の
  `LoadSlotI64(loop_slot); LoadSlotI64(stop_slot); JumpIfGtI64(fallthrough_exit)`
  で、tail の fallthrough が header exit と一致する場合に限定する。
  初回の empty-range check は header branch として残し、hot backedge だけを
  fused 命令に置き換える。
- VM 実行側は通常の typed slot case で `slot_i64` sidecar を直接使い、
  error path 以外では slot name allocation や汎用 setter を避ける。
- `calc_pi(500)` の profile 付き release CLI aggregate は
  `1,809,198` → `1,308,198` instructions (約 27.7% 減)。
  `JumpIfGtI64Slots` は `251,001` → `501`、loop-tail の
  `AddConstI64SlotAndJumpIfLe` は `250,500` 回実行された。
- VM-only Criterion (`cargo bench -p subset_julia_vm --bench calc_pi_benchmark -- --sample-size 10 --measurement-time 3 --warm-up-time 1`) は
  `vm_calc_pi/run_only/500` median `511.19ms` 近辺 → `462.31ms`,
  `clone_new_program_run/500` median `514.35ms` 近辺 → `494.74ms`。
  Criterion は両方を performance improved と判定した。
- Regression: peephole unit tests が `+1` と `+2` の positive const-step fusion、
  fallthrough-exit guard、negative step no-fusion を固定する。
  `const_step_for_loop_5166_tests` は `for i in 1:2:n` が delta=2 の fused
  backedge を使うことを検証する。

### Mandelbrot loop slot branch / Float64 slot conversion fusion (Issues #6167, #6253)

- #6300 後の `benchmarks/vm_mandelbrot.jl` では `mandel_count` の x/y loop exit に
  `LoadSlotI64(lhs); LoadSlotI64(rhs); JumpIfGtI64` が残り、さらに
  `Float64(x)`, `Float64(width)`, `Float64(y)`, `Float64(height)` が
  `LoadSlotI64(slot); CallBuiltin(Float64, 1)` として hot path に残っていた。
- slot-to-slot branch fusion の guard を、`CallSpecialize` body 限定から
  「forward loop body が lhs slot を increment/decrement する」条件へ広げた。
  これにより Mandelbrot の direct-call loop でも `JumpIfGtI64Slots` を使うが、
  任意の if branch には広げない。
- 新 bytecode `LoadSlotI64ToF64(slot)` を追加し、
  `LoadSlotI64(slot); CallBuiltin(Float64, 1)` を fusion する。VM 実行側は
  既存 `LoadSlotI64` と同じ numeric slot 値だけを読み、`Float64(x)` と同じ
  `convert_to_f64` 経路で `Value::F64` を push する。`convert(Int64, x)` は
  method lookup semantics を持つため今回の peephole では消さない。
- `benchmarks/vm_mandelbrot.jl` の profile 付き release CLI aggregate は
  main baseline から `337,837` → `298,955` instructions (約 11.5% 減)。
  `LoadSlotI64` は `67,684` → `28,802`、`CallBuiltin` は `29,011` → `9,651`。
  `calc_pi` profile aggregate は `9,108,184` のまま変わらず、対象外 hot loop に
  regression は見えない。
- VM-only Criterion (`cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark -- --sample-size 10 --measurement-time 3 --warm-up-time 1`) は
  `run_only` median `38.226ms` 近辺 → `35.639ms`,
  `clone_new_program_run` median `58.937ms` 近辺 → `56.636ms`。Criterion は
  `run_only` / `clone_new_program_run` とも performance improved と判定した。
- Regression: `mandelbrot_6259_tests` が `mandel_count` の x/y loop exit に
  `JumpIfGtI64Slots` を使うこと、`Float64(slot)` が `LoadSlotI64ToF64` へ
  fusion されること、結果が `166265` のまま保たれることを検証する。
  peephole unit tests は loop-slot-update guard、unfused increment pattern、
  `LoadSlotI64ToF64` を固定する。

### calc_pi slot-argument specialized-call fusion (Issue #6167)

- #6299 後の `calc_pi` hot loop では loop exit の slot-to-slot branch は
  fused されたが、`LoadSlotI64(a); LoadSlotI64(b); CallSpecialize(mygcd, 2)`
  が `N^2` 規模で残り、slot 値を specialized call 引数に渡すためだけに
  stack materialization していた。
- peephole optimizer に、連続する
  `LoadSlotI64(arg)...; CallSpecialize(func, argc)` /
  `CallSpecializeInbounds(func, argc)` を
  `CallSpecializeI64Slots(func, slots)` /
  `CallSpecializeInboundsI64Slots(func, slots)` へ畳む fusion を追加した。
  関数名には依存せず、argc と直前の `LoadSlotI64` 列だけで guard する。
- VM 実行側は slot から既存 `LoadSlotI64` と同じ numeric 値を読み、通常の
  `CallSpecialize` と同じ helper に渡す。特殊化 cache、fallback frame binding、
  generated function handling、`@inbounds` context は既存経路と共有する。
- `benchmarks/calc_pi_benchmark.jl` は未変更。profile 付き release CLI aggregate は
  #6299 baseline から `11,628,184` → `9,108,184` instructions (約 21.7%
  減)。`LoadSlotI64` は `2,520,006` → `6` まで落ち、hot profile top 20 から
  消えた。同 profile run の `@time calc_pi(1000)` は `2.894s` → `2.607s`
  (約 9.9% 短縮)。これは CLI aggregate / VM instruction profile であり、
  VM-only Criterion ではない。
- VM-only Criterion (`cargo bench -p subset_julia_vm --bench calc_pi_benchmark -- --sample-size 10 --measurement-time 3 --warm-up-time 1`) を
  #6299 baseline/current として同じ worktree window で測定した。中央値:
  `run_only/100` `35.764ms` → `35.929ms`,
  `clone_new_program_run/100` `60.517ms` → `58.826ms`,
  `run_only/500` `503.51ms` → `487.27ms`,
  `clone_new_program_run/500` `534.93ms` → `517.22ms`。
- Mandelbrot は `CallSpecializeI64Slots` を出さない。直近の
  `vm_mandelbrot_benchmark` 中央値は `run_only` `37.826ms` → `38.226ms`,
  `clone_new_program_run` `59.378ms` → `58.937ms`。run-only は約 1%
  遅めだが bytecode shape は未変更で、clone 側は同等から微改善。
- Regression: `scalar_hot_loop_6167_tests` が `calc_pi` の specialized call 引数から
  `LoadSlotI64 + LoadSlotI64 + CallSpecialize` が消え、
  `CallSpecializeI64Slots` を使うことを検証する。peephole unit test は argc 一致、
  inbounds variant、余分な stack 値が下にある partial fusion を固定する。

### calc_pi scoped slot-to-slot loop branch fusion (Issue #6167)

- #6298 後も `calc_pi` hot loop の最上位 profile は `LoadSlotI64` で、
  outer/inner loop exit が `LoadSlotI64(var); LoadSlotI64(stop); JumpIfGtI64`
  として `N^2` 規模で stack materialization を続けていた。
- peephole optimizer に
  `LoadSlotI64(lhs); LoadSlotI64(rhs); JumpIfGtI64(target)` →
  `JumpIfGtI64Slots(lhs, rhs, target)` を追加した。VM 実行側は `frame.slot_i64`
  の pair fast path を先に読み、通常の typed slot case では stack push/pop を避ける。
- この fusion は body に `CallSpecialize` / `CallSpecializeInbounds` を含む forward
  exit branch に限定している。Mandelbrot grid loop へ広げると Criterion が不安定に
  なったため、今回の対象は call-specialized scalar hot loop に絞った。
- `benchmarks/calc_pi_benchmark.jl` は未変更。profile 付き release CLI aggregate は
  #6298 後 baseline から `14,154,590` → `11,628,184` instructions (約 17.8%
  減)。`LoadSlotI64` は `5,046,412` → `2,520,006` (約 50.1% 減)。
  同 profile run の `@time calc_pi(1000)` は `3.122s` → `2.876s` (約 7.9%
  短縮)。これは CLI aggregate / VM instruction profile であり、VM-only Criterion
  ではない。
- VM-only Criterion (`cargo bench -p subset_julia_vm --bench calc_pi_benchmark -- --sample-size 10 --measurement-time 3 --warm-up-time 1`) を
  #6298 baseline/current として再測定した。中央値: `run_only/100`
  `35.739ms` → `35.601ms`, `clone_new_program_run/100` `61.345ms` →
  `59.502ms`, `run_only/500` `530.23ms` → `499.75ms`,
  `clone_new_program_run/500` `543.31ms` → `516.56ms`。
- Mandelbrot は scope guard により `JumpIfGtI64Slots` を出さない。直近の
  `vm_mandelbrot_benchmark` 中央値は `run_only` `37.329ms` → `37.337ms`,
  `clone_new_program_run` `57.094ms` → `57.851ms` で、run-only は実質同等。
- Regression: `scalar_hot_loop_6167_tests` が `calc_pi` の loop exit から
  `LoadSlotI64 + LoadSlotI64 + JumpIfGtI64` が消え、inner/outer loop が
  `JumpIfGtI64Slots` を使うことを検証する。peephole unit test は
  `CallSpecialize` を含まない loop body では fusion しないことも固定する。

### calc_pi const-step slot increment fusion (Issue #6167)

- #6293 後の `calc_pi` hot loop では gcd call 自体は direct fast path に乗るが、
  `for` loop の `b += 1` / `a += 1` が `PushI64(1); IncVarI64Slot(slot)` として
  残り、`N^2` 規模で定数 increment 用の stack materialization が発生していた。
- peephole optimizer に `PushI64(k); IncVarI64Slot(slot)` /
  `PushI64(k); DecVarI64Slot(slot)` → `AddConstI64Slot(slot, +/-k)` を追加した。
  既存の `AddConstI64Slot` 実行経路を使うため、新しい VM opcode は追加していない。
- `benchmarks/calc_pi_benchmark.jl` は未変更。profile 付き release CLI aggregate は
  #6293 後 baseline から `15,416,190` → `14,154,590` instructions (約 8.2% 減)。
  同 profile run の `@time calc_pi(1000)` は `3.365s` → `3.147s` (約 6.5%
  短縮)。これは CLI aggregate / VM instruction profile であり、VM-only Criterion
  ではない。
- VM-only Criterion (`cargo bench -p subset_julia_vm --bench calc_pi_benchmark -- --sample-size 10 --measurement-time 3 --warm-up-time 1`) を
  同じ環境で #6293 後 baseline/current として再測定した。中央値: `run_only/100`
  `37.523ms` → `35.526ms`, `clone_new_program_run/100` `61.785ms` →
  `59.559ms`, `run_only/500` `530.84ms` → `524.14ms`,
  `clone_new_program_run/500` `550.21ms` → `520.55ms`。
- Mandelbrot VM-only Criterion も同じ条件で確認した。`vm_mandelbrot/run_only`
  `36.376ms` → `35.219ms`, `clone_new_program_run` `56.368ms` →
  `56.127ms`。Mandelbrot 側の実質 regress は見えていない。
- Regression: `scalar_hot_loop_6167_tests` が `calc_pi` の loop increment から
  `PushI64 + Inc/DecVarI64Slot` が消え、inner/outer loop が
  `AddConstI64Slot` を使うことと結果 parity を検証する。既存
  `const_step_for_loop_5166_tests` は `AddConstI64Slot` を const-step loop の
  valid increment fusion として扱う。

### calc_pi gcd direct call fast path (Issue #6293)

- Coprime π estimation の hot loop は #6167 系の `GcdI64Loop` executable block
  には乗っていたが、各 `mygcd(a,b)` 呼び出しで `CallSpecialize` の frame setup /
  `ReturnI64` / `mygcd(a,b) == 1` の `CallDynamicBinaryBoth(EqFloat, ...)` /
  `JumpIfZero` が `N^2` 回残っていた。
- `CallSpecialize` の specialization cache hit/miss 結果を `SpecializedCode`
  として保持し、specialized bytecode が「`GcdI64Loop` shape + `LoadSlot(a);
  ReturnI64`」で、実引数が `Int64` の場合だけ、frame を作らず Euclid loop を
  直接実行するようにした。関数名ではなく bytecode shape と parameter slot を見る。
  `typemin(Int64) % -1` など通常 VM の例外処理に任せるべきケースは既存経路に戻る。
- 直後の bytecode が `PushI64(const); Eq/Ne; JumpIfZero` の branch-context
  comparison なら、direct call の `Int64` 結果で branch まで消費する。式値が必要な
  shape では従来どおり stack に値を戻して fallback する。
- `benchmarks/calc_pi_benchmark.jl` は未変更。profile 付き release CLI aggregate で
  `21,716,190` → `15,416,190` instructions (約 29.0% 減)。同じ profile run の
  `@time calc_pi(1000)` は `4.758s` → `3.240s` (約 31.9% 短縮)、process real は
  `16.76s` → `14.72s` (約 12.2% 短縮)。これは CLI aggregate / VM instruction
  profile であり、VM-only Criterion ではない。
- Regression: `calc_pi_6293_tests`。profiling feature では
  `ExecutableBlock::GcdI64Function` と
  `ExecutableBlock::I64FunctionCompareBranch` が発火することを検証する。

### Mandelbrot Complex runtime specialization (Issue #6259)

- `mandelbrot_escape(c::Any, maxiter::Any)` のように broadcast から erased
  型で呼ばれる関数でも、runtime の実引数が `Complex{Float64}` / `Int64` で安定して
  いる場合に lazy specialization へ入れるようにした。`ValueType` に
  `ComplexF32` / `ComplexF64` タグを追加し、Complex struct / array 要素から
  concrete Complex 型を復元する。
- specializer は `abs2(::ComplexF64)`、`z^2`、`ComplexF64` 同士の `+`/`-`/`*`、
  `a + b*im` / `a - b*im` を Float64 field arithmetic に展開する。escape loop の
  specialized code は `DynamicPow` / `CallDynamicBinaryBoth` / `CallResolved(abs2)` を
  出さず、`GetField` + `MulF64` / `AddF64` と既存の `NewParametricStruct("Complex",2)`
  で Complex を再構成する。
- `CallFunctionVariable` / runtime callable / HOF frame path が、dispatch で選ばれた
  fallback 関数と実引数値から同じ specialization cache を引くようになった。これにより
  `Base._broadcast_apply` の `f(args...)` 経路でも、最初の要素で compile した
  `mandelbrot_escape(::ComplexF64, ::Int64)` を以後の要素で再利用する。
- `benchmarks/mandelbrot_benchmark.jl` は未変更。VM profile は CLI aggregate で
  `3,162,141` → `2,608,939` instructions。通常 release `sjulia` の `@time`
  3-run 平均は `3.31s` → `1.48s`。これは CLI 全体値であり VM-only Criterion
  ではない。Mandelbrot 出力は同一。
- テスト: `mandelbrot_6259_tests` に direct specializer bytecode 検査、元コード結果
  parity、actual broadcast parity、profiling feature 時の function-variable /
  broadcast `DynamicPow` 回帰ガードを追加。

### Mandelbrot escape executable block (Issue #6253)

- runtime-specialized `mandelbrot_escape(::ComplexF64, ::Int64)` の bytecode shape
  (`abs2(z) > 4.0; z = z^2 + c; k += 1`)を VM predecode が検出し、
  `ExecutableBlock::ComplexF64MandelbrotEscapeLoop` として scalarized Float64 loop
  を直接実行するようにした。これにより `GetField` / `NewParametricStruct` /
  `StoreAny` などの per-iteration overhead を避ける。
- executable block の early return は既存の return routing に通し、通常の関数 return /
  top-level return の扱いを保つ。profile 出力には `ExecutableBlock::...` event を
  独立表示する。
- `benchmarks/mandelbrot_benchmark.jl` は未変更。VM profile は CLI aggregate で
  `2,608,939` → `1,561,995` instructions (約 40.1% 減)。prelude/base cache を
  埋め込んだ release CLI `@time` 3-run 平均は `1.471s` → `1.369s` (約 6.9% 短縮)。
  これらは CLI aggregate / VM instruction profile であり、VM-only Criterion ではない。
- Regression: `vm::executable::tests::complex_mandelbrot_escape_runtime_specialization_adds_executable_block_6253` と
  profiling feature の `mandelbrot_6259_tests::broadcast_runtime_callable_escape_uses_executable_block_6253`。

### Complex scalar times real array dynamic fallback (Issue #6294)

- `ComplexF64 * Vector{Float64}` / `Vector{Float64} * ComplexF64` が compile 時に
  `Cannot convert ComplexF64 to I64` で落ちる main 既存バグを修正した。
- `compile/expr/binary` の array-scalar dynamic dispatch 判定で、dedicated
  `ValueType::ComplexF32` / `ValueType::ComplexF64` を `Struct(_)` と同じ scalar 側
  として扱い、既存の `CallDynamicBinaryBoth(MulFloat, ...)` runtime fallback に乗せる。
- Regression: 既存 `complex::chunk_000` (`complex_scalar_real_array_mul`) と
  `complex::chunk_001` (`complex_binary_both_helpers_3908`)。

## 最新対応 (2026-06-09)

### 手続き間例外推論を純 Julia 分類に委譲 (gcd/lcm の Rust 特例撤去) (Issue #6272)

- `Base.infer_exception_type` / `infer_effects` の手続き間コンポーザが、ユーザ
  ラッパー内の純 Julia Base callee(`gcd`/`lcm` 等)の例外型を、純 Julia の
  リフレクション分類 `Base._classified_exception_type` を**同期参照**して合成
  するようにした。従来は Rust 側が `gcd`/`lcm` を名前でハードコードし、かつ本体
  ループを歩こうとして `g_gcd(a,b)=gcd(a,b)` の推論が ~35 秒ハングしていた(暫定
  ワークアラウンドで回避)。上流 `abstractinterpretation.jl` の
  `state.exctype ⊔ₚ this_exct`(callee のキャッシュ済み例外型を join)に倣う設計。
- 実装: エンジンの例外ウォークに `BaseCalleeExceptionClassifier` を通し、
  function_table 中の Base 関数(provenance: `base_function_count`)は本体を歩かず
  分類を参照し、ユーザ関数のみ再帰。VM 側 `VmBaseExceptionClassifier` が
  `eval_dispatch_call` で純 Julia を同期呼び出し。Rust の gcd/lcm 名特例
  (`immediate_exception_type` / `exception_type_for_expr` /
  `terminal_exception_classified_call`)を撤去。
- 純 Julia 分類 `_classified_exception_type` を全固定幅整数幅に拡張(符号付き
  `gcd`→`OverflowError`、符号なし `gcd`→`Union{}`、全固定幅 `lcm`→
  `Union{DivideError,OverflowError}`)。直接呼び出しもラッパーも上流 julia 1.12.6
  と一致(BigInt のみ上流 `Any` vs 当方 `Union{}` の既存差)。
- テスト: `reflection::*`(`fixtures/reflection/infer_exception_type_gcd_lcm_6272.jl`
  15 assertion、sjulia/julia パリティ確認。throwing 引数 `gcd(v[i],b)` が callee の
  例外を落とさず合成する回帰も含む)。既存 `..._interprocedural_5600.jl` も不変で
  通過。フル 3449 テスト緑。

### gcd/lcm over BigInt → Any 例外パリティ (Issue #6284)

- `Base.infer_exception_type` / `infer_effects` の `gcd`/`lcm` over `BigInt` が
  `Union{}`(nothrow)と過小報告していたのを、上流 julia 1.12.6 に合わせ `Any`
  (`nothrow == false`)を報告するよう修正。BigInt `gcd`/`lcm` は GMP への `ccall`
  に委譲し inferrer が nothrow を証明できないため上流は `Any`。直接呼び出し・
  BigInt と固定幅の混在(BigInt へ昇格)・ユーザラッパーいずれも一致。#6272 で
  唯一残っていた「BigInt のみ上流差」を解消。
- 実装(協調する 4 箇所): ① 純 Julia `_classified_exception_type` に BigInt
  アーム追加(`Any` を返す)、② `classified_value_to_exception_type` が
  `JuliaType::Any`→`ExceptionType::Any`(ラッパー入口)、③ `exception_type_to_julia_type`
  が `ExceptionType::Any`→`Some(JuliaType::Any)`(ラッパー出口)、④ 手続き間
  再帰上限(`depth>16`)の戻り値を `Any`→`Bottom`(merge 単位元)へ変更し、`Any` を
  可視化した後もクリーンな深い再帰が `Union{}` を維持するようにした。固定幅整数幅は
  従来どおり精密(回帰ガード)。
- テスト: `fixtures/reflection/infer_exception_type_gcd_lcm_bigint_6284.jl`
  16 assertion、sjulia/julia パリティ確認。既存 `..._gcd_lcm_6272.jl`(15)/
  `..._interprocedural_5600.jl`(12)も不変で通過。

### closure scalar capture observes reassignment (function-local) (Issue #6262)

- クロージャが捕捉したスカラーの**関数ローカル**変数を、後の再代入後に最新値で
  観測するようにした(従来は値スナップショットで stale)。`function f()
  counter=0; g=()->counter; counter=5; g() end` が `5`(従来 `0`)を返す。
  Julia の `Core.Box` セル意味論に一致。
- 修正: post-lowering パス `lowering/closure_box.rs` が、クロージャに捕捉され
  かつスコープ top-level で ≥2 回再代入されるローカルを `Ref` 化(束縛
  `v = Ref(init)`、読み `v[]`、再代入 `v[] = x` を定義スコープと捕捉クロージャ
  の両方で書き換え)。単一代入捕捉・shadowing・複合代入・read 以外の使用は
  box せず保守的。網羅的 match は `compile::free_vars` をミラー。
- 残課題(follow-up #6281): `@testset`/`@time` ブロックローカルのスカラーで、
  束縛とクロージャが別々の bare `begin` ブロックに分かれる場合は未対応(bare
  block は Julia ではスコープを作らないが、現状パスはスコープ扱い)。トップ
  レベル/モジュールスコープの捕捉はグローバル(動的参照)のため既に動作。
- テスト: `closures::*`(`fixtures/closures/scalar_capture_reassign_6262.jl`)。

### closure scalar capture observes reassignment (@testset / bare block) (Issue #6281)

- #6262 の follow-up。`@testset` ブロックローカル(およびトップレベルの bare
  `begin … end`)のスカラーを捕捉するクロージャが、後の top-level 再代入を最新値で
  観測するようにした(従来は stale)。`@testset "t" begin counter=0;
  get_counter=()->counter; counter=5; @test get_counter()==5 end` が通る。
- 根本原因(IR ダンプで判明): `@testset`/`@test`/bare `begin` の本体は
  `Stmt::Block` ではなく**空束縛 let ブロック**(`Stmt::Expr(LetBlock{bindings:[]})`)
  の入れ子に lower される。束縛・再代入・捕捉クロージャ(`FunctionRef` 化された
  lifted `__lambda_N`)は最内 LetBlock 本体に**同居**しているが、#6262 のパスは
  空束縛 let ブロックへ降りないため到達できなかった(Issue の「別 bare block に
  分離」という推測は誤りだった)。
- 修正: `lowering/closure_box.rs` の `recurse_scopes_stmt` に、空束縛
  `Stmt::Expr(LetBlock{bindings:[]})` の本体を定義スコープとして降りるアームを
  1 つ追加。束縛を持つ `let` は本物の束縛スコープなので対象外。
- テスト: `fixtures/closures/testset_closure_capture_reassign_6281.jl`(最終値が
  回帰ガード。sjulia/julia パリティ)。既存 `testset_closure_capture.jl` の
  `get_counter()==5`(従来 trailing `true` でマスクされていた失敗)も解消。
- 別件として残: `@time` ブロックローカルのクロージャ捕捉は read-only でも
  「Undefined variable」でコンパイル失敗する別バグ(FVA 段階、boxing とは無関係)。
  Issue #6288 として起票し、続けて解決(下記)。

### closure capturing a @time/@elapsed-block-local (compile + boxing) (Issue #6288)

- `@time`/`@elapsed` ブロックローカルのスカラーを捕捉するクロージャが (1) read-only
  でも「Undefined variable」でコンパイル失敗していたのを解消し、(2) ブロック内の
  再代入も最新値で観測する(boxing)ようにした。`@time begin c=7; g=()->c; g() end`
  が動作し、`r = @time begin counter=0; v=()->counter; counter=5; v() end` の
  `r` が `5`(従来 stale `0`)になる。上流 julia 1.12.6 と一致。
- 根本原因(IR ダンプで判明): `@time`/`@elapsed` は本体を
  `#result# = let … end`(**空束縛 let ブロックを代入の VALUE** に持つ)へ lower
  する。`@testset` の捕捉プリスキャンも boxing パスも、代入の VALUE に潜る経路が
  無く、`c` をブロックローカルとして認識できなかった(`_testset_begin!` を持つ
  `@testset` だけが拾えていた)。
- 修正(3 箇所): ① `collect_testset_local_binding_names_from_stmts` に
  `Stmt::Assign{value: 空束縛 LetBlock}` アーム追加(@time wrapper を捕捉スコープ
  として収集)、② `collect_testset_scope_assigned_binding_names` の Assign アームが
  VALUE にも降りる(ネストした `#result# = let` を辿る)、③ `closure_box.rs`
  `recurse_scopes_stmt` に同形の `Stmt::Assign{value: 空束縛 LetBlock}` アーム追加
  (@time 本体を boxing 対象スコープに)。
- テスト: `fixtures/closures/time_block_closure_capture_6288.jl`(read-only /
  reassign / `r=@time …` の 3 形態 + @testset 回帰、最終値ガード、sjulia/julia
  パリティ)。

## 最新対応 (2026-06-08)

### value-position `&&` / `||` final-operand value preservation (Issue #6278)

- 値位置の `&&` / `||` が最終オペランドの値を Bool に強制変換せずそのまま返すよう
  にした。`true && 1` → `1`、`false || "y"` → `"y"`、`true && "x"` → `"x"`
  (従来 `true && "x"` は「Cannot convert Str to Bool」コンパイルエラー)。
  #6162(左/条件オペランド)の follow-up で、右オペランドの強制変換を解消。
- `compile_and_expr` / `compile_or_expr` は右オペランドを自然型でコンパイルし、
  その型が Bool でない場合は式型を Any に広げる(片方は値、もう片方は定数 Bool)。
- 重要: 二項演算の型推論経路(`infer/mod.rs` ×2、`inference.rs` ×3)を codegen と
  同時に共有 `short_circuit_result_type` で更新。さもないとインライン
  `(a && b) == lit` 比較が stale な Bool 左型でミスコンパイルする
  (dual-inference-gate)。Bool オペランドは結果型 Bool のままで既存コードは無影響。
- テスト: `bool::*`(`fixtures/bool/short_circuit_value_6278.jl`)。

### value-position `&&` / `||` non-Bool operand accepted (Issue #6162)

- 値位置の `&&` / `||`(`x = a && b`、`println(a || b)`、関数本体が裸の
  `&&` / `||`)で、左オペランドが非 Bool(例: `1 && true`)の場合に
  `TypeError: non-boolean (Int64) used in boolean context` を送出するようにした。
  従来は `I64ToBool` で Bool に強制変換し `true` を返していた。
- 条件(branch)位置 — `if`/`while`/三項、条件としての `&&`/`||` — は
  PR #6165 で既に厳格化済み。本対応は残っていた値位置のギャップを塞ぐ。
- 修正: `compile_and_expr` / `compile_or_expr`(`compile/expr/unary.rs`)が左
  オペランドを自然型でコンパイルし(`I64ToBool` を挟まない)、後続の
  `JumpIfZero` が VM の Bool 限定チェック(`expect_bool` → `TypeError`)を
  行うようにした。
- 対象外(follow-up): 最終オペランドの値保存。`true && 1` は `1` を返すべき
  (現状 `true`)。
- テスト: `bool::*`(`fixtures/bool/boolean_context_6162.jl`)。

### try/catch implicit-return value discarded (Issue #6223)

- 関数の末尾式が `try/catch[/else/finally]` の場合に、実行されたブランチの値
  (例外が無ければ try body、捕捉時は catch body、`else` があれば try 値を置換、
  `finally` は値に寄与しない)を返すようにした。従来は値が捨てられ、戻り値型の
  既定値(`Int64` なら `0`)が返っていた。
- 原因: compile 層の implicit-return 経路(`compile_block_with_implicit_return`
  と `compile/stmt.rs` の関数末尾文 match)が末尾 `Stmt::Try` を catch-all
  (`compile_stmt` + `emit_default_return`)で処理し、スタックに値を残さなかった。
- 修正: 式位置の try 変換(`lower_try_as_expr`, Issue #4784)を共有関数
  `try_stmt_into_value_expr` に切り出し、`Stmt::Try` を値を生む `Expr::LetBlock`
  へ変換。implicit-return 経路は新規 `compile_try_with_implicit_return` でこれを
  再利用し、末尾位置と式位置の try/catch が同一変換を共有する。
- テスト: `exceptions::chunk_000`(`fixtures/exceptions/try_implicit_return_6223.jl`)。

### Rational bare-parametric binary dispatch cache invalidation (Issue #6270)

- `x::Rational` / `y::Rational` のような bare parametric struct annotation を持つ
  Base method body で、compile-time に `Rational{BigInt}` などの concrete specialization へ
  静的 dispatch し、runtime では `Int64` 実引数を `*(Rational{BigInt}, Rational{BigInt})` に
  渡して `GetField(0): expected struct, got Int64` になる問題を修正した。
- function parameter の `JuliaType::Struct("Rational")` 形状を保持し、binary operator compilation では
  bare parametric struct operand の過度に具体的な static dispatch を避けて runtime dispatch に残す。
- precompiled Base cache version を 24 へ bump し、古い direct-call bytecode を再生成する。
- Regression: `rational::chunk_001` (`fixtures/rational/test_div_fld_cld_rem_mod.jl`)。

### @testset declared global String Any-carrier reads and concat (Issues #6268/#6269)

- `@testset` 内で `global s` 宣言した String global を `s = s * "-suffix"` すると、
  Base `*` の `Union{Char,String}` method へ静的に入り `Any` を coercion して
  `Cannot convert Any to Union([Char, Str])` になる問題を修正した。
- String/Char が片側に見えている `*` は method-table probing より前に `StringConcat` へ回し、
  `global` 宣言名の read は slotization されない `LoadGlobalAny` で frame 0 を読むようにした。
- `global x; x = 42; @test x == 42` のような global 型変更後の read/inference は `Any` として扱い、
  古い `String` slot 型による stale read や `String != Int` の false constant fold を避ける。
- Regression: `strings::chunk_003` (`fixtures/strings/string_local_any_carrier_5081.jl`)。

### explicit Rational parametric constructor runtime dispatch fallback (Issue #6267)

- `Rational{Int64}(Int8(3)//Int8(4))` のような explicit parametric constructor で、
  compile-time inference が 1 引数式を十分具体化できず raw struct constructor に落ち、
  `Struct constructor expects 2 arguments, got 1` になる問題を修正した。
- concrete parametric constructor table (`Rational{Int64}` など) に同 arity method がある場合、
  static dispatch が失敗しても `CallTypedDispatch` を発行し、runtime の実引数型で
  `x::Integer` / `x::Rational` method を選ばせる。
- Regression: `rational::chunk_001` (`fixtures/rational/parametric_typed_constructor.jl`)。

### heap-backed Rational unary float intrinsic conversion (Issue #6266)

- `round(5//3)` / `sqrt(1//4)` のような Rational 入力が `CallBuiltin(Round)` や
  `SqrtF64` に直行した場合、primitive-only `value_to_f64` が `StructRef` を解決できず
  `expected numeric value, got StructRef(N)` で落ちる問題を修正した。
- unary float op 実行は heap-aware conversion helper を使い、`StructRef` の Rational/Irrational を
  `Float64` に変換する。`Float16` / `Float32` の primitive result width preservation は維持する。
- Regression: `rational::chunk_000` (`fixtures/rational/math_round.jl`) と
  `rational::chunk_001` (`fixtures/rational/vm_extraction_generic_5160.jl`)。

### linalg Array/ArrayOf rank-unknown matrix multiplication dispatch (Issue #6264)

- compile-time `ValueType::Array` / `ValueType::ArrayOf(_)` は rank 情報を持たないため、
  LinearAlgebra の `*` candidate filtering では matrix/vector candidate を落とさず runtime dispatch に残すようにした。
- `C * nullspace(C)` のように compile-time では rankless array に見える値でも、runtime の実 shape に基づいて
  `AbstractMatrix, AbstractMatrix` method を選べるようになり、`MethodError operator(Matrix{Float64}, Matrix{Float64})`
  を回避する。
- Regression: `linalg::chunk_001` (`fixtures/linalg/nullspace_logdet_adjoint.jl`)。

### Irrational DynamicPow inline Float64 fallback (Issue #6265)

- `ℯ^2` のような Irrational singleton を含む `^` は inline dynamic op に残し、既存の
  Irrational-to-`Float64` fallback を使うようにした。
- generic `^` method dispatch が同じ `^` へ戻る再帰に落ちず、`log(ℯ^2)` が stack overflow しなくなった。
- Regression: `math::chunk_000` (`fixtures/math/log_two_arg.jl`)。

### @testset global Dict haskey Any receiver fallback (Issue #6263)

- macro-expanded `@testset` を local scope として扱う際に、`global d` 宣言済みの
  Dict global を `haskey(d, key)` で読むと、通常 method dispatch が `Any` receiver を
  `Dict` に静的変換しようとして `Cannot convert Any to Dict` で compile error になる問題を修正した。
- `haskey(::Any, key)` / `haskey(::Dict, key)` は通常 method dispatch 前に
  `CallTypedDispatchOrBuiltin(DictHasKey, ...)` へ routing し、runtime type に合う method があれば dispatch、
  なければ retained Dict builtin probe に落とす。
- Regression: `dict::chunk_000` (`fixtures/dict/testset_global_haskey_any_6263.jl`,
  `fixtures/dict/dict_local_any_carrier_5081.jl`)。

### ndims array DataType rank before value-method dispatch (Issue #6260)

- `ndims(Vector{Int})` / `ndims(Matrix{Int})` / `ndims(Array{T,N})` が値用
  `ndims(a::Array)` method に誤 dispatch して `DataType._size` を読みに行く問題を修正した。
- VM `BuiltinId::Ndims` で `Value::DataType` の array rank projection を generic method dispatch より先に行い、
  type-level array forms は直接 `N` を返す。`ndims(::Type{T}) where {T<:Number}` は従来どおり method dispatch に残す。
- Regression: `arrays::chunk_000` (`fixtures/arrays/test_ndims_type_5118.jl`)。

### @testset lambda capture pre-analysis keeps scoped names (Issue #6261)

- macro-expanded `@testset` LetBlock の型 pre-scan は #6256 のとおり外側へ concrete type を漏らさない一方で、
  module-level lambda capture pre-analysis 専用に testset 内の代入名を `Any` capture candidate として追加するようにした。
- `@testset` 内で `x = 10; f = () -> x + 1` のように定義した lambda が、compile 時に
  `Undefined variable: x` で落ちなくなった。
- Regression: `closures::chunk_000` (`fixtures/closures/testset_closure_capture.jl`)。

### while true dead-tail reflection Bottom preservation (Issue #6258)

- `while true; end; "dead"` のように空 body の無限 loop 後に dead literal tail が残る関数で、
  `Base.return_types` / `Base.infer_return_type` が `String` ではなく upstream Julia と同じ
  `Union{}` を返すようにした。
- `FunctionInfo.return_julia_type == Union{}` の reachability snapshot は bytecode literal scan より優先し、
  tiny bytecode scan も `Jump` を含む window は straight-line literal return と扱わない。
- Regression: `type_inference::chunk_002` (`fixtures/type_inference/while_true_no_exit_4679.jl`) と
  `compile::abstract_interp::engine::tests::test_issue_6258_empty_while_true_dead_tail_infers_bottom`。

### dump-bytecode broken stdout pipe handling (Issue #6254)

- `sjulia --dump-bytecode` の出力を `io::Write` 経由にし、downstream が先に閉じた
  `BrokenPipe` は panic ではなく clean exit として扱うようにした。
- Unix pipeline で `rg | head` のような早期終了 consumer と組み合わせても
  `failed printing to stdout: Broken pipe` panic を出さない。
- Regression: `sjulia_cli_dump_bytecode_tests::dump_bytecode_tolerates_closed_stdout_issue_6254`。

### Macro-expanded @testset local type-scope isolation (Issue #6256)

- Pure Julia `@testset` 展開後の `Expr::LetBlock` に `_testset_begin!` が含まれる場合、
  pre-scan の local type collection と compile-time local maps を Julia local scope として隔離するようにした。
- 別々の `@testset` で同じ local 名を再利用しても、先の testset の `ComplexF64` slot/type 情報が
  後の `ComplexF32` local に残らない。該当 bytecode は stale `Struct` slot ではなく unknown slot に落ちる。
- Regression: `fixtures/macro/testset_reuse_local_slot_type_6256.jl`。

### Mandelbrot Complex integer-power fast path (Issues #6252/#6255, refs #6253)

- `Complex{Float64}` / `Complex{Float32}` の `abs2` / `+` / `-` / `*` / `/` に
  concrete method を追加し、Pure Julia 側の Complex 動的呼び出し先で field access と
  typed float arithmetic を使えるようにした。
- `Complex{Float64}` / `Complex{Float32}` の `^(_, ::Integer)` を integer-power semantics にし、
  `n == 2` は `z*z` 相当の field arithmetic へ直接落とす。Mandelbrot benchmark の
  `z^2 + c` が analytic real-exponent path (`log`/`exp`) に流れない。
- `ComplexF32` の `+` / `-` / `*` / `inv` / integer powers / `abs2` は upstream Julia と同じく
  `ComplexF32` / `Float32` を保持する。
- `benchmarks/mandelbrot_benchmark.jl` は変更せず、タイミング行を除く sjulia/Julia ASCII 出力は一致。
  `mandelbrot_escape(c::Any, maxiter::Any)` の bytecode 自体はまだ `Any` 引数のままで、
  function-variable broadcast call の runtime specialization はこの slice では触っていない。
- Regression: `fixtures/complex/float_integer_pow_6252_6255.jl`。
- 調査中に切り出した `@testset` 間の同名 local reuse stale slot 問題は Issue #6256 として
  別途修正し、`fixtures/macro/testset_reuse_local_slot_type_6256.jl` で回帰を固定した。

### Tuple bounded fallback after diagonal miss (Issue #6251, refs #5072)

- `f(::Tuple{T,T}) where {T<:Real}` と `f(::Tuple{<:Real,<:Real})` の competing methods で、
  homogeneous real tuple は diagonal method、mixed real tuple は independent bounded fallback を選ぶようにした。
- anonymous bounded TypeVar `_ <: Real` は repeated binding ではなく、各 tuple slot の独立した covariant bound として扱う。
- non-Real element を含む tuple は従来通り `MethodError` になる。
- direct / `Any`-routed runtime calls が一致する。
- Regression: `fixtures/dispatch/tuple_bounded_fallback_after_diagonal_6251.jl`。

### Type/AbstractArray rank-TypeVar diagonal specificity (Issue #6249, refs #5072)

- `f(::Type{T}, ::AbstractArray{T,N}) where {T<:Real,N}` と
  `f(::Type{Integer}, ::AbstractArray{<:Real,N}) where {N}` の competing methods で、
  concrete `Type{Int64}, Vector{Int64}` / `Matrix{Int64}` actual pair は diagonal method を選ぶようにした。
- `Type{Integer}, Vector{Int64}` のような abstract type singleton binding は固定
  `Type{Integer}, AbstractArray{<:Real,N}` method を維持する。
- exact `Type{Int64}, AbstractArray{Int64,N}` method が存在する場合は exact method を diagonal より優先する。
- `MethodTable` と runtime `CallDynamic` / FunctionInfo-backed candidate selection の両方で
  direct / `Any`-routed calls が一致する。
- Regression: `fixtures/dispatch/type_abstract_array_rank_typevar_diagonal_6249.jl`。

### Type/AbstractArray rank-omitted diagonal specificity (Issue #6247, refs #5072)

- `f(::Type{T}, ::AbstractArray{T}) where {T<:Real}` と
  `f(::Type{Integer}, ::AbstractArray{<:Real})` の competing methods で、
  concrete `Type{Int64}, Vector{Int64}` / `Matrix{Int64}` actual pair は diagonal method を選ぶようにした。
- `Type{Integer}, Vector{Int64}` のような abstract type singleton binding は固定
  `Type{Integer}, AbstractArray{<:Real}` method を維持する。
- exact `Type{Int64}, AbstractArray{Int64}` method が存在する場合は exact method を diagonal より優先する。
- `MethodTable` と runtime `CallDynamic` / FunctionInfo-backed candidate selection の両方で
  direct / `Any`-routed calls が一致する。
- Regression: `fixtures/dispatch/type_abstract_array_rank_omitted_diagonal_6247.jl`。

### Type/AbstractArray rank-1 diagonal specificity (Issue #6245, refs #5072)

- `f(::Type{T}, ::AbstractArray{T,1}) where {T<:Real}` と
  `f(::Type{Integer}, ::AbstractArray{<:Real,1})` の competing methods で、
  concrete `Type{Int64}, Vector{Int64}` actual pair は diagonal method を選ぶようにした。
- `Type{Integer}, Vector{Int64}` のような abstract type singleton binding は固定
  `Type{Integer}, AbstractArray{<:Real,1}` method を維持する。
- exact `Type{Int64}, AbstractArray{Int64,1}` method が存在する場合は exact method を diagonal より優先する。
- `MethodTable` と runtime `CallDynamic` / FunctionInfo-backed candidate selection の両方で
  direct / `Any`-routed calls が一致する。
- Regression: `fixtures/dispatch/type_abstract_array_rank1_diagonal_6245.jl`。

### Type/AbstractArray rank-2 diagonal specificity (Issue #6243, refs #5072)

- `f(::Type{T}, ::AbstractArray{T,2}) where {T<:Real}` と
  `f(::Type{Integer}, ::AbstractArray{<:Real,2})` の competing methods で、
  concrete `Type{Int64}, Matrix{Int64}` actual pair は diagonal method を選ぶようにした。
- `Type{Integer}, Matrix{Int64}` のような abstract type singleton binding は固定
  `Type{Integer}, AbstractArray{<:Real,2}` method を維持する。
- exact `Type{Int64}, AbstractArray{Int64,2}` method が存在する場合は exact method を diagonal より優先する。
- `MethodTable` と runtime `CallDynamic` / FunctionInfo-backed candidate selection の両方で
  direct / `Any`-routed calls が一致する。
- Regression: `fixtures/dispatch/type_abstract_array_rank2_diagonal_6243.jl`。

### Type/AbstractMatrix diagonal specificity (Issue #6240, refs #5072)

- `f(::Type{T}, ::AbstractMatrix{T}) where {T<:Real}` と
  `f(::Type{Integer}, ::AbstractMatrix{<:Real})` の competing methods で、
  concrete `Type{Int64}, Matrix{Int64}` actual pair は diagonal method を選ぶようにした。
- `Type{Integer}, Matrix{Int64}` のような abstract type singleton binding は固定
  `Type{Integer}, AbstractMatrix{<:Real}` method を維持する。
- exact `Type{Int64}, AbstractMatrix{Int64}` method が存在する場合は exact method を diagonal より優先する。
- `MethodTable` と runtime `CallDynamic` / FunctionInfo-backed candidate selection の両方で
  direct / `Any`-routed calls が一致する。
- Regression: `fixtures/dispatch/type_abstract_matrix_diagonal_6240.jl`。

### Type/AbstractVector diagonal specificity (Issue #6239, refs #5072)

- `f(::Type{T}, ::AbstractVector{T}) where {T<:Real}` と
  `f(::Type{Integer}, ::AbstractVector{<:Real})` の competing methods で、
  concrete `Type{Int64}, Vector{Int64}` actual pair は diagonal method を選ぶようにした。
- `Type{Integer}, Vector{Int64}` のような abstract type singleton binding は固定
  `Type{Integer}, AbstractVector{<:Real}` method を維持する。
- exact `Type{Int64}, AbstractVector{Int64}` method が存在する場合は exact method を diagonal より優先する。
- `MethodTable` と runtime `CallDynamic` / FunctionInfo-backed candidate selection の両方で
  direct / `Any`-routed calls が一致する。
- Regression: `fixtures/dispatch/type_abstract_vector_diagonal_6239.jl`。

### Type/matrix diagonal specificity (Issue #6237, refs #5072)

- `f(::Type{T}, ::Matrix{T}) where {T<:Real}` と
  `f(::Type{Integer}, ::Matrix{<:Real})` の competing methods で、
  concrete `Type{Int64}, Matrix{Int64}` actual pair は diagonal method を選ぶようにした。
- `Type{Integer}, Matrix{Int64}` のような abstract type singleton binding は固定
  `Type{Integer}, Matrix{<:Real}` method を維持する。
- exact `Type{Int64}, Matrix{Int64}` method が存在する場合は exact method を diagonal より優先する。
- `MethodTable` と runtime `CallDynamic` / FunctionInfo-backed candidate selection の両方で
  direct / `Any`-routed calls が一致する。
- Regression: `fixtures/dispatch/type_matrix_diagonal_6237.jl`。

### Type/vector diagonal specificity (Issue #6235, refs #5072)

- `f(::Type{T}, ::Vector{T}) where {T<:Real}` と
  `f(::Type{Integer}, ::Vector{<:Real})` の competing methods で、
  concrete `Type{Int64}, Vector{Int64}` actual pair は diagonal method を選ぶようにした。
- `Type{Integer}, Vector{Int64}` のような abstract type singleton binding は固定
  `Type{Integer}, Vector{<:Real}` method を維持する。
- exact `Type{Int64}, Vector{Int64}` method が存在する場合は exact method を diagonal より優先する。
- `MethodTable` と runtime `CallDynamic` / FunctionInfo-backed candidate selection の両方で
  direct / `Any`-routed calls が一致する。
- Regression: `fixtures/dispatch/type_vector_diagonal_6235.jl`。

### Type/value diagonal specificity (Issue #6233, refs #5072)

- `f(::Type{T}, ::T) where {T<:Real}` と `f(::Type{Integer}, ::Integer)` の competing
  methods で、actual pair が concrete `Type{Int64}, Int64` の場合は diagonal method を選ぶようにした。
- `Type{Integer}, Int64` のように type singleton 側が abstract binding の場合は固定
  `Type{Integer}, Integer` method を維持する。
- exact `Type{Int64}, Int64` method が存在する場合は exact method を diagonal より優先する。
- `MethodTable` と runtime `CallDynamic` / FunctionInfo-backed candidate selection の両方で
  direct / `Any`-routed calls が一致する。
- Regression: `fixtures/dispatch/type_value_diagonal_6233.jl`。

### Union specificity against broader supertypes (Issue #6231, refs #5072)

- finite `Union` method が、actual argument の入った Union arm で competing supertype より狭い場合に
  Union method を選ぶ narrow dominance rule を追加した。
- `Union{Int64,String}` vs `Integer`、`Union{Integer,String}` vs `Real` は Julia と同じく
  covered integer arguments で Union method を選ぶ。
- `Union{Real,String}` vs `Integer` のように actual arm が competing method より広い場合は
  narrower supertype method を維持する。
- `MethodTable` と runtime `CallDynamic` / FunctionInfo-backed candidate selection の両方で
  direct / `Any`-routed calls が一致する。
- Regression: `fixtures/dispatch/union_specificity_6231.jl`。

### Vector diagonal specificity against independent bounds (Issue #6229, refs #5072)

- `f(::Vector{T}, ::Vector{T}) where {T<:Real}` と
  `f(::Vector{<:Real}, ::Vector{<:Real})` の competing methods で、
  actual vector element type が同じ場合は diagonal `Vector{T}` method を選ぶようにした。
- mixed element types (`Vector{Int64}`, `Vector{Float64}`) は repeated `T` binding を満たさないため、
  independent-bound method に落ちる。
- `MethodTable` と runtime `CallDynamic` の FunctionInfo-backed candidate selection の両方に
  同じ narrow dominance rule を追加した。
- Regression: `fixtures/dispatch/vector_diagonal_specificity_6229.jl`。

### Nested Matrix literal rank-aware element type (Issue #6227, refs #6225/#5072)

- nested array literal の logical element type projection で inner literal の rank を見て、
  `[[1], [2]]` は `Vector{Vector{Int64}}`、`[[1 2], [3 4]]` は
  `Vector{Matrix{Int64}}` として扱うようにした。
- `ValueType::ArrayOf(T)` だけでは rank を持たないため、AST の `ArrayLiteral.shape` を持つ
  compile / inference path で rank-aware helper を使う。
- Regression: `fixtures/dispatch/nested_matrix_literal_rank_6227.jl`。

### Nested Vector literal element type for runtime dispatch (Issue #6225, refs #5072)

- `[[1], [2]]` の外側 array literal が logical element type として `Vector{Int64}` を保持し、
  `typeof` が Julia と同じ `Vector{Vector{Int64}}` になるようにした。
- `Vector{T}` と `Vector{Vector{T}}` の competing methods で、`Any` slot 経由の runtime dispatch が
  shallow `Vector{T}` method ではなく nested method を選ぶ。
- 物理 storage は boxed `Any` のまま、`ArrayElementType::Abstract("Vector{...}")` carrier で
  logical element type だけを保持する。
- Regression: `fixtures/dispatch/nested_vector_runtime_dispatch_6225.jl`。

### Invariant Vector TypeVar runtime specificity (Issue #6222, refs #5072)

- `f(::T, ::Vector{T}) where {T<:Real}` と `f(::Integer, ::Vector{<:Real})` の competing
  methods で、wrapper 経由の runtime dispatch が string-pattern の TypeVar reuse bonus に寄りすぎて
  `Vector{T}` method を選ぶ不一致を修正した。
- `CallTypedDispatch` でも `CallDynamic` と同じ FunctionInfo-backed candidate matching を先に使い、
  invariant `Vector{T}` occurrence の binding check を通してから fallback string resolver に戻るようにした。
- direct top-level call と wrapper call の両方で `Vector{Int64}` / `Vector{Real}` が
  Julia と同じ `::Integer, ::Vector{<:Real}` method を選ぶ。
- Regression: `fixtures/dispatch/invariant_vector_typevar_runtime_6222.jl`。

### Tuple vararg ambiguity filtering (Issue #6220, refs #5072)

- `Tuple{Vararg{Integer}}` と `Tuple{Int64,Vararg{Any}}` のように、fixed prefix slot は後者が
  specific だが vararg element は前者が specific になる competing methods を scalar score へ落とさず、
  Julia と同じ曖昧 `MethodError` として残すようにした。
- `()` は `Tuple{Vararg{Integer}}` のみ、`(1, "x")` は `Tuple{Int64,Vararg{Any}}` のみが match するため、
  unique method dispatch は維持する。
- #6218 の strict winner case (`Tuple{Vararg{Int64}}` vs `Tuple{Int64,Vararg{Any}}`) は引き続き
  all-Int tuple で `Tuple{Vararg{Int64}}` を選ぶ。
- Regression: `fixtures/dispatch/tuple_vararg_ambiguity_6220.jl`。
- Verification: upstream Julia / direct `target/release/sjulia` で同 fixture pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests dispatch::`。

### Tuple vararg specificity by actual shape (Issue #6218, refs #5072)

- `Tuple{Vararg{Int64}}` と `Tuple{Int64,Vararg{Any}}` のような competing tuple
  vararg method params で、実引数 tuple の長さへ pattern を一時展開して specificity を比較するようにした。
- `Tuple{Int64,Int64}` に対しては `Tuple{Vararg{Int64}}` を `[Int64, Int64]`、
  `Tuple{Int64,Vararg{Any}}` を `[Int64, Any]` として比較し、前者を選ぶ。
- mixed tail `Tuple{Int64,String}` は従来通り fixed-prefix fallback を選び、
  `Tuple{Vararg{Int64}}` と `Tuple{Int64,Vararg{Int64}}` のように展開後と vararg element が同等な場合は
  既存の fixed-prefix scoring を維持する。
- Regression: `fixtures/dispatch/tuple_vararg_specificity_6218.jl`。
- Verification: upstream Julia / direct `target/release/sjulia` で同 fixture pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests dispatch::`。

### Empty vararg element specificity (Issue #6216, refs #5072)

- `f(xs::Int64...)` と `f(xs::Integer...)` のような competing unbounded vararg
  methods で、`f()` の空 vararg 呼び出しでも宣言された vararg element type を比較するようにした。
- runtime argument へ展開すると vararg 部分が 0 slots になり score が同点化するため、
  同一 fixed prefix かつ非 parametric な trailing vararg の範囲で `Vararg{Int64}` が
  `Vararg{Integer}` を strict subtype dominance で上回る場合に選択する。
- declaration order に依存せず、fixed prefix 付き `f(::String, xs::Int64...)` でも
  prefix-only call が Julia と同じ method を選ぶ。
- Regression: `fixtures/dispatch/empty_vararg_specificity_6216.jl`。
- Verification: upstream Julia / direct `target/release/sjulia` で同 fixture pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests dispatch::`。

### @generated direct body type-argument execution (Issue #6214, refs #5074)

- 小さい pure 関数の IR inliner が `@generated` method を通常関数として inline しないようにした。
- direct generated body expression の argument name は runtime 値ではなく generated-time type object を指すため、
  `@generated function f(x); x + 1; end; f(2)` は Julia と同じ `MethodError` になる。
- returned Expr payload での bare `x` は引き続き runtime frame で評価され、
  `@generated function f(x); return :(x + 1); end; f(2) == 3` を維持する。
- Regression: `fixtures/generated/direct_body_type_args_6214.jl`。
- Verification: upstream Julia / direct `target/release/sjulia` で同 fixture pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::`。

### Empty vararg unbound type parameter matching (Issue #6212, refs #5074)

- `xs::T... where T` の空 vararg 呼び出しでは `T` が static parameter matching で
  制約されないため、body が `T` を読む場合は Julia と同じ `UndefVarError` を送出する。
- VM の lazy type-binding fallback が空 vararg collector slot から `Tuple{}` を `T` として
  推論しないようにした。
- `xs` 自体を読む value-only path は `()` を返し、非空 homogeneous vararg では従来通り
  `T` を concrete element type に束縛する。
- Regression: `fixtures/generated/empty_vararg_unbound_type_param_6212.jl`。
- Verification: upstream Julia / direct `target/release/sjulia` で同 fixture pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::`。

### @generated Array static parameter body binding (Issue #6210, refs #5074)

- generated body の実行前に通常 call と同じ `where` static parameter binding を行い、
  `Array{T,N}` signature から concrete argument type `Matrix{Float64}` の `T = Float64` /
  `N = 2` を抽出するようにした。
- positional argument slots はその後で generated-time type objects に差し替えるため、
  body arguments は型として観測されつつ、`T` は型、`N` は rank 値として body 内で参照できる。
- `where {N,T}` と `where {T,N}` の両順序で `"N = 2, T = Float64"` を返す。
- Regression: `fixtures/generated/array_static_params_6210.jl`。
- Verification: upstream Julia / direct `target/release/sjulia` で同 fixture pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::`。

### @generated vararg `$args` interpolation (Issue #6208, refs #5074)

- generated syntactic-unquote body に `$x` / `$(...)` interpolation が含まれる場合は、
  unquote 後も generated metadata を保持し、body 実行時の positional/vararg slots を
  concrete argument type objects に差し替えるようにした。
- `@generated function f(x...); :($x); end` は runtime value tuple `(1, 2)` ではなく、
  Julia と同じ generated-time type tuple `(Int64, Int64)` を返す。
- #6204 の mixed interpolation/runtime refs は引き続き returned-Expr eval へ fallback し、
  #6206 の bare-only syntactic-unquote default method は通常 runtime method として残る。
- Regression: `fixtures/generated/vararg_interpolation_6208.jl`。
- Verification: upstream Julia / direct `target/release/sjulia` で同 fixture pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::`。

### @generated syntactic-unquote default arguments (Issue #6206, refs #5074)

- `@generated function f(x, a=5); :(x + a); end` のように generated body が
  syntactic-unquote で通常 runtime code へ変換された method は、generated fallback metadata を
  付けずに通常 method として登録するようにした。
- optional positional default wrapper が 2-arg method を呼ぶ経路でも、`x` / `a` は
  generated-time `DataType` ではなく runtime 引数 `7` / `5` または `7` / `6` として評価される。
- returned-Expr fallback path は従来通り generated metadata を保持し、型フレーム実行と
  concrete signature Expr cache を使う。
- Regression: `fixtures/generated/unquote_default_args_6206.jl`。
- Verification: upstream Julia / direct `target/release/sjulia` で同 fixture pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::`。

## 最新対応 (2026-06-07)

### @generated mixed interpolation/runtime argument refs (Issue #6204, refs #5074)

- generated body の returned code で `:(($a, $b, a, b))` のように `$arg` interpolation と
  裸の runtime argument 参照が同居する quote は、Phase 3 syntactic-unquote ではなく
  generated returned-Expr eval 経路に回すようにした。
- `$a` / `$b` は generated-time の concrete type object / vararg type tuple として splice しつつ、
  裸の `a` / `b` は runtime call frame の実引数 `1` / `(2, 3)` に解決される。
- `$(N + 1)` のような parenthesized interpolation は既存の Phase 3 対応を維持する。
- Regression: `fixtures/generated/mixed_interpolation_runtime_args_6204.jl`。
- Verification: upstream Julia / direct `target/release/sjulia` で同 fixture pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::`。

### Runtime bounded dispatch from Any containers (Issue #6202, refs #5926/#5072)

- `Any` container から取り出した `Type{T}` / `Vector{T}` runtime value に対し、
  `where {T<:Integer}` が `where {T<:Real}` より tight な method として選ばれるようにした。
- `CallDynamic` の single-arg dispatch は user candidate では `FunctionInfo` の `type_params` /
  `param_julia_types` を使う VM metadata scorer を優先し、candidate 表示文字列で失われていた
  `where` bounds を保持する。Base/prelude candidate は従来の string resolver に残して、legacy
  native array と Pure Julia Array wrapper の境界を越えない。
- `value_matches_param_with_bindings` は runtime value の `JuliaType` から `Vector{T}` などの型変数を
  抽出し、既存の bound check と diagonal check に流す。
- Regression: `fixtures/dispatch/runtime_bounded_dispatch_from_any_6202.jl`。
- Verification: `julia subset_julia_vm/tests/fixtures/dispatch/runtime_bounded_dispatch_from_any_6202.jl`、
  `./target/release/sjulia subset_julia_vm/tests/fixtures/dispatch/runtime_bounded_dispatch_from_any_6202.jl`、
  `timeout 1800 cargo test -p subset_julia_vm test_find_best_method_index_issue_6202 --release`、
  `timeout 1800 cargo nextest run --release --test fixture_tests dispatch::`。

### Predecoded typed loop executable blocks (Issue #6169)

- VM に `vm/executable.rs` を追加し、canonical `Instr` bytecode から hot typed loop を保守的に
  predecode する executable layer を導入した。
- `TypedLoopBlock` は関数名・変数名・Mandelbrot 固有式を見ず、`Float64` / `Int64` typed slot
  arithmetic、typed compare branch、internal forward branch、loopback、`RandF64`、counted-for
  increment を小さな executable op に変換して、loop-local scalar slots で実行する。
- 形が合わない場合や runtime slot 型が合わない場合は同じ IP から通常の stack interpreter に
  fallback する。
- `GcdI64Loop` は gcd hot path の専用 block として残し、`CallSpecialize` で runtime に append された
  specialized bytecode にも executable predecode を適用する。
- Mandelbrot VM-only Criterion (`benchmarks/vm_mandelbrot.jl`):
  `run_only` は約 `49.7 ms` から `15.1 ms`、`clone_new_program_run` は約 `54.0 ms` から
  `21.4 ms` に改善（短縮 run、約 `3.3x` / `2.5x`）。
- `SJULIA_VM_PROFILE=1 target/release/sjulia benchmarks/vm_mandelbrot.jl` では
  `ExecutableBlock::TypedLoop` が `9600` 回記録され、total instructions は `337,837`。
- `estimate_pi` の typed-friendly 版（`n::Int64`, `x*x`, `1.0`, direct `x = rand(); y = rand()`）は
  `ExecutableBlock::TypedLoop` に乗る。
- Verification: `cargo check -p subset_julia_vm --lib`、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm executable::tests`、
  `timeout 1800 cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark -- --warm-up-time 1 --measurement-time 1 --sample-size 10`、
  `timeout 1800 cargo bench -p subset_julia_vm --bench calc_pi_benchmark -- --warm-up-time 1 --measurement-time 1 --sample-size 10`。

### estimate_pi loop inference/lowering fast path (Issue #6178)

- `x, y = rand(), rand()` のように RHS tuple literal が LHS 名を参照しない tuple destructuring は、
  一時 tuple / tuple slot / tuple get を作らず、各要素の direct assignment に lowering する。
  `x, y = y, x` のような swap 形は従来通り一時 tuple に fallback する。
- runtime function specializer は `x^2` / `y^2` の literal exponent `2` を `DupF64; MulF64`
  または `DupI64; MulI64` に展開し、汎用 `DynamicPow` を避ける。
- 通常 compiler も primitive `x^2` / `y^2`（exponent literal `2`）を typed
  square arithmetic に lowering し、peephole 後の `sjulia --dump-bytecode` では
  原形 `estimate_pi` hot loop が `LoadSquareF64Slot` へ到達する。
- `Float64` と exact に表せる `Int64` literal の比較は、comparison context で
  literal 側を `Float64` に寄せて typed compare を emit する。これにより
  `x::Float64 <= 1` は generic `<=` call ではなく `JumpIfNotLeF64` に融合する。
  `2^53` を超える整数 literal は精度を落とさないため従来の generic path に残す。
- `TypedLoopBlock` は runtime-specialized bytecode 内の generic `LoadSlot` を Int64 live-in として
  runtime guard 付きで扱えるため、untyped `estimate_pi(n)` の `for _ in 1:n` loop も append 後に
  executable block 化できる。
- 原形 `estimate_pi(10000)` は typed-friendly 版と同じ seed で同じ結果 `3.126` を返す。
  `SJULIA_VM_PROFILE=1 target/release/sjulia -e ...` の total instructions は
  `2,065,865` から `232` に減少した。
- Verification: `cargo check -p subset_julia_vm --lib`、
  `timeout 1800 cargo nextest run --release --test estimate_pi_6178_tests`、
  `timeout 1800 cargo nextest run --release --test float_compare_jump_fusion_tests --test branch_context_lowering_tests`、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm executable::tests`、
  `timeout 1800 cargo nextest run --release`、
  `timeout 1800 cargo clippy --all-targets -- -D warnings`。

### VM bytecode dump and Mandelbrot branch/increment fusion (Issue #6159)

- `sjulia --dump-bytecode` を追加し、user functions と main tail の VM bytecode、
  slot table、slot/call/specialization の inline annotation を CLI から確認できるようにした。
- n-ary `+` / `*` call の type inference を binary op と同じ fold 形に寄せ、
  `zi = 2.0 * zr * zi + ci` が `Float64` slot path に落ちるようにした。
- `LeF64 + JumpIfZero` などの Float64 compare false branch を
  `JumpIfNotLeF64` 系に融合した。ordered float comparison は NaN を考慮し、
  `!(a <= b)` を `a > b` に置き換えない。
- `LoadSlotI64; PushI64(k); Add/SubI64; StoreSlotI64` を
  `AddConstI64Slot(slot, delta)` に融合し、Mandelbrot の `iter += 1` / `x += 1` /
  `y += 1` から load/add/store 列を削った。
- Follow-up: 2-3x を狙う抜本案（branch-context lowering、loop-local typed
  registerization、superblocks、VM-only benchmark formalization）は Issue #6159 に分離した。
- Precomputed bytecode benchmark (`benchmarks/vm_mandelbrot.jl`): baseline
  `Vm::run()` median `0.0509s`、current median `0.046546s`（約 8.6% faster）。
- Verification: `cargo check --bin sjulia --features repl`、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm compile::peephole::tests::test_f64_compare_jump_false_branch_fusion compile::peephole::tests::test_slot_const_increment_fusion`、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm --test float_compare_jump_fusion_tests --test slot_const_increment_fusion_tests`。

### VM-only Mandelbrot Criterion benchmark formalization (Issue #6159)

- `subset_julia_vm/benches/vm_mandelbrot_benchmark.rs` を追加し、
  `benchmarks/vm_mandelbrot.jl` を setup で parse/lower/compile した後、
  Criterion 上で `Vm::run()` 単体 (`run_only`) と
  `CompiledProgram::clone + Vm::new_program + run` (`clone_new_program_run`) を分離して測れるようにした。
- setup 時に Mandelbrot の出力 `166265` を検証し、CLI startup / frontend / bytecode compile を
  VM hot-path 計測から外す。
- 実行方法: `cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark`。
- Verification: `cargo check -p subset_julia_vm --bench vm_mandelbrot_benchmark`、
  `timeout 1800 cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark -- --warm-up-time 1 --measurement-time 1 --sample-size 10`
  pass。短縮 run では `run_only` が約 `49.5 ms`、`clone_new_program_run` が約 `54.8 ms`。

### Branch-context lowering for Bool conditions (Issue #6159, Bug #6162)

- `if` / `while` / ternary / implicit-return `if` の条件コンテキストで
  `&&` / `||` を stack Bool に materialize せず、false/true branch へ直接 lower するようにした。
- Mandelbrot の `while zr*zr + zi*zi <= 4.0 && iter < maxiter` は
  `JumpIfNotLeF64(exit); JumpIfGeI64(exit)` の連続 branch になり、
  以前残っていた `PushBool(false)` / 条件 materialization 用 `Jump` / 後段 `JumpIfZero` が消える。
- Leaf 条件は従来どおり `JumpIfZero` の Bool check を使うため、条件文コンテキストでは
  `1 && true` が Julia と同じ non-Bool condition error になる（既存 expression 経路の互換バグは #6162）。
- `SJULIA_VM_PROFILE=1 target/release/sjulia benchmarks/vm_mandelbrot.jl`:
  total instructions は `5,096,052` から `4,040,622` へ減少（約 `20.7%` fewer）。
- Formal Criterion bench の参考値:
  `run_only` は約 `47.2 ms`、`clone_new_program_run` は約 `51.9 ms`
  （直前の formalization smoke はそれぞれ約 `49.5 ms` / `54.8 ms`）。
- Verification: `timeout 1800 cargo nextest run --release -p subset_julia_vm --test branch_context_lowering_tests`、
  `timeout 1800 cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark -- --warm-up-time 2 --measurement-time 3 --sample-size 20`、
  `cargo run --release --bin sjulia --features repl -- --dump-bytecode benchmarks/vm_mandelbrot.jl`。

### calc_pi VM benchmark runner (Issue #6159)

- `benchmarks/calc_pi_benchmark.jl` を VM benchmark 対象として扱うため、
  `benchmarks/scripts/run_vm_calc_pi.sh` を追加した。
- `calc_pi_benchmark.jl` は `@time` 行が非決定的なので、runner は Julia / sjulia の
  deterministic な `N=...` result lines だけを比較し、全スクリプトの process wall time を記録する。
- 対象 workload は `N=100` / `N=500` / `N=1000` の gcd-heavy nested loop。
- 実行方法: `RUNS=3 ./benchmarks/scripts/run_vm_calc_pi.sh`。
- Verification: `RUNS=1 ./benchmarks/scripts/run_vm_calc_pi.sh` pass。
  Result lines は Julia / sjulia で一致し、参考 wall time は Julia `0.23s`、sjulia VM `5.78s`。

### VM-only calc_pi Criterion benchmark formalization (Issue #6167)

- `subset_julia_vm/benches/calc_pi_benchmark.rs` を precomputed-bytecode VM-only
  harness に整理した。
- `benchmarks/calc_pi_benchmark.jl` の関数定義を reuse しつつ、CLI 用の `@time` /
  `println` 部分は Criterion setup から外し、`calc_pi(100)` と `calc_pi(500)` を
  setup で parse/lower/compile して戻り値 `Float64` を検証する。
- Criterion group は `vm_calc_pi` で、`run_only` は `Vm::run()` 単体、
  `clone_new_program_run` は `CompiledProgram::clone + Vm::new_program + run` を測る。
- 短縮 Criterion run の参考値:
  `N=100 run_only` は約 `16.84 ms`、`N=500 run_only` は約 `271.6 ms`。
- Verification: `cargo check -p subset_julia_vm --bench calc_pi_benchmark`、
  `timeout 1800 cargo bench -p subset_julia_vm --bench calc_pi_benchmark -- --warm-up-time 1 --measurement-time 1 --sample-size 10`
  pass。

### VM Mandelbrot F64 slot superinstructions (Issue #4301)

- Mandelbrot inner loop の `Float64` slot arithmetic に対し、`LoadSquareF64Slot` と
  `Load{Add,Sub,Mul,Div}F64Slot` superinstruction を追加した。
- `LoadSlotF64; DupF64; MulF64` / repeated `LoadSlotF64; LoadSlotF64; MulF64` を peephole で融合し、
  protected jump target のため静的融合できない fall-through path は VM runtime fast path で `x*x` として実行する。
- `zi = 2.0 * zr * zi + ci` の残存 `CallDynamicBinaryBoth(AddFloat)` は、
  `Float64` stack top + `LoadSlotF64` の場合に primitive add として直接実行し、dynamic binary instruction をスキップする。
- Precomputed bytecode benchmark:
  `benchmarks/vm_mandelbrot.jl` は `Vm::run()` median `0.049954s`、`clone + Vm::new_program + run`
  median `0.054315s`。`benchmarks/julia/mandelbrot.jl` は `Vm::run()` median `0.236279s`。
- Profile: `benchmarks/vm_mandelbrot.jl` の VM instruction count は `7,295,897` から `5,096,052` へ減少し、
  `CallDynamicBinaryBoth::AddFloat/17` は top profile から消えた。
- Verification: `cargo check -p subset_julia_vm --features repl`、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm compile::peephole::tests::test_slot_f64_square_and_load_op_fusion`、
  `timeout 1800 cargo nextest run --release mandelbrot`、`git diff --check`。

### @generated signature Expr cache compatibility (Issue #5936)

- `@generated` fallback が返した staged `Expr` を、関数 index と concrete argument signature
  (`Tuple{argtypes...}` 相当) ごとに VM 内で cache するようにした。
- cache hit では generated body を再実行せず、cached Expr を現在の call frame 上で `eval` するため、
  `@generated function f(x); counter[] += 1; return :(x); end` は同じ `Int64` 呼び出しで counter を増やさない。
- Full generated staging driver / lower-to-bytecode cache ではなく、#5936 の returned-Expr fallback に対する
  tuple-signature cache compatibility slice。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/signature_cache_5936.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。

### @generated body argument type binding (Issue #5936)

- generated body の positional / vararg argument slots を runtime value ではなく concrete argument type object
  (`Int64`, `Type{Int64}`, `(Int64, Float64)` など) で実行するようにした。
- cache hit の returned staged `Expr` eval は引き続き実引数 frame 上で実行するため、body の型分岐と
  staged expression の runtime 引数参照を分離する。
- Full generated staging driver ではなく、#5936 の generated-body environment compatibility slice。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/body_arg_types_5936.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。

### @generated splat call compatibility (Issue #5936)

- `f(args...)` 経由の generated call でも direct call と同じ concrete argument type binding と
  returned staged `Expr` signature cache を使うようにした。
- named generated splat call の expanded args から `where` type params を束縛し、cache hit と
  first miss の returned `Expr` eval は実引数 frame で行い、generated body 実行だけ type-object slots へ差し替える。
- Full generated staging driver ではなく、#5936 の generated call-site coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/splat_call_5936.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。`timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated function alias splat calls (Issues #6163/#5936)

- function-valued alias 経由の `alias(args...)` が `CallFunctionVariableWithKwargsSplat` に lower される場合も、
  generated body argument slots を concrete type object に差し替えるようにした。
- named splat call と同じく、cache hit と first miss の returned `Expr` eval は実引数 frame で行う。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/alias_splat_6163.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo nextest run --release -p subset_julia_vm --test float_compare_jump_fusion_tests`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated function alias calls (Issues #6166/#5936)

- function-valued alias 経由の `alias(x)` が `CallFunctionVariable` に lower される場合も、
  generated body argument slots を concrete type object に差し替えるようにした。
- direct call と同じく、cache hit と first miss の returned `Expr` eval は実引数 frame で行う。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/alias_call_6166.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated keyword calls (Issues #6171/#5936)

- `CallWithKwargs` / `CallWithKwargsSplat` 経由の generated call でも、
  positional と keyword slots を generated body 実行時だけ concrete type object に差し替えるようにした。
- generated `Expr` cache key に keyword argument type を含め、同じ positional 型でも
  `y::Int64` と `y::Float64` の staged `Expr` を混同しないようにした。
- cache hit と first miss の returned `Expr` eval は、keyword runtime values を保持した frame で行う。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/keyword_calls_6171.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated quoted Expr payload eval (Issues #6172/#5936)

- generated body の `cond ? :(x + 1) : :(0)` のような ternary tail/return も、
  両分岐が staged Expr 候補なら `GeneratedEval` で包むようにした。
- `GeneratedEval` は `QuoteNode(Expr)` payload を一段 unwrap した後、その `Expr` を runtime argument frame 上で
  eval するようにした。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/quoted_expr_6172.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated calls through map (Issues #6170/#5936)

- value-mode / numeric-mode HOF helper が直接 frame を作る経路でも、generated body argument slots を
  concrete type object に差し替えるようにした。
- HOF state machine の frame-return path は維持し、first miss の returned `Expr` eval は runtime element frame 上で行う。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/map_call_6170.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated returned Expr(:return) eval (Issues #6183/#5936)

- generated fallback の returned `Expr` eval が `Expr(:return, value_expr)` を staged result marker として扱い、
  payload を runtime argument frame 上で評価するようにした。
- `@generated function f(x); return Expr(:return, Expr(:call, :+, :x, 3)); end` は Julia と同じく
  `f(4) == 7` になる。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback eval head coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/return_head_6183.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated returned Expr(:let) eval (Issues #6185/#5936)

- generated fallback の returned `Expr` eval が `Expr(:let, binding..., body)` を一時 eval frame 上で評価し、
  binding を body にだけ見せるようにした。
- `Expr(:let, Expr(:(=), :y, Expr(:call, :+, :x, 2)), Expr(:call, :*, :y, 3))` のように
  binding RHS と body が runtime 引数を読む代表ケースを Julia と同じ結果にした。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback eval head coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/let_head_6185.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated returned Expr(:call, GlobalRef, ...) eval (Issues #6187/#5936)

- generated fallback の returned `Expr(:call, callee, args...)` eval が `GlobalRef(Base, :+)` などの
  GlobalRef callee を qualified function dispatch へ渡せるようにした。
- `Expr(:call, GlobalRef(Base, :+), :x, 4)` / `GlobalRef(Base, :*)` の代表ケースを Julia と同じ結果にした。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback call-callee coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/globalref_call_6187.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### eval/generated Expr(:copyast) head (Issues #6190/#5936)

- runtime `eval` が `Expr(:copyast, QuoteNode(ex))` を評価し、quoted AST payload を data として返すようにした。
- generated fallback の returned `Expr(:copyast, QuoteNode(Expr(:call, :+, :x, 6)))` も Julia と同じく
  `Expr(:call, :+, :x, 6)` 値を返す。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback eval head coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/copyast_head_6190.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### eval/generated Expr(:comparison) chains (Issues #6192/#5936)

- runtime `eval` が `Expr(:comparison, value, op, value, op, value...)` の全ペアを左から評価し、
  最初の false で `false` を返すようにした。
- generated fallback の returned `Expr(:comparison, 1, :<, 2, :>, 3)` が Julia と同じく `false` になる。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback eval head correctness slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/comparison_chain_6192.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### eval/generated Expr(:elseif) head (Issues #6194/#5936)

- runtime `eval` が `Expr(:elseif, cond, then[, else])` を `Expr(:if, ...)` と同じ conditional head として
  評価するようにした。
- generated fallback の returned `Expr(:elseif, B, 10, 20)` が Julia と同じく `Val(true)` で `10`、
  `Val(false)` で `20` になる。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback eval head coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/elseif_head_6194.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated returned keyword Expr calls (Issues #6196/#5936)

- generated fallback の returned `Expr(:call, callee, Expr(:parameters, Expr(:kw, ...)), args...)` eval が
  keyword entries を positional args から分離し、既存 runtime kwargs dispatch に渡すようにした。
- `Expr(:call, :f, Expr(:parameters, Expr(:kw, :y, 5), Expr(:kw, :z, 3)), :x)` の代表ケースが
  Julia と同じ keyword binding result になる。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback keyword-call AST coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/keyword_expr_6196.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated returned block/logical/quote Expr heads (Issue #5936)

- `Expr(:block, ...)` と `Expr(:(=), ...)` assignment は generated returned-Expr eval 経路で
  Julia と同じ sequential body として扱えることを fixture 化した。
- `Expr(:&&, ...)` / `Expr(:||, ...)` short-circuit heads と `Expr(:quote, ...)` AST-data return も
  同じ compatibility path の代表ケースとして固定した。
- これは full lower-to-bytecode staging driver ではなく、既存 returned-Expr compatibility path の回帰固定。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/block_logical_quote_5936.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated staged loop Expr reproduction (Issue #5936)

- #5936 本文の `@generated function sumn(::Val{N}) where N; ex = :(0 + 0); for i in 1:N; ex = :($ex + $i); end; return ex; end`
  代表再現を fixture 化した。
- generated body が loop で `Expr` を組み立て、返却された staged `Expr` を runtime eval して
  `Val(3) == 6` / `Val(5) == 15` を返す。
- これは full lower-to-bytecode staging driver ではなく、既存 returned-Expr compatibility path の回帰固定。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/staged_loop_expr_5936.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### Function-table precise callee backedges (Issue #5939)

- precise な user-function call は、method-table dispatch と同じ `method_edges` に observed callee argtypes を stamp するようにした。
- `function_table` 経由で `callee(::Int64)` だけを呼んだ caller cache は、後続の `callee(::Float64)` mutation では retire しない。
- `CachedReturn` / Base cache schema は変えず、imprecise args や arity/type が明確でない call は従来どおり bare edge fallback に残す。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939 issue_5603` pass（17/17）。

### PartialStruct precise callee backedges (Issue #5939)

- PartialStruct return inference の user-function call も、precise な function-table call なら
  `method_edges` に observed callee argtypes を stamp するようにした。
- `outer(::Int64)` の PartialStruct side-cache が `inner(::Int64)` だけに依存する場合、後続の
  `inner(::Float64)` mutation では caller の PartialStruct fact を retire しない。
- arity/type binding が明確でない call は従来どおり bare edge fallback に残し、健全性優先の invalidation を維持する。
- Verification: `timeout 1800 cargo nextest run --release -p subset_julia_vm --lib issue_5939 issue_5603`
  pass（18/18）。

### Method-edge transitive global reads (Issues #6176/#5939)

- precise `DispatchedMethodEdge` 経由の caller cache が、typed callee method identity
  (`callee(Int64)` など) に記録された `global_reads` も fold するようにした。
- `caller(::Int64) -> callee(::Int64) -> G` のような経路で、`G` の binding change が caller cache を
  targeted に retire することを engine test で固定した。
- Bare callee name と method identity の両方を読むため、nullary/legacy dependency の互換性は維持する。
- Verification: `timeout 1800 cargo test -p subset_julia_vm compile::abstract_interp::engine::tests::test_issue_5939_method_edge_transitive_global_read_invalidates_caller --release`
  pass。`timeout 1800 cargo nextest run --release -p subset_julia_vm --lib issue_5939 issue_5603 issue_4285`
  pass（24/24）。

### Binding invalidation method-edge cleanup (Issue #5939)

- binding change で cache を retire した関数について、`global_binding_dependencies` と
  `function_dependencies` だけでなく `method_dependencies` も clear するようにした。
- `caller(::Int64) -> callee(::Int64) -> G` のような precise method-edge 経由の global-read cache は、
  `G` の binding change 後に古い method-edge record を残さず、再推論で current world の dependency を作り直す。
- Verification: `timeout 1800 cargo test -p subset_julia_vm compile::abstract_interp::engine::tests::test_issue_5939_method_edge_transitive_global_read_invalidates_caller --release`
  pass。`timeout 1800 cargo nextest run --release -p subset_julia_vm --lib issue_5939 issue_5603 issue_4285`
  pass（26/26）。

### Method-edge transitive dependency propagation (Issues #6179/#5939)

- `record_call_dependency` / `record_method_call_dependency` が、callee の bare name だけでなく
  function-table method identity key (`callee(Int64)` など) からも transitive dependency edges を fold するようにした。
- cold callee inference では最初の caller edge 記録時点で callee dependencies が未確定なため、
  callee inference 完了後に dependency recording を再実行し、dedupe しつつ transitive method edges を取り込む。
- `caller(::Int64) -> mid(::Int64) -> leaf(::Int64)` の cache が leaf mutation で targeted に retire することを固定した。
- Verification: `timeout 1800 cargo test -p subset_julia_vm compile::abstract_interp::engine::tests::test_issue_5939_method_identity_dependency_edges_propagate_transitively --release`
  pass。`timeout 1800 cargo nextest run --release -p subset_julia_vm --lib issue_5939 issue_5603 issue_4285`
  pass（25/25）。

### PartialStruct transitive method-edge propagation (Issues #6181/#5939)

- PartialStruct return inference でも cold callee inference 完了後に precise dependency recording を再実行し、
  caller side-cache entry が callee method identity の transitive edges を取り込むようにした。
- `outer(::Int64) -> mid(::Int64) -> inner(::Int64)` の PartialStruct fact が、`inner(::Int64)` mutation で
  targeted に retire することを固定した。
- Verification: `timeout 1800 cargo test -p subset_julia_vm compile::abstract_interp::engine::tests::test_issue_5939_partial_struct_method_edges_propagate_transitively --release`
  pass。`timeout 1800 cargo nextest run --release -p subset_julia_vm --lib issue_5939 issue_5603 issue_4285`
  pass（26/26）。

### Limited/tentative side-cache precise method edges (Issue #5939)

- `limited_results` と `tentative_results` の side-cache entry も `DispatchedMethodEdge` による
  signature-aware method invalidation を使うことを engine test で固定した。
- `callee(::Int64)` に依存する limited/tentative entry は、後続の `callee(::Float64)` mutation では残り、
  `callee(::Int64)` mutation で targeted に retire する。
- Verification: `timeout 1800 cargo test -p subset_julia_vm compile::abstract_interp::engine::tests::test_issue_5939_side_cache_method_edges_preserve_unmatched_callee_mutation --release`
  pass。`timeout 1800 cargo nextest run --release -p subset_julia_vm --lib issue_5939 issue_5603 issue_4285`
  pass。

### Call-site MethodInstance cache keys (Issue #5939)

- user-function call-site inference の return cache key を bare callee name ではなく、
  callee の primary method identity (`name(declared_param_types)`) から作るようにした。
- `infer_function_with_arg_types` 直呼びと、関数 body 内からの interprocedural inference が同じ
  MethodInstance-oriented key contract を使うため、legacy bare-name cache entry の再生成を防ぐ。
- Legacy `get_cached_return_type(name, args)` は unique primary-key entry への fallback を維持し、
  既存の name-based lookup 互換性は残す。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939 issue_5603`。

### Diagonal morespecific dominance coverage (Issue #5926)

- `Tuple{T,T} where T` が `Tuple{Any,Any}` fallback より specific になる #5926 の diagonal-family contract を、
  compile-time `MethodTable::dispatch` と runtime `Vm::find_best_method_index` の両方で regression test 化した。
- same-typed args は diagonal method を選び、mixed args は diagonal rule を満たさず `Any,Any` fallback に戻ることも固定。
- Full topological morespecific replacement ではなく、既存 dominance pre-check が両選択サイトで同じ
  diagonal behavior を維持するための coverage slice。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926_method_table_dominance_selects_diagonal_over_any_any test_find_best_method_index_issue_5926_dominance_selects_diagonal_over_any_any`。

### Method-identity dependency stamps (Issue #5939)

- inference 中の dependency map key を bare `func.name` ではなく primary cache identity に寄せるため、
  lexical `active_function` と invalidation 用 `active_dependency_key` を分離した。
- 同名の別メソッド body が 1 つの dependency bucket を共有し、片方の precise callee edge がもう片方の
  cache entry に stamp される過剰失効を防ぐ。
- Serialized `CachedReturn` / persisted `InferenceCacheKey` の format は変えず、#5939 の
  method-instance backedge 精密化を内部 map key の粒度から前進させる slice。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939 issue_5603`、
  `timeout 1800 cargo check -p subset_julia_vm --lib`、`timeout 1800 cargo clippy --all-targets -- -D warnings`。

### Dispatch-winner based method invalidation (Issue #5939)

- same-name method mutation invalidation を「mutated signature が call argtypes に単に match するか」ではなく、
  post-mutation dispatch winner かどうかで判定するようにした。
- `f(::Int64)` の cache は `f(::Any)` 追加/置換では retire せず、逆に `f(::Any)` cache は
  `f(::Int64)` 追加で retire するため、#5939 の method-identity 精度に一段近づく。
- precise method-edge invalidation も同じ winner 判定を使い、callee の less-specific method mutation が
  more-specific callee に dispatch した caller を過剰 invalidation しない。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939 issue_5603`。

### @generated returned Expr eval head coverage (Issue #5936)

- `@generated` fallback が返した staged `Expr` を `eval(...)` する経路で、
  `:tuple` / `:vect` / `:if` / `:curly` / `:string` / `:ref` head が実際の generated body として動くことを
  fixture で固定した。
- Full generated staging driver ではなく、#5927-#5932 の eval head support と #5936 の returned-Expr
  compatibility を接続する regression coverage。
- Verification: upstream Julia / `target/release/sjulia` direct で
  `generated/expr_head_eval_5936.jl`。`timeout 1800 cargo nextest run --release --test fixture_tests generated::`。

### Precise method-table dependency edge stamping (Issue #5939)

- method-table dispatch が precise argtypes で成功した caller cache は、legacy bare `edges` ではなく
  `method_edges` に observed callee argtypes を stamp する contract を regression test で固定した。
- `callee(::Float64)` の mutation が `callee(::Int64)` だけに依存した caller を retire しない #5603 の
  既存挙動を、#5939 の bare-edge 削減前提として明文化する。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939 issue_5603`。

### Structured MethodInstanceKey groundwork (Issue #5939)

- `InferenceCacheKey.fn_id` の legacy `name(declared_param_types)` 文字列を直接組み立てる代わりに、
  structured `MethodInstanceKey` から legacy projection を生成する経路を追加した。
- `MethodInstanceKey` は function 名、declared arg types、where type params、vararg metadata を保持し、
  #5939 の method-identity cache/backedge key 置換で string parse 依存を外す足場にする。
- Persisted inference cache key はまだ `InferenceCacheKey` のままなので、Base cache format / version は変更しない。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939`。

### Origin-fenced morespecific dominance pre-check (Issue #5926)

- #5926 の dominance pre-check が Base-origin method を winner にするとき、user-origin 候補が同じ
  candidate set に含まれる場合は pre-check で選ばず、既存の score path へ戻す。
- compile-time `MethodTable::dispatch_inner` と runtime `Vm::find_best_method_index_uncached` の
  両選択サイトで同じ origin fence を使う。
- Full morespecific 統合ではなく、Base method が user candidate を dominance override だけで
  cross-origin に上書きする codegen hazard を抑える slice。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926`。

### VM runtime base-function origin context (Issue #5926)

- `Vm` に `CompiledProgram::base_function_count` を保持させ、runtime dispatch mirror でも
  Base/prelude prefix と user function の origin を判定できるようにした。
- `MethodTable` 側の origin context と揃え、後続の morespecific dominance fence が compile-time /
  runtime の両選択サイトで同じ Base/user origin 条件を使える足場にする。
- Full morespecific 統合ではなく、#5926 の runtime origin-visibility groundwork。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926`。

### MethodTable base-function origin context (Issue #5926)

- `MethodTable` に `base_function_count` を非永続 dispatch context として持たせ、
  compiler の method-table projection 時に `Program::base_function_count` を thread する。
- `base_function_count` は cache format へ serialize せず、cached method tables でも compile 時に再設定する。
  これにより後続の morespecific dominance fence は `is_base_extension` ではなく
  `global_index < base_function_count` で Base/user origin を判定できる。
- Full morespecific 統合ではなく、#5926 の origin-visibility groundwork。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926_method_table_tracks_base_function_count_for_origin_fences`、
  `timeout 1800 cargo nextest run --release --lib issue_5926`。

### @generated implicit returned Expr eval compatibility (Issue #5936)

- `@generated` の full-body compatibility fallback で、body 最後の expression が quote/`Expr(...)` 由来の
  staged `Expr` なら `eval(...)` に包み、implicit return でも評価結果を返す。
- 明示 `return ex` の wrapping と同じく、syntactic-unquote path はそのまま使い、loop body などの
  非 tail expression は評価しない。
- Full generated staging driver ではなく、#5936 の implicit returned-Expr compatibility slice。
- Verification: upstream Julia / `target/release/sjulia` direct で
  `generated/implicit_return_expr_eval_5936.jl`。`timeout 1800 cargo nextest run --release --test fixture_tests generated::`
  pass。

### Reflection `hasmethod` sees LinearAlgebra Diagonal Base extensions (Issue #6124)

- `FunctionInfo` now retains `is_base_extension`, so VM reflection can treat
  methods defined as `Base.f(...)` inside stdlib modules as methods of `f` /
  `Base.f` instead of only `Module.f`.
- `hasmethod(*, Tuple{typeof(F.U), typeof(Diagonal(F.S))})` now returns `true`
  for SVD reconstruction signatures, matching upstream Julia.
- Verification: upstream Julia fixture pass, direct `target/release/sjulia` repro pass,
  `timeout 1800 cargo nextest run --release --test fixture_tests linalg::` pass.

### @generated returned Expr eval compatibility (Issue #5936)

- `@generated` の full-body compatibility fallback で、明示 `return ex` が返す quote/`Expr(...)` 由来の staged `Expr` を
  そのまま runtime value にせず、`eval(ex)` として評価結果を返す。
- `try_unquote_generated_block` / `try_unquote_generated_short_body` が扱える syntactic-unquote path は
  既存どおり unquoted IR を使い、今回の `eval(...)` wrapping は fallback path の staged-Expr return に限定する。
- Full generated staging driver ではなく、#5936 の returned-Expr compatibility slice。
- Verification: upstream Julia / `target/release/sjulia` direct で
  `generated/return_expr_eval_5936.jl`。`timeout 1800 cargo nextest run --release --test fixture_tests generated::`
  pass。

### @generated parenthesized unquote expression (Issue #5936)

- `@generated` の syntactic-unquote compatibility path で `$ident` だけでなく `$(expr)` を lower する。
- `@generated f(::Val{N}) where N = :($(N + 1) * 2)` のような parenthesized interpolation は
  generated-unquote 中だけ inner expression として扱い、quote 外の `$` は引き続き unsupported/error のままにする。
- Full generated staging driver ではなく、#5936 本丸前提の expression-splicing slice。
- Verification: upstream Julia / `target/release/sjulia` direct で
  `generated/paren_unquote_expr_5936.jl`。

### Legacy inference bare-key co-write removal (Issue #5939)

- `infer_function` / `infer_function_with_arg_types` は non-nullary method result を
  primary `inference_cache_function_id(func)` key のみに保存し、legacy bare `func.name` key の
  co-write をやめた。
- `get_cached_return_type(name, argtypes)` は legacy 互換の bare-name lookup として残すが、
  matching primary key が一意な場合だけ fallback する。`f(::Any)` と `f(::Int64)` のように同じ
  call-site argtypes で複数 method identity が見える場合は first-writer を返さず miss にする。
- Verification: `timeout 1800 cargo nextest run --release --lib test_issue_5939_primary_keys_preserve_method_identity_without_bare_co_write`。

### Inference cache base function id projection (Issue #5939)

- `InferenceCacheKey::base_fn_id()` を追加し、`name(declared_param_types)` から bare `name` を取り出す
  #5939 の legacy projection を key 型側へ集約した。
- `InferenceEngine` の invalidation / dependency stamping は ad hoc な string helper ではなく
  `InferenceCacheKey` の projection を使うため、将来 `MethodInstanceKey` へ置き換える際の監査範囲が狭くなる。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939_cache_key_exposes_base_function_id`。

### Method identity cache lookup helper (Issue #5939)

- `InferenceEngine` の test helper として、legacy bare-name key ではなく
  `inference_cache_function_id(func)` で primary method-identity cache key を読む経路を追加した。
- #5939 の `MethodInstanceKey` 移行では lookup contract を bare name から method identity へ寄せる必要があるため、
  primary key では `f(::Any)` と `f(::Int64)` が分離して読めることを
  regression test で固定した。
- Verification: `timeout 1800 cargo nextest run --release --lib test_issue_5939_primary_keys_preserve_method_identity_without_bare_co_write`。

### Web Playground startup warmup before first Run (Issue #6127)

- Web Playground は cache-enabled WASM artifact でも初回 `run_from_source` に
  user source parser/lowering、embedded cache deserialize/restore、first user program compile の
  cold path が残る。
- `web/app.js` は Run button を有効化する前に `warmupWasm()` を await し、
  すぐ Run を押した user execution が scheduled warmup を cancel して cold path を踏むのを避ける。
- Verification: `node --check web/app.js` pass。local `python3 web/server.py` +
  Playwright Chrome channel で `WASM plot warmup completed` log と Run button enabled を確認。

### Android Flutter native build embeds precompiled caches (Issue #6126)

- `mobile/scripts/build_android.sh` は host `sjulia` を release build し、
  `target/prelude_program_cache.bin` と `target/base_cache.bin` を生成する。
- 各 `cargo ndk` ABI build は `SJULIA_PRELUDE_PROGRAM_CACHE` と `SJULIA_BASE_CACHE` を渡し、
  Android `.so` に parsed/lowered prelude Program cache と Base bytecode cache を埋め込む。
- Verification: `bash -n mobile/scripts/build_android.sh` pass、
  `./scripts/build_android.sh` pass、`flutter build apk --debug` pass。

### REPL LinRange StructRef display through FFI (Issue #6123)

- REPL FFI の textual value formatting を `session.get_struct_heap()` 付きの
  heap-aware formatter に切り替え、heap-resident `StructRef` を解決して表示するようにした。
- `LinRange` は内部 field dump や `<struct ref>` ではなく、`start:step:stop` の range 表示にする。
  `x = y = range(-3, stop = 3, length = 100)` は
  `-3.0:0.06060606060606061:3.0` を返す。
- Verification: `timeout 1800 cargo nextest run --lib test_repl_eval_linrange_struct_ref_formats_range_6123`
  pass。#6122 と合わせた targeted run も pass。

### REPL inline lambda result suppression for surface snippets (Issue #6122)

- `surface(x, y, (x, y) -> sinc(norm([x, y])))` の inline anonymous function は
  lowering で `__lambda_*` 内部関数になるが、REPL の「新規関数定義なら関数値を返す」判定から
  内部 lowered function を除外した。
- これにより初回評価でも `function __lambda_0` が表示されず、`surface(...)` の Plot return value から
  Plotly artifact が生成される。
- Verification: `timeout 1800 cargo nextest run --lib test_repl_surface_inline_lambda_returns_plotly_artifact_6122`
  pass、`git diff --check` pass。

### Flutter mobile 3D Plotly surface rendering (Issue #6121)

- Flutter mobile の `PlotlyView` を 2D Canvas renderer から iOS と同じ WebView +
  bundled `plotly.min.js` renderer に切り替えた。2D trace と 3D `surface` /
  `scatter3d` trace はどちらも `Plotly.newPlot` で描画する。
- Flutter assets に `assets/plotly/plotly.min.js` を追加し、`webview_flutter` を依存に追加した。
- ユーザー例の `using LinearAlgebra; using Plots; x = y = range(...); surface(... -> ...)`
  は Editor state regression で Plotly JSON に `"surface"` を含むことを確認する。
- Verification: `flutter test`、`flutter build apk --debug`、`git diff --check` pass。

### Flutter mobile Editor Plotly artifact display (Issue #6118)

- Flutter mobile Editor の `compile_and_run_detailed` / `CExecutionResult` 経路で
  `artifact_mime` / `artifact_data` を読み、`application/vnd.plotly+json` を
  `ExecutionResult.plotlyJSON` として保持するようにした。
- `EditorState` は直近の Plotly JSON を output state と一緒に保持し、Editor output pane は
  REPL と同じ `PlotlyView` で 2D Plotly trace を表示する。
- Verification: `flutter test`、`flutter build apk --debug`、`git diff --check` pass。
  Android emulator への APK install / launch は確認済み。ADB による code editor 全文置換は
  既存テキストが残って Syntax Error になったため、スクリーンショットでの manual plot 表示確認は未完了。

### Flutter mobile REPL Plotly artifact display (Issue #6115)

- Flutter mobile REPL の `CREPLResult` FFI binding に iOS と同じ
  `artifact_mime` / `artifact_data` を追加し、dedicated REPL worker から
  `REPLState` / `REPLEntry` まで Plotly JSON を伝搬するようにした。
- `application/vnd.plotly+json` artifact は `PlotlyView` で表示する。Android では
  まず `plot(sin)` などの 2D scatter/line/bar trace を Flutter Canvas で描画する。
- Verification: `flutter test`、`flutter build apk --debug`、`git diff --check` pass。
  `flutter analyze` は既存の 14 件（`_freeString` unused と既存 `withOpacity` deprecation）で
  引き続き fail するが、今回追加した Plotly path からの新規 warning はない。

### Android Flutter REPL background worker (Issue #6113)

- Flutter mobile REPL の native `repl_session_eval` を UI isolate から外し、
  専用 Dart isolate が長寿命 `REPLSession` を保持する worker 構成にした。
- REPL の変数/関数定義の永続性は worker 内 session で維持し、UI state は map payload の
  response だけを受け取る。reset 後に古い評価結果が履歴へ戻らないよう generation guard も追加した。
- Verification: `flutter test`、`flutter build apk --debug`、Android emulator
  `sdk gphone16k arm64` で `1 + 1` → `2` を確認。評価中/後の logcat filter で
  ANR / input dispatch timeout / activity pause timeout はヒットしないことを確認。

### Method origin helper for dispatch fences (Issue #5926)

- `MethodSig::is_base_program_method(base_function_count)` を追加し、Base/prelude 由来かどうかを
  `global_index < base_function_count` で判定する入口を明示した。
- `is_base_extension` は `Base.:+` などを構文的に拡張したかの flag であり、origin marker ではない。
  #5926 の morespecific/topological selection fence は両者を混同しない必要があるため、
  helper と unit test で契約を固定した。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926_method_origin_uses_global_index_not_base_extension_flag`。

### MethodSig where-wrap nested typevar characterization (Issue #5926)

- `MethodSig::core_signature()` が `x::Vector{T} where T` を
  `Tuple{Vector{T}} where T` として再構成し、`Tuple{AbstractVector}` への subtype /
  strict dominance に使えることを unit test で固定した。
- #5926 の morespecific/topological selection 化では、nested typevar を落とした
  lossy signature では dominance 判定が壊れるため、MethodTable の structured signature
  経路を prerequisite coverage として守る。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926_method_sig_where_wrap_preserves_nested_vector_typevar`。

### Bounded typevar dispatch regression coverage (Issue #5926)

- `h(x::T) where T<:Number` が `h(x)` の untyped `Any` fallback より優先される
  #5375 regression を、`MethodTable::dispatch_inner` と `Vm::find_best_method_index_uncached`
  の両方の実 dispatch selection test として固定した。
- #5926 の morespecific/topological selection 化で `type_reuse_bonus` や dominance pre-check を
  変更しても、bounded typevar が fallback に負ける退行を両選択サイトで検出できる。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926_method_table_preserves_bounded_typevar_over_untyped_any_5375 test_find_best_method_index_issue_5926_preserves_bounded_typevar_over_untyped_any_5375`。

### Dispatch dominance selection-site characterization (Issue #5926)

- `MethodTable::dispatch_inner` と `Vm::find_best_method_index_uncached` の両方で、
  #5926 の dominance pre-check が `Vector{T} where T` を `AbstractVector` fallback より
  優先することを unit test で固定した。
- morespecific 本体への移行では compile-time / runtime の 2 選択サイトを同時に更新する
  必要があるため、fam1 representative を両サイトの回帰テストにした。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926_method_table_dominance_selects_vector_over_abstractvector test_find_best_method_index_issue_5926_dominance_selects_vector_over_abstractvector`。

### Legacy inference bare-key co-write migration characterization (Issue #5939)

- `infer_function_with_arg_types` の `name(declared_param_types)` primary key と
  legacy bare `func.name` lookup の乖離を engine unit test で固定した。
- 同じ call-site argtypes に対して `f(::Any)` と `f(::Int64)` の inferred result は
  primary key では分離される。#5939 の MethodInstanceKey 化では、bare lookup が method identity を
  持たない時に first-writer を返さないようにする必要がある。
- Verification: `timeout 1800 cargo nextest run --release --lib test_issue_5939_primary_keys_preserve_method_identity_without_bare_co_write`。

### Bare callee backedge over-invalidation characterization (Issue #5939)

- `CachedReturn.edges` が bare callee name だけを持つ legacy backedge では、
  `callee(::Float64)` の mutation でも `callee(::Int64)` に依存した caller cache を
  保守的に retire する現状挙動を engine unit test で固定した。
- `DispatchedMethodEdge` 付きの精密 method edge は既に unmatched signature を温存できるため、
  残る #5939 work は bare `edges: BTreeSet<String>` を method-instance identity へ置き換える
  storage/recording/invalidation migration。
- Verification: `timeout 1800 cargo nextest run --release --lib test_issue_5939_bare_callee_edge_overinvalidates_unmatched_signature`。

### Module-qualified `Diagonal` runtime multiplication dispatch (Issue #6117)

- `LinearAlgebra.Diagonal{T}` のように module-qualified な parametric struct instance が
  bare `Diagonal` family へ subtype match できるよう、`CoreType` / `JuliaType`
  の struct family 比較で module prefix を正規化する。
- SVD が返す `F.U` / `F.S` から `F.U * Diagonal(F.S) * F.Vt` を runtime dispatch
  しても `MethodError: no method matching operator(Matrix{Float64}, LinearAlgebra.Diagonal{Float64})`
  に落ちない。
- `hasmethod(*, Tuple{typeof(F.U), typeof(Diagonal(F.S))})` はまだ upstream と異なり
  false を返すため、reflection 側の follow-up bug として #6124 を作成済み。
- Verification: upstream Julia direct で `linalg/diagonal_test.jl` /
  `linalg/matmul_svd_reconstruct.jl` / mobile + iOS SVD samples、
  `target/release/sjulia` direct で同 fixture/sample、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm module_qualified_parametric_struct_subtypes_bare_family_issue_6117`、
  `timeout 1800 cargo nextest run --release --test fixture_tests linalg::`、
  `timeout 1800 cargo check -p subset_julia_vm --lib`、
  `timeout 1800 cargo clippy --all-targets -- -D warnings`、
  `bash scripts/check_fixture_test_names.sh`、`git diff --check`。

### Tuple user-parametric bare-family covariance (Issue #6116)

- `Foo{Int64} <: Foo` のような user parametric instance と bare family の subtype が
  CoreType subtype で true になり、tuple covariant element check でも
  `Tuple{Foo{Int64}} <: Tuple{Foo}` が通る。
- 既存 #5064 fixture の direct sjulia failure (`v isa Tuple{Foo}` /
  `Tuple{Foo{Int}} <: Tuple{Foo}`) は解消済み。
- Verification: upstream Julia と `target/release/sjulia` direct で
  `tuple/tuple_user_parametric_covariance_5064.jl`、
  `timeout 1800 cargo build --release --bin sjulia --features repl`、
  `timeout 1800 cargo nextest run --release --test fixture_tests tuple::`、
  `timeout 1800 cargo nextest run --lib parametric_structs_are_invariant_but_match_bare_base test_check_subtype_core_gate_handles_authoritative_runtime_pairs`、
  `timeout 1800 cargo check -p subset_julia_vm --lib`、
  `timeout 1800 cargo clippy --all-targets -- -D warnings`、`git diff --check`。

### `isa(::Type{...})` runtime subtype routing (Issue #5921)

- `isa(x, Type{...})` の first-class Type special case は `Vm::check_subtype` を使い、
  `<:` builtin と同じ runtime subtype entry point に寄せている。
- `isa` の kind-level final fallback (`Vector isa Type`, `Vector isa UnionAll` など) は
  まだ `JuliaType` subtype 経路に残る。runtime string subtype 側に kind hierarchy を
  統合するまではこの境界を保持する。
- 検証で `tuple_user_parametric_covariance_5064.jl` の direct sjulia failure を確認し、
  再発 bug として #6116 を作成済み。今回の #5921 slice では workaround しない。
- Verification: upstream Julia direct で `types/typeof_first_class_5068.jl` /
  `dispatch/subtype_isa_first_class_5115.jl` /
  `tuple/tuple_user_parametric_covariance_5064.jl` /
  `reflection/isa_typeof_kind_consistency_3909.jl`、
  `target/release/sjulia` direct で `types/typeof_first_class_5068.jl` /
  `dispatch/subtype_isa_first_class_5115.jl` /
  `reflection/isa_typeof_kind_consistency_3909.jl`、
  `timeout 1800 cargo build --release --bin sjulia --features repl`、
  `timeout 1800 cargo nextest run --lib builtins_types`、
  `timeout 1800 cargo nextest run --release --test fixture_tests types:: dispatch:: reflection::`、
  `timeout 1800 cargo check -p subset_julia_vm --lib`、
  `timeout 1800 cargo clippy --all-targets -- -D warnings`、`git diff --check`。

### Type value equality via mutual subtyping (Issue #5921)

- runtime type-object equality は `type_objects_equal` 経由で
  `type_values_subtype(left, right) && type_values_subtype(right, left)` に基づく。
- `Tuple === Tuple{Vararg{Any}}` / `Tuple == Tuple{Vararg{Any}}` が runtime fixture で
  upstream Julia と一致する。
- Verification: upstream Julia と `target/release/sjulia` direct で
  `operators/type_value_equality_mutual_subtype_5921.jl`、
  `bash scripts/check_fixture_test_names.sh`、
  `timeout 1800 cargo build --release --bin sjulia --features repl`、
  `timeout 1800 cargo nextest run --release --test fixture_tests operators::`、
  `timeout 1800 cargo nextest run --lib test_type_objects_equal_uses_mutual_subtyping test_type_values_subtype_uses_julia_subtype_relation`、
  `timeout 1800 cargo check -p subset_julia_vm --lib`、
  `timeout 1800 cargo clippy --all-targets -- -D warnings`、`git diff --check`。

### Android Flutter REPL Enter evaluation (Issue #6110)

- Android soft keyboard の Enter/done action が Flutter REPL の current input を評価し、
  再生ボタンと同じ `_evaluate` path で履歴へ追加される。
- hardware Enter / numpad Enter も modifier 無しでは評価として扱い、履歴移動や
  Unicode completion の既存 keyboard handling と共存する。
- Regression: `mobile/test/widget_test.dart` に TextInputAction.done 経路の widget test を追加。
- Verification: `git diff --check` pass。Flutter/Dart SDK がこの shell の PATH に無いため、
  `flutter test` は未実行。

### Runtime `<:` / `>:` builtin subtype path unification (Issue #5921)

- `BuiltinId::Subtype` / `BuiltinId::SupertypeOp` の DataType-only fast path を削除し、
  `<:` / `>:` の runtime 判定を `subtype_operand_name` → `Vm::check_subtype`
  に一本化した。
- core DataType ペアだけ `type_values_subtype` を直接呼ぶ分岐が消え、user hierarchy
  DataType や callable `Ref` operand と同じ subtype entry point を使う。
- Verification: upstream Julia direct で `operators/subtype_basic.jl` /
  `operators/operators_supertype.jl` / `dispatch/subtype_isa_first_class_5115.jl`、
  `timeout 1800 cargo build --release --bin sjulia --features repl`、
  `target/release/sjulia` direct で同 3 fixture、
  `timeout 1800 cargo nextest run --lib builtins_types`、
  `timeout 1800 cargo nextest run --release --test fixture_tests operators:: dispatch:: ref_tests:: comparison:: types_tests::`、
  `timeout 1800 cargo check -p subset_julia_vm --lib`、
  `timeout 1800 cargo clippy --all-targets -- -D warnings`。

### VM `type_matches` numeric/range subtype unification (Issue #5921)

- `Vm::type_matches` の numeric abstract / concrete と range family の runtime match を
  `Vm::check_subtype` に委譲し、`Integer` / `Signed` / `Unsigned` /
  `AbstractRange` などの局所 hardcode を削除した。
- `Rational` / `Complex` の Pure Julia struct parent は `StructHierarchy` を入れた
  `check_subtype` 側で解決し、dispatch matching 側の個別 fallback を減らした。
- Verification: `timeout 1800 cargo nextest run --lib runtime_type_matches_abstract_numeric_params_via_core_subtype_issue_5921`、
  `timeout 1800 cargo nextest run --lib vm::type_ops::comparison`、
  `timeout 1800 cargo nextest run --lib vm::tests::runtime_type_matches_abstract_numeric_params_via_core_subtype_issue_5921 vm::tests::test_runtime_diagonal_type_var_rejects_mixed_bigint_rational`、
  `git diff --check`、`timeout 1800 cargo check -p subset_julia_vm --lib`、
  `timeout 1800 cargo clippy --all-targets -- -D warnings`。

### Termux Docker smoke (Issue #6021)

- `termux/termux-docker` 上で Termux package の `rust` / `clang` /
  `pkg-config` / `openssl` を使い、`subset_julia_vm` library が
  `x86_64-linux-android` host toolchain で type-check できることを確認した。
- `docker/Dockerfile.termux` と `docker/README.md` に再現手順を追加した。

### Raspberry Pi 32-bit Docker smoke (Issue #6017)

- armv7/armhf Docker smoke は QEMU 下で release `sjulia` build が 43m31s かかるため、
  smoke build timeout を 3600 秒へ拡大した。
- 2026-06-07 の smoke run で `target/release/sjulia` が `ELF 32-bit ... ARM`
  executable として生成され、`Sys.WORD_SIZE == 32` / `Int === Int32` /
  `UInt === UInt32` assertions と `println(1 + 2)` が pass した。
- README には binfmt 登録、Docker smoke、遅い環境向けの
  `armv7-unknown-linux-gnueabihf` host-side `cargo check` fallback を記載した。

### native word-size `Int` / `UInt` aliases (Issues #6097, #6105)

- bare `Int` / `UInt` は target pointer width に応じた concrete DataType へ解決される。
  64-bit では `Int64` / `UInt64`、32-bit では `Int32` / `UInt32`。
- `JuliaType` と `CoreType` の alias normalization、compiler/AoT inference、VM convert、
  typed-array element metadata の代表経路を同じ native alias helper に寄せた。
- Raspberry Pi 32-bit smoke は native alias assertion を含むようになった。
- 32-bit runtime smoke は QEMU release build timeout で未完了だが、
  armv7 target の `cargo check` と portable reflection fixture で compile-time alias surface を確認済み。

## AoT (Ahead-of-Time) Pipeline Status

**Tracking Issue**: [#2596](https://github.com/AtelierArith/ailujsoi/issues/2596)

AoT パイプラインは Julia ソースコードを Rust に変換し、ネイティブ実行するためのインフラ。~20,500 行、30 ファイルで構成。

```text
Julia source → Parser → Core IR → AoT Analyze → AoT IR → Optimizer → Codegen → Rust Code
```

### Current Status

| Step | Issue | Status | Description |
|------|-------|--------|-------------|
| 1. Compilation fixes | [#2590](https://github.com/AtelierArith/ailujsoi/issues/2590) | ✅ Closed | 24+ compilation errors fixed (PR #2599) |
| 2. Import fixes | [#2592](https://github.com/AtelierArith/ailujsoi/issues/2592) | ✅ Closed | Type/module imports resolved (PR #2599) |
| 3. Enum support | [#2591](https://github.com/AtelierArith/ailujsoi/issues/2591) | 🔧 Open | `Stmt::EnumDef` and `JuliaType::Enum` across AoT components |
| 4. E2E tests | [#2593](https://github.com/AtelierArith/ailujsoi/issues/2593) | ✅ Closed | 35+ E2E tests compilable and runnable (PR #2599) |
| 5. Cranelift JIT | [#2594](https://github.com/AtelierArith/ailujsoi/issues/2594) | 🔧 Open | Function calls, array/field access, phi nodes, libm linking |
| 6. E2E verification | [#2595](https://github.com/AtelierArith/ailujsoi/issues/2595) | 🔧 Open | Julia → Rust → compile → run full pipeline |
| CI prevention | [#2600](https://github.com/AtelierArith/ailujsoi/issues/2600) | ✅ Closed | GitHub Actions CI with `--features aot` (PR #2660) |

### Key Components

| Component | Path | Description |
|-----------|------|-------------|
| Analyze | `subset_julia_vm/src/aot/analyze/` | Core IR analysis, IR conversion |
| Inference | `subset_julia_vm/src/aot/inference/` | Type inference engine |
| IR | `subset_julia_vm/src/aot/ir/` | AoT intermediate representation |
| Optimizer | `subset_julia_vm/src/aot/optimizer/` | 8 optimization passes (constant folding, CSE, DCE, inlining, loop optimization, LICM, strength reduction) |
| Codegen | `subset_julia_vm/src/aot/codegen/` | Rust code generation |
| Call Graph | `subset_julia_vm/src/aot/call_graph.rs` | Function call graph analysis |
| CLI | `subset_julia_vm/src/bin/aot.rs` | AoT compiler binary |
| Runtime | `subset_julia_vm_runtime/` | Runtime support for AoT-compiled code |
| E2E Tests | `subset_julia_vm/tests/aot_e2e_tests.rs` | 35+ end-to-end tests |
| Core IR AoT Tests | `subset_julia_vm/tests/core_ir_aot_tests.rs` | Core IR save/load and AoT conversion tests |

---

## 設計方針

### Pure Julia 優先

**Rust 依存を最小化し、可能な限り Pure Julia で実装する**

- 新規配列操作の約 80% は Pure Julia で実装可能
- Rust は最小限のプリミティブ (zeros, length, push!) と HOF (map, filter) のみ
- VM コア変更なしで新機能追加が可能

---

### Higher-Order Function (HOF) Support Status

**Status: Fully operational** (Issues #1665, #1671)

| Feature | Status | Notes |
|---------|--------|-------|
| `map(f, arr)` | Pure Julia | Falls through from `compile_builtin_hof` → method table dispatch |
| `map(f, A, B)` | Pure Julia | Binary map via `zip()` |
| `filter(f, arr)` | Pure Julia | Via `Filter` wrapper + `collect` |
| `reduce(op, arr)` / `foldl` / `foldr` | Pure Julia | iterate-based loop |
| `mapreduce` / `mapfoldl` / `mapfoldr` | Compiled builtin | Arity-aware operator resolution (Issue #2004) |
| `map!` / `filter!` | Compiled builtin | In-place operations |
| `foreach(f, arr)` | Pure Julia | Side-effect-only application |
| `any(f, arr)` / `all(f, arr)` | Compiled builtin | Returns `Bool` (Issue #2031) |
| `count(f, arr)` / `sum(f, arr)` | Compiled builtin | Predicate counting / mapped sum |
| `findall` / `findfirst` / `findlast` | Compiled builtin | Predicate-based index search |
| `broadcast(f, A [, B])` | Compiled builtin (Pure Julia migration in progress) | With type inference |
| `ntuple(f, n)` | Compiled builtin | Tuple generation via function |

**HOF argument forms:**
- Named functions: `map(triple, arr)` — resolved via `FunctionRef` (Issue #1658)
- Lambdas: `map(x -> x^2, arr)` — lowered to `FunctionRef`
- Bare operators: `reduce(+, arr)` — lowered to `FunctionRef` with arity (Issue #1985)

**HOF chaining:** Fully supported — `map(f, filter(g, arr))` works via natural function composition (Issue #2072)

**Type inference:** Call-site specialization for `map`, `filter`, `reduce` return types in `infer.rs`

---

### Scalar-Array Binary Operations Coverage (Issue #1797)

**Status: Fully operational**

The following matrix shows which scalar-array operation combinations are supported:

| Operation | Int64 × Int64[] | Float64 × Float64[] | Int64 × Float64[] | Float64 × Int64[] | Complex × Float64[] |
|-----------|-----------------|---------------------|-------------------|-------------------|---------------------|
| `*` (plain) | ✅ | ✅ | ✅ | ✅ | ✅ |
| `.*` (broadcast) | ✅ | ✅ | ✅ | ✅ | ✅ |
| `/` (plain) | ✅ | ✅ | ✅ | ✅ | ✅ |
| `./` (broadcast) | ✅ | ✅ | ✅ | ✅ | ✅ |
| `.+` (broadcast) | ✅ | ✅ | ✅ | ✅ | ✅ |
| `.-` (broadcast) | ✅ | ✅ | ✅ | ✅ | ✅ |

**Notes:**
- Plain `scalar * array` and `array * scalar` are equivalent to broadcast `.* ` (Issue #1795)
- Plain `scalar / array` and `array / scalar` are equivalent to broadcast `./` (Issue #1929)
- Addition and subtraction require explicit broadcast syntax (`.+`, `.-`)
- All operations support commutativity: `scalar op array` ≡ `array op scalar` (except for division)

**Test coverage:** `subset_julia_vm/tests/fixtures/broadcast/`
- `scalar_array_mul.jl` - broadcast multiplication
- `scalar_array_add.jl` - broadcast addition/subtraction
- `scalar_array_div.jl` - broadcast division
- `scalar_array_plain_mul.jl` - plain multiplication (Issue #1795)
- `scalar_array_plain_div.jl` - plain division (Issue #1929)
- Regression suite (Issue #2550): `broadcast_regression_dotops.jl`, `broadcast_regression_unary.jl`, `broadcast_regression_nested.jl`, `broadcast_regression_mixed_types.jl`, `broadcast_regression_ref.jl`, `broadcast_regression_tuple.jl`, `broadcast_regression_inplace.jl`

---

### Broadcast Pure Julia Migration (Issues #2544-#2552)

**Status: In progress (Phase 5 + Phase 7-2/3/4 complete, .= lowering done)**

Migrating VM-level broadcast instructions to Pure Julia implementation based on Julia's `base/broadcast.jl`. This enables loop fusion and custom BroadcastStyle dispatch.

**Recent progress (2026-02-13):**
- `.=` broadcast assignment lowering implemented (PR #2805) — `Z .= expr` lowers to `materialize!(Z, expr)`
- `copyto!` fast paths for f64, generic, and scalar broadcast operations (PR #2807, #2809)
- Broadcast dispatch path and specialization typing improvements (PR #2815, #2817)
- AoT pipeline can compile Mandelbrot broadcast + Complex + @time (PR #2818)

| Phase | Issue | Status | Description |
|-------|-------|--------|-------------|
| 5-1 | #2544 | ✅ Done | flatten / isflat (loop fusion foundation) |
| 5-2 | #2545 | ✅ Done | AndAnd / OrOr (short-circuit broadcast) |
| 6-1 | #2546 | 🔧 Blocked | dot syntax lowering → Broadcasted generation |
| 6-2 | #2547 | 🔧 Blocked | @__dot__ / @. macro |
| 6-3 | #2548 | 🔧 Blocked | broadcast / broadcast! Pure Julia entry points |
| 7-1 | #2549 | 🔧 Blocked | VM instruction deprecation |
| 7-2 | #2550 | ✅ Done | Regression test suite (7 new tests) |
| 7-3 | #2551 | ✅ Done | show / display methods |
| 7-4 | #2552 | ✅ Done | Documentation update |

**Dependencies:** Phase 6 requires Phase 0-4 (language prerequisites, core types, shape computation, indexing, materialization).

---

## Memory{T} Primitive Status

**Tracking Issue**: [#2768](https://github.com/AtelierArith/ailujsoi/issues/2768)

`Memory{T}` は Julia 1.11 の `GenericMemory` に対応する Rust ネイティブプリミティブ。`Value::Array` の内部ストレージを段階的に `Memory{T}` へ移行するための基盤。

### Implementation Status

| Component | Status | Description |
|-----------|--------|-------------|
| `Value::Memory` variant | ✅ Done | `MemoryValue`/`MemoryRef` Rust types (PR #2771) |
| VM instructions | ✅ Done | `NewMemory`, `NewMemoryDynamic`, `MemoryGet`, `MemorySet`, `MemoryLength` (PR #2771) |
| Compiler support | ✅ Done | `Memory{T}(undef, n)` constructor compilation (PR #2774) |
| Builtin functions | ✅ Done | `memoryref`, `memoryrefget`, `memoryrefset!`, `memorynew` migration (PR #2776) |
| Dict migration | ✅ Done | Dict uses Memory-based open-addressing hash table (PR #2773) |
| FFI support | ✅ Done | Memory display in FFI layer (PR #2777) |
| Phase 2: Value::Memory handling | ✅ Done | Medium-impact files handle Value::Memory (PR #2780) |

### Array Instruction Deprecation Analysis

以下の旧 Array VM 命令はすべて **現在も使用中** であり、まだ非推奨化できない:

| Instruction | Usage Count | Used In | Deprecation Status |
|-------------|-------------|---------|-------------------|
| `NewArray` | Active | `collection.rs` (array literal compilation) | ❌ Still needed |
| `NewArrayTyped` | Active | `collection.rs`, `expr/mod.rs`, `specialize.rs` | ❌ Still needed |
| `PushElem` | Active | `array_basic.rs` (VM execution) | ❌ Still needed |
| `PushElemTyped` | Active | `expr/mod.rs`, `specialize.rs` | ❌ Still needed |
| `FinalizeArray` | Active | `collection.rs` (comprehension compilation) | ❌ Still needed |
| `FinalizeArrayTyped` | Active | `collection.rs`, `expr/mod.rs`, `specialize.rs` | ❌ Still needed |
| `LoadArray` | Active | `collection.rs`, `builtin.rs`, `stmt.rs`, `expr/mod.rs` | ❌ Still needed |
| `StoreArray` | Active | `collection.rs`, `builtin.rs`, `stmt.rs`, `expr/mod.rs` | ❌ Still needed |
| `PushArrayValue` | Active | `expr/mod.rs` (constant array literals) | ❌ Still needed |
| `AllocUndefTyped` | Active | `collection.rs` (Array{T}(undef, dims)) | ❌ Still needed |

**Value::Array**: 305 occurrences across 39 files — 主要なデータ構造として広く使用中。

**Value::Memory**: 88 occurrences across 27 files — 新規パスで増加中。

**Next Steps** (Issue #2768):
1. Array{T,N} を Pure Julia mutable struct として実装し、内部ストレージを Memory{T} に移行
2. コンパイラの配列リテラル生成を Memory{T} ベースに段階的に切り替え
3. 旧 Array VM 命令を順次非推奨化・削除

---

## Pure Julia Dict{K,V} Migration — Milestone 9

**Tracking Issue**: [#2738](https://github.com/AtelierArith/ailujsoi/issues/2738)

Julia 本家の `Dict{K,V}` (`julia/base/dict.jl`) を Pure Julia `mutable struct` として定義。Rust `Value::Dict` と Pure Julia `Dict{K,V}` struct のデュアルディスパッチ方式で共存。

### Prerequisites (All Completed)

| Prerequisite | Issue | Status |
|-------------|-------|--------|
| `Memory{T}` typed memory buffer | [#2746](https://github.com/AtelierArith/ailujsoi/issues/2746) | ✅ Closed |
| Parametric types for `mutable struct` | [#2744](https://github.com/AtelierArith/ailujsoi/issues/2744) | ✅ Closed |
| `Dict{K,V}` type parameter runtime tracking | [#2737](https://github.com/AtelierArith/ailujsoi/issues/2737) | ✅ Closed |

### Implementation Status

| Phase | Issue | PR | Status | Description |
|-------|-------|-----|--------|-------------|
| 1. `AbstractDict{K,V}` parametric | [#2745](https://github.com/AtelierArith/ailujsoi/issues/2745) | #2786 | ✅ Done | `abstract type AbstractDict{K,V} <: Any end` in `boot.jl`、lowering の parametric abstract type パーサー拡張 |
| 2. Hash table helpers | [#2747](https://github.com/AtelierArith/ailujsoi/issues/2747) | #2785 | ✅ Done | `_tablesz`, `_shorthash7`, `hashindex`（定数 `maxallowedprobe`, `maxprobeshift`） |
| 3. Dict{K,V} Pure Julia struct | [#2748](https://github.com/AtelierArith/ailujsoi/issues/2748) | #2788 | ✅ Done | `mutable struct Dict{K,V} <: AbstractDict{K,V}`（8 フィールド untyped）、コアアルゴリズム・公開 API・ディスパッチガード |
| 4. Infrastructure gaps tracking | [#2738](https://github.com/AtelierArith/ailujsoi/issues/2738) | #2787 | ✅ Closed | 全ギャップ解消確認 |

### Architecture: Dual Dispatch Model

| Aspect | Rust-backed `Value::Dict` | Pure Julia `Dict{K,V}` struct |
|--------|--------------------------|-------------------------------|
| 構築方法 | `Dict()` / `Dict{K,V}()` with pair/empty args | struct constructor（非 pair 引数） |
| ストレージ | Rust `HashMap<DictKey, Value>` | `Vector{Int64}` (slots) / `Vector{Any}` (keys, vals) |
| ディスパッチ | `::Dict` bare annotation | `::Dict{K,V} where {K,V}` annotation |
| ハッシュ関数 | Rust `Hash` trait | Julia `hash()` + open-addressing linear probing |
| 型パラメータ | Rust-level tracking | Julia-level `{K,V}` parametric type |

---

## 目指す方向性

### VM アーキテクチャ改善

Julia 本体のソースコード（`julia/src/intrinsics.h`, `julia/src/builtin_proto.h`）の分析に基づき、3 層アーキテクチャへの移行を計画：

```
Layer 3: SubsetJulia Code     ← Julia コードで実装可能な関数
Layer 2: Builtin Functions    ← Rust 実装の組み込み関数
Layer 1: VM Intrinsics        ← 最小限の固定命令セット（約 50 命令）
```

**目標**:
- VM 命令を約 100 種類 → 約 50 種類に削減
- 新機能追加時に VM コア変更不要
- `sin`, `cos`, `map`, `filter` 等は Builtin 関数として実装

**最近の進捗**:
- `Abs` 命令を削除し Builtin 関数に移行
- プリコンパイル JSON 機構を廃止し、起動時パースに移行

詳細:
- [ARCHITECTURE_OVERVIEW.md](./ARCHITECTURE_OVERVIEW.md) - 現行アーキテクチャ概要
- [BUILTIN_REMOVAL.md](./BUILTIN_REMOVAL.md) / [BUILTIN_OWNERSHIP.md](./BUILTIN_OWNERSHIP.md) - Builtin route と handler ownership

---

## 参考資料

- [DONE.md](./DONE.md) - 実装済み機能一覧
- [UNIMPLEMENTED.md](./UNIMPLEMENTED.md) - 未実装機能一覧
- [archived/REFERENCES_20260105.md](./archived/REFERENCES_20260105.md) - 2026-01-05 技術リファレンススナップショット
- [TESTING.md](./TESTING.md) - テスト戦略
- [archived/implementation_plans.md](./archived/implementation_plans.md) - 完了した実装計画アーカイブ
- [CLAUDE.md](../../CLAUDE.md) - プロジェクト概要とビルド手順
