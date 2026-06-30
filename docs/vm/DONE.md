# 実装済み一覧 (DONE)

**最終更新**: 2026-06-30. 実装済み項目は下の日付別「最新対応」セクションを正とし、先頭メタデータには長い issue 要約を重複させない。

> 更新方針 (Issue #3760): 新しい項目は日付ごとの共有 `## ...YYYY-MM-DD...` 見出しの下に、Issue ごとの `### ... (Issue #NNNN)` 小見出しとして追加する。同日の見出しが既にある場合は、その下に小見出しを追加し、先頭に新しい独立セクションを増やさない。
>
> 過去分(2026-06-06 以前)は [archive/DONE-2026.md](./archive/DONE-2026.md) にアーカイブ済み (Issue #6341)。年が変わったら前年分を `archive/DONE-<YYYY>.md` へ移す。

---

## 最新対応 (2026-06-30)

### REPL callable residue-ring parent persistence fixed ✅ (Issue #8496)

- `resolve_global_types()` now falls back to the provided REPL global type when
  a parametric struct name cannot be resolved against the freshly built struct
  table, matching its documented behavior instead of silently dropping the
  binding's compile-time type.
- Split REPL evaluations now keep persisted package parent objects callable:
  `Z7 = residue_ring(ZZ, 7)[1]` followed by `a = Z7(10)` succeeds and returns
  the expected residue element.
- Regression coverage pins the iOS-style REPLSession path.

### iOS full test suite restarts fixed ✅ (Issue #8489)

- `VMBridge` and `REPLSessionManager` no longer assume Swift-side FFI result
  structs contain every newest Rust field; they keep the ABI-stable prefix and
  read artifact strings through optional exported C accessors.
- `repl_result_artifact_mime` / `repl_result_artifact_data` are now exported and
  declared in `subset_vm.h`, matching the detailed execution result accessor
  pattern.
- The stale individual sample test name was updated to `Pendulum Animation`,
  REPL concurrency coverage now uses lightweight deterministic inputs, and
  sample performance benchmarks are skipped unless `SJULIA_IOS_PERF_TESTS=1`.
- Verification: the full iOS Simulator unit suite passed with 172 tests, 2
  opt-in performance tests skipped, and no app restarts.

### AoT CodeInstanceKey / InferenceCacheKey unification ✅ (Issue #8372)

- AoT `CodeInstanceKey` now embeds the compile-side `InferenceCacheKey` for
  specialization identity instead of carrying a parallel AoT argument-key type.
- The AoT inference engine now feeds call-site literals into shared
  `CacheArgType` slots and keeps `StaticType` only as the ABI/codegen layout
  projection.
- Regression coverage asserts direct `InferenceCacheKey` storage and keeps the
  Issue #4272 compile/AoT const-specialization agreement pinned on the unified
  key type.

### Base cache schema fingerprint invalidation ✅ (Issue #8444)

- Base cache serialization now writes explicit schema and compiler build
  fingerprints in the envelope, so a stale cache from an incompatible
  bytecode/method-table schema fails cleanly before payload decode.
- The schema fingerprint is generated from an explicit manifest of
  serialized-shape inputs (`Instr`, method-table wire format, inference cache
  keys, Core/Value type schemas, and VM compiled-program metadata).
- `scripts/audit_base_cache_schema_fingerprint.sh` compares the current schema
  hash and `CACHE_VERSION` against the checked-in snapshot; Issue #8491 tracks
  wiring it into CI with a workflow-scoped token. Regression coverage rewrites
  a same-version cache envelope with a stale schema fingerprint and asserts a
  clean rejection.

### Parser diagnostics line/column formatting and recovery ✅ (Issue #8454)

- `subset_julia_vm_parser::Span` now displays as stable line/column ranges,
  and all `ParseError` messages use that display instead of debug-formatted
  spans.
- `format_with_context` now renders multi-line carets for spans that cross line
  boundaries, with stable exact-message coverage.
- Regression coverage: parser unit tests pin line/column error text, multi-line
  context output, and two recovered independent syntax errors formatted through
  `ParseErrors::format_all`.

### iOS code completion state crash fixed ✅ (Issue #8487)

- `CodeCompletionState` and `UnicodeCompletionState` now keep replacement
  ranges as character offsets and reconstruct `String.Index` ranges only for
  the current text being edited.
- The state objects no longer depend on `@Published` teardown and define a
  `nonisolated deinit`, preventing the Simulator crash seen during
  `CodeCompletionProviderTests`.
- Regression coverage applies both code and Unicode completions to copied but
  equivalent `String` instances.

### FFI typed result and header audit ✅ (Issue #8455)

- `CExecutionResult` now exposes owned structured `value_json` plus stable
  `CValueKind` tags and C accessors for complex parts, array elements,
  dictionary entry JSON, and plot artifact strings.
- `subset_vm.h` now declares all exported FFI functions, including
  `compile_and_run_streaming`, documents pointer ownership, and is checked as
  C11/C++17 by `scripts/check_ffi_header_compiles.sh`.

### WASM typed result API and unsupported-feature cleanup ✅ (Issue #8456)

- Web execution results now include `typed_value` objects for returned VM
  values, and `run_from_source_typed` gives JS callers an explicit typed
  result entrypoint.
- Regression coverage asserts array, complex, and plot artifact typed
  round-tripping, and `get_unsupported_features()` no longer reports user macro
  definitions as unsupported.

### iOS sample catalog schema and count audit ✅ (Issue #8457)

- `CodeSample` metadata decoding now uses the Swift `Category` and
  `Difficulty` enums directly instead of string post-processing.
- `scripts/check_ios_sample_catalog.sh` validates sample IDs, category and
  difficulty raw values, backing `.jl` files, extra unlisted files, and README
  counts; docs now reflect the 38-sample, 9-category catalog.

### iOS AbstractAlgebra.jl sample FFI warm-start ✅ (Issue #8463)

- Package and stdlib module methods outside `Main`/Base/Core without declared
  return annotations now record safe `Any` dispatch snapshots during
  method-table construction instead of eagerly inferring every package method at
  `using Package` startup.
- Native FFI detailed and streaming entrypoints now warm-start the Base-cache
  prefetch before parse/lower, matching the CLI startup overlap used by
  package-heavy samples.
- Verified the `AbstractAlgebra.jl` sample still takes the Base-cache HIT path;
  local CLI proxy time is about 1.04s in release and about 1.22s under
  `dev-fast` after dropping from the earlier ~2.25s baseline.

### Cached Base keyword/block-local callable shadowing fixed ✅ (Issue #8469)

- Base-cache reuse now falls back to full compilation when user functions,
  including user main-block functions and expression `let` blocks from `@testset`,
  shadow Base keyword callable names such as `retry(...; check=...)`.
- Regression coverage: focused cache unit tests for top-level, block, and
  `LetBlock` shadowing; `kwargs/typed_default_preservation.jl` passes on the
  default execution path without `SUBSET_JULIA_VM_DISABLE_CACHE`.

### Dense symmetric eigvals ordering fixed ✅ (Issue #8475)

- `LinearAlgebra.eigvals` now sorts real symmetric eigenvalues from the
  `SymmetricEigen` builtin path before returning the sjulia array wrapper.
- Regression coverage: `linalg_factorization_inplace_values_7464` now observes
  `eigvals!([2.0 1.0; 1.0 2.0])` values in ascending order while retaining the
  supported in-place mutation behavior.

### Tuple equality for OneTo/UnitRange axes fixed ✅ (Issue #8478)

- Tuple `==` now compares inline `OneTo` struct snapshots and native
  `UnitRange` values by their logical range sequence, matching upstream for
  `(Base.OneTo(2),) == (1:2,)`.
- Regression coverage: focused equality unit test plus the existing
  `iteration_product` fixture axes assertions.

### Iterators.product vararg wrapper recursion fixed ✅ (Issue #8479)

- The embedded `Base.Iterators.product(args...)` wrapper now mirrors the Base
  vararg body by constructing `Base.ProductIterator(args)` directly instead of
  re-dispatching through `Base.product(args...)`.
- Regression coverage: `product([1,2], [10,20], [100,200])` construction and
  the existing `iteration_product` fixture.

### Dict getindex on statically typed Dict slots fixed ✅ (Issue #8480)

- Statically typed `Dict` indexing now emits the shared `IndexLoad` getindex
  path instead of the `get(dict, key[, default])` builtin trampoline.
- Regression coverage: `dict_getbang_writeback_5225` now preserves the
  `get!` counting loop result and subsequent `counts["l"]` access under the
  release fixture harness.

### Built-in irrational constants preserve singleton bindings ✅ (Issue #8481)

- Bare `pi`, `π`, and `ℯ` references now let the registered global const-struct
  bindings win before the legacy Float64 fallback.
- Regression coverage: `mathconstants_irrational_singletons_5133` now sees
  upstream-compatible `Irrational{:π}` / `Irrational{:ℯ}` identities while
  numeric conversions still produce the expected Float64/Float32 values.

### Testset-local varargs tuple slots accept dynamic values ✅ (Issue #8482)

- Tuple-typed slots now accept dynamically typed values without compile-time
  rejection, matching the existing boxed aggregate handling for structs and
  arrays.
- Regression coverage: `splat_tuple_literal_7741` now compiles a
  `@testset`-local `f(xs...) = (10, 20, xs...)` helper and verifies tuple
  splicing.

### Typed varargs fixture restored to upstream parity ✅ (Issue #8483)

- `varargs_typed.jl` no longer assumes `sum(()) == 0`; the empty-varargs
  computation path returns the leading argument before calling `sum`.
- Regression coverage: upstream Julia and sjulia both accept the corrected
  fixture shape while retaining the typed-varargs zero-argument `length` tests.

### Clippy all-targets Function literal build fixed ✅ (Issue #8468)

- Predicate-narrowing integration tests now set `is_runtime_eval: false` on
  their synthetic `Function` values, matching the new IR field introduced by
  the `@eval` world-age implementation.
- Verification: `cargo clippy --all-targets -- -D warnings` and
  `cargo nextest run --release --test issue_3710_predicate_narrowing`.

### `@eval` function world-age semantics ✅ (Issue #8452)

- `@eval f() = ...` inside a function now defines a global method in a newer
  world: the defining frame keeps the old method visibility, while subsequent
  top-level calls see the new method.
- Newly `@eval`-defined function names are bound globally, so
  `Base.invokelatest(h)` can call a method defined by `@eval h() = ...` after
  the older frame correctly raises `MethodError`.
- Regression fixture: `macros_eval_function_world_age_8452`.

### LinearAlgebra in-place factorization fixture parity fixed ✅ (Issues #8411, #8465)

- `linalg_factorization_inplace_values_7464` now returns `true` under upstream
  Julia and the `release-fast` fixture nextest harness.
- `cholesky!` and `isposdef!` now leave the attempted upper-Cholesky work state
  in the input matrix on the same failure path covered by Julia's
  `PosDefException(2)` behavior.
- Regression coverage: existing `linalg_factorization_inplace_values_7464`
  fixture plus the #8465 `cholesky!` / `isposdef!` MWE.

### Where lower/cross-bound static parameter parity fixed ✅ (Issue #8427)

- `where {T>:Int}` and `where {T<:Real,S<:T}` signatures now follow upstream
  Julia when static parameter bindings violate lower or cross-variable bounds:
  value-only method bodies still run, but reading the invalid static parameter
  raises `UndefVarError`.
- Regression fixture: `dispatch_where_bounds_upstream_applicability_8427`.
- Re-verified `dispatch::` fixtures and release lib tests.

### `DataType.name` TypeName identity ✅ (Issue #8451)

- `A.T.name === B.T.name` for same-named types in different modules now returns
  `false`, while `(Vector{Int}).name === (Vector{Float64}).name` returns `true`.
- `TypeName.name` projects the underlying symbol, so `Int.name.name === :Int64`
  works through the same runtime reflection value.
- Regression fixture: `reflection_datatype_name_typename_identity_8451`.

### Core type lattice parity regressions ✅ (Issue #8415)

- Fixed tuple/Array/value-parameter `typejoin`, diagonal tuple
  `typeintersect`, where lower/cross-bound dispatch, nested UnionAll subtype
  solving, and parenthesized UnionAll application parity with upstream Julia.
- Regression fixture: `types_coretype_parity_8415`.
- Re-verified `types_tests::` under release nextest.

### `sjulia -e` include path evaluation fixed ✅ (Issue #7766)

- `println(include("/tmp/file.jl"))` in `sjulia -e` now reaches Base runtime
  file evaluation and returns the included file's last expression value instead
  of failing during lowering with `UnsupportedFeature(IncludeCall(...))`.
- Regression test: `include_tests::test_eval_include_file_path_in_expression_position_7766`.

### Base.retry implemented ✅ (Issue #8371)

- `retry(() -> 42)()` now matches upstream, and retry wrappers support a
  delayed retry, `check=false`, and positional/keyword forwarding through the
  returned closure.
- Function-valued splat calls now fall back to dynamic callable dispatch when
  the callee is not statically known, fixing the captured `f(args...)` path
  needed by `retry` (Issue #8434).
- Regression fixtures: `retry_8371`, updated `hof/variadic_splat`.

### Nested `rethrow()` propagation fixed ✅ (Issue #8435)

- `try; try; error("x"); catch; rethrow(); end; catch; ... end` now reaches
  the outer catch with the original `ErrorException`, matching upstream Julia.
- `retry(f; check=...)` now uses upstream-style `rethrow()` for rejected retries
  instead of the retired W-50 `throw(err)` workaround.
- Regression fixtures: `exceptions_rethrow_nested_8435`, existing
  `exceptions/rethrow.jl`, and `retry_8371`.

### Partial SymTridiagonal constructor fixed ✅ (Issue #8393)

- `SymTridiagonal{Float64}(dv, ev)` now fixes the leading element type
  parameter and infers the vector-storage parameter from the constructor
  fields, matching upstream as
  `LinearAlgebra.SymTridiagonal{Float64, Vector{Float64}}`.
- Regression fixture: `linalg_symtridiagonal_partial_constructor_8393`.
- Re-verified `packages_quadgk_scalar_integrals_8140`.

### Parenthesized UnionAll application fixed ✅ (Issue #8430)

- `(Vector{T} where T){Int}` now evaluates the parenthesized `UnionAll` base
  and applies `Int` at runtime, matching upstream as `Vector{Int64}`.
- Regression fixture: `types_unionall_apply_parenthesized_8430`.

### Distributions truncated sampling path fixed ✅ (Issue #8421)

- `td = truncated(Normal(), lo, hi); rand(td, n)` now keeps `td` typed as
  `Distributions.Truncated`, so sampling dispatches to the package `rand`
  methods instead of the native random-array dimension path.
- Regression fixture re-verified: `distributions_truncated_7325`.

### DataType-return fit_mle call-site regression covered ✅ (Issue #8414)

- Added a regression fixture for the Binomial `fit_mle` call-site that previously
  treated the direct `fit_mle(Binomial, n, data)` assignment as `nothing`.
- Regression fixture: `distributions_fit_mle_datatype_return_8414`.

### Rational power keeps concrete integer field types ✅ (Issue #8418)

- `Rational{T} ^ Int64` now preserves the original rational element type for
  integer-backed rationals, matching upstream for `Rational{Int64}`,
  `Rational{Int32}`, and `Rational{BigInt}` smoke checks.
- Regression fixture: `rational_power_preserves_int64_8418`.

### Struct-backed range indexing by range ✅ (Issue #8416)

- `Base.OneTo(10)[2:4]`, `getindex(Base.OneTo(10), 2:4)`, `slice(r::AbstractRange,
  inds::AbstractRange)`, and `view(::OneTo, range)` now preserve lazy
  `UnitRange`/`StepRange` results instead of materializing `Vector`.
- Regression fixture re-verified: `range_struct_indexed_by_range_5842`.

### Module-private type objects in method bodies ✅ (Issue #8410)

- Module method bodies and the lazy runtime specializer now share the same
  defining-module type-object lookup for unqualified private types such as
  `Hidden`/`AAPerm`.
- Regression fixture: `modules_method_body_private_type_object_8410`.
- Re-verified the original package trigger
  `packages_abstract_algebra_perm_mvp_8306`.

### QuadGK semi-infinite interval cache invalidation ✅ (Issue #8408)

- Invalidated persistent package-loader caches written before the `let` tuple
  destructuring lowering fix, which could leave QuadGK's `handle_infinities`
  path with `si` unbound even on newer binaries.
- Regression fixture: `packages_quadgk_infinite_intervals_8408`.

### Matrix(::SymTridiagonal) eigvals range path ✅ (Issue #8391)

- `Matrix(::AbstractMatrix)` is reachable again for non-dense structured matrix
  values. The compiler keeps the bare `Matrix(x)` array-constructor fast path
  only when `x` is already known to be matrix-like; otherwise it dispatches to
  the public Julia constructor methods.
- Regression fixture: `linalg_symtridiagonal_matrix_eigvals_range_8391`.
- Re-verified the original package fixture chunk containing
  `packages_quadgk_scalar_integrals_8140`.

### Typed Matrix(::SymTridiagonal) constructor fixed ✅ (Issue #8395)

- `Matrix{Float64}(SymTridiagonal(...))` now returns a dense
  `Matrix{Float64}` instead of a `Vector{Float64}`.
- The compiler keeps `Matrix{T}(undef, dims...)` on the allocation fast path but
  lets valid one-argument typed matrix conversions use Julia constructor
  dispatch.
- Regression fixture: `linalg_symtridiagonal_typed_matrix_constructor_8395`.

### Module-qualified @eval exposes runtime module bindings ✅ (Issue #8362)

- `@eval M begin y = ... end; M.y` now matches upstream at top level: the
  module-qualified macro lowers through `Core.eval`, stores the binding in the
  target module, and later module field access can read it dynamically.
- Regression fixture: `macros_eval_module_qualified_8362`.

### QuadGK batched integrand bug closure ✅ (Issues #8373, #8375, #8377, #8378, #8380, #8382, #8383, #8384, #8385, #8386, #8387)

- Added a concrete `ArrayElementType::Nothing` path so `similar(Float64[],
  Nothing)` and sized variants report `Vector{Nothing}` / `eltype == Nothing`
  instead of `Vector{Any}`. The boxed storage keeps zero-length/sized allocation
  working while the logical element tag is preserved through reflection and
  compiler lattice conversion.
- Regression fixture: `array_similar_nothing_eltype_8387`.
- Re-verified the open milestone-52 `bug` MWEs for keyword forwarding/defaults,
  tuple RHS broadcast assignment, `eval(:(println(...)))`, tuple literal
  eltypes, tuple `fieldtypes`, `promote_type()`, and bounded `AbstractVector`
  parametric constructors.

### QuadGK scalar/Gauss support closure ✅ (Issues #8140, #8363, #8364, #8366, #8367, #8368, #8369, #8370)

- The bundled upstream QuadGK source now passes the scalar finite-interval
  integration fixture plus Gauss rule construction/rescaling:
  `quadgk(x -> x^2, 0.0, 1.0)`, `quadgk(sin, 0.0, 1.0)`,
  `cachedrule(Float64, 7)`, `QuadGK.gauss(Float64, 3)`, and
  `QuadGK.gauss(Float64, 3, 0.0, 2.0)`.
- Closed the supporting compatibility gaps with focused regression fixtures:
  numeric coefficient powers, typed comprehensions under runtime `where`
  parameters, `@inbounds` tuple-index assignment, keyword defaults on
  default/vararg/where methods, fused scalar-array broadcast assignment, n-ary
  `*` reduction in untyped keyword frames, and stateful `Iterators.filter`
  over tuples.
- Invalidated stale package-loader caches (version 14) so cached bundled package
  modules are rebuilt with the corrected lowering semantics.

### OrdinaryDiffEq keyword solve dispatch ✅ (Issue #8396)

- Bare exported and qualified `solve(...; kwargs...)` calls now keep enough
  resolved method-table context for runtime dispatch to choose the Tsit5 method,
  including keyword-vararg candidates.
- Covered by `packages_ordinarydiffeq_linear_solve_7363` and the existing
  OrdinaryDiffEq algorithm-dispatch fixture.

### Symbolics Num Dict-key indexing through Any receivers ✅ (Issue #8397)

- `IndexLoad(1)` now routes struct keys against pure-Julia `Dict` receivers to
  Dict dispatch instead of treating them as array indices. This fixes
  `Dict(Symbolics.Num=>...)[x]` in `packages_symbolics_substitute`.
- Extended `dict_indexing_any` with a user `struct <: Real` key regression.

### AbstractAlgebra YoungTableau linear indexing ✅ (Issue #8400)

- Base's local `AbstractMatrix` linear-index fallback now uses `::Integer`, so
  concrete subtype methods such as `getindex(::YoungTableau, ::Integer)` win for
  direct calls and `Y[i]`.
- Covered by `dispatch_abstract_matrix_integer_getindex_specificity_8400` and
  `packages_abstract_algebra_young_tableau_mvp_8302`.

### AbstractAlgebra alias where-bound dispatch ✅ (Issue #8406)

- Type-alias expansion now applies to `where` bounds in lowering and compiler
  method signatures, including uniquely resolvable qualified alias leaves.
- Covered by `dispatch_alias_where_bound_parametric_struct_8406` and the
  polynomial MVP fixture `packages_abstract_algebra_poly_mvp_7491`.

### AbstractAlgebra union-alias typevar binding ✅ (Issue #8409)

- Runtime dispatch now recovers type variables from the selected method pattern
  when a union alias admits a user abstract/concrete struct, preventing the
  `Fraction{T,R}` `UndefVarError` path.
- Covered by `dispatch_union_alias_user_struct_typevar_binding_8409` and
  `packages_abstract_algebra_fraction_residue_7491`.

### Module-private type objects in specialized methods ✅ (Issue #8410)

- Main compilation and runtime specialization now resolve bare module-private
  type names as module `DataType` values, while preserving local variable
  shadowing.
- Covered by `dispatch_module_private_type_object_return_8410` and
  `packages_abstract_algebra_perm_mvp_8306`.

### Dynamic DataType call return inference keeps runtime values ✅ (Issue #8414)

- Call-site body re-inference for `Any`-returning methods no longer narrows the
  result to `Nothing`, which prevented assignments from storing real runtime
  values.
- Covered by `distributions_fit_suffstats_7326`, including
  `fit_mle(Binomial, 5, data)` returning a `Binomial` value.

### DataStructures heap helper validation for QuadGK ✅ (Issue #8365)

- Added `packages_data_structures_heap_validation_8365` to validate the full
  QuadGK dependency slice of bundled DataStructures array-backed heap helpers:
  `heapify!`, `heapify`, `heappop!`, `heappush!`, `isheap`,
  `percolate_down!`, and `percolate_up!`.
- The fixture covers both `Forward` and `Reverse` orderings and the
  QuadGK-style bounded `percolate_down!(xs, i, x, Reverse, len)` active-prefix
  path. Verified against official Julia with DataStructures v0.19.5 and
  `sjulia` fixture chunk `packages::chunk_000`.

### QuadGK finite domains, segment buffers, in-place/batch dispatch ✅ (Issues #8286, #8287, #8288, #8289, #8290, #8401, #8403, #8404, #8405, #8407)

- Added upstream-parity fixtures for `kronrod(Float64, 3)`, finite multi-domain
  `quadgk`, vector/tuple segment inputs, `quadgk_segbuf`/`eval_segbuf`,
  concrete `segbuf=` reuse, `quadgk!`, and direct `BatchIntegrand` calls with
  keyword forwarding.
- Fixed the required compatibility gaps in linalg materialization/eigenvalues,
  `let` tuple destructuring, `NTuple{N}` length binding, expression-position
  `@inbounds` tuple destructuring, partial-parametric struct matching, and
  kwargs runtime dispatch over `kwargs...` wrappers.
- Regression fixtures: `packages_quadgk_domains_segbuf_8288`,
  `packages_quadgk_inplace_batch_8289`, `linalg_symtridiagonal_matrix_eigvals_8401`,
  `dispatch_ntuple_length_where_8404`,
  `dispatch_runtime_kwargs_vararg_partial_param_8407`,
  `let_blocks_tuple_destructuring_8403`, and
  `macros_inbounds_tuple_destructuring_8405`.

### Milestone 56 structural debt inventory ratchet ✅ (Issues #8327/#8329/#8332/#8333/#8334/#8335/#8336/#8337)

- Added `scripts/check_structural_debt_inventory.sh` as a CI-registered ratchet
  for the milestone-56 structural debt categories: hardcoded Julia name
  branches, duplicated configuration strings, panic-prone unwrap/expect sites,
  crate-boundary bypasses, unsafe FFI surface shape, giant files/functions,
  inline tests, and workaround/TODO drift.
- Replaced stale TODO references to closed #1447/#3510 with current follow-up
  Issues #8371/#8372 and removed the remaining active-source `Issue #XXXX`
  placeholder.

### AoT broadcast call-site collection accepts bare function vars ✅ (Issue #8374)

- Lowered `Broadcasted(Var("f"), ...)` forms now feed the same element-wise
  call-site specialization as `FunctionRef`, restoring the
  `mandelbrot_escape.(C, Ref(maxiter))` AoT regression path.
- Covered by the existing AoT inference and e2e mandelbrot broadcast tests.

### Foo{<:Bound} covariant bound type args lower again ✅ (Issue #8352)

- `Foo{<:Bound}`/`Foo{>:Bound}` lowers as a static bounded type expression again
  (was `UnsupportedOperator("<:")` after #8339). Fixed in `is_dynamic_type_arg`
  by classifying `<:`/`>:` shorthands as static. Covered by
  `types/covariant_bound_type_arg_8352.jl`; restores the `types/typeof_*`
  fixtures.

### Val Char / -Inf constructor value params stay concrete ✅ (Issue #8353)

- `Val{'x'}()` and `Val{-Inf}()` now construct instances whose concrete types
  retain the value parameter (`Val{'x'}` / `Val{-Inf}`), matching the bare type
  form instead of falling back to `Val{Any}`. The existing
  `types/value_param_binding_4268.jl` fixture now directly asserts the
  constructed instance type for both regression cases.

### Bare exported parametric inner constructor resolves in scope ✅ (Issue #8313)

- A parametric struct with an inner constructor, exported and brought in via
  `using .M`, is now callable by its bare name even when it collides with a
  bundled same-named struct (`Base.Order.Perm`). `resolve_parametric_struct_name`
  resolves scope-first (current module → `using`-imported modules) before a
  deterministic suffix-match fallback, fixing the `HashMap`-order nondeterminism
  that made `Perm([1,2,3])` intermittently resolve to the wrong struct.
- Covered by `modules/parametric_inner_ctor_using_8313.jl`.

### AbstractAlgebra.Generic Young diagram namespace ✅ (Issue #8302)

- Added the bundled `AbstractAlgebra/src/Generic.jl` submodule for the
  Young diagram/tableau MVP. The upstream-qualified issue MWE
  `AbstractAlgebra.Generic.Partition([4, 2, 1, 1, 1])` now constructs the
  supported partition type and exposes `n` / `part` through the qualified path.
- `packages_abstract_algebra_young_tableau_mvp_8302` now covers both qualified
  `Generic.Partition` / `Generic.YoungTableau` and the top-level aliases.
### Parser: Unicode superscript identifier suffixes ✅ (Issue #8298)

- `dderiv⁻¹` and similar identifiers now parse after infix operators. The lexer
  accepts Latin-1 superscript digits `¹²³` as identifier continuation characters.
- Regression fixture: `milestone55_unicode_superscript_identifier_after_infix_8298`.

### Base: @views in statement position ✅ (Issue #8300)

- `@views y = x[1:2]` now lowers through the existing `@views` expression
  transformation path instead of reporting `unknown macro @views`.
- Regression fixture: `milestone55_views_macro_assignment_8300`.

### Base: @pure compatibility metadata ✅ (Issue #8301)

- `Base.@pure f(x) = ...` is accepted as a no-op compiler metadata annotation in
  statement position.
- Regression fixture: `milestone55_pure_macro_noop_8301`.

### Lowering: field broadcast assignment ✅ (Issue #8303)

- `h.xs .= ys` and `h.xs .+= ys` now mutate the destination field value with
  `materialize!` rather than attempting to reassign the field.
- Regression fixture: `milestone55_field_broadcast_assignment_8303`.

### Parser: multiline return tuple continuation ✅ (Issue #8304)

- `return 1,\n       2` now parses as an implicit return tuple, matching
  upstream Julia.
- Regression fixture: `milestone55_multiline_return_tuple_8304`.

### Base: @. in statement position ✅ (Issue #8305)

- `@. x = x + 1` now lowers through the existing broadcast dotification path.
- Regression fixture: `milestone55_dot_macro_assignment_8305`.

### Base: Matrix constructor ✅ (Issue #8307)

- `Matrix(x)` is available through the public array constructor bridge alongside
  `Array` and `Vector`.
- Regression fixture: `milestone55_matrix_constructor_8307`.

### Compile: imported parametric inner constructors ✅ (Issue #8313)

- Exported parametric inner constructors imported by `using .M` can be called by
  bare name; bare calls now try visible qualified constructor names.
- Regression fixture: `milestone55_imported_parametric_inner_constructor_8313`.

## 最新対応 (2026-06-29)

### Parser: range in ternary else-branch ✅ (Issue #8318)

- An unparenthesized range in a ternary else-branch now keeps its `:` as a range
  operator: `cond ? a : b:c` parses as `cond ? a : (b:c)` (e.g. `true ? 1 : 4:6`
  is `1`, not `1:6`), matching upstream Julia. Fixed by gating the colon-break
  guard on the `in_ternary_then` flag so it applies only to the then-branch.
- Covered by a parser unit test and `ternary/else_branch_range_8318.jl`.

### iOS AbstractAlgebra.jl sample ✅ (Issue #8295)

- Added an iOS app sample for bundled `AbstractAlgebra.jl` covering exact
  polynomial rings over `ZZ`, polynomial quotient rings
  (`ZZ[x] / (x^2 + x + 1)`), residue ring arithmetic modulo 7, small dense
  matrix operations (`identity_matrix`, `det`, `tr`), and permutation group
  operations (`SymmetricGroup`, `Perm`, composition, inverse, powers, sign, and
  cycle type), plus Young diagram/tableau basics (`Partition`, `YoungTableau`).
- Registered the sample in `Resources/Samples/samples.json` and the Swift
  fallback sample catalog so both app resource loading and XCTest fallback paths
  expose it.
- Added a focused catalog regression test that requires the `AbstractAlgebra.jl`
  sample to be present, categorized as Mathematics, and backed by source using
  `using AbstractAlgebra`.

### AbstractAlgebra permutation group MVP ✅ (Issue #8306)

- Added a pure-Julia bundled `AbstractAlgebra/src/PermGroups.jl` slice for the
  iOS-safe permutation group surface: `SymmetricGroup`, `Perm`, `perm`,
  multiplication, `inv`, integer powers, `one`, `isone`, `parent`,
  `elem_type`, `parent_type`, `cycles`, `permtype`, `parity`, `sign`, `gens`,
  `gen`, and display helpers.
- Registered the new package source in the embedded package loader so
  `using AbstractAlgebra` exposes the permutation API without Rust intrinsics.
- Added `packages_abstract_algebra_perm_mvp_8306` to cover cycle display,
  composition, inverse, powers, sign/parity, cycle type, parent metadata, and
  generator count.

### AbstractAlgebra polynomial residue ring MVP ✅ (Issue #8299)

- Added polynomial quotient ring support for bundled `AbstractAlgebra`:
  `residue_ring(P::GenericPolyRing, f::GenericPoly)` returns `Q, alpha`, where
  `alpha` is the image of the polynomial generator.
- Implemented monic-modulus reduction, parent/base/modulus metadata,
  `data`/`lift`, zero/one, equality, scalar coercion, addition, subtraction,
  multiplication, powers, and display for the MVP surface.
- Added `packages_abstract_algebra_poly_residue_ring_8299`, covering
  `ZZ[x] / (x^2 + x + 1)`, the relation `alpha^2 + alpha + 1 == 0`, and
  `alpha^3 == 1`.

### AbstractAlgebra Young diagram/tableau MVP ✅ (Issue #8302)

- Added bundled `AbstractAlgebra/src/YoungTabs.jl` for an iOS-safe Young
  diagram/tableau slice: `Partition([..])`, `YoungTableau([..])`, shape
  metadata, partition fields (`n`, `part`), linear tableau indexing, equality,
  and compact ASCII diagram helpers.
- Registered the source in the embedded package loader and exported
  `Partition` / `YoungTableau`.
- Added `packages_abstract_algebra_young_tableau_mvp_8302`, covering the docs
  examples `Partition([4, 2, 1, 1, 1])` and `YoungTableau([4, 3, 1])`.
### Parser: comparison operator in ternary then-branch ✅ (Issue #8314)

- `cond ? a > b : c` (and `==`, nested ternaries, etc.) now parse: a new
  `in_ternary_then` parser flag makes the whitespace-delimited `:` end the
  then-branch at any operator-recursion depth, while genuine parenthesized ranges
  in the then-branch are preserved (the flag clears on entering any grouping).
- Covered by parser unit tests and `ternary/comparison_then_branch_8314.jl`.

### SpecialFunctions: Hurwitz zeta(s, z) and Dirichlet eta(s) ✅ (Issue #8310)

- `zeta(s, z)` (generalized/Hurwitz zeta) and `eta(s)` (Dirichlet eta) are now
  available from the bundled `SpecialFunctions` package for real arguments,
  completing the zeta family begun in Issue #8297.
- `zeta(s, z)` is a Float64 port of upstream `_zeta(s, z)` covering `z > 0`,
  `z < 0`, and the `z == 1`/`z == 0` reductions; `eta(s)` is derived from the
  Riemann zeta with a Taylor branch near `s == 1`.
- Verified against upstream within `1e-6` via
  `special_functions/special_functions_zeta_hurwitz.jl`.

### Accurate sinpi/cospi/sincospi ✅ (Issue #8309)

- `Base.sinpi`/`cospi`/`sincospi` are now accurate ports of the upstream
  `Base.Math` minimax-kernel algorithm (`julia/base/special/trig.jl`): exact at
  integer/half-integer arguments (`±0.0`/`0.0`/`±1.0`), accurate for large `x`,
  and within ~1 ULP of upstream elsewhere — replacing the naive
  `sin(pi*x)`/`cos(pi*x)`.
- Regression coverage in `math/sinpi_cospi.jl` now asserts exactness and large-x
  accuracy.

### SpecialFunctions: Riemann zeta function ✅ (Issue #8297)

- `zeta(s)` is now available from the bundled `SpecialFunctions` package for
  real `s`, ported from upstream `SpecialFunctions._zeta` (Float64 path):
  reflection + small-`s` Taylor branch for `s < 0.5`, Bernoulli/Stirling
  asymptotic series for `s >= 0.5`.
- Verified against upstream within `1e-6` for positive/negative/fractional `s`,
  the pole at `s == 1`, trivial zeros, and non-finite inputs via fixture
  `special_functions/special_functions_zeta.jl`.

### Public Base stdlib escape-hatch audit ✅ (Issue #8278)

- Closed the remaining `Base.Random.<fn>` public route so `Random` stays a
  stdlib root module loaded by `using Random`, matching upstream Julia's
  `Base.Random` `UndefVarError` behavior.
- Added `scripts/check_no_public_base_stdlib_routes.sh` and registered it in
  the code-audits workflow to prevent compiler/module-call special cases from
  exposing `Base.<stdlib>`, `using Base.<stdlib>`, or
  `Base.<stdlib>.<fn>` routes for stdlib roots.
- Extended integration coverage so `Base64`, `Dates`, `InteractiveUtils`,
  `LinearAlgebra`, `Printf`, `Random`, `Statistics`, and `Test` are rejected as
  public Base submodules.

### LinearAlgebra stdlib module loading and builtin bridge fix ✅ (Issue #8276)

- Matched upstream Julia's module model: `LinearAlgebra` is a stdlib root module
  loaded by `using LinearAlgebra`, not a public `Base.LinearAlgebra` submodule.
- Prevented `Base.LinearAlgebra` from being canonicalized to `LinearAlgebra`;
  user code now sees it as undefined, while real bundled Base submodules such as
  `Base.Order` still resolve.
- Replaced the old public-looking `Base.LinearAlgebra.<fn>` builtin escape hatch
  with a private compiler bridge used only by bundled LinearAlgebra wrappers.
- Added integration smoke tests for `det`, `inv`, `svd`, and `eigen`.

### DataStructures heap helper MVP for QuadGK dependency ✅ (Issue #8141)

- Added bundled `DataStructures` package metadata and a pure-Julia
  array-backed heap helper slice adapted from
  `extern/DataStructures.jl/src/heaps/arrays_as_heaps.jl`.
- Implemented the heap API required by QuadGK's adaptive segment heap usage:
  `heapify!`, `heapify`, `heappop!`, `heappush!`, `isheap`,
  `percolate_down!`, and `percolate_up!`, including `Base.Order.Reverse`
  support.
- Uses the bundled `Base.Order` ordering surface and registers
  `DataStructures` in the embedded package registry.
- New fixture `packages_data_structures_heap_8141` passes under upstream Julia
  with the bundled package path and under sjulia.
- Audited QuadGK v2.11.3 and confirmed its `DataStructures` dependency is
  limited to the array-backed heap helpers already bundled for the milestone.
- New fixture `packages_data_structures_quadgk_segment_heap_8141` covers
  QuadGK-like `Segment` ordering by error estimate plus the bounded
  `percolate_down!` path used by batched refinement (Issue #8293).

### QuadGK scalar finite-interval integration bundle ✅ (Issue #8140)

- Bundled the upstream `QuadGK.jl` package source and metadata in the embedded
  package registry without package-local rewrites.
- Enabled the cached scalar finite-interval path:
  `quadgk(x -> x^2, 0.0, 1.0)`, `quadgk(sin, 0.0, 1.0)`, and
  `QuadGK.cachedrule(Float64, 7)` now execute under sjulia.
- Fixed the Julia-compatibility gaps exposed by the upstream source across the
  parser, lowering, Base/LinearAlgebra shims, constructor dispatch, `where`
  parameter binding, Unicode comparison call normalization, value type
  parameters, and keyword default function binding.
- New fixture `packages_quadgk_scalar_integrals_8140` passes under upstream
  Julia with the bundled package path and under direct sjulia; the package
  category nextest gate is green.

### Nested module and Base.Order binding fix ✅ (Issue #8269)

- Fixed nested submodule availability for later parent-module statements by
  emitting submodule initialization before the parent body that references it.
- Added compiler resolution for nested module values in the current module and
  for `Base.Order` direct field/call access after Base preload.
- New fixture `modules_nested_module_order_binding_8269` covers nested
  `Child.x`, `Base.Order.Forward` / `Reverse` values, and `Base.Order.lt`.
### Fix: abstract_algebra core_traits seed-gate fixture red on main ✅ (Issue #8273)

- Corrected the stale `occursin("NotImplementedError", err)` assertion in
  `tests/fixtures/abstract_algebra/core_traits_7489_7490.jl` to match the
  upstream `NotImplementedError` `showerror` message (which does not echo the
  type name). The red was a semantic merge conflict between #8268 (added the
  fixture under the pre-#8256 type-name fallback) and #8256 (fixed
  package-defined `Base.showerror` dispatch); it only appeared on a full
  `cargo nextest run --release` of merged main. Surfaced by #7797 verification.

### AbstractAlgebra Phase 3/4 seed: ZZ/QQ traits and exact arithmetic ✅ (Issues #7489/#7490)

- Bundled `AbstractAlgebra` now loads the core Julia parent types and trait
  interface needed by the MVP: `Integers`, `Rationals`, `ZZ`, `QQ`, `zz`,
  `qq`, `parent`, `elem_type`, `parent_type`, `base_ring`,
  `base_ring_type`, `is_exact_type`, `is_domain_type`, `characteristic`,
  `is_known`, and `check_parent`.
- Added pure-Julia integer/rational operations for the Phase 4 seed:
  `zero`/`one` on `ZZ`/`QQ`, unit and zero-divisor predicates,
  divisibility/exact division, numerator/denominator, square/root helpers, and
  AbstractAlgebra error type construction.
- New fixture `abstract_algebra_core_traits_7489_7490` exercises bare and
  qualified `AbstractAlgebra.<name>` calls over `Int`, `BigInt`,
  `Rational{Int}`, and `Rational{BigInt}`. Incidental VM gaps are filed as
  #8253, #8254, #8255, and #8256, with active workarounds W-44/W-45/W-46.

### AbstractAlgebra Phase 5 polynomial/fraction/residue MVP seed ✅ (Issue #7491)

- Added a pure-Julia dense univariate polynomial layer for bundled
  `AbstractAlgebra`: `polynomial_ring(ZZ, "x")` /
  `polynomial_ring(QQ, "y")`, parent/element traits, generator access,
  display helpers, arithmetic, power, degree/coeff, evaluation, derivative,
  and exact division for simple exact quotients.
- New package fixture `packages_abstract_algebra_poly_mvp_7491` covers
  upstream-style examples over `ZZ` and `QQ`, including `(x + 1)^2`,
  `(x + 2)*(x - 2)`, rational coefficients, evaluation, derivative, and
  `divexact`.
- Added a constructor-level fraction/residue slice: `fraction_field(R)` for a
  univariate polynomial ring and `residue_ring(ZZ, n)` for integer residues,
  with small arithmetic and trait coverage in
  `packages_abstract_algebra_fraction_residue_7491`.
- Filed #8262 for `BigInt` widening through `Any` coefficient slots, #8263 for
  `println`/`string` display routing ignoring custom `show`, and #8264 for
  callable fraction-field parent dispatch. Active workarounds W-47/W-48 are
  documented.

### AbstractAlgebra Phase 6 matrix/module/map MVP seed ✅ (Issue #7492)

- Added pure-Julia dense matrix spaces and elements for the AbstractAlgebra MVP:
  `matrix_space`, `matrix(R, r, c, entries)`, `zero_matrix`,
  `identity_matrix`, indexing, parent/base-ring traits, arithmetic,
  transpose, determinant, trace, and small rank.
- Added a small free-module/map layer: `free_module`, `gen`, `gens`,
  `number_of_generators`, module element arithmetic, `identity_map`, `hom`,
  `domain`, and `codomain`.
- New fixture `packages_abstract_algebra_matrix_module_map_7492` covers `ZZ`,
  `QQ`, polynomial matrices, free modules, and maps under upstream Julia and
  sjulia. Filed #8266 for typed `BigInt` array storage; active workaround W-49
  is documented.

### AbstractAlgebra Phase 7 validation tranche ✅ (Issue #7493)

- Added final documentation for the supported MVP surface and known deferrals
  across `STATUS.md`, `DONE.md`, `UNIMPLEMENTED.md`, and `WORKAROUNDS.md`.
- Final validation passed on Linux: full release nextest (4067/4067), focused
  `linalg:: packages::` release fixture gate, fixture-name audit, workaround
  documentation/sync audits, upstream Julia parity checks, and direct release
  `sjulia` smoke fixtures.
- Recorded release CLI smoke timings for package-load workflows and documented
  host limitations for iOS/WASM validation (`xcrun`/Xcode SDK and `wasm-pack`
  unavailable in this environment).
### Open bug sweep: Rational / BigInt / display / callable dispatch regressions ✅ (Issues #8253, #8254, #8255, #8256, #8262, #8263, #8264, #8266)

- `Rational{BigInt}(1)` のような method typevar 経由の parametric constructor が
  raw field allocation へ落ち malformed struct を作る問題を、field arity が一致しない
  `T{...}(args...)` を DataType call へ戻すことで解消。回帰 fixture
  `rational/parametric_method_typevar_constructor_8253.jl`。
- 同一 module 内の `const` function alias を後続 method body から呼ぶと unqualified global
  lookup になり `UndefVarError` になる問題を、module-qualified constant binding load へ修正。
  回帰 fixture `modules/same_module_const_function_alias_8254.jl`。
- `Rational // Rational` と `Rational{BigInt} // Rational{BigInt}` を upstream と同じ exact
  division path に追加。回帰 fixture `rational/rational_over_rational_slashslash_8255.jl`。
- package/module 側で定義した `Base.showerror` / `Base.show` extension を method registry が見落とす
  問題を修正し、custom exception display と parametric struct の `println`/`string` がユーザ定義
  `show` を通るようになった。回帰 fixtures
  `error/custom_exception_showerror_8256.jl` と
  `strings/parametric_struct_custom_show_8263.jl`。
- `Any` array slot や `Matrix{BigInt}` slot へ書いた `BigInt` が `Float64` に widened して
  読み戻される問題を、boxed `BigInt` として保存する array store path へ修正。回帰 fixtures
  `bigint/any_array_slot_addition_8262.jl` と
  `bigint/matrix_bigint_assignment_8266.jl`。
- concrete callable object の call overload が abstract parent 型上に定義されている場合、
  concrete の `__callable_*` だけでなく parent chain の callable method も dispatch candidate に含める。
  AbstractAlgebra fraction-field parent pattern の回帰 fixture
  `dispatch/callable_abstract_parent_dispatch_8264.jl`。

### REPL: 再構築不能なグローバルの実 Value キャリー永続化 ✅ (Issue #8260)

- `prob = ODEProblem(f, u0, tspan)` の次行で `solve(prob, …)` /
  `prob.tspan` が `UndefVarError: prob` になっていた問題を解消。`ODEProblem` の
  `kwargs::Base.Pairs` フィールドに init 式形が無く、init 式再構築方式では構造体ごと
  drop されていた。
- 再構築できなかったグローバルだけ、前 eval の struct heap を移植して実 Value を次の VM へ
  seed する方式を追加（`inject_globals` が drop 分を返し、`Vm::seed_persisted_globals`
  が heap 移植 + `type_id` 名前再マップ + frame 0 束縛）。
- `LoadStruct` がスロットのみ参照し `locals_any` を見ていなかった潜在不整合も修正
  （`get_local` フォールバック追加）。
- 回帰: `test_repl_value_carried_global_with_pairs_field_persists_8260`,
  `test_repl_odeproblem_global_persists_8260`（実 OrdinaryDiffEq）。

### array-like wrapper equality / inference drift の予防 ✅ (Issue #8246)

- #8240 の `view == view` 再発を防ぐため、compile-time 側は
  `array_like_view_constructor_contract_infers_concrete_subarray_8246` で
  `view(Vector{Int64}, UnitRange)` の `SubArray{...}` narrowing を固定。
- runtime 側は fixture `subarray_array_like_wrapper_contract_8246` で `view` と
  非 `SubArray` wrapper の `reshape` が wrapper 同士・native array/matrix と
  `==`/`isequal` で要素比較されることを検証。
- `docs/vm/CHECKLISTS.md` に、array-like wrapper constructor 追加時の
  推論/equality 同期ルールと、method-table `Any` body re-inference を広げる場合の
  recursion/work-budget 保護および REPL user `show` regression 実行ルールを追加。

### Symbolics.jl サンプルのロード時推論を短縮 ✅ (Issue #8213)

- バンドル Symbolics の再帰的な式ウォーカー（`_simplify`/`_expand`/
  `substitute`/show/derivative 周辺）と記号行列 `det`/`inv` helper に戻り値注釈を
  付け、`compile.build_method_tables` がロード時に本文推論を展開しないようにした。
- `using Symbolics` の `build_method_tables` は約 9.7s → 88ms。基本サンプルは
  約 9.8s → 1.1s、`Symbolics + LinearAlgebra` 行列サンプルは約 10.3s → 1.7s。
  実行結果は従来通り（変数・代数・微分、行列積・det・inv・solve・3x3 determinant）。
- 回帰テスト `using_symbolics_load_inference_stays_bounded_8213` を追加し、
  #8213 型のロード時推論再発を work budget で検出。

### SubArray view equality を要素比較へ復旧 ✅ (Issue #8240)

- `view(Vector{T}, UnitRange)` の戻り値を call-site で
  `SubArray{T,1,Vector{T},Tuple{UnitRange{Int64}},true}` として推論し、`view(...) == ...`
  が `Any`/identity fallback に落ちないようにした。
- runtime の array equality 正規化に 1D contiguous `SubArray` を追加し、
  `view == view` / `view == Vector` / `Vector == view` が `ArrayValue` logical view の
  要素比較を使うようにした。fixture `subarray/view_equality_8240.jl` と
  `fixture_tests subarray::` で検証。

### Plots: `push!` 3D アニメ（Aizawa/Lorenz）の空フレームを解消 ✅ (Issue #8214)

- `plot3d(1)`+`push!`+`@animate` の 3D アニメが REPL で空の 2D アニメになっていた。`@animate` は
  `frame(_anim, current())` でスナップショットするが、`push!(plt,…)` が `plt` だけ変更しグローバル
  `_CURRENT_*` を更新しないため、`current()` が空のまま（全フレーム空→`extract_series` が空シリーズを
  落とし 2D・traces 空）。in-place `push!` 拡張ヘルパが変更後 `plt` を `current()` に再公開するよう修正
  （`_plots_resync_current!`）。アーティファクトが `scene`+scatter3d+非空フレームに復帰。回帰テスト
  `test_push_based_3d_animation_frames_are_3d_and_nonempty_8214`。memory
  `reference_plots_push_current_sync`。fixture_tests(169)/plot_artifact_mime_tests(31) 全通過。

### iOS REPL: 複数プロット一括ペーストのメインスレッド・レイアウト嵐を解消 ✅ (Issue #8214)

- REPL 履歴の自動スクロール `proxy.scrollTo(...)` を `withAnimation(.easeOut(0.2))`
  から**アニメーション無し**に変更。アニメーション化は行（WKWebView/Plotly を内包しうる）
  の挿入まで毎フレーム再レイアウトさせ、一括ペースト時にアニメーションが積み重なって
  メインスレッドが `CA::Transaction::commit`→`List.applyNodes`/`ScrollViewLayoutComputer`
  で飽和→UI 固まり＋`main.async` eval/描画コールバック枯渇（「3 evals」で停止）していた。
- 実機プロセスの `sample` で根本原因を特定（VM・eval ロックは無実）。iPad (A16) シミュレータ
  で同一ペーストを再実行し、6/6 evals 完了・全プロット描画・メインスレッド idle・履歴スクロール
  正常を確認。memory `reference_ios_swiftui_scrollto_animation_storm` に知見記録。
- 嵐解消に伴い履歴プロットを**再インタラクティブ化**（pan/zoom/hover・3D orbit）。#8235 の
  非インタラクティブ化は「WebView がスクロールを奪う」推定だったが、真因はこの嵐。嵐が消えた
  後はインタラクティブ・プロット上でも履歴は ScrollView が縦パンを取りスクロール可（検証済み）。
### AbstractArray サブタイプの `==`/`isequal` 要素比較 + `::AbstractArray` ディスパッチ ✅ (Issue #8229)

- ユーザ `struct <: AbstractVector` / `SubArray` view に対する `==`/`isequal` が要素比較される
  ようになった（従来は identity `false`）。`builtin_abstract_param_name` に `AbstractArray` を
  追加して `f(::AbstractArray)` ディスパッチを回復、`base/abstractarray.jl` に
  `isequal(::AbstractArray, ::AbstractArray)` を追加、`isequal` ビルトインが読めない
  `AbstractArray` オペランドをこの Pure-Julia メソッドへ fallback、`compile_binary_op` の gate が
  `struct == struct` を要素比較へ。native/Memory/StaticArray carrier は fast path 維持。
- fixture: `array/abstractarray_subtype_equality_8229.jl`（julia パリティ済み）。

### 一般 Base 関数の修飾 `Base.<fn>` 値アクセス ✅ (Issue #8137)

- `f = Base.map` 等（Pure Julia 化された Base 関数の値アクセス）が解決されるようになった。
  `compile_module_function_ref` を一般化し method table 裏付けの Base 名を非修飾 `<fn>` と同じ
  関数値に解決。fixture: `reflection/base_function_value_general_8137.jl`。

## 最新対応 (2026-06-28)

### using-package 推論爆発の再発防止 (作業量バジェット+スモーク) ✅ (Issue #8185)

- #8182 再発防止。`infer_block_with_fixpoint` にルート単位 `analysis_work` カウンタ +
  `MAX_INTERPROCEDURAL_ANALYSIS_WORK` バックストップ（超過で `Top` widen）を追加。常時オン
  メトリクス `work_budget_metrics` + ロード時スモークテスト（`using Optim` peak work<50k）+
  バックストップ単体テスト（`tests/work_budget_8185.rs`）。CHECKLISTS.md/PURE_JULIA_DESIGN.md
  に「in-loop closure を再帰ヘルパへ引回す関数は戻り値型注釈で短絡」パターンを明文化。
- 経験的知見: バジェットは catastrophe バックストップ止まり（注釈無し `_bfgs` 174k ≈
  当時未注釈だった `using Symbolics` 159k で区別不能）。`_bfgs` 注釈は維持し、
  注釈+スモークが #8182 の実ガード。
  フル `--release` で検証。

### native-array vs AbstractArray-subtype-struct `==` を一般階層解決へ ✅ (Issue #8149)

- `==`/`!=` の native-array-vs-array-struct ルーティング判定を StaticArrays 名前リスト
  ハードコードから登録済み struct 階層解決へ一般化（名前リストは fast path として保持）。
  新 strict 述語 `struct_is_registered_subtype_of_abstract`（未登録は `false`、
  conservatively-accept 分岐なし）で、グローバル `==` パスの誤ルーティング回帰を回避。
  unit test `strict_abstractarray_subtype_predicate_no_conservative_accept_issue_8149`。
  フル `--release` 4063/4063。end-to-end の要素比較は下流ギャップ #8229 が別途必要。

### convert 失敗例外が catch 可能な InexactError オブジェクトに ✅ (Issue #8212)

- `convert(T, x)` の変換失敗（直呼び・typed local・`for i::T in itr` ループ変数）が
  投げる例外を `catch e` で受けると `typeof(e) == String` になっていたのを解消。
  `vm/exec/error_handling.rs::vm_error_to_exception_value` に `VmError::InexactError` →
  `InexactError(func, T, val)` 構造体復元アームを追加（#5648 の DomainError/BoundsError 等と
  同型）。`base/errorshow.jl::_showerror_str(ex::InexactError)` を upstream 準拠に修正
  （`nameof(ex.T) === ex.func` で `T` を省略）。fixture
  `exceptions/catchable_inexact_error_8212.jl` で `typeof(e) == InexactError`・`isa`・
  `showerror` を julia とパリティ確認（11/11）。

### 外部ローカルを捕捉する相互再帰ネストクロージャ ✅ (Issue #8118)

- 外部ローカルを捕捉するクロージャ同士の相互再帰（`s=9; a(n)=…b(n-1);
  b(n)=n<=0 ? s : a(n-1); a(3)`）や 3-way 相互再帰が `Unknown function: b` で
  失敗していたのを解消（PR #8142 は自己再帰・捕捉なし相互再帰のみ解決済みだった）。
- `compile/stmt.rs::prescan_mutual_closure_captures` が本体コンパイル前にネスト関数群を
  事前スキャンし、兄弟関数名を捕捉から除外（再構築で解決）しつつ、呼ぶ兄弟クロージャの
  外部ローカル捕捉を不動点で推移伝播して各員に共有させる。グループ各員が共有捕捉から
  互いのクロージャを再構築可能になる。fixture `closures/recursive_nested_closure_8118.jl`
  に残存ケース（callee 捕捉/両者捕捉/3-way）を追加。フル `--release` 4061/4061。
### iOS REPL: プロット貼り付けの完全フリーズ/クラッシュ（データ競合）を修正 ✅ (Issue #8214)

- 症状: Plots のサンプルを iOS REPL に貼ると稀に完全フリーズ、シミュレータでは
  クラッシュ。「`t = 0:0.1:2π` / `scatter!(…, aspect_ratio=:equal)` の辺り」と報告。
- 根因: **アプリ側のデータ競合**。`REPLSessionManager` が評価を並行キュー
  `DispatchQueue.global()` で実行し、スレッドセーフでない Rust `REPLSession` を直列化
  していなかった。重なり評価で `repl_session_eval` が同一 session に同時アクセスし
  `&mut` エイリアス → ヒープ破壊（`SIGABRT` / 破壊状態の無限ループ＝フリーズ）。
- 修正: `REPLSessionManager` に `NSLock` を入れ、`eval`/`reset`/`newSession` の
  session 触接を相互排他化（`SubsetJuliaVMApp/.../Services/FFI/REPLSessionManager.swift`）。
- 検証/回帰: VM/FFI は host 140 + シミュレータ 60 回で無罪確認。実機相当の
  シミュレータで重なり評価を強制し crash を再現 → ロックで解消。回帰テスト
  `SubsetJuliaVMAppTests/REPLConcurrentEvalSafetyTests`（ロック除去で abort、導入で pass）。
- 追加修正(順序, PR #8231→#8233): `evalAsyncSplit` が各行表示を `main.async` 撃ちっぱなしで
  投げ次の eval に先走り、`plot!(cos)` が `plot(sin)` の描画前に評価＋6 行分が一斉到達
  →WebView 同時多数生成でフリーズ要因。#8231 の `main.sync` は履歴追加までで実 Plotly
  描画は待たず不十分→ **WebView の描画完了を JS `WKScriptMessageHandler`(plotRendered) で通知**し、
  各プロット行はその `done`(semaphore, 8s タイムアウト) を待ってから次行を評価するゲートに変更。
  「コード実行→描画完了→次のコード実行」を厳密逐次化、同時描画解消。シミュレータで
  `EVAL→DISPLAY→RENDER-DONE→次EVAL`(全 signaled) を確認。回帰
  `testPasteSequenceEvaluatesAndDisplaysInSourceOrder`。
- 追加修正(履歴スクロール, PR #8235): 完走後に REPL 履歴をスクロールできない症状。原因は
  各プロットが生きた WKWebView でスクロールジェスチャを奪うため(ハングではない)。`PlotlyView`
  に `interactive` を追加し履歴は `interactive=false`(`staticPlot`+`isUserInteractionEnabled=false`)で
  タッチを ScrollView へ通す。Editor 出力は interactive のまま。`plotRendered` 通知は維持され
  #8233 ゲートに影響なし。

### 二項演算 codegen の二重化を共有化 + 再発防止ガード ✅ (Issue #8192)

- 二項演算バイトコード生成の **2 経路**（主コンパイラ `compile/expr/binary/` と
  実行時引数型 specializer `vm/specialize/expr.rs` #8167）で型付き命令選択が二重化し、
  片方の修正が他方に伝播しないフットガン（#8183 で表面化）を解消。
- 型ペア → 命令選択を単一の真実 `compile::typed_scalar_binary_instr` に集約。
  `typed_instr_for_intrinsic`（主経路）は薄いアダプタ化、specializer の
  `emit_binary_op` は直接これを呼ぶ。さらに specializer の `compile_binary_op` fast path
  を一般化して `Int64 / Int64` 除算もコンパイル時 `ToF64` で promote し、ホットループ
  から残存 `Swap` を排除。
- 再発防止: end-to-end ガード
  `untyped_scalar_hot_loops_specialize_to_swap_free_typed_loops_issue_8192`
  （untyped `+ - * /` が特殊化後に Swap なしで typed-loop 認識される）、単体トリップワイヤ
  `shared_binary_table_only_emits_typed_loop_recognized_instrs_issue_8192`、共有テーブル
  pin テスト 2 件。`docs/vm/BINARY_DISPATCH.md` に「Two Binary-Op Codegen Paths」節と
  相互参照コメント。
- 既存 #8183 / #8167 回帰テスト・arithmetic/control_flow fixture・clippy すべて緑。

### ループ変数の型注釈 `for i::T in itr` ✅ (Issue #8208)
### Bool 同士のビット演算子 `&` / `|` / `⊻` ✅ (Issue #8197)

- `true & false` / `true | false` / `true ⊻ true`（Bool 同士のビット演算子）を
  upstream `base/bool.jl` どおり実装（`&=and_int`, `|=or_int`, `xor/⊻=(x!=y)`）。
  これまでは `MethodError: no method matching &(::Bool, ::Bool)` だった。
- Bool メソッドは `base/int.jl` の `Int64` メソッド直後に co-locate。これにより
  exact な同型メソッドが無い mixed 呼び出し (`0x05 & 5 === 5`) のランタイム
  フォールバックが型安全な `Int64` メソッドに落ち、`LoadSlotBool` 回帰を回避
  （詳細は STATUS.md / `base/bool.jl` の注記）。fixture `bool/bitwise_bool_8197.jl`
  が Bool 同士・mixed・同型の各ケースを upstream パリティで検証。

- upstream で動く `for i::Int64 in itr`（ループ変数の型注釈）が sjulia ではパース
  エラーだったのを解消。パーサ (`parse_for_binding`) が単一識別子直後の `::` を
  `TypedExpression` 束縛として受理し、lowering (`control_for.rs`) が upstream と同じ
  desugar `for #i in itr; i = convert(T, #i); <body>` を生成して各反復値を `T` へ変換。
- 整数レンジ fast path はループ変数自身を I64 カウンタに使うため、反復は隠し変数
  `i#fortyped<span>` で回し convert で別スロットの `i` に束縛（スロット破壊回避）。
  Range / `=` head / array-iterable / cartesian の全形に対応。`InexactError` も upstream
  一致。`for (a,b)::T in itr` は upstream 自体が構文エラーのため対象外。
- テスト: parser `test_for_loop_typed_variable_issue_8208`、lowering 2 件、fixture
  `loops/for_loop_typed_variable.jl`（julia パリティ）。
- 関連: #8204 (typed for-loop 性能 = PR #8206)。

### 実行時特殊化本体の peephole 融合 ✅ (Issue #8205)

- untyped 引数を具体型で呼んだとき生成される特殊化本体 (#8167) を、main コンパイラと
  同じ post-slotize `peephole::optimize` に通すよう修正。特殊化本体の append を
  `Vm::install_specialized_body` (`vm/exec/call.rs`) に一本化し、2 特殊化 site が
  共有 (codegen 重複の縮小 = #8192 方向)。融合により untyped 版 for/while ループが
  typed 版とほぼ同速に (release N=2M Aizawa: untyped FOR/WHILE 0.53→0.41-0.42s)。
- typed-loop 認識は #8206 の `AddConstI64SlotAndJumpIfLe` back-edge 認識で維持。
- テスト: unit `specialized_body_peephole_8205_tests` + fixture
  `loops/for_loop_untyped_arg.jl` (julia パリティ)。
- 関連: #8204 (typed for-loop back-edge 認識 = PR #8206), #8159 (改善案1=#8167), #8192。

### 混合 Int64/Float64 比較の値ベース厳密化 ✅ (Issue #8187)

- `Int64`×`Float64` の `==`/`!=`/`<`/`<=`/`>`/`>=`・`isequal`/`in`/tuple-`==` を、
  整数を `Float64` に丸めずに比較するよう修正（2^53 超で誤判定していた）。VM の
  `cmp_i64_to_f64` (`vm/numeric_identity.rs`) + コンパイラの `CallDynamicBinaryBoth`
  ルーティング (`compile/expr/binary/mod.rs`) で typed/dynamic/array/isequal/in を網羅。
  Pure Julia の concrete メソッドは使わない（BigFloat 比較を coercion で壊すため）。
- fixture: `comparison/mixed_int_float_exact_8187.jl`。残課題は #8199（他幅・関数呼び出し形）。

### 複数行型付き配列リテラル `T[...]` ✅ (Issue #8188)

- `[` 直後・末尾の改行をスキップし、`Bool[⏎ …,⏎]` を型なし `[…]` と同様に許容
  （`parser/expressions/index.rs`）。複数行インデックス `v[⏎ 2⏎]` も対応。
- fixture: `array/typed_array_multiline_8188.jl`。

### `a!=b`（空白なし `!=`）の lexer 修正 ✅ (Issue #8194)

- 末尾 `!` の直後が `=` のとき lexer ラッパ (`parser/lexer.rs`) が `restart_from` で
  `!` を返却し `!=`/`!==` として再字句解析。`in!`/`push!`/単項 `!` は不変。
- fixture: `parse/bang_not_equal_8194.jl`。

### AoT: n 項 `+`/`*`（`a+b+c`）の reachability 修正 ✅ (Issue #8180)

- n 項演算子呼び出しは IR 変換で入れ子 binary に畳まれるのに、call graph が `+`
  へ辺を張り変分 `+(xs...)`/`afoldl` を到達可能にして `HasShape{1}` 未対応で
  落ちていた。畳み込み対象の演算子呼び出し（非 splat・kwargs なし・引数 ≥2）には
  辺を張らないよう修正（`is_folded_binary_operator`）。テスト 2 本 (#8180)。
- 残課題: n 項 `+` の結果型は Any 推論のままで、typed スロット代入は #6978/#6968。

### AoT: 分岐内初代入・分岐後参照の局所変数を関数スコープへ hoist ✅ (Issue #8181)

- 分岐内で初代入し分岐後に使う局所変数を、codegen が分岐ブロック内 `let` で出力し
  `cannot find value` で生成 Rust がコンパイル不能だった。入れ子ブロックで初代入され
  別スコープから参照される局所を関数先頭の遅延宣言 `let mut x: T;` に巻き上げ、
  ブロック内は代入で出力（`compute_hoisted_locals`）。e2e テスト 2 本 (#8181)。

### ベンチマーク: Aizawa attractor / IFS フラクタル 5 実装比較 ✅ (Issue #8183)

- Julia / juliars(AoT) / sjulia / sjulia(型注釈) / Python 3.14 を Float64 ループ
  2 種で比較（`docs/benchmark/aizawa_ifs_comparison.md`、生データ
  `benchmarks/results/aizawa_ifs_20260628.md`）。AoT≈Julia、汎用 float ループで
  sjulia は 100〜200x 遅く Python にも負ける、型注釈は IFS で逆効果、を記録。
### VM: 汎用 Float64 ホットループの native 高速化 (混合算術特化 + typed-loop 認識器拡張) ✅ (Issue #8183)

Float64 スカラー演算が支配的なホットループ (Aizawa attractor の Euler 積分、
Barnsley fern の IFS) が公式 Julia/AoT 比 100-200x 遅く、型注釈が IFS で逆効果
だった件を 3 段階で解消。N=5M で **untyped/typed とも 3.6-5.8x 高速化**し、4 変種
すべてが native typed-loop 高速路 (`vm::executable`) に乗る。出力は upstream Julia と
bit 一致。

- **Stage 1 — 混合 Int/Float スカラー算術の特化** (`compile/expr/binary/mod.rs`)。
  `Int64 / Float64` などの混合プリミティブ算術 (`+ - * /`) は毎実行 Base 演算子への
  動的メソッド `Call` にコンパイルされていた。両辺が具体プリミティブ数値で一方が
  Float のとき、整数→Float 昇格 (`…ToF64; <op>F64`) は Julia の `promote` と
  bit 一致なので typed 経路 (`compile_builtin_binary_op`) へ委譲。毎反復のコール除去 +
  認識器ヒットの前提。**比較 (`== < …`) は除外**（2^53 超で厳密性が崩れるため）。
- **Stage 2 — typed-loop 認識器の拡張** (`vm/executable.rs`)。`TypedLoopOp` に
  `DivF64` / `ModI64` / 融合 `LoadDivF64Slot` / `LoadModI64Slot` /
  `LoadAddI64Slot` / `LoadSubI64Slot` / `LoadMulI64Slot` / 単項 `NegF64` を追加し
  executor 実装 (`%` は `checked_i64_rem` でゼロ除算/`i64::MIN%-1` を interpreter に
  退避)。`MAX_TYPED_LOOP_OPS` 64→128、`TYPED_LOOP_SLOT_CAP` 16→24 に引き上げ
  (Aizawa 68 命令/16 F64 スロット、IFS 92 命令)。
- **Stage 3 — untyped ループの runtime 特化を認識可能に** (`vm/specialize/expr.rs`)。
  関数引数型特化 (#8167) が混合 `Int64/Float64` を `Swap; ToF64; Swap` で昇格し、
  whitelist 外の `Swap` が認識を阻んでいた。混合算術を `compile_numeric_as_f64` で
  各オペランド個別に F64 強制し `Swap` を排除。untyped IFS が 3.25s→0.91s に。
- テスト: bytecode-dump 1 本、`vm::executable` ユニット 5 本、fixture parity 2 本
  (`arithmetic/mixed_int_float_promotion_8183`,
  `control_flow/float_hot_loop_recognition_8183`)。混合 `== / <= / >=` の 2^53 超
  精度欠落という**別の既存バグ**を検出→ #8187 を起票。

### VM: 動的二項演算ディスパッチの per-call-site キャッシュ ✅ (Issue #8168)

- 構造体×構造体の `CallDynamicBinaryBoth` 解決を `call_site_ip →
  (left_type_hash, right_type_hash) → Option<func_index>` でメモ化
  (`binary_both_dispatch_cache`)。resolver は `resolve_binary_both_candidate`
  へ抽出。値依存ガードが構造体ペアで発火しないため型名キーで健全。
- 多態的な構造体二項演算（`Vector{Any}` 畳み込み）で約 −25%。calc_pi は無回帰
  (ホットパスは resolver を通らない)。回帰テスト 2 本 (#8168)。

### VM: untyped 呼び出し型特殊化を直接ディスパッチ化 ✅ (Issue #8167)

- `CallSpecializeI64Slots` の hot path から、毎回の `vec![I64; n]` 確保 +
  `Vec` キー HashMap 引き + `param_slots` clone を排除。`(spec_func_index,
  arity)` キーの `specialization_i64_cache` に初回特殊化結果 (`I64SpecDispatch`)
  を載せ、2 回目以降は解決済みエントリへ直接ジャンプする (#8159 案1)。
- calc_pi untyped が typed-args 版とほぼ同速に: N=2000 −29%, N=3000 −30%
  (upstream julia と結果一致)。回帰テスト
  `untyped_calc_pi_uses_specialize_i64_dispatch_cache_8167` (profiling)。
### `using Optim` 起動の ~5.5s → ~0.28s 高速化 ✅ (Issue #8182)

- iOS サンプル `advanced/optim_package.jl` のベンチで、`using Optim` だけで ~5.5 秒
  (Base キャッシュ込み)・`compile.build_method_tables` が 97% を占めていた原因が、
  `_bfgs` のコンパイル時戻り値型推論の組合せ爆発(ループ内クロージャ `phidphi` を
  HagerZhang の深い相互再帰呼び出し木へ引き回す)だと特定。`_bfgs` に
  `::MultivariateOptimizationResults`(常に厳密)を注釈して本体推論をスキップさせ、
  `build_method_tables` 5097ms→42ms / `using Optim` ~5.5s→~0.28s / フルサンプル
  ~5.57s→~0.32s。出力・BFGS 厳密パリティは不変(`optim_bfgs_*` green)。一般修正
  (相互手続き戻り値型推論の深さガード)は #8182 で追跡。詳細は STATUS.md 同日項 /
  `docs/vm/OPTIM.md`「Load-time performance」。

### StaticArrays all-static 融合ネスト broadcast のクラッシュ修正 ✅ (Issue #8176)

- `abs.(SVector .+ SVector)` 等の全 static 融合ネスト broadcast が out-of-bounds で
  落ちていたのを修正。内側の全 static `Broadcasted` が空軸 `()` になり
  `_broadcastable_shape(::Broadcasted)` が `instantiate` 内で空 `ax[1]` を参照しクラッシュ、
  `copy` の static hook 到達前に死んでいた。`n == 0` ケースを追加してスカラー shape `()` を
  返すようにし(`base/broadcast.jl`)、hook(#8161 ツリー分類)が all-static ネストを
  static path で処理して `SVector`/`SMatrix` を返す。回帰
  `tests/fixtures/static_arrays/static_arrays_fused_nested_broadcast_8176.jl`。詳細は STATUS.md 同日項。

### StaticArrays mixed static/dynamic broadcast ✅ (Issue #8161)

- `SVector .- Vector` 等、`StaticArray` と動的配列を混ぜた broadcast が動的 operand を
  スカラー扱いして誤 `MethodError` になっていたのを修正。static-broadcast hook を
  **upstream の `BroadcastStyle` 優先規則**(static⊙scalar→静的、static⊙動的配列→動的)に
  沿って融合・ネストを含む operand ツリー全体の再帰分類へ書き換え。動的混在時は static 葉を
  `collect` して generic pipeline で再 materialize し plain `Array`(upstream の `Sized*`
  相当)を返す。Base に `Broadcasted` introspection ヘルパを追加。回帰
  `tests/fixtures/static_arrays/static_arrays_mixed_broadcast_8161.jl`(upstream パリティ)。
  詳細は STATUS.md 同日項。all-static 融合ネストは別バグ #8176。

### module-qualified call の runtime dispatch 修正 ✅ (Issue #8158)

- `Module.f(x)` 修飾呼び出しが、引数静的型 `Any` + callee に catch-all `f(::Any)` の
  とき実行時 dispatch せず catch-all を静的バインドしていたのを修正。bare-call path の
  広い `use_runtime_dispatch` 判定を共有ヘルパ `should_runtime_dispatch`
  (`compile/expr/call/dispatch.rs`)に抽出し、qualified path
  (`compile_module_call_via_method_table`)からも使用。`SciMLBase._callbacks(::CallbackSet)`
  が catch-all に誤 dispatch し `CallbackSet` の全コールバックを無効化していた実害を解消。
  SciMLBase の `_callbacks` は 2 メソッド形式に戻し isa-branch ワークアラウンドを撤去。
  詳細は STATUS.md 同日項参照。

## 最新対応 (2026-06-27)

### AoT top-level `@time` の末尾 `result;` パス文を除去 ✅ (Issue #8150, #8154 重複)

- **根因**: `@time <expr>` は `local result = <expr>; ...; result` に展開され、
  末尾の裸の `result`(マクロの戻り値)が生成 `main()` に `result;` という
  dead path statement として残っていた。plain `cargo check` は通るが、rustc の
  `path_statements` lint は `-D warnings` 下でこれを弾く(AoT クレートは #7076 で
  `-D warnings` を通す必要がある)。`convert_let_block_stmt`
  (`aot/analyze/ir_converter/stmt.rs`)は `#` 接頭辞付き bare temporary しか
  drop しておらず、`@time` の result は `result`(`#` 無し)なので漏れていた。
- **修正**: statement(値破棄)位置の let-block 末尾の bare `Stmt::Expr {
  Expr::Var }` は、変数読み取りに副作用が無くブロックの値も破棄されるため常に
  no-op。`#` 接頭辞に関係なく drop するよう一般化。`println(#elapsed_s,
  " seconds")` など副作用を持つ形は `Expr::Call`(≠ `Expr::Var`)なので従来通り
  保持される。
- **テスト**: `aot_e2e` の top-level `@time` が rustc `-D warnings` を通ることを
  検証する回帰テストを追加(`test_aot_toplevel_time_no_trailing_path_statement_issue_8150`)。

### AoT `@time` codegen の cast 括弧欠落 ✅ (Issue #8146)

- **根因**: `emit_arithmetic` の wrapping 整数演算パス
  (`operations.rs`) が左オペランドを括弧で囲まず
  `{left}.wrapping_sub({right})` を出力していた。`@time` が
  lower する `elapsed_ns = time_ns() - t0` では左辺が
  `... .as_nanos() as i64`(末尾が cast)になるため、生成 Rust は
  `... as i64.wrapping_sub(t0)` となり、Rust は `as` をメソッド呼び出しより
  低優先度で解釈 → `error: cast cannot be followed by a method call` で
  コンパイル不能。`length(s) - 1`(`.len() as i64`)など、末尾が cast に
  なる任意の左オペランドで同様に発生する一般的なバグ。
- **修正**: wrapping 整数演算の receiver を括弧で囲み
  `({left}).wrapping_sub({right})` を出力(他の二項演算 `({} {} {})` と
  同じく完全括弧化)。`@time` は `(... as i64).wrapping_sub(t0)` となり
  コンパイル可能。
- **テスト**: `aot_codegen` 単体テスト(`time_ns()` receiver の括弧化を
  直接検証)と `aot_e2e` の `@time` テスト(壊れた部分文字列が出ないこと +
  括弧化形を検証)。既存 e2e の `@time` テストは文字列 contains のみで
  生成コードを実コンパイルしておらず本バグを見逃していた。
- **既知の別件**: AoT 化した top-level `@time <expr>` は末尾に `result;` の
  path statement を残し `-D warnings` を通らない(本 Issue とは独立した
  既存の lint 問題)。

### 型注釈のホットパス Convert 削除とスロット型保持 ✅ (Issue #8147)

- **問題**: 戻り値型注釈 `function f()::Int` と型付きローカル `tmp::Int = b` は
  lowering(`function/short_form.rs`・`function/full_form.rs` の `make_convert_call`、
  `stmt/assignment.rs` の `TypedExpression`)で `convert(Int, x)` 呼び出しに展開され、
  `compile_convert`(`expr/call/handlers/misc.rs`)が値の型に関係なく必ず
  `CallBuiltin(Convert, 2)` を出していた。値が既に `Int64` でもホットパスで
  `find_best_method_index(["Base.convert"], ...)` のメソッド探索 + dispatch が走る。
  加えて `compile_convert` は結果型を常に `ValueType::Any` で返していたため、
  `tmp::Int = b` のスロット型が `Any` に縮退し、`LoadSlotI64`/`StoreSlotI64`/
  `LoadModI64Slot` 等の typed 命令を失って後続全体が脱最適化された(issue 計測:
  N=2000 で 引数のみ 0.368s / 戻り値付き 1.893s / ローカル付き 10.16s)。
- **修正**: `compile_convert` に2つの最適化を追加。
  - 第1引数が具体型名(`Int`/`Float64`/`Bool`/ユーザー struct 等、`narrowing::
    value_type_for_type_name` で解決)で、値の推論型(`infer_expr_type`)が一致する
    場合、`convert(::Type{T}, x::T) = x` の恒等変換として **Convert を完全に省略**し
    値のみコンパイル(型 push も builtin 呼び出しも出さない)。
  - 変換が実際に必要な場合も、具体型が判明していれば結果型を `Any` ではなく具体型で
    返し、代入先ローカル / 戻り値スロットの specialization を維持。
- **効果**: MWE の `gcd_args` / `gcd_return` / `gcd_full` が同一バイトコード
  (末尾 `LoadSlotI64(0); ReturnI64`、`tmp::Int = b` → `StoreSlotI64(2)`)・同一速度
  (N=2000 で全て 0.36s)になった。恒等でない変換(`::Float64` への widening 等)は
  従来どおり実行される。
- **テスト**: `tests/sjulia_cli_dump_bytecode_tests.rs`
  (`return_and_local_type_annotations_elide_noop_convert_issue_8147`、生成バイトコードに
  `Convert` が出ないこと・`tmp` が `StoreSlotI64` であることを確認)、
  `tests/fixtures/conversion/type_annotation_noop_convert_8147.jl`(恒等・widening 双方の
  挙動を julia と parity 確認)。

### AoT: `@time` 生成 Rust の `as i64.wrapping_sub()` 括弧欠落 ✅ (Issue #8146)

- **問題**: `juliars`(AoT)で `@time` を含むコードを Rust に変換すると、`time_ns()` の
  codegen(`aot/codegen/aot_codegen/expressions.rs` の `TimeNs`)が末尾 `... as i64` を
  出力し、経過時間の減算(`emit_arithmetic` の wrapping 整数演算、`operations.rs`)が
  `{}.{}({})` 形式で `... as i64.wrapping_sub(t0)` を生成。Rust は method call が `as` より
  強く束縛するため `... as (i64.wrapping_sub(t0))` と解釈し「cast cannot be followed by a
  method call」でコンパイル不能だった。
- **修正**: `emit_arithmetic` の wrapping 整数演算で receiver を常に括弧で囲む
  (`({}).{}({})`)。`time_ns()` のような cast を含む低優先度 receiver でも
  `(... as i64).wrapping_sub(t0)` となり正しくコンパイルできる。compound 代入
  (`emit_compound_assign`)は receiver が常に lvalue 変数のため対象外で変更不要。
- **テスト**: `aot_codegen` の `test_aot_codegen_subtraction_cast_receiver_parenthesized_issue_8146`
  (receiver 括弧化を確認)、`aot_e2e_tests.rs` の
  `test_aot_time_macro_generated_rust_compiles_issue_8146`(生成 Rust が
  `cargo check -D warnings` を通ることを確認)。既存 wrapping スナップショット/アサート群も
  括弧付き出力に更新。
### bare/braces パラメトリック inner/outer コンストラクタ選択 ✅ (Issue #8121)

- **根因**: `register_inner_constructors` の `skip_this_struct` が、precompiled
  Base 使用時に「作業用 `method_tables` が非空」を「キャッシュ済み Base struct」の
  代理判定にしていたため、outer コンストラクタを持つユーザパラメトリック struct の
  inner 登録を誤スキップ → bare/braces 呼びが生フィールド格納にフォールバック。
  判定を元の `cached_method_tables` 基準へ修正。
- **inner/outer 同シグネチャ回帰**: `add_method` の dedup が inner(`where T`)で
  outer を置換し bare 呼びが未束縛 `T` の inner に到達(`Angle2d`/`Channel` 型)。
  両メソッドを保持する `add_method_keep_existing` を追加し、tie-breaker 4 が bare に
  outer を選ぶよう修正。
- **inner 型パラ束縛**: braces inner 呼びを `CallStaticParametric` で発行し
  `new{T}`/`T(x)` 用に型パラメータを束縛(`Q{S}(x) where {S}` の名前替えも対応)。
- **テスト**: `struct/parametric_inner_ctor_with_outer_8121.jl`(Foo scaling inner +
  Rotations 風 `AngleAxis` 正規化 multi-field inner + Complex/Rational 退行ガード)。

### ユーザー定義 `Base.getproperty` override 全般 ✅ (Issue #8127)

- **問題**: `compile/expr/struct_.rs` の `compile_field_access` は既知の
  `ValueType::Struct(type_id)` の `obj.field` を宣言フィールド一覧で解決し、
  未宣言名は `Unknown field '<f>' on struct '<T>'` でコンパイルエラーにしていた。
  上流 Julia では `x.f` は常に `getproperty(x, :f)` に lower され既定が `getfield`
  にフォールバックするため、ユーザー定義 `getproperty` が無視されていた。
- **修正**: レシーバの静的型が構造体で、`getproperty` の dispatch が*ユーザー*メソッド
  (`function_ir_by_global_index` に IR を持つ)へ解決される場合、`obj.field` を
  `getproperty(obj, :field)` 呼び出しへ経路付け。宣言・計算プロパティの双方が override
  経由で解決し、宣言フィールドは override の `getfield` フォールバックで従来どおり。
  override の無い型は直接フィールドアクセスの fast path を維持。
- **specializer**: 関数 specializer は IR の `Expr::FieldAccess` を独立にコンパイルし
  直接 `GetField` を出すため override を bypass しうる。プログラムにユーザー
  `getproperty` override がある場合 `RuntimeCompileContext.disable_field_access_specialization`
  を立て、specializer の `compile_field_access` を `Unsupported` にしてインタプリタ
  (= `getproperty` 経路を含むメインコンパイラ生成 bytecode)へフォールバックさせる。
- **テスト**: `fixtures/struct/getproperty_override_8127.jl`(計算プロパティ・宣言
  フィールド・ラッパー型・typed parameter・specializer ホットループ回帰)。

### 再帰・相互再帰なネスト closure の自己/兄弟参照 ✅ (Issue #8118)

- **問題**: 外側ローカルを capture するネスト関数(closure)の自己再帰・相互再帰が
  `ErrorException: Unknown function: <name>` で失敗。自己/兄弟名は capture 環境に
  入らず(free-variable 解析時点で束縛前)、closure は環境経由でしか呼べないため
  plain method dispatch でも解決できなかった。非 closure の自己/兄弟は Issue #8105 で
  対応済みだったが closure 経路は意図的に除外されていた。
- **修正**: `try_compile_nested_scope_call`(`compile/expr/call/mod.rs`)で、候補が
  closure かつ capture 集合が現スコープで解決可能なとき、現フレームの capture から
  `CreateClosure` で closure を再構築して呼ぶ(自己再帰では候補の capture 集合は現
  closure の capture そのものなので常に成立)。兄弟が*非 closure*関数のときは
  `CreateClosure` ランタイム(`vm/exec/stack.rs`)が、フレームに無い capture 名を
  enclosing scope の `parent#name` 関数値(`FunctionValue::new`)へフォールバック解決する
  (`resolve_sibling_nested_function`)。兄弟自身が closure の場合は環境を運べないため
  経路付けせず従来動作を維持。
- **テスト**: `fixtures/closures/recursive_nested_closure_8118.jl`(MWE1 自己再帰 +
  値 capture、MWE2 純粋相互再帰、capture 越し実計算の階乗、even/odd 相互再帰、
  非再帰 escaping closure と関数引数 capture の回帰ガード)。
### ネストしたパラメトリックコンストラクタ引数の型パラメータ欠落ディスパッチ ✅ (Issue #8090)

- **症状**: `Wrap(SMatrix{2,2}((1.0,2.0,3.0,4.0)))` のようにパラメトリック
  コンストラクタ結果を直接(ネストして)引数に渡すと `MethodError`。同じ値を
  ローカルに束縛してから渡すと成功(上流 julia は両方成功)。
- **根因**: `infer_julia_type` が `SMatrix{2,2}`(宣言は `SMatrix{M,N,T}` の 3
  パラメータ)を末尾パラメータ未束縛のまま静的型として返し、`SMatrix{N,N,T}` で
  特殊化したメソッドに一致せず、`Any` 引数のような実行時フォールバックにも
  回らなかった。束縛経路はスロット型が `Any` に広がるため実行時ディスパッチで
  正しく解決していた。
- **修正**: `compile/expr/infer/julia_type.rs` のパラメトリックコンストラクタ枝
  (`function.contains('{')`)に
  `parametric_constructor_has_unbound_trailing_params` ガードを追加。書いた型
  パラメータ数が宣言数より少ない場合は静的型を `Any` へ広げ、束縛経路と同一の
  実行時多重ディスパッチへ回す。
- **テスト**: `static_arrays/nested_parametric_ctor_arg_8090.jl`(ネスト vs 束縛、
  2×2 Float64 / 3×3 Int64、上流 julia と出力完全一致)。

### StaticArrays.jl Phase 4–5: protocol / broadcast / 小型線形代数 ✅ (Issue #7433, #7460, #7461)

`subset_julia_vm/packages/StaticArrays/` の immutable MVP を Phase 4(プロトコル)
と Phase 5(小型線形代数)まで前進させ、マイルストーン #7433 を完了。

- **Phase 4 プロトコル (#7460)**: スカラ/多次元/線形 `getindex`、反復
  (`for`/`reduce`/`sum`/`prod`/`minimum`/`maximum`/`any`/`all`)、`map` と単項
  `-`(いずれも静的型を保持)、要素型を強制変換する `convert(SVector/SMatrix)`。
- **Phase 4 broadcast (#7460)**: `v .+ 10`/`10 .+ v`/`v .- 1`/`sin.(v)`/`abs.(v)`/
  `v .^ 2`/`v ./ 2`/`v .+ w`/`2 .* v` と行列 broadcast が**静的配列**を返す。
  非 broadcast の `v + 10`/`sin(v)` は上流同様 `MethodError`(parity 維持)。
- **Phase 5 線形代数 (#7461)**: `transpose`(2×2/3×3、SMatrix を返す)、`tr`、
  `diag`(SVector を返す)、`det`(1×1/2×2/3×3 閉形式)、`inv`(1×1/2×2)、
  既存の `dot`/`norm`。値は上流 StaticArrays と一致。

実装の要点:

- **VM iterate アーム**(`vm/type_ops/iteration.rs`): `Value::StaticArrayInline`/
  `StaticArray` を列優先バッキングタプルへ委譲し `iterate_first`/`iterate_next`
  を実装。`for`/`reduce`/`collect` を解放。
- **Base broadcast フック**(`base/broadcast.jl`): `_STATIC_BROADCAST_HOOK`
  (`Ref{Any}`)を `copy(::Broadcasted)` が実行時に読み、StaticArrays が
  `_set_static_broadcast_hook!` で実装を登録。Base 内部の名前呼び出しは
  devirtualize されパッケージ override を見ないため、関数値を Ref 経由で動的
  呼び出しする方式を採用(broadcast 専用なので非 broadcast の parity を壊さない)。
- 行列結果(broadcast/map/transpose/inv)は runtime パラメータ `SMatrix{M,N}`
  構築が未対応(#8125)のためリテラルサイズ分岐で構築。

保留・既知ギャップ: `adjoint`(#8132)、型一致する `collect`(#8131)、`MArray`/
`SizedArray`/`FieldArray`/分解系・拡張。fixtures: `static_arrays_protocol_7460.jl`,
`static_arrays_linalg_7461.jl`(上流 julia と parity 照合済み)。
### `import/using ... as ...` のエイリアス束縛 ✅ (Issue #8117)

`import X as Y` / `using X: y as z` のリネームが no-op で、エイリアス名 (`Y`/`z`)
が一切束縛されず、呼び出すと `Unknown function` になっていた問題を修正。lowering
(`lower_one_import_path`) で ` as ` を構文解析し、`UsingImport.alias_bindings`
に `(source_dotted_path, target_name)` を記録。`using`/`import` を下ろす 3 箇所で
`target = source` の合成代入文を発行することで、エイリアス名を取り込んだエンティティ
(関数値またはモジュール)へ束縛する。

- シンボルエイリアス `using .Mz: q as qq` → `qq = Mz.q`、モジュールエイリアス
  `import LinearAlgebra as LA` → `LA = LinearAlgebra`。元の名前 (`q`) は束縛しない
  (上流 Julia と一致)。
- `Base` は sjulia の暗黙のグローバル名前空間なので `import Base: sin as mysin` は
  bare 名 `mysin = sin` へ束縛(多くの組み込みは `Base.sin` を値として解決できない
  ため)。
- fixture: `modules/using_import_as_alias_8117.jl`(`using`/`import` 両形式・
  ユーザモジュール・stdlib・`Base`・whole-module を網羅、上流とパリティ)。
- 既知の別件(本修正の範囲外, Issue #8137): `Base.map` 等の多くの Base 関数を
  **値**として修飾アクセスする経路は未対応(`import Base: f as g` は上記 bare 束縛で
  正しく解決済み)。
### collect(::StaticVector) が要素型を保持 ✅ (Issue #8131)

`collect(SVector(1,2,3))` が `Vector{Any}` に広がる(値は正しいが要素型が
喪失する)バグを修正。根因は当初の issue 推測(`_collect` の `HasEltype`/
`EltypeUnknown` 特異点ディスパッチ)ではなかった(`_collect` の
`::HasEltype,::HasLength` メソッドは SVector/Array いずれでも正しく選択される
ことを実機計測で確認済み)。真因は **1点**:
`eltype` ビルトイン (`BuiltinId::Eltype`) が `StaticArrayInline`/`StaticArray`
キャリアを処理せず `Any` にフォールバックしていた → 静的型が不透明
(`itr::Any`)なジェネリック `_collect` 内で `eltype(itr)` が `Any` を返し
`Vector{Any}` を生成(静的配列型が静的に判明する呼び出し位置では純 Julia の
`eltype(::StaticArray)` が走るためバグがマスクされていた)。
- 修正: `eltype` ビルトインに静的配列の arm を追加(`StaticArrayInlineData.tag` /
  新規 `StaticElem::element_type_name()` から具体要素型を返す)。
  注: 当初は `iterate_first`/`iterate_next` の静的配列 arm も追加していたが、
  origin/main の #7460 Phase 4 が既に `StaticArrayInline` の iterate を tuple
  iterate 経路へ委譲しており冗長と判明したためリベース時に破棄(本 PR の
  コード差分は `eltype` のみ)。
- 効果: `collect(::StaticVector)` が `Vector{T}`(`Int64`/`Float64`/`Int32`/空)で
  上流一致。`sum`/`for`/内包表記の iterate 系も静的配列で動作。
- 既知の積み残し: `collect(::StaticMatrix)` は 2-D `Matrix` でなく平坦 `Vector`
  になる(Base `collect(itr)` 内で `IteratorSize(itr)` が `HasLength` に
  devirtualize されるため)→ #8139 で追跡。
- fixture: `static_arrays/static_arrays_collect_eltype_8131.jl`(上流 StaticArrays と
  パリティ確認済み)。

### collect(::StaticMatrix) が 2-D 形状を保持 ✅ (Issue #8139)

`collect(SMatrix{2,2}(1,2,3,4))` が平坦 `Vector{Int64}` になり 2-D 形状を喪失する
バグ(#8131 の続き、値・要素型は正しく形状のみ誤り)を修正。
- 根因: ジェネリック `collect(itr)` は `_collect(1:1, itr, IteratorEltype(itr),
  IteratorSize(itr))` 経由で経路を決める。`itr` は静的に `Any` なので
  `IteratorSize(itr)` は実行時値でディスパッチするが、base には非 `Array` の
  `AbstractArray`(StaticArrays の `SMatrix{2,2,Int64} <: ... <:
  AbstractArray{Int64,2}`)に一致する `IteratorSize` 規則が無く、ジェネリックな
  `IteratorSize(::Type)=HasLength()` に落ちて `_collect` が 1-D Vector を生成して
  いた。
- 上流規則は型レベル `IteratorSize(::Type{<:AbstractArray{<:Any,N}})=HasShape{N}()`
  だが、sjulia のディスパッチャは静的配列の抽象スーパータイプ鎖越しに型パラメータ
  `N` を束縛できない(実機計測: `::Type{<:AbstractArray{T,N}}` は plain
  `Vector{Int64}` でも `T`/`N` 未束縛で `UndefVarError`)。これが sjulia が
  `IteratorSize(::Type{Vector{T}})`/`IteratorSize(::Type{Matrix{T}})` を個別に
  持つ理由でもある。
- 修正: 値ベースの `IteratorSize(a::AbstractArray)=HasShape{ndims(a)}()` を
  `base/generator.jl` に追加。`Array`/`AbstractRange`/`Memory` の既存のより特異な
  メソッドが優先されるため挙動不変。実行時値でディスパッチするので静的 `Any`
  引数を通る `collect` 内でも正しく `HasShape` 経路に乗り、`_similar_shape`/
  `axes(itr)` で 2-D 形状を復元する。
- 効果: 正方/非正方/Float の `SMatrix` が `Matrix{T}` に(上流一致)、`SVector` は
  `Vector{T}` のまま(#8131 維持)、transpose/`view` 等の他の非 `Array`
  `AbstractArray` も `Matrix` を保持、plain Array は不変。
- 残: 完全に静的型が判明する `IteratorSize(SVector(1,2,3))` のトップレベル
  インライン呼びは依然コンパイル時に `HasLength` へ devirtualize する(同じ
  devirtualization クラス。`collect` の実行時経路は不変でユーザ可視のバグは解消)。
- fixture: `static_arrays/static_arrays_collect_matrix_shape_8139.jl`(上流
  StaticArrays とパリティ確認済み)。

### Rotations.jl サポート MVP ✅ (Issue #7434, Phase 0–5 #7471–#7476)

純 Julia の Rotations.jl MVP を `subset_julia_vm/packages/Rotations/` にバンドル完了。

- **型**: `Rotation{N,T}`(抽象), `RotMatrix{2,3}`/`Angle2d`, `RotX/Y/Z`,
  `AngleAxis`, `RotationVec`/`RodriguesParam`/`MRP`, `QuatRotation`
  (`.w/.x/.y/.z`), ジェネレータ `Angle2dGenerator`/`RotationVecGenerator`。
- **関数**: `rotation_angle`/`rotation_axis`/`rotation_between`(2D/3D)/
  `isrotation`/`isrotationgenerator`/`Rotations.params`/`Rotations.skew`/
  `slerp`/`Tuple`/`getindex`/`one`/`inv`/`*`(回転・合成)/`/`/`\`/`adjoint`/
  `transpose`/`size`。
- **検証**: fixtures 8 本 (`tests/fixtures/rotations/`) が上流 Rotations 1.7.1 と
  数値一致 (oracle 照合)。`cargo nextest run --test fixture_tests rotations` green。
- **sjulia 適応**: 単一メソッド+`isa`分岐 (#7960)、`StaticMatrix` 非継承 (#8103-B)、
  Tuple ベース matvec (#8090)、コンストラクタは default field ctor 経由
  (#8103/#8121 回避)、`QuatRotation` は `.w/.x/.y/.z` 実フィールド化 (#8127)。
- **付随修正**: StaticArrays スカラ除算・`norm`/`normalize` (#8125)、列優先 (#8084)、
  型システム (#8092)、コンストラクタ特定性 (#8103)。
- **保留**: ForwardDiff/RecipesBase/Unitful/乱数/`RotMatrixGenerator`/exp・log マップ/
  全 Euler 変種/eigen — 詳細は [ROTATIONS.md](./ROTATIONS.md)・[UNIMPLEMENTED.md](./UNIMPLEMENTED.md)。
### Module 値を介したメンバアクセス（ネスト sub-module / const エイリアス）✅ (Issue #8113/#8114)

`Module` 値を介したメンバ（型/関数/const）アクセスを解決:
- #8113: ネスト sub-module `Outer.Inner.T1`（旧: コンパイルエラー
  `Field access requires a struct type, got Module`）。
- #8114: const モジュールエイリアス `const MA = Mod1; MA.S`（旧: 実行時
  `GetFieldByName: expected struct, got Module`）。

- **根因（共通）**: 修飾アクセス経路はトップレベル `Module.member` のみ解決していた。
  (a) `const X = Mod`（ユーザモジュール）はエイリアス未登録、(b) ネスト多段パスは object が
  `Var` でなく `FieldAccess` のため `Expr::Var` モジュール分岐を素通りし中間 `Module` 値を
  struct 扱い、(c) 修飾呼び出しのエイリアス解決が全体名のみでルートエイリアス（`AA`→`A`）
  非対応。
- **修正**:
  1. `compile/stmt.rs`: `const/var = <ユーザモジュール>` を `module_aliases` + `locals=Module`
     に登録（`module_functions`/`module_exports` で判定）。以後 `MA.member` は既存
     `Expr::Var` モジュール経路で解決。
  2. `compile/expr/struct_.rs`: `compile_field_access` で多段の既知モジュールパスを
     `compile_module_function_ref` に委譲（`resolve_user_module_path`）。ルートが非モジュール
     ローカルでシャドウされる場合は除外（`module_path_root_shadowed_by_local`、#7245 維持）。
  3. `compile/expr/call/module_call.rs`: `resolve_module_alias_path` を導入し、全体名エイリアス
     に加えドット区切りパスの**ルートセグメント**エイリアスも解決（`AA.B.C` → `A.B.C`）。
     `compile_module_call` と `resolve_user_module_path` の双方で共有。
- **カバレッジ**: 型/関数/const メンバ、2–3 段ネスト、エイリアス起点のネスト鎖
  （`const AO = Outer; AO.Inner.Deeper.U`）まで上流パリティ。
- **無影響**: モジュール名と同名のローカルパラメータは依然シャドウ（#7245）、非モジュール
  const 代入も不変。
- **回帰**: `module_tests::module_value_field_access_8113_8114`（17 assertions, julia パリティ）。
- **既知の差分（本 issue 外）**: 修飾型名の表示は sjulia が `Outer.Inner.T1`、upstream は
  `Main.Outer.Inner.T1`（`Main.` 接頭辞）— 単段 `Mod1.S` と同様の既存表示規約差で本修正前から存在。

### ネストローカル関数が同名グローバルをスコープ内でシャドウ ✅ (Issue #8105)

`function h(); g() = 2; return g(); end`（外に `g() = 1` あり）で `h()` が内側の
`g`（=2）を、トップレベルの `g()` と値参照 `f = g; f()` がグローバル（=1）を、
それぞれ正しく解決するようにした。修正3点: (1) インライナ `inline_block` がブロック内の
直下 `FunctionDef` 名を事前にローカル束縛として登録（グローバル本体の誤インラインを防止）。
(2) `build_method_tables` がネスト関数を `parent#name` 修飾名テーブルのみに登録（ショート名
テーブルでの dedup 置換による値参照クロバーを除去）。(3) `try_compile_nested_scope_call`
追加: ネスト関数本体内の素の呼び出しを `current_function_name` のスコープ連鎖を辿って
修飾名テーブルへ解決（自己/兄弟再帰を維持。クロージャは除外）。引数あり・型付きメソッド
可視性・自己再帰（非クロージャ）・グローバル無しケースも網羅。fixture
`closures/nested_local_shadows_global_8105.jl`（parity 12/12、full nextest 165 fixtures /
3026 lib green）。派生未対応＝キャプチャ付きネストクロージャの自己/相互再帰 (Issue #8118)。

### 回帰修正: インラインラムダ HOF 戻り値型のローカル束縛伝播 ✅ (Issue #8105 後退)

#8105 の副作用回帰を修正: `y = reduce((acc,x) -> acc + x*0.5, [1,2,3])` の `y` が
`Float64` でなく `Any`（`StoreSlot`）として格納されていた（`map`/`mapreduce`/`foldl` ほか
インラインラムダ HOF も同様）。根因: 巻き上げ済みインラインラムダ実引数（末尾が**素の**
`__lambda_nested_N` の `LetBlock`）が、#8105 でショート名メソッドテーブルから素名が消えた
ため `infer_julia_type=Any` → `has_any_arg` → `dispatch` が `NoMethodFound` → ランタイム
ディスパッチ arm が `Ok(Any)` を返し**呼び出し点 HOF 戻り値推論に未到達**。修正:
`dispatch.rs` の `NoMethodFound`/ランタイム arm で `Any` widen 前に新ヘルパ
`infer_hof_call_site_return_type`（map/broadcast/filter/reduce/foldl/foldr/mapreduce/
mapfoldl/mapfoldr を呼び出し点式から推論; インラインラムダは `resolve_hof_callable` の
`LetBlock` arm で解決）で静的戻り値型を回収。#8105 のランタイムディスパッチには非干渉
（束縛の型注釈のみ復元）、非 HOF は従来どおり `Any`。`map`/`reduce`/`mapreduce`/`foldl`/
`filter` を上流 julia とパリティ確認。回帰: `type_propagation_call_tests` の
`test_reduce_inline_lambda_return_type_inference_issue_5094` /
`test_qualified_reduction_hof_return_type_inference_issue_5094`。

### `type` / `as` をコンテキスト依存キーワード化（識別子扱い）✅ (Issue #8108)

`#8099`（`outer`）と同根。`type`/`as` をレキサの予約語トークン `KwType`/`KwAs` から
撤去し普通の `Identifier` として字句解析。コンテキスト依存位置（`abstract`/`primitive type`
の `type`、import/using 別名の `as`、`for outer` の `outer`）はテキスト一致の共通ヘルパ
`check_contextual_keyword`/`expect_contextual_keyword`（`Parser` に追加）で検出。これにより
`function type() … end`／`type() = …`／`function as() … end`／`type`/`as` 変数・引数・
フィールド・関数値が上流同様に識別子として動作。`abstract type`/`primitive type`/
`using … as …` のパース・降ろしは不変（`import X as Y` の別名束縛は main でも未実装の
no-op で対象外）。fixture `parse/type_as_identifiers_8108.jl`（julia/sjulia parity 15/15）+
`corpus_statements.rs` にパーサ単体テスト追加。`abstract`/`mutable`/`primitive` の識別子化は
同系統だが別途。

### ローカル `DataType` 値への明示 apply-type コンストラクタ ✅ (Issue #8101)

`t = A.Pt; t{Float64}(1.0, 2.0)`（ローカル変数が保持する parametric struct の `DataType`
値へ明示型パラメータを与えて構築）が `Unknown parametric struct: t` で失敗していたのを修正。

- **根因**: `t{Float64}(...)` のベース名 `t` はコンパイル時に静的解決できず（`parametric_structs`
  未登録）、`resolve_instantiation_with_type_expr` が失敗。
- **修正**: ベースがローカル `DataType` のとき `compile_local_datatype_parametric_call` で
  実行時 `ApplyTypeDynamic` により `Base{Float64}` を構築→`CallFunctionVariable` で構築呼び。
  実行時 `try_construct_parametric_datatype` に明示型パラメータ経路を追加し、名前に
  `{...}` があれば推論せず convert（`coerce_fields_to_explicit_type_args`）。型無し動的形
  `t(1.0, 2.0)` (#8070) の明示-`{T}` 版で、上流 `Base{T...}(args)` の convert 意味論に一致。
- **fixture**: `modules/local_datatype_applytype_ctor_8101`（parity 検証済み）。

### parametric デフォルトコンストラクタの非統一引数を MethodError 化 ✅ (Issue #8102)

`Pt9(1, 2.0)`（`struct Pt9{T}; x::T; y::T; end`）が `Float64` へ誤昇格していたのを上流同様
`MethodError` に修正。

- **根因**: 型パラメータ推論の `record_binding` が同一 `T` の `Int64`/`Float64` を `Float64`
  へ数値昇格していた。上流のデフォルトコンストラクタ `Pt9(x::T,y::T) where T` は単一具体
  `T` を要求し、混在は no-method。
- **修正**: `record_binding` を厳密統一に変更（異なる**具体**型は昇格せずエラー）。`Any`
  プレースホルダ（コンパイル時に確定不能な引数）のみ具体型へ refine/defer して保持
  （`Truncated(...)` 等の不確定構築を維持）。コンパイル経路は推論失敗で `ThrowMethodError`
  emit、実行時経路は推論 Err→`MethodError`。明示 `Pt9{Float64}(1,2.0)` は別経路 convert で不変。
  `widen_numeric_types` は唯一の利用者を失い削除。
- **fixture**: `struct/parametric_ctor_no_widen_methoderror_8102`（MethodError + 正当全ケース）。
### モジュール内の短い型名を値として参照すると DataType に解決 ✅ (Issue #8100)

`module M; struct E end; getE() = E; end` の `typeof(M.getE())` が `TypeVar` に
誤解決していた回帰を修正（upstream は `DataType`）。

- **根因**: 短い大文字綴り (`E`/`T1`) は `is_type_variable_name` により `CoreType::TypeVar`
  へ解釈される。`kind_for` は宣言済み型なら DataType に戻すが、判定 `declares_base_name`
  が `struct_defs` を完全一致で照合していた。モジュール private 型は修飾名 `M.E` で登録され、
  本体内裸参照は `Struct("E")` を射影するため不一致 → TypeVar。さらに `===`（型同一性）は
  `Struct("E")` と `Struct("M.E")` を `CoreType::from` 経由で短名 TypeVar 化し reconcile
  できず false だった。
- **修正**:
  1. `vm/type_objects.rs` `declares_base_name` をモジュール末尾一致でも照合
     (`declared_name_matches_base` / `module_unqualified_name`)。裸クエリは tail 一致、
     修飾クエリは完全一致のみ。typeof / `isconcretetype` 経路（`declares_type_name` /
     `declares_bare_type_name`）を両方カバー。
  2. `vm/type_utils.rs` `type_objects_equal` に Struct 同士の正規化名一致 fast-path を追加。
     `normalize_type_for_isa` で module prefix 除去 + alias 正規化した名前が等しければ同型
     （加算的; 長い名前で既に subtype engine が行う module-strip と整合）。
- **無影響**: 真の `where {T}` 短い型パラメータ（宣言済み型でない名前）は依然 TypeVar。
- **回帰**: `module_tests::module_short_type_name_value_8100`（22 assertions: typeof /
  `===` 修飾名一致 / isa / `<:` / isconcretetype / `where {T}` / 実 TypeVar 値, julia パリティ）。
### キーワード引数デフォルト `Inf`/`NaN` が `0` に解決される修正 ✅ (Issue #8078)

`g(; a=Inf) = a; g()` が `0` を返していた（上流 `Inf`）。`-Inf`/`NaN`/`Inf32`/`Inf16`/
`Inf64`/`NaN*`、`@kwdef` の `Inf` フィールド、転送キーワードも同様に壊れていた。位置引数
デフォルトは無事。

- **根因**: `Inf`/`NaN`（および `Inf32` 等）は式位置では float リテラルとして emit される
  Base グローバル定数だが、実行時グローバルには束縛されない。キーワードデフォルトの2つの
  評価器（`compile::utils::eval_literal_default` のベイク定数、`vm::exec::call::value_from_bound_name`
  の実行時ミニインタプリタ）は名前を束縛スロット/グローバルとして検索 → ミス →
  `Value::I64(0)` フォールバック。
- **修正**: 共有 `float_special_constant_value`（`compile/constants.rs`）が
  `Inf`/`Inf64`/`Inf32`/`Inf16`/`NaN`/`NaN64`/`NaN32`/`NaN16`（+ `pi`/`ℯ`）を `Value` に解決し、
  両評価器で使用（束縛名が優先＝同名パラメータでシャドウすると勝つ）。`infer_default_type` の
  `Var`/`UnaryOp` arm も精密な float 型を返し単項 `-`/`+` を operand へ再帰（`-Inf`/`-1.5` の
  `@kwdef` フィールドが内側コンストラクタ dispatch を `Int64` で誤解決していた別バグ #8109 も
  解消。実装中に発見し起票）。
- **W-40 撤去**: `HagerZhang(; alphamax = Inf, ...)` を上流形に復元し、コンストラクタ本体も
  `Float64(alphamax)` に戻した（負センチネル `-1.0` 回避策を撤去）。
- **検証**: 新規 fixture `kwargs/kwarg_inf_nan_default_8078.jl`（35 アサート、julia 1.12.6 と
  パリティ）。`kwargs`/`functions`/`numeric`/`kwargs_splat`/`optim` カテゴリ green（W-40 無しで
  BFGS 通過）。`compile::utils` / `compile::constants` の単体テスト追加、clippy clean。
### `outer` をコンテキスト依存キーワード化（関数名/変数名で識別子扱い）✅ (Issue #8099)

`function outer() … end`／`outer() = …`／`outer` 変数・引数・フィールド・関数値が上流 Julia
同様に普通の識別子としてパースされるようにした。レキサの `#[token("outer")]`（予約語
`KwOuter`）を撤去し `outer` を通常 `Identifier` として字句解析。`for outer x in …` 修飾子の
検出は `parse_for_binding` でテキスト一致に変更し、`for outer in itr`（ループ変数名 `outer`,
Issue #6414）の挙動も維持。fixture `parse/outer_as_identifier_8099.jl`（julia/sjulia
parity 7/7）。`for outer x` 修飾子そのものの lowering 未対応（Issue #6465）は対象外。

### REPL: 空配列フィールド struct の配列グローバル永続化修正 ✅ (Issue #8086)

#8063 (#7850) が `Plot` struct に空配列フィールド (`hlines`/`vlines = Float64[]`) を
追加した結果、`@gif`/`push!(ps, p)` の後 `ps`（Plot 配列）が次の REPL eval で
`UndefVarError` になる回帰を修正。

- **根因**: グローバル永続化 (`repl/converters.rs::value_to_init_expr`) は struct を
  位置コンストラクタ呼び出しで再構築する。空配列を `None` にする規則（トップレベルで
  モジュール初期化子に委譲する #5296 の意図）が**ネストした再構築（struct フィールド・
  配列要素）にも適用**され、空配列フィールドで struct 全体が `None` → 配列要素変換失敗 →
  グローバルが丸ごとドロップ → 次 eval で未定義。
- **修正**: `value_to_init_expr` を `nested: bool` 付き内部関数に分離。トップレベルは
  従来通り空配列で `None`（#5296 維持）、ネスト時のみ `empty_array_init_expr` で
  `TypedEmptyArray` を生成して再構築。
- **検証**: `repl::tests::test_repl_gif_with_global_accumulator_7151` が再 green。
  Plots 非依存の焦点回帰 `test_repl_persist_array_of_struct_with_empty_array_field_8086`
  を追加（空 `Vector{Float64}` フィールドを持つ struct の配列が eval をまたいで永続）。
  repl:: 124/124, clippy clean。

### 修飾 `Base.f` 呼び出しのシャドウ誤再ディスパッチ修正 ✅ (Issue #8079)

モジュールが Base 関数と同名・同シグネチャの自前関数を定義したとき、明示的な
`Base.<name>(...)` 修飾呼び出しが Base 実装へ到達するよう修正（W-41 撤去）。

- **根本原因**: 共有の短縮名メソッドテーブル（モジュール非依存）で `MethodTable::add_method`
  がシグネチャ重複として Base メソッドを置換 → `Base.log2(float(x))` がシャドウ
  `NaNMath.log2` に解決し自己再帰 → `MAX_CALL_DEPTH` 超過で偽 `StackOverflowError`。
- **発見手法**: `try_push_call_frame` の overflow 地点に env-gated でフレーム名ダンプを
  仕込み、再帰サイクル `NaNMath.log2 → … → NaNMath.log2` を特定。`compile_module_call`
  の `Base.` フォールバックが裸名 `compile_call("log2")` に落ちてモジュール所有
  リダイレクト経由でシャドウに当たることを突き止めた。
- **修正**: `pipeline_ctx.rs::build_method_tables` がユーザメソッド追加で base メソッドが
  実際にクロバーされた瞬間（追加前後で base-program メソッド数が減少）に、クロバー前の
  base メソッド群を `Base.<name>` テーブルへ退避。`compile/expr/call/module_call.rs::compile_module_call`
  は `Base.<name>` テーブルが base-program メソッドを持つ場合に
  `compile_module_call_via_method_table` 経由でディスパッチ。退避はクロバー時のみ起きるので
  型付き多メソッド base 関数（`log`/`sqrt` 等、無型シャドウで非クロバー）は従来経路を維持。
  append-only enum 変更なし。
- **一般性**: `sqrt` の #8042/W-34（builtin 候補集合での対処）の純 Julia 版を汎用化
  （`log2`/`log10`/`transpose`/`adjoint` 等 builtin を持たない Base 関数全般をカバー）。
- **テスト**: `modules/module_qualified_base_shadow_8079.jl`（julia 1.12.6 / sjulia parity 9/9）。
  `modules::` + `optim::` カテゴリ green（BFGS が定数 52 ハードコード無しで収束）。

### Optim `BFGS` 準ニュートンソルバ ✅ (Issue #8059)

bundled Optim.jl に BFGS 準ニュートンソルバを追加。`optimize(f, g!, x0, BFGS())`
（ユーザ勾配）と `optimize(f, x0, BFGS())`（中心差分 `autodiff = :finite`）の両形式に対応。

- **上流忠実移植**: `LineSearches.HagerZhang`（近似 Wolfe 直線探索、`hagerzhang_search`）+
  `InitialStatic`（BFGS 既定）を上流から忠実移植。`NLSolversBase` に value/gradient
  キャッシュ（`x_f`/`x_df`）と中心差分勾配（FiniteDiff 既定 `cbrt(eps)` ステップ）を追加。
  Sherman-Morrison 逆ヘッセ更新を含む BFGS ループを `bfgs.jl` に実装。
- **パリティ**: 1 ステップ収束する 2 次形式は minimizer/minimum と f/g 呼び出し回数まで
  上流と完全一致（`quadf`/`sumsq`: 1 反復, 3/3 calls, minimizer `[1,2]`/`[0,0,0]` 完全一致）。
  Rosenbrock は minimizer/minimum が許容誤差内で `[1,1]` に収束（両者）。反復・f/g 呼び出し
  回数は線形探索内部と逆ヘッセ `dot`/`mul!` の縮約順序（上流 BLAS vs VM スカラループ）で差が出る
  うえ Optim のリリース間でもドリフトするため assert しない（installed Optim 2.2.1 ではユーザ勾配
  Rosenbrock が上流 21 反復・35/35 calls に対し sjulia 16 反復、`f_calls == g_calls` は両者成立）。
  全 fixture は上流 Optim 2.2.1 / julia 1.12.6 と sjulia で同一 pass 数（parity 検証済み）。
- **発見した VM バグ**: `Inf` キーワード既定値が 0 に化ける (#8078, W-40)、BFGS 直線探索
  クロージャ内で `ceil(Int,-log2(eps))` が StackOverflow (#8079, W-41)、目的関数を `f` という
  変数名に束縛すると BFGS クロージャ捕捉が壊れる (#8080)。各々起票し、必要な箇所を回避
  （W-40/W-41/W-42 は [WORKAROUNDS.md](./WORKAROUNDS.md)）。
- Fixtures: `optim_bfgs_quadratic.jl`, `optim_bfgs_rosenbrock.jl`。詳細は [OPTIM.md](./OPTIM.md)。

### `optimize(f, …, BFGS())` 目的関数名 `f` のクロージャ捕捉衝突は既に解消済み → W-42 撤去 ✅ (Issue #8080)

#8059 移植中に発見した「目的関数を `f` という変数に束縛すると BFGS で
`Captured variable not found: f`」のバグを調査。**現 `origin/main` では再現しない。**

- **根本原因と所在**: 呼び出し側の `f`・`optimize`/内部ソルバのパラメータ `f`・中心差分勾配の
  クロージャファクトリ `_central_difference_gradient(f)` が返すクロージャの捕捉 `f` が、捕捉名
  解決で衝突しうるキャプチャ解決バグ。エラー `Captured variable not found: f` は
  `LoadCaptured("f")` 時に `captured_vars` に `f` が無い＝`CreateClosure` の捕捉名リストから
  `f` が脱落していた状態（中間スコープの同名パラメータ `f` を束縛済みと誤認して自由変数から
  外す）に対応する。
- **二分探索で解消済みと確認**: BFGS フィーチャコミット `582648adc`（W-42 が書かれた地点）に
  クロージャファクトリ形を復元してビルドしても、目的関数名 `f` の明示勾配・有限差分の両形式が
  上流一致で成功する。基盤バグは BFGS マージ前にマージされたネストクロージャ捕捉修正群
  （#7600 nested-closure global miscapture / #7618 free-var capture-on-assign / #7759
  named-closure capture box）で既に解消されていた。
- **対応**: 回避策 W-42 を撤去し、`NLSolversBase` の有限差分勾配を上流忠実な
  `_central_difference_gradient(f)`（`f` 捕捉クロージャを返す＝`OnceDifferentiable(f, x0)`
  コンストラクタ本体から構築）に戻した。非捕捉の `_central_diff_gradient!(G, obj.f, x)` を削除。
  VM 変更は不要。WORKAROUNDS.md の W-42 を「Resolved」へ移動。
- **回帰 fixture**: `closures/closure_factory_name_collision_8080.jl`（standalone: グローバル `f` ＋
  パラメータ `f` ＋ コンストラクタ本体経由の捕捉 `f` の三つ巴を、ネストした line-search 風ループで
  呼ぶ。upstream julia と出力完全一致）、`optim/optim_objective_named_f.jl`（実 Optim 経路で
  目的関数を `f` に束縛し BFGS 明示勾配・有限差分クロージャファクトリ・GradientDescent を検証）。

### Plots.jl — `title!`, `xlims!/xlims`, `ylims!/ylims`, `hline!/hline`, `vline!/vline` 実装 ✅ (Issue #7850)

`Plot` 構造体に `xlims`/`ylims`/`hlines`/`vlines` フィールドを追加（末尾追加で既存 Rust 読み取りは無変更）。
純 Julia 側 API 全 9 関数を実装。Rust 描画パイプラインが `xaxis.range`/`yaxis.range` と Plotly `shapes` を生成。
fixture 4 本・Rust ユニットテスト 36 件（既存 + 8 件新規）がすべて通過。

### macro の `quote` 内 function 定義 3 件 ✅ (Issues #8064 #8065 #8066)

ユーザマクロの bare `quote` 内で関数を定義する系の関連 3 ギャップを一括修正。

- **#8064**: 非 esc 関数名を **module-private gensym** にして hygienic 化
  (`gg(x)=x+1` を定義したマクロの後 `gg` は不可視 = upstream `UndefVarError`)。
  内部ヘルパ名が同じ 2 マクロのメソッドテーブル共有も解消。適用は「本体末尾が
  直接 `quote` を返すマクロ」のみにゲート (`@qq` 風の esc 経由マクロは除外)。
- **#8065**: `local f(x) = ...` 短縮形 (および `where` 変種) を quote 内・通常関数
  本体の両方で正しく lower (パーサ `parse_var_declaration_item` + lowering
  `lower_local_statement`)。
- **#8066**: esc'd / 補間された関数名の call target を可視のまま定義可能に。短縮形
  `$(esc(:f))(x)=...` は quote 変換の `paren_dollar_payload` で、完全形
  `function $(esc(:f))(x) ... end` はパーサ `parse_function_name` で対応。

fixtures: `macros/quote_funcdef_hygiene_8064.jl`, `quote_local_funcdef_8065.jl`,
`quote_esc_funcdef_8066.jl`。詳細は STATUS.md 同日エントリ参照。

### パラメトリック struct の `DataType` 値の動的コンストラクタ呼び ✅ (Issue #8070)

`t = A.Pt; t(1.0, 2.0)`（再エクスポート `const Pt = A.Pt` + `using .B` 経由の
`t = Pt; t(1.0,2.0)` も含む）が `Function 'A.Pt' not found` で失敗していたのを修正。
`call_function_variable.rs` の `try_construct_default_datatype` は `struct_defs` の具体行
しか見ず、`parametric_structs` にしか登録されないパラメトリック base を構築できなかった。
新規 `try_construct_parametric_datatype` をフォールバックとして追加し、引数値型から
コンパイル時と同じ `infer_parametric_type_args` で型パラメータを推論して
`A.Pt{Float64}` を構築する。名前解決はコンパイラの `resolve_parametric_struct_name` を
ミラー（`resolve_runtime_parametric_def`）。動的呼びは静的 `A.Pt(...)` と一致。新規
intrinsic/BuiltinOp なし。1型/2型パラメータ・struct 値 field まで上流一致。#8058 の
パラメトリック版。fixture `modules/dynamic_parametric_ctor_8070.jl`。

### DataType を値として扱う 3 ギャップ修正 ✅ (Issues #7940 #7941 #7935 #7934)

型 (`DataType`) を**値として**渡す系のコンパイル時ギャップ 3 件を修正 (+ 既存解決済み 1 件の回帰 fixture)。

- **#7940**: generic `DataType` を Dict キーに使う (`D[T]`, `D[T] = v`、`T` は `where`
  パラメータ) と `Cannot convert DataType to I64` でコンパイル失敗していた問題を修正。
  getindex は `DataType` 添字を I64 強制せず実行時 Dict ディスパッチに委ね、setindex は
  `setindex!`/`DictSet` 経路へ振って値型 (`d[T]=1` が `1.0` 化するバグ) を保持。
  `compile/expr/builtin_array.rs`, `compile/stmt.rs`。fixture
  `dict/dict_generic_datatype_keys_7940.jl` (julia/sjulia 11/11)。
- **#7941**: guarded な generic フィールド代入 (`if !isdefined(G,:__attrs); G.__attrs=…`、
  `G::T where T`) がコンパイル時に `Unknown field` で拒否されていた問題を修正。受け手が
  `Any` (generic) の場合は実行時 `SetFieldByName` に遅延。具象 struct の検証は据え置き。
  `compile/stmt.rs`。fixture `struct/struct_guarded_generic_field_assign_7941.jl` (3/3)。
- **#7935**: inner constructor の `new{elem_type(R), …}` 型パラメータを実行時に評価して
  **計算された具象パラメトリック型**を構築 (従来は `{Any}` に潰れていた)。lowering で型引数を
  ブラケット対応分割し計算式を `TypeExpr::RuntimeExpr` 化、`type_expr_is_resolvable` を更新して
  既存 `NewDynamicParametricStruct` 経路に接続。`lowering/expr/call.rs`,
  `compile/expr/collection.rs`。`typeof(r).parameters == (MyElem, MyElem)`。fixture
  `struct/struct_dynamic_new_type_params_7935.jl` (6/6)。
- **#7934**: `Dict{Type, Dict{Symbol, Any}}()` は現行 main で動作済み。回帰 fixture
  `dict/dict_typed_datatype_param_ctor_7934.jl` を追加 (3/3)。

派生して発見した既存の別バグを起票: module-global const Dict の関数内 getindex (#8068)、
inner-ctor 本体での module-private 関数呼び出しの解決スコープ (#8069 — 本日修正)。
### inner constructor 本体の名前を定義モジュール scope で解決 ✅ (Issue #8069)

inner constructor 本体が**トップレベルの import scope** でコンパイルされ、定義モジュールの
module-private 関数 / const / 型が見えず、caller が `using .M` していないと
`M.UR(...)` が `function 'helper' is not imported` で失敗していた問題を修正。upstream
Julia はメソッド本体の名前を常に定義モジュールで解決する。`compile_inner_constructors` を
通常の module メソッド (`compile_functions`) と同じ module-scope セットアップに揃えた:
`InnerCtorInfo` に定義モジュール path を保持し、`module_functions`/`module_imports_map`/
`module_usings_map` から import 集合と `resolved_usings` を構築、`current_module_path`/
`current_module_imports` を設定 (`compile/pipeline_ctx.rs`)。module-private 関数呼び出し
(`helper()` / `new(mk())`)・`const` (`new(K)`)・通常長の module-private 型の値参照が解決。
fixture `modules/inner_ctor_body_module_scope_8069.jl` (5/5)。派生バグを follow-up に分離:
2 文字以下の短い module-private 型名 (`E`) を module メソッド本体で値参照すると `TypeVar`
に誤解決 (inner-ctor 固有でなく通常 module 関数でも発生、`name.len() <= 2` 由来)。
### quote 内の補間型注釈付き短縮関数定義を lower できるよう修正 ✅ (Issue #7933)

`quote ... g(x::$T) = ... end` のように短縮形関数定義の引数型を補間した形が lower 時に
`UnsupportedFeature { MacroCall, "macro expansion returned unsupported assignment expression
target" }` で失敗していた問題を修正 (AbstractAlgebra `@attributes` のメソッド生成形)。macro
結果→IR 変換の `Expr(:(=), ...)` 処理に `:call`/`:where` を LHS に持つ関数定義ケースを追加し、
`Expr(:function, ...)` と同じ `function_stmt_from_values` へ委譲。block tail 判定でも短縮関数定義の
`=` を statement 経路必須とし、`function_stmt_from_values` は `constructor_signature_from_value`
を再利用して `where` 型パラメータを保持する。補間された型は実型として保持されディスパッチに反映
(補間違いで別メソッド選択 / 不一致型は MethodError)。`subset_julia_vm/src/lowering/macro_runtime.rs`、
fixture `macros/macro_interp_typed_param_7933.jl`。補間とは独立した別ギャップを follow-up
issue に分離: macro 定義関数名の top-level leak (#8064 bug)、`local f(x)=...` 短縮形 (#8065)、
esc した関数名 `$(esc(:f))(x)=...` (#8066)。
### Workaround retirement audit ✅ (Epic #7812 / Milestone 45: #7814–#7818)

`docs/vm/WORKAROUNDS.md` の監査と stale workaround の撤去。各候補を実 VM と upstream
julia で probe し、ギャップが修正済みのものだけを上流形へ戻し、対象 fixture + フル
`cargo nextest run --release` (4012 pass) でゲート。

- **撤去**: `isdefined(::Module,::Symbol)` 分割 (#5005)、2D SubArray reshape を
  generic `_reshapedarray_checked` へ (#5611)、`_new_dict_kv` の匿名 `::Type{K},::Type{V}`
  復元 (#6661)、MacroTools `gatherwheres` の `(params1...,params2...)` splat (#7741)、
  `rsplit` の `#NEW-VM-BUG` kwarg-default-Bool orphan、LinearAlgebra の stale fill/zeros。
- **doc 整理**: 既に消えていた W-03 (#2425) と上流形済みの Categorical (#7266) を撤去、
  撤去分を Resolved へ移動、未文書化 issue-tagged workaround を W-36〜38 として inventory 化。
- **維持 (gap 実在を probe で確認)**: MacroTools #7628/#7541/#7711/#7630/#7637/#7636/#7634、
  tuple×struct-field 等価 #7803、VM #7535/#7538(upstream 互換の runtime defer / typed
  array-slot store 未実装)。教訓: top-level quote / tuple-eq が通っても
  `@match`/`prewalk`/struct-field 文脈では失敗しうる。
### 無名/アロー関数の省略可能な位置引数デフォルトを束縛 ✅ (Issue #8047)

`(x, d=2) -> (x, d)` などアロー/無名関数の省略可能な位置引数デフォルトを実装。アロー
lowering 経路がデフォルト抽出・縮約アリティ stub 生成を行っていなかったため、`a(1)` が
`UndefVarError: d`、`a(1, 9)` が `NoMethodFound` になっていた。param 収集に `Assignment`
（デフォルト付き引数）と `TypedExpression`（型付き引数）を追加し、全アロー経路で
`generate_default_arg_stubs` による stub を emit するよう修正。`lower_arrow_function_with_name`
/ `lower_lambda_assignment` は `Vec<Function>` を返す。fixture
`functions/anonymous_default_arg_8047.jl`。詳細は STATUS.md 参照。

### `using` で再エクスポートした `const` 型エイリアスをコンストラクタとして呼べるよう修正 ✅ (Issue #8049)

`module B; const Foo = A.Foo; export Foo; end` を `using .B` した呼び出し側で、`Foo` が値としては
読めるのに **`Foo()` が `UndefVarError: Foo not defined`** で失敗していた問題を修正。値参照経路は
using-import/const-エイリアスを解決して型を `PushDataType` するのに対し、関数呼び出しの名前解決は
`const` が登録した **`Any` 型グローバル `Foo`** を callable-variable と誤認して `LoadAny("Foo")`
(実在は `B.Foo` のみ) を出していた。`try_compile_callable_variable_call` の `Any` グローバル分岐で、
名前が可視な型 (struct / parametric struct / 型エイリアス) なら `Ok(None)` で `compile_call` の
コンストラクタ解決チェーンへ委譲するよう変更し、値経路 (`t = Foo; t()`) と同一の underlying 型へ
解決させる。non-parametric/parametric struct エイリアス・selective `using .B: Foo`・素の callable
グローバル (回帰なし) を fixture `modules/const_alias_ctor_via_using_8049.jl` で upstream julia 1.12
と照合 (8 assertions)。

### 再エクスポート(import)したバインディングへの修飾アクセス `Module.X` を解決 ✅ (Issue #8053)

`module Facade; import ..Defn: T, g; end` の selective import で再エクスポートした名前へ修飾アクセス
(`t isa Facade.T`、`Facade.g(t)`) するとコンパイルエラーになっていた問題を修正。collect 時に selective
import を解決し `SharedCompileContext.module_imported_bindings`(`"Facade.T" -> "Defn.T"`)を構築し
(`resolve_using_module_name` を再利用)、`compile_module_function_ref` / `compile_module_call` は通常
解決失敗時に再エクスポート鎖(連鎖再エクスポート・循環検出付き)を辿ってソース修飾名で同じ型/関数解決を
再実行する。型位置・呼び出し位置の両方に対応。非選択 `using M` は IR が `import M` と区別不能のため対象外。
fixture `modules/reexported_qualified_access_8053.jl` を upstream julia 1.12 と照合 (7 assertions)。

### `using` 取り込みの const 型エイリアスの局所変数経由 動的呼び出しを解決 ✅ (Issue #8058)

`const Bar = A.Bar`(デフォルトフィールドコンストラクタのみ)を `using .B` で取り込み `t = Bar; t(7)` と
動的に呼ぶと `Function 'Bar' not found` になっていた問題を修正。`t = Bar` は短縮名の DataType 値を保持
するため、ランタイムの `try_construct_default_datatype` を、完全一致に失敗した場合に最終 `.` セグメントが
**一意**に一致する struct へフォールバックさせ(曖昧時はフォールバックしない)、実体 `A.Bar` のフィールド
コンストラクタへ解決させた。デフォルトコンストラクタ struct・修飾エイリアス値経由・inner constructor を
fixture `modules/dynamic_const_alias_ctor_8058.jl` で upstream julia 1.12 と照合 (5 assertions)。
parametric struct の DataType 値からの動的構築は Issue #8070 として起票。

### 他モジュールの関数へメソッドを追加できるよう修正 ✅ (Issue #8052)

`function Inner.f(x::Float64) ... end` 等の他モジュール関数拡張が lowering で `missing function name` に
なっていた問題を修正。シグネチャの `FieldExpression` callee を Base 以外でも受理し、非 Base は修飾名
`Inner.f` を採用(`module_extension_function_name` ヘルパに統一、full/short/where 形を網羅)。
`build_method_tables` は修飾名ユーザ関数を `Inner.f` と裸の `f` の両表へ登録するので、`Inner.f(2.0)` と
`using .Inner` 由来の無修飾 `f(2.0)` の両方が全メソッドへディスパッチ。`import Inner: f; function f(...)`
変種もトップレベル selective import のソースを記録して `Inner.f` 表へ join し shadow ではなく拡張になる。
fixture `modules/cross_module_method_extension_8052.jl` を upstream julia 1.12 と照合 (10 assertions)。

### builtin `sqrt` が他モジュールの `sqrt` に誤ディスパッチして無限再帰する問題を修正 ✅ (Issue #8042)

bundled package (NaNMath/Optim) のように **別モジュールが新規 `sqrt(x) = … Base.sqrt(float(x))`**
を定義すると、その `NaNMath.sqrt` がグローバルな bare `"sqrt"` メソッドテーブルに混入し、
**ヘルパ呼び出し連鎖で生成された `Any` 型の `Float64`** に対する bare `sqrt` / 明示 `Base.sqrt`
呼び出しが本来の builtin ではなく `NaNMath.sqrt(::Any)` にディスパッチ。その本体
`Base.sqrt(float(x))` が再び自分自身へ解決して **無限再帰 (Stack overflow)** していた
(リテラル `Float64` は具象型のため builtin に直行し問題なし。別モジュールをロードした
コンテキスト依存で #5966 の promote-fallback 再帰トラップに類似)。`compile_sqrt` の候補収集
(`sqrt_runtime_candidates` と struct 経路) で、単一セグメント `"<Module>.sqrt"` (Module≠Base)
テーブルに属するメソッドを **foreign として除外** し、`Any` 経路は generic dispatch へ
フォールスルーせず常に builtin 裏付きの `CallTypedDispatchOrBuiltin` を発行するよう変更。
`function Base.sqrt(...)` で定義された真の拡張 (`"Symbolics.Base.sqrt"` 等の多段キー) は
single-segment ガードで除外されず温存。foreign メソッド自身の内部 `Base.sqrt(...)` も
再帰しないよう、コンパイル中のモジュール自身の定義も除外する。これに伴い bundled Optim の
`_sqrt` Newton 反復回避策 (W-34) を撤去し `_nmobjective` を builtin `sqrt` に戻した。
fixture `module/module_base_sqrt_foreign_shadow_8042.jl` で upstream julia 1.12 と照合、
`optim_nelder_mead_mvp` も builtin `sqrt` で収束を維持。
### `SciMLBase.solve(prob, Tsit5())` 修飾呼び出しの alg dispatch 回帰を修正 ✅ (PR #8050 review, Issue #7996)

PR #8050（Issue #7996）の alg dispatch で、`OrdinaryDiffEq` が独自の
`OrdinaryDiffEq.solve` を定義していたため、修飾呼び出し `SciMLBase.solve(prob,
Tsit5())` が `Tsit5` メソッドを見つけられず汎用エラーフォールバック
（`Algorithm Tsit5 is not supported yet`）に落ちる回帰を Codex レビューが指摘。
`Tsit5` 型と `solve(::ODEProblem, ::Tsit5)` メソッドを `SciMLBase` 側へ移し
（`_tsit5_solve` ステッパと同居）、`OrdinaryDiffEq` は `Tsit5` を再エクスポートして
`solve` を `SciMLBase.solve` への単純 forwarder にした。これで `solve(prob, Tsit5())`
と `SciMLBase.solve(prob, Tsit5())` が単一メソッドテーブルに収束し、両方とも正しく
dispatch する。fixture `packages/ordinarydiffeq_alg_dispatch_7996.jl` に修飾パスの
回帰テストを追加、`ordinarydiffeq_skeleton_7362.jl` は `OrdinaryDiffEq.Tsit5` →
`SciMLBase.Tsit5` に更新。発見した sjulia ギャップ（別モジュールの関数を拡張不可
#8052、再エクスポート binding への修飾アクセス不可 #8053）を起票。回避策 W-35。

### ブロック形式関数の素の識別子デフォルト引数が脱落する問題を修正 ✅ (Issue #8017)

`function f(x, l=nothing) ... end` のブロック形式で、デフォルト値が素の識別子 /
グローバル参照 (`nothing`, `missing`, 定数, 型名) のオプショナル引数が黙って脱落し、
デフォルト引数スタブが生成されず減アリティ呼び出しが `No method matching …` で
失敗していたのを修正。lowering の `extract_default_from_parameter_node` が `=` の後ろの
デフォルト値を「型注釈の識別子」と誤認して読み飛ばしていたため、`=` を含む `Parameter`
ノードでは末尾の名前付き子 (`[name, 型?, デフォルト?]` の最後) をデフォルト値として採る
よう変更。リテラルデフォルト・短形式・モジュール内外は回帰なし。fixture
`functions/block_form_identifier_default_8017.jl` で upstream julia 1.12 と照合。

### Optim.jl サポート MVP ✅ (Issue #7432, milestone #39)

Optim.jl の上流適応 pure-Julia MVP を bundled package 化
(`subset_julia_vm/packages/Optim/`)。実装済み: GoldenSection / Brent 単変数最適化
（整数境界 promotion、`x_lower>x_upper` エラー）、結果クエリ API
（`minimizer` / `minimum` / `iterations` / `converged` / `f_calls` / `g_calls` /
`x_converged` / `f_converged` / `g_converged` / `lower_bound` / `upper_bound`）、
`Options`、`maximize` ラッパ、NelderMead（微分なし多変数）、ユーザ勾配
GradientDescent（`LineSearches.BackTracking` Armijo line search）。依存パッケージ
（`NLSolversBase` / `LineSearches` 機能実装、`ADTypes` / `NaNMath` / `EnumX` /
`FillArrays` / `PositiveFactorizations` stub）を bundle (Issue #7478)。fixture
`tests/fixtures/optim/`（6 本）は upstream Optim.jl 1.12.6 と sjulia で同一にパスし、
GoldenSection/Brent/NelderMead は反復数・f呼数まで上流と完全一致。子 Issue
#7477–#7483 を解決。詳細は [OPTIM.md](./OPTIM.md)。実装中発見の builtin `sqrt`
再帰バグ #8042 は Newton `_sqrt` で回避 (W-34)。

### Symbolics 線形代数の dispatch 回避策を撤去（#8019/#8025 修正後）✅

#8019（bare `{<:Num}` 束縛の不マッチ）と #8025（`AbstractMatrix{<:Num}` の
specificity）が別途修正されたため、bundled Symbolics の `linear_algebra.jl` から
回避策を撤去してクリーンな上流相当の形に復元: matmul/det/inv のシグネチャを
修飾 `AbstractMatrix{<:Symbolics.Num}` → bare `AbstractMatrix{<:Num}`（W-32 解消）、
stdlib LinearAlgebra の `inv` を untyped 回避形から本来の typed `inv(::AbstractMatrix)`
に戻し、Symbolics 側 `inv(::AbstractMatrix{<:Num})` が specificity で勝つことを確認
（W-33 解消）。全 symbolics fixture + 数値 inv/det/`\` 回帰なし。WORKAROUNDS W-32/W-33
を Resolved へ移動。

### `Value::StaticArrayInline` Phase 3 完了 ✅ (Issue #7964)

N≤4 の `<:Real` StaticArray を `Copy` な 40-byte `StaticArrayInlineData` に格納する Phase 3 を完成。
修正一覧: `get_value_julia_type`（WHERE 句バインド）、`get_type_name`（dispatch 型名）、
`isa` 演算子（`builtins_types.rs` の struct-name 経路）、`GetField(0)`/`GetFieldByName("data")`
（`.data` field を実 TupleValue に実体化）、`IndexLoad`（1D/2D インデックス、行優先公式）、
`inline_matvec`/`inline_matmat`（行優先 `data[i*k+j]` に修正）、SMatrix{M,1} は
`is_vector()` 混同回避のため StaticArray パスへ。static_arrays fixture 8 件・全 163
fixture・3013 unit tests 通過、Clippy ゼロ警告。

### `floor(T, x)` 等を CallTypedDispatch から CallBuiltin へコンパイル時降格 ✅ (PR #8043)

`floor(Int, x[1])` / `ceil` / `round` / `trunc` の型変換形式が、ループ内で第 2 引数が
Any 型（StaticArrayInline 要素など）のとき `has_datatype_arg && has_typeof_methods` 分岐で
`CallTypedDispatch` を生成していた問題を修正。`CallTypedDispatch` はキャッシュなしの完全
dispatch 解決（`resolve_typed_runtime_core_candidates_with_subtype_fallback`）を毎イテレーション
実行するため、ループホットパスで大きなオーバーヘッドとなっていた。
`compile_generic_dispatch_call` の `has_datatype_arg` 判定よりも前に、丸め関数 + 既知型名の
組み合わせを検出して `compile_builtin_call` へ短絡するガードを追加。
生成バイトコードが `PushDataType + CallTypedDispatch` から `FloorF64 + DynamicToI64`（2 命令）
に変わり、`vm_staticarrays_matvec_benchmark`（IFS カオスゲームカーネル）が 23.9 ms → 3.3 ms
（約 86% 削減、～7× 高速化）。163 fixture 全通過・Clippy ゼロ警告。

### iOS サンプル：StaticArrays 追加・IFS フラクタル更新 ✅ (PR #8044)

- **新規サンプル** `intermediate/static_arrays.jl`: `SVector`/`SMatrix` の基本操作（算術・
  インデックス）と Barnsley fern カオスゲームを `SMatrix * SVector` ゼロアロケーションループで
  示す。`samples.json` と `CodeSamples+Intermediate.swift` に追加。
- **更新** `ifs_fractals.jl`: Issue #7949 で導入した `Affine` 構造体スカラー手展開ワークアラウンド
  （INTERIM コメント付き）を削除し、`SMatrix`/`SVector` ペアを用いたクリーンな実装に置き換え。
  `@SMatrix`/`@SVector` マクロをトップレベルで呼ぶことでリテラル展開を `@manipulate` 外で完結
  させている（Issue #7733）。カオスゲームの各ステップが `inline_matvec` Rust ファストパスを
  通るため、`Affine` 版と同等以上の速度でコードは大幅にシンプルになった。

### SciMLBase.solve alg dispatch の実装 ✅ (Issue #7996)

`SciMLBase._tsit5_solve` としてソルバーロジックを分離し、`OrdinaryDiffEq.jl` に
`solve(prob::ODEProblem, alg::Tsit5; ...)` と汎用エラー fallback を追加。未サポート
アルゴリズムを渡すと明示的エラー。fixture `packages/ordinarydiffeq_alg_dispatch_7996.jl`
で Tsit5 dispatch と ErrorException throw を確認。副産物として Issue #8049 を起票
（`const TypeAlias` 経由のコンストラクタ呼び出し失敗）。

### ODE `sol.u` エイリアスバグ修正 + 振り子アニメーションサンプル追加 ✅ (Issue #8094 / W-43)

PR #8094 が `OrdinaryDiffEq.jl` に追加した `SciMLBase._tsit5_solve` オーバーライドで、
`SciMLBase._copy_state(u)` の修飾呼び出しがクロスモジュールディスパッチバグ
(Issue #8104, W-43) により総称メソッド (`u` をそのまま返す) にフォールバックし、
`sol.u` の全エントリが最終状態にエイリアスしていた不具合を修正。

- **根因**: sjulia は `ModName.f(::AbstractVector)` を別モジュールコンテキストから呼んだとき、
  型注釈付きメソッドを選ばず総称 `f(u)` にフォールバックする (Issue #8104)。
- **修正 (W-43)**: `SciMLBase._copy_state(u)` 呼び出しをすべて
  `ismutable(u) ? copy(u) : u` に置き換え。可変配列 (`Vector`) は `copy`、
  不変状態 (`SVector`、スカラー) はエイリアス安全。
- **副産物**: `packages/OrdinaryDiffEq/src/OrdinaryDiffEq.jl` の中間デバッグ `println`
  をすべて削除し、in-place バッファ最適化パスを完全に復元。
- **新サンプル**: iOS アプリ向け振り子アニメーションサンプル
  (`SubsetJuliaVMApp/.../Samples/intermediate/ordinarydiffeq_pendulum_animation.jl`) を追加。
  振り子 ODE を Tsit5 で解き、`@gif` で揺れる棒の軌跡アニメーションを生成。
  `samples.json` と `CodeSamples+Intermediate.swift` にエントリを追加。
- **検証**: `packages::` + `static_arrays::` カテゴリ全 pass。WORKAROUNDS.md W-43 エントリ追加、
  `check_workarounds_documented.sh` / `check_workarounds_sync.sh` 両 OK。

## 最新対応 (2026-06-26)

### 空白区切り `for ... for` 内包表記が 1 次元 Vector になる ✅ (Issue #8014)

`[expr for x in A for y in B]` (空白区切りの複数 `for` 句 = flatten 形式) が誤って
2 次元 `Matrix` を生成していたのを修正し、`Iterators.flatten` 意味論どおり 1 次元
`Vector` を返すようにした。lowering でカンマ形式 (`for x in A, y in B` = カルテシアン /
多次元) と空白区切り flatten 形式を CST から区別 (`Expr::MultiComprehension.flatten`)。
flatten 用コンパイル経路 (`compile_flatten_comprehension` / `emit_flatten_levels`) は
反復子を外側→内側にネストし、内側反復子を外側ステップごとに再評価して依存レンジ
(`for i in 1:3 for j in 1:i`) に対応、reshape を省略。型推論 (`infer/mod.rs`,
`julia_type.rs`) も flatten 時は rank 1 (`Vector`) を返す。カンマ形式は回帰なし。
fixture `comprehension/flatten_vs_cartesian_8014.jl` で flatten / カンマ / 3 句 / 依存 /
混在 / フィルタ / 型付き / 空レンジを upstream julia 1.12 と照合。

### Symbolics 記号式の上流同形表示（項順序・負係数）✅ (Issue #7894, Epic #7888)

`simplify` 出力の表示を上流 Symbolics と一致させた（Epic #7888 Phase 2）。
`simplify.jl` で和の項を上流同形に整序（昇順 total degree → 同次数内は早い変数の
指数降順なので `x^2` が `x*y` より先、定数は先頭）し、`show.jl` で負係数を減算として
描画（`a + (-1)*b` → `a - b`、`-1*(x*y)` → `-x*y`）。結果 `det([x y; x x])` の文字列が
上流と同じ `x^2 - x*y` になる（fixture `packages_symbolics_canonical_form`、上流
julia 1.12.6 とバイト一致）。スコープは det/inv が生む単項式の和（Epic の最小ライン）。
完全な多項式正準化・`2x` 係数表記・生 `x*x→x^2` 折り畳みは範囲外（後者は #7893 の
raw 表示 fixture と整合のため据え置き）。

### Symbolics 記号行列の `det` / `inv` / `\` ✅ (Issue #7892, Epic #7888)

記号行列（`AbstractMatrix{<:Symbolics.Num}`）の行列式・逆行列・線形ソルブを
bundled Symbolics パッケージ `linear_algebra.jl` にラプラス（余因子）展開で実装。
数値 `det`/`inv` builtin を一切踏まない。`det` は untyped generic `det(A)` を
parametric override（`simplify` で `det([x y; x y])` は構造的に 0）。`inv` は余因子/
随伴 ÷ det。`\` はメソッド追加不要で、stdlib の `\(A,b)=inv(A)*b` が記号 inv と
#7889 記号 matmul 経由で記号解を返す。minor は slicing せず `similar`+要素コピーで
構築（slicing は記号要素で "expected I64, got StructRef" を誘発しうるため）。値の
正しさは `substitute` で具体点検証（fixture `packages_symbolics_linear_algebra`、
3×3 det=70・inv*A=I・A*(A\b)=b を上流 julia とパリティ）。実装中に dispatch の
specificity ギャップ #8025 を発見（`AbstractMatrix{<:Num}` が bare `AbstractMatrix`
より具体的と判定されない）→ stdlib `inv` を untyped 化して override 点を露出（W-33）。
### bare な imported type-alias 境界 `{<:Num}` が `Matrix{Symbolics.Num}` に一致 ✅ (Issue #8019)

`using Symbolics` の別名 `Num` (`Num === Symbolics.Num`) を使った境界
`f(::AbstractMatrix{<:Num})` が `Matrix{Symbolics.Num}` 実引数に一致せず MethodError と
なる一方、qualified な `{<:Symbolics.Num}` は一致していた bare↔qualified `Named`
正規化バグを修正。コア subtype エンジンの `(Named, Named)` アーム
(`subset_julia_vm/src/inference_core/type_core/subtype.rs`) に
`base_type_name(child) == base_type_name(parent)` の module 接頭辞除去後 reflexive 等価
を追加 (隣接する `(Struct, Named)` / `(Named, AbstractUser)` アームと同じ前例)。
これで `Symbolics.Num <: Num` の境界判定が真になり bare 表記でも一致する。回帰テスト
`packages_symbolics_dispatch_bare_alias_bound_8019` (競合メソッドのない単独メソッドで
一致のみを検証)。修正はマッチング (Named 正規化) に限定。Symbolics `linear_algebra.jl` の
qualified 境界 (暫定対処 W-32) は specificity ランキング (#8025) と絡むため本 PR では
据え置き、#8025 解決時に bare へ戻す。
### 即時適用される無名アロー `(x -> ...)(arg)` が関数内の制御フローを壊す ✅ (Issue #8018)

関数本体内で末尾以外に置かれた即時適用の無名アロー `(x -> body)(arg)` の lowering で、
持ち上げたラムダ呼び出しを `Stmt::Return` で包んでいたため `return` が外側関数フレームに
漏れ、`r = (x -> body)(arg)` がラムダ値を即座に return する / 後続使用時に
`Cannot convert Nothing to I64` でコンパイル失敗する不具合を修正。`lower_iife_as_nested`
(`subset_julia_vm/src/lowering/expr/call.rs`) の末尾文を `Stmt::Expr` に変更し、IIFE を
値を生成する通常呼び出し式に lowering するようにした。fixture
`closures_iife_arrow_control_flow_8018`。
### パラメータ付き `::AbstractMatrix{<:Num}` が裸の `::AbstractMatrix` より特定 ✅ (Issue #8025)

ユーザ struct 要素 (`MyNum` / `Symbolics.Num`) の `Matrix{T}` に対する
`::AbstractMatrix{<:T}` / `::Matrix{T}` が、裸の `::AbstractMatrix` に競り負けて
`"generic"` を選んでいた問題を修正。ディスパッチ用の `get_value_julia_type` が
配列ラッパ要素型をレジストリ非依存の `array_wrapper_julia_type()` で解決し、
`StructOf` 要素を `Any` に潰して `Matrix{MyNum}` を `Matrix{Any}` と見なしていたのが原因。
`typeof`/reflection 同様に struct_defs 参照の `array_wrapper_julia_type_resolved`
(Issue #7304) を使うよう `Value::Struct`/`Value::StructRef` 腕を統一。真にヘテロな
`Any[...]` 行列は `"generic"` のまま。fixture
`dispatch_parametric_matrix_specificity_8025`。

### isdefined(::Module, Symbol("@macro")) がマクロ束縛を参照する ✅ (Issue #7948)

関数形式 `isdefined(::Module, ::Symbol)` がマクロ束縛 (`Symbol("@alias")`) に対し
常に `false` を返していた reflection バグを修正。マクロは lowering 時に展開・消去され、
VM 実行時には通常のグローバル/関数レジストリに残らないため照合できなかった。
コンパイル時にモジュールごとの可視マクロ名集合を `CompiledProgram.macro_bindings`
(`module path -> {"@name", ...}`) に記録し、VM の `module_binding_is_defined` が
`@`-始まりの名前をこの表で照合する。所有モジュール / `using` 取り込み (export 済み)
/ トップレベル Main マクロを upstream Julia と一致させた。`isdefined(Main,
Symbol("@alias"))` と `isdefined(AbstractAlgebra, Symbol("@alias"))` がともに `true`。
fixture `module_isdefined_macro_bindings_7948`。

### Symbolics 記号行列積 `A*v` / `A*B` ✅ (Issue #7889, Epic #7888)

記号要素配列（`Matrix{Symbolics.Num}`）の行列積を Pure Julia で実装。VM の数値
`matmul` builtin は Float64/Complex 前提で `Num` を拒否する（`matmul: expected
Complex struct, got Symbolics.Num`）ため、bundled Symbolics パッケージに
`linear_algebra.jl` を新設し、要素型ジェネリックな
`Base.:*(::AbstractMatrix{<:Symbolics.Num}, ::AbstractVector)` /
`(::AbstractMatrix{<:Symbolics.Num}, ::AbstractMatrix)` を定義。最初の積で結果
eltype を確定し `similar` で確保するループ（上流 `promote_op`/`matprod` 相当）。
これらは数値経路より具体的なので記号行列で dispatch 勝ち、純数値配列は従来通り
高速な `Instr::MatMul` を使う（Rust ディスパッチ変更なし・性能退行なし）。fixture
`packages_symbolics_matmul`（`substitute` で具体値検証、上流 julia と同一 pass）。
実装中に発見した dispatch ギャップ（bare `{<:Num}` 束縛が `Matrix{Symbolics.Num}`
にマッチしない）を #8019 として起票し、修飾名 `Symbolics.Num` で回避（WORKAROUNDS W-32）。
### SubArray (view) の copy/similar/zero と element-wise 表示 ✅ (Issue #8003)

`similar`/`copy`/`zero` は `Array`/`Memory` 用しか定義がなく、`SubArray`
(=`view` の結果) に対しては Rust builtin の `similar` が
「similar requires an array or memory argument」で失敗し、`copy`(内部で `similar`)
も同様に失敗、`zero` も convert エラーになっていた。さらに `SubArray` は専用 `show`
が無く生の struct を dump していた。`AbstractArray` の契約を `SubArray` に配線:
`similar(::SubArray[, ::Type{S}][, dims])` は eltype/shape に合わせた新規 `Array` を
返し、`copy` は要素を materialise した独立 `Vector`、`zero` は `zeros(eltype, size)`、
`show(io, ::SubArray) = show(io, collect(v))` で `[1.0, 2.0, 3.0]` / `[a b; c d]` の
element-wise 表示にする。`subset_julia_vm/src/julia/base/subarray.jl` に追加。fixture
`subarray_copy_similar_zero_display_8003`。`#7986` (ODE stepper の汎用
`AbstractVector` 状態) のブロッカ解消。
### 整数値パラメータ付き抽象型のディスパッチ修正 ✅ (Issue #7960)

整数の**値パラメータ**を持つ抽象スーパータイプ (`abstract type AbsM{M,N,T} end`)
を、その具象サブタイプ (`ConM{2,2,Float64} <: AbsM{M,N,T}`) 経由で呼んだとき、
`h(x::AbsM{2,2,T})` と `h(x::AbsM{3,3,T})` の特殊化を選び分けられず、常に最後に定義
された方を選んでいた。原因は `resolve_abstract_type` がパラメータをすべて捨てて素の
族名 (`AbsM{2,2,T}` → `AbstractUser("AbsM")`) に落としていたため、全特殊化が同一
シグネチャに潰れていたこと。値パラメータを持つ抽象注釈はパラメータを保持し、ディスパッチ
時に具象サブタイプを宣言スーパータイプのインスタンス化 (`ConM{2,2,Float64}` →
`AbsM{2,2,Float64}`) へ射影してから不変パラメータを比較・束縛するよう修正
(`abstract_value_param_match` + `registered_instantiated_struct_supertype_in`)。
特殊化が generic (`::AbsM`) を定義順に依らず上回るよう specificity も補正。fixture
`dispatch_abstract_multi_value_param_dispatch_7960` が `h(ConM{2,2})=spec-2x2` /
`h(ConM{3,3})=spec-3x3` / `h(ConM{4,4})=generic` と単一値パラメータ・定義順非依存を
検証。残: 抽象スーパータイプ値パラメータからの実行時 `where` 変数束縛
(`f(x::AbsM{2,2,T}) where T = T` は型のみパラメータの場合と同様 pre-existing で未対応)。

### OrdinaryDiffEq callbacks & events ✅ (Issue #7983)

#7865 から昇格した子 Issue。bundled `SciMLBase`/`OrdinaryDiffEq` に
`DiscreteCallback` / `ContinuousCallback`（bisection root-find）/ `CallbackSet` を
追加し、`solve(prob, alg; callback=...)` の fixed-step RK4 経路で event 検出する。
fixture `packages_ordinarydiffeq_callbacks_7983` がバウンシングボール（初回バウンス
t≈0.4515s・床貫通なし）、毎ステップ DiscreteCallback、CallbackSet 結合を検証。
`VectorContinuousCallback` / adaptive 経路 / `save_positions` は #7983 残。発見ギャップ:
無名関数のインデックス代入本体 #8007、複数行フィルタ内包表記 #8008。
### bare `Module` の `isa` 短縮名一致バグを修正 ✅ (Issue #7963)

`Box() isa Module`（bare `Module` = `Base.Module`）がモジュールローカル抽象型
`TypeOwner.Module`（短縮名 `Module`）に短縮名ファミリ一致してしまい、upstream の
`false` に対し `true` を返していたのを修正。`vm/builtins_types.rs` の `isa` で、対象が
接頭辞なしかつ `JuliaType::from_name` で builtin concrete DataType に解決する場合は
短縮名/抽象型インデックス一致を抑止し `type_values_subtype` にフォールバック。
回帰 fixture `module/module_bare_module_isa_shortname_7963.jl`。
### 関数値の `===`（object identity / egal）✅ (Issue #7993)

ジェネリック関数値 (`Value::Function`) は `===` の一致判定 match に専用 arm を持たず
`_ => false` に落ちていたため、`ff === ff` すら `false` を返し、struct フィールドに
格納して読み戻すと identity が失われていた（`Box(ff).f === ff` も `false`）。関数は
Julia ではシングルトンなので、`builtins_equality.rs` の `Egal` に
`(Value::Function(a), Value::Function(b)) => a.name == b.name` arm を追加。識別子は
(モジュール修飾されうる) 関数名で、`candidate_indices` は dispatch ヒントにすぎず
（同一関数が `PushFunction` 経由で `None`・`PushResolvedFunction` 経由で `Some` に
なりうる）identity には関与させない。`!==` は `!(a === b)` に lowering されるため自動的に
追従。fixture `functions_function_egal_identity_7993` が `ff===ff` / struct フィールド
往復 / `sin===sin` / 別関数は非一致 を検証。

### OrdinaryDiffEq SecondOrderODEProblem + symplectic 積分 ✅ (Issue #7985)

#7865 から昇格した子 Issue。bundled `SciMLBase` に `SecondOrderODEProblem`（加速度
RHS `f(du, u, p, t)` / in-place `f(ddu, du, u, p, t)`）と velocity-Verlet symplectic
積分器を追加し、`OrdinaryDiffEq` から `VelocityVerlet` を export。保存状態は
`[du...; u...]`（upstream `[v; u]` 順）。fixture
`packages_ordinarydiffeq_secondorder_7985` が調和振動子の解析解一致とエネルギー有界を
検証。`ArrayPartition` / 高次 symplectic / refined examples は #7985 残。

### OrdinaryDiffEq dense output / 連続補間 `sol(t)` ✅ (Issue #7982)

#7865 から昇格した子 Issue。bundled `SciMLBase` の `ODESolution` を callable に
し、`sol(t)` / `sol(t; idxs=...)` / `sol(ts)` で保存グリッド外の状態を**線形補間**で
返せるようにした（scalar/vector 両対応、`tspan` 外は端点クランプ）。Tsit5 4 次
dense interpolant は #7982 Phase B 残。fixture
`packages_ordinarydiffeq_dense_output_7982` で検証。

### OrdinaryDiffEq SciML integrator interface subset ✅ (Issue #7981)

#7865 から昇格した子 Issue。bundled `SciMLBase` に integrator interface の最小
subset を pure Julia で追加: `init` / `step!` / `solve!` / `reinit!` / `remake` /
`successful_retcode`(既存 adaptive Tsit5 stepper を再利用)。`step!(integ)` は次の
出力点まで前進し、`solve!(init(prob, alg; ...))` は `solve(prob, alg; ...)` の
`t`/`u` を再現する。`OrdinaryDiffEq` から re-export。fixture
`packages_ordinarydiffeq_integrator_7981` で scalar/vector の再現・`remake`・
`reinit!`・`successful_retcode` を検証。残: `step!(integ, dt, stop_at_tdt)` / 任意
`tstops` / `ReturnCode` enum。実装中に sjulia バグ #7992(オプション位置引数+kwarg の
縮約アリティ)/ #7993(関数の `===` 自己同一性)を起票し bundled package 側で回避。
### コンストラクタ署名パーサの leftover debug 出力を削除 ✅ (Issue #7974)

`subset_julia_vm/src/lowering/struct_.rs` の `parse_ctor_signature` に残っていた
`#[cfg(debug_assertions)]` の `writeln!(stderr(), "parse_ctor_signature: ...")` /
`"  ArgumentList children: ..."` の2ブロックを削除。debug/dev-fast ビルドで
コンストラクタ署名を parse するたびに stderr を汚染していた（#7955 / AbstractAlgebra
phase2 作業の残骸）。release ビルドは元から無影響（`writeln!(stderr())` は
`print_stderr` lint を回避するため CI clippy も未検出）。ロジック変更なし、既存の
inner-constructor fixtures が parse パスをカバー。
### `Matrix{T}(undef, m, n)` / `Vector{T}(undef, n)` コンストラクタをサポート ✅ (Issue #7890)

`Matrix{Float64}(undef, 2, 3)` が `"Unknown parametric struct: Matrix"` で失敗していた。
根因: `try_compile_parametric_constructor_call` で `Array`/`Vector` の組み込み
parametric 型コンストラクタ攔截に `Matrix` が含まれていなかったため、`Matrix{T}` が
parametric struct 解決に回されて未登録エラーとなった。修正:
`subset_julia_vm/src/compile/expr/call/constructors.rs` の攔截条件に `Matrix` を追加。
`Matrix{T}(undef, dims...)` は既存の `Array`/`Vector` と同じく `compile_array_constructor`
経由で `_array_undef_from_dims` へ振り分け、2D 配列として allocate される。
`Vector{T}(undef, n)` も維持。回帰: `array::matrix_vector_undef_constructor_7890`。

### multi-param parametric inner ctor の typeof が全 type param を報告 ✅ (Issue #7972)

`new{A,B,...}(...)` で構築した多パラメータ parametric struct の `typeof` が
最初の型パラメータしか報告しなかった（`P3(1,2.5)` → `P3{Int64}`、本家
`P3{Int64,Float64}`）。フィールド値は正しく格納されていた。根因: inner-ctor の
frame に type_bindings が無く、`NewParametricStruct` ハンドラ
（`subset_julia_vm/src/vm/exec/struct_ops.rs`）が **最初のフィールド値からの単一
パラメータ推論** に fallback していた（`ctor_arg_bound_type_vars` が空で
`NewDynamicParametricStruct` 経路に乗らない=#5059 領域）。修正: type param が 2 個
以上で各々が **bare field 型**（`a::A; b::B`）として現れる場合、対応フィールドの値
から全パラメータを復元（単一パラメータ struct や bare field に対応しないパラメータは
従来の first-value heuristic 維持）。`P3{Int64,Float64}`/`T3{Int64,Float64,Bool}` 正常、
Rational/Complex 無回帰。残: 非スカラ field 型（String 等）のパラメータは値→型名
マッピング未対応で `Any` 表示（`P3{UInt8,String}`→`P3{UInt8,Any}`、ただしパラメータ数は
正しい）。回帰: `struct::multiparam_inner_ctor_typeof_7972`。

### `Bool(x)` 数値コンストラクタを配線 ✅ (Issue #7971)

`Bool(1)` が `"Unknown function: Bool"` で呼べなかった（`Bool` は型としては登録
済みだが Int8/Float64 等と違いコンストラクタ builtin が無かった）。upstream
`Bool(x::Real)=x==0 ? false : x==1 ? true : throw(InexactError(...))` に合わせ、
範囲検査済み `convert(Bool, x)`（[[#7970]]）経由の `BuiltinId::Bool` を配線。配線箇所:
`builtins.rs` の hand-written enum **末尾**に `Bool` 追加（bincode discriminant 互換
=既存キャッシュ無効化不要）+ `define_builtin_table!` に `Bool: "Bool" => ["Bool"]`;
`compile/expr/builtin_types.rs` に `Bool` arm（`CallBuiltin(BuiltinId::Bool,1)`）+
arity guard; `vm/builtins_numeric.rs` に handler（`convert_value("Bool",..)`）+ argc
guard; `call_function_variable.rs` に first-class 名（`map(Bool, xs)` 用）。
`Bool(2)`→InexactError, `Bool(0/1)`/`Bool(0x01)`/`Bool(1.0)` OK, `map(Bool,[0,1])`
動作。`Bool` の型用法（`Bool[]`/`Vector{Bool}`/`isa`/`convert`/`zeros`）は無回帰。
回帰: `conversion::bool_constructor_7971`。

### `convert(Bool, x)` の範囲検証（0/1 以外は InexactError）✅ (Issue #7970)

`convert(Bool, 2)` が `true` を返す（緩い `x != 0` 真偽値判定）バグを修正。upstream
`Bool(x::Real) = x==0 ? false : x==1 ? true : throw(InexactError(:Bool,Bool,x))` /
`convert(::Type{Bool}, x::Number) = Bool(x)` に合わせ、`convert_to_bool`
（`subset_julia_vm/src/vm/convert.rs`）を全整数/浮動ソース対応の faithful な
レンジ検査に書き換え（0→false, 1→true, それ以外 `InexactError("Bool(v)")`）。
加えて Convert builtin（`builtins_exec.rs` / `builtins_types_conversion.rs` の2経路）が
`convert_value` の `Err` で常に pure-Julia `convert` へ fallback していたため、
`InexactError` が `Bool(x)` 未配線コンストラクタ呼び出しに化けて
`"Function 'Bool' not found"` でマスクされていた問題も修正（`InexactError` は最終
結果として即 propagate、fallback しない）。副産物として `convert(Bool, 0x01)` 等の
unsigned ソースも convert_value 経由で動くように。`Bool[2]` も `InexactError` へ
（`literal_element_needs_convert` に `Bool` を追加、[[#7953]] で除外していたもの）。
`Bool(x)` コンストラクタ自体の未配線は別 issue #7971。回帰:
`conversion::convert_bool_inexact_7970` + `convert.rs` 単体テスト。

### module 修飾 parametric inner ctor インスタンスの名前付き field access ✅ (Issue #7958)

`Mod.Wrapped(41)`（module 修飾 + parametric struct + inner constructor
`new{T}(...)`）で生成したインスタンスへの名前付き field access `w.x` が
`"type Wrapped{Int64} has no field x"` で失敗していた（`getfield(w, 1)` は成功）。
根因: この経路のインスタンスは struct 名が instantiation 名 `Wrapped{Int64}` で、
その instantiation が runtime `struct_defs` に未登録のため `type_id` が 0
（ErrorException）に fallback。`GetFieldByName` ハンドラの type_id 引き・名前
スキャン（`def.name == struct_name` は base 名と `{Int64}` 付き名で不一致）が
共に外れていた。修正: `GetFieldByName` に最終 fallback を追加し、struct 名の
base 部（`{` 前）で `compile_context.parametric_structs` schema を引いて宣言順の
field index を解決する（`subset_julia_vm/src/vm/exec/struct_ops.rs`）。bug は
module 修飾 AND parametric AND inner ctor の組合せ限定（他は static field access で
既に動作）。multi-param / 同型2 field も解決。`typeof` の multi-param 欠落
（`Pair2{Int64}`）は別 root cause として #7972 起票。WORKAROUNDS.md の
"Qualified Parametric Inner Constructor Field Access" を Resolved へ移動し
`#7955` fixture を `@test w.x == 42` に復元。回帰:
`module::module_qualified_parametric_inner_field_access_7958`。

### 型付き配列リテラル `T[...]` が UInt hex 要素を convert ✅ (Issue #7953)

型付き配列リテラル `T[a, b, ...]`（非 splat 形）の各要素を、宣言要素型 `T` へ
`convert(T, x)` してから格納するようにした。上流 Julia の `T[...]` は
`a = Vector{T}(undef, n); a[i] = vals[i]` に lower され、`setindex!` が
`convert(T, x)` する。従来 sjulia は complex 要素型のみ `convert` を通し、整数/浮動
要素型はストレージ層 (`ArrayData::set_value`) の `as` 系強制に依存していたが、整数
配列ストレージは符号付きソースしか受け付けないため、UInt 系 hex リテラル
（`Int[0x30, 0x39]` の `0x30::UInt8`）を格納できず `"Cannot store U8 in I64 array"`
で失敗していた。`ArrayElementType::literal_element_needs_convert()`（数値スカラ +
complex）を導入し、`compile_builtin_array` の `getindex`（typed literal）arm でこの
種別の要素を `convert` 経由にした。`Int[0x30, 0x39] == [48, 57]`、`UInt8[1, 2]`、
`Float64[0x30]`、2D `Int[0x1 0x2; 0x3 0x4]`（`reshape(Int[...], r, c)` 経由）が通り、
範囲外要素（`Int8[0xc8]`, `UInt8[300]`, `Int[0xffffffffffffffff]`）は上流同様
`InexactError` を投げる。非数値タグ（`Any`/`String`/`Char`/struct 等）は従来の
verbatim/storage パスを維持。回帰: `arrays::typed_array_literal_uint_convert_7953`。

### AbstractAlgebra Phase 2 driver gate ✅ (Issues #7723/#7488)

Bundled `AbstractAlgebra` に Phase 2 driver files (`AliasMacro.jl`,
`Aliases.jl`, `Assertions.jl`, `Attributes.jl`, `AbstractTypes.jl`,
`ConcreteTypes.jl`) を追加し、upstream include order で package load できるようにした。
`@req` macro import、`PolynomialElem` / `MatrixElem` type aliases、macro-expanded
`UniversalRing`、`MatSpace` が `using AbstractAlgebra` 後に解決する。module default
`names(::Module)` は exported bindings を `Vector{Symbol}` で返す。fixture:
`abstract_algebra/phase2_driver_7723.jl`、`reflection/module_names_7938.jl`。

macro-expanded struct 登録のため、nested `Expr(:macrocall, ...)` を statement path へ戻す
(#7943) ことと、include merge 後に parent macro context の struct queue を drain する
(#7945) ことも実装した。loader package cache は `CACHE_VERSION = 12` へ更新済み。

### `isnumeric` を pure-Julia 化 (Unicode Nd/Nl/No カテゴリテーブル) ✅ (Issue #6752)

Rust の `BuiltinId::Isnumeric` (`char::is_numeric()`) を撤去し、`isnumeric(c::Char)` を
pure-Julia (`base/strings/unicode.jl`) で再実装した。utf8proc バインディングが無いため、
**upstream julia 自身の utf8proc で生成した Nd/Nl/No コードポイント範囲表 (144 範囲)** を
ソート済み非重複テーブルとして埋め込み二分探索する。ASCII 近似の `isdigit`/`isletter` と
違い非ASCII の Nd/Nl/No (`'٣'` `'½'` `'Ⅷ'` `'③'` 等) で upstream と完全一致する
(0..0x3300 で 639 コードポイント・総和一致を確認)。compiler の builtin 迂回を外し
DispatchFirst でメソッド解決。`CACHE_VERSION` 65 へ bump (mid-enum BuiltinId 削除で
discriminant shift)。回帰 fixture `strings/string_isnumeric.jl` を ASCII のみから非ASCII
Nd/Nl/No 網羅へ拡充。副産物として `Int[0x30,...]` の UInt hex 変換バグ #7953 を起票
(回避策: テーブルは10進表記)。表は Unicode バージョン更新時に再生成可能 (手順をファイル先頭にコメント)。
`isletter`/`iscntrl` 等の同様 pure-Julia 化は将来の拡張余地。

### StaticArrays 静的配列の算術 `SMatrix*SVector` ほか ✅ (Issue #7461)

`packages/StaticArrays/src/arraymath.jl` の stub を実装。`Base.:*(::StaticMatrix,
::StaticVector)` の行列×ベクトル積、`Base.:+ / :-(::StaticVector, ::StaticVector)`、
`Base.:*` のスカラー倍（両順序）を追加し、いずれも upstream 同様 `SVector` を返す。
`getindex`/`size`/`length` 経由で SMatrix の行優先内部レイアウト非依存に実装。
`W*x + b` アフィン核（IFS カオスゲーム, Issue #7949）が StaticArrays で書けるようになった。
回帰 fixture: `static_arrays/static_arrays_matvec_arithmetic_7461.jl`（upstream とバイト一致）。

### StaticArrays 静的配列算術の高速化（反射除去 + `.data` 直接アクセス）✅ (Issue #7956)

#7461 の算術は正しいが VM 上で非常に遅かった。プロファイルで真因を特定し最適化:
中間 `Vector`/splat ではなく、(1) `size`/`length` の `typeof(x).parameters[i]`
反射と (2) 要素ごとの型付き `getindex` が支配的だった。
- `indexing.jl`: `size`/`length` を where 句で具象構造体から直接取得する値メソッド
  化（`size(x::SMatrix{M,N,T}) where {M,N,T} = (M,N)`）。約20倍。
- `arraymath.jl`: 2/3/4 サイズを手アンロールし backing `.data` タプルを直接
  行優先インデックス、結果タプルを直接構築。型付き getindex 比 約4倍。汎用ループは
  fallback として保持。n==4 まで特殊化。
- 値パラメータ特殊化（`StaticMatrix{2,2,T}`）は sjulia のディスパッチ不具合 #7960
  を踏むため単一メソッド内ランタイム `size` 分岐で代替。
結果: IFS 2x2 核が 84s → 29s（約2.9倍, 200k 反復, dev-fast）。残りは VM の per-call
ディスパッチオーバーヘッド。スカラー手書きは同条件 0.013s（約2200倍速）のため
`ifs_fractals` サンプルはスカラー実装（#7952）を維持。回帰ベンチ
`benchmarks/vm_staticarrays_matvec.jl` + `vm_staticarrays_matvec_benchmark`（#3210）、
fixture に 4x4 ケース追加。

### macro-expanded `Expr(:struct, ...)` definitions を call-site module に登録 ✅ (Issue #7915)

macro が返す upstream-style `Expr(:struct, mutable, header, body)` を `StructDef` に戻し、
top-level / module body lowering が `Program.structs` / `Module.structs` へ登録するようにした。
これで imported macro が `esc(struct Box ... end)` を返すケースでも
`isdefined(Consumer, :Box)` と `Consumer.Box(...)` が upstream 同様に動く。関数 body 内で
返された struct definition は外側へ漏らさず unsupported として扱う。回帰 fixture:
`macros/macro_expanded_struct_7915.jl`。

### package entry の no-op top-level doc statement を module 前に許容 ✅ (Issue #7913)

package entry file の `module Package ... end` 前にある `@doc raw"""..."""` を
`Base.@doc(str)` の documentation no-op として `nothing` に lower し、
`extract_module` が top-level `Literal::Nothing` statement だけを無害な header として
許容するようにした。これにより upstream-valid な package docs header は読み込めるが、
`println(...)` など effectful な top-level statement は引き続き
`InvalidPackageLayout` で拒否する。回帰: loader 単体テスト2件
(`test_extract_module_allows_noop_package_header_statement`,
`test_extract_module_rejects_effectful_package_header_statement`)。

### パッケージローダーキャッシュの古いメタデータ再利用を無効化 ✅ (Issue #7921)

`subset_julia_vm/src/loader.rs` のパッケージ `.ji.json` キャッシュが、lowered
`Module` のメタデータ形状変更 (type-alias / module-binding 追加。例:
`PolynomialElem` / `MatrixElem`) 後も `CACHE_VERSION` 据え置きのため同一ソースの古い
エントリを再利用していた問題を修正。`CACHE_VERSION` を 9→10 にバンプし、`Module`
メタデータスキーマを `module_schema_fingerprint()` (probe `Module` の JSON SHA-256)
として `CachedModule.schema_fingerprint` に畳み込み、`read_cache` で不一致なら miss
させる。これにより将来のメタデータ形状変更でも定数バンプ無しに古いキャッシュが自動
無効化される。docs: `docs/vm/CACHE_ARCHITECTURE.md` に「Package Loader Cache」節を追
加。回帰: loader 単体テスト3件
(`test_stale_cache_with_mismatched_schema_is_rejected` ほか)。

### 単一引数 rand/randn が `Any` 経由の struct でユーザ method に defer ✅ (Issue #7901)

`rand(x)` / `randn(x)` で `x` の静的型が `Any` のとき発行される Rust builtin
(`Instr::RandArg` / `Instr::RandnArg`) が、`StructRef` 引数を `rng_value_to_dim` で
次元扱いして error にしていた。builtin-defer-through-`Any` の先例 (#6657/#6610/#6638)
に倣い、struct 引数では method table の `rand`/`randn` を再 dispatch する
(`subset_julia_vm/src/vm/exec/rng.rs::try_defer_rand_to_user_method`)。整数次元引数は
従来どおり。回帰: `subset_julia_vm/tests/fixtures/dispatch/rand_defer_any_struct_7901.jl`。
### マクロの `__module__` を呼び出し元 module に束縛 ✅ (Issue #7919)

`module M ... end` 内で展開されるマクロの `__module__` が、合成マクロ関数の
呼び出し引数でハードコードされた `Main` を受け取っていたため、`$__module__` が
`Main` に解決されていた (upstream は展開先 module に束縛する)。`LambdaContext`
に enclosing module 名スタックを追加し (`push_current_module` /
`pop_current_module` / `current_module`)、`lower_module_definition` が module
本体 lowering 中にその名前を積み、`evaluate_macro` /
`evaluate_macro_from_value_args` がその名前を `__module__` の
`Literal::Module` 値として渡すようにした。これで `M.owner === M` が `true`、
トップレベル展開は従来どおり `Main` に解決される。fixture:
`macros/module_macro_module_callsite_7919.jl`。
### module top-level `if` ブロック内の const 代入が module binding を作るように修正 ✅ (Issue #7917)

`module M; if true; const x = 1; end; end` の `const x = 1` が `M.x` として参照できず
`Module M has no function named x` で失敗していた。module body から top-level binding を
集める `collect_module_body_binding_names`
(`subset_julia_vm/src/compile/collect.rs`) が module block の直接の文と `begin` block
(`Stmt::Block`) しか歩かず、`if`/`elseif`/`else` の分岐 body に再帰していなかったため。
upstream Julia では module top-level の `if` は新しいスコープを導入しないので、各分岐の
`const`/`global` 代入は module のメンバーになる。`Stmt::If` の `then_branch` と
`else_branch` (elseif chain は `else_branch` 内の入れ子 `Stmt::If` として lower される)
に再帰するよう修正。`for`/`while`/`let`/関数 body はローカルスコープを導入するため再帰
しない (binding が漏れない)。fixture: `modules/module_if_const_binding_7917.jl`、
unit test: `compile::collect::tests::test_collect_module_bindings_recurses_into_if_branches_issue_7917`
と `..._does_not_leak_loop_scope_issue_7917`。

### Macro-returned where bound の TypeVar 参照を保持 ✅ (Issue #7924)

macro が返す `Tuple{S} where {T, S<:T}` で、`S` の upper bound 内にだけ現れる
先行 TypeVar `T` を `UnionAll` 参照判定が認識するようにした。`JuliaType` の
nested `UnionAll` 表示も `Tuple{S} where {T, S<:T}` に整え、quote 由来と
constructed `Expr(:where, ...)` 由来の両方を fixture
`macros/macro_return_where_bound_typevar_7924.jl` で固定。

### isdefined(::Module, ::Symbol) が module 内 struct を認識 ✅ (Issue #7916)

`module_binding_is_defined` が `struct_defs` / `abstract_types` を非修飾名だけで
照合していたため、module 内で定義され module 修飾名 (`M.Box`) で登録される struct を
見落としていた。修飾名 `format!("{}.{}", module_name, field_name)` も照合するように
修正し、`isdefined(M, :Box)` が upstream 同様 `true` を返すようになった
(`M.Box(3)` は元から構築できていた)。fixture:
`reflection/isdefined_module_struct_7916.jl`。

### Matrix{Num}/Vector{Num} の配列表示が要素ごとの scalar show を経由するように修正 ✅ (Issue #7893)

`println([x y; x x])` などが要素を構造体デバッグ表現
(`Symbolics.Num[Num(Sym(:x)) ...]`)で出していた問題を修正。配列(`Value::ExprArgs`
および Memory-backed の `Array{T,N}` ラッパー struct の両方)の各要素を、登録済み
`Base.show(io, ::T)` 経由で事前レンダする VM helper `render_array_via_user_show` を追加し、
`print`/`println`/`string`/`repr` の Rust 経路から呼ぶ。`show`/`repr` が通る pure-Julia
`show(io::IO, arr::Array)` は compact 1D/2D 形を `print(io, arr)` へ委譲して同じ helper
経路に乗せた(Base 関数内の直接 `show(io, x)` は候補メソッド凍結で user `show` を
dispatch できない既存制約の回避)。数値/文字列/Rational/Complex/Bool/ネスト配列の表示は
upstream と完全一致のまま不変。回帰 fixture: `packages_symbolics_matrix_show`。
### Tuple destructuring in esc'd macro body ✅ (Issue #7900)

esc した macro body 内の tuple 分割代入 `a, b = f(x)` を、call 引数位置への splice 時
(例: `push!(acc, <body>)`)・statement 位置・block 末尾の式位置のいずれでも destructuring
代入として lowering するよう修正 (従来は `=` 演算子への CALL になり `Unknown function: =`)。
CST 経路の destructuring machinery (`DestructureTarget` /
`lower_destructuring_from_targets`) を macro-runtime の `value_to_stmt` /
`call_expr_from_values` から再利用。fixture: `macros/macro_tuple_destructure_7900.jl`。

---

## 最新対応 (2026-06-25)

### Quoted export statements in macro-returned blocks ✅ (Issue #7908)

quoted CST construction が `export` statement を `Expr(:export, ...)` として構築し、
macro-return lowering がその head を `Stmt::Export` に戻せるようになった。
interpolation (`export $alias_name`) も macro body の `Symbol` 値として解決される。
これにより AbstractAlgebra `@alias` 型の quote block が package-specific workaround
なしで通る。fixture: `macros/quoted_export_macro_return_7908.jl`。

### AbstractAlgebra Phase 1 package/dependency gate を追加 ✅ (Issue #7487)

`using MacroTools; using AbstractAlgebra` が default `@stdlib:@packages` loader で
解決するように、bundled `AbstractAlgebra` skeleton と dependency shims
(`Preferences` / `RandomExtensions` / `SparseArrays`) を追加した。`AbstractAlgebra` は
upstream 0.50.1 の dependency gate を保持し、`imports.jl` / `exports.jl` を別 file として
include する。macro/lowering の early source gate は #7488 に残す。fixture:
`abstract_algebra/package_load_7487.jl`。

### AbstractAlgebra Phase 0 audit baseline を追加 ✅ (Issue #7486)

AbstractAlgebra.jl 0.50.1 の source map、dependency gate、phase boundary を
`docs/vm/ABSTRACTALGEBRA.md` に記録した。fixture:
`abstract_algebra/phase0_parse_baseline_7486.jl` は top-level `@doc raw"""..."""`
と early module/type skeleton の各 source 片が `Meta.parse` で upstream Julia /
sjulia ともに通ることを固定する。package bundle と macro/lowering 実行 gate は
#7487/#7488 に残す。

### Macro-returned quote の Expr.args array 再構築を追加 ✅ (Issue #7898)

runtime-expanded macro が `Expr(:quote, ex.args)` を返すケースをサポートした。
`Expr.args` は mutable array-ref として VM 値に残るため、従来の literal 変換では
`Array` を quote できず lowering error になっていた。quote payload 直下の `ExprArgs` を
`Any[...]` typed array literal として再構築し、要素は再帰的に quote constructor へ通す
ことで、upstream Julia と同じ `Vector{Any}` を返す。fixture:
`macros/macro_return_quote_expr_args_7898.jl`。

### LinearAlgebra dispatch-first の splat module call を修正 ✅ (Issue #7896)

`LinearAlgebra.det((A,)...)` / `LinearAlgebra.lu((A,)...)` が user/extension method を
無視して builtin fallback に進む問題を修正した。`LinearAlgebra.<fn>` の dispatch-first
shortcut は通常の generic call path へ委譲するが、その際に空の splat mask を渡していた。
元の `splat_mask` / `kwargs_splat_mask` を保持することで、splat 展開後の引数で
user method が選ばれる。回帰 fixture:
`linalg/det_lu_module_dispatch_first_4020.jl`。
### Interactive fractal explorer サンプル (barnsley_fern を置き換え) ✅

iOS/Web/Flutter の `barnsley_fern` を `ifs_fractals`(name: "Interactive fractals
(@manipulate)")へ置き換え。Interact `@manipulate` の dropdown で Barnsley fern /
Sierpinski 三角形 / Heighway ドラゴンを chaos game で切り替え描画。maps と
`Categorical` picker は本体内 `if/elseif` の直接ローカル束縛で Issue #7900/#7901 を
回避。`sjulia` で `Interact.Manipulate`(plots=3, control=:dropdown)を構築確認。
副産物として gap を起票: **#7900**(`@manipulate` 本体の分割代入 lowering) /
**#7901**(Any 型引数の `rand` がユーザメソッドへ defer しない)。

### push! O(n²) → O(1) — package が同 arity の push! を定義したときの劣化を解消 ✅ (Issue #7883)

`using Plots`(`push!(::Plot, ::Number)` を定義)が読み込まれると、コンパイラは
**すべての** `push!(v, x)`(素の `Vector` でも)を `CallTypedDispatchOrBuiltin` 経由に
回す。その `BuiltinId::Push` builtin フォールバックは毎回 backing `Memory` 全体を
スナップショット+再確保していた(O(n)/push → ループで O(n²))。ネイティブ `ArrayPush`
命令は既に `push_array_wrapper` で in-place 償却成長していた(Issue #6873)ので、builtin
フォールバックも同じ経路を先に試すようにし、dispatch 経由の `push!` がネイティブ命令と
同一(O(1) 償却)になった。Aizawa アトラクタ demo(9000 steps / `@animate every 40` +
`gif`): 5.88s → 1.13s、`push!` ループ単体 4.90s → 0.26s。回帰 fixture:
`packages_plots_push_vector_dispatch_7883`。iOS Aizawa サンプルを 9000 / every 40 に更新。
class="user/package override through Any が native op を dispatch 経由にし遅い builtin
フォールバックを踏む"([[#6657]] 同類)。

### Distributions MvNormal explicit RNG sampling ✅ (Issue #7756)

`MvNormal` の bundled implementation に explicit-RNG scalar sampler を追加した。
`rand(rng, d::MvNormal)` は concrete package method に dispatch し、
`μ + L*z` の `z` を scalar `randn(rng)` で埋めるため、inline `Xoshiro(...)` と
local RNG variable のどちらも RNG state を進める。fixture:
`distributions_mvnormal_sampling`。

### OrdinaryDiffEq README 可視化 MVP 完了 ✅ (Issue #7360)

milestone 33 の親 Issue #7360 を README visualization MVP として完了した。実装済み:
`using OrdinaryDiffEq` / `using SciMLBase`、`ODEProblem`、`Tsit5()`、adaptive
`solve(prob, Tsit5(); dt, saveat, reltol, abstol)`、`ODESolution` fields、
`plot(sol)`、`plot(sol, idxs=(1,2,3))`、`plot!(sol.t, f)`、Plotly artifact routing、
iOS/Web/Flutter README samples。MVP 外の broader parity は #7865。

### Tsit5 solver backend を OrdinaryDiffEq subset に実装 ✅ (Issue #7367)

`Tsit5()` が通常の README MVP examples で fixed-step RK4 に委譲しないようにし、
SciMLBase subset の `solve` に Tsitouras 5/4 tableau stepper と adaptive error
controller を追加した。`reltol` / `abstol` は internal step acceptance に影響し、
`saveat` は stable output grid として維持される。scalar/vector state の両方で
FSAL-style RHS accounting を使い、`stats` に `:algorithm => :Tsit5`、
`:steps`、`:attempts`、`:rejected_steps`、`:rhs_evals` を記録する。fixture:
`packages_ordinarydiffeq_tsit5_adaptive_7367`。
### MacroTools の nested macrocall 展開で struct-heap を解決し @capture を load 可能にした ✅ (Issue #7856)

`evaluate_macro_from_value_args`(macro 展開結果に含まれる nested
`Expr(:macrocall, …)` を専用 VM で再展開する経路)が戻り値の `StructRef` を
解決せずに返していたため、MacroTools の OR パターン `@capture`
(`splitdef` / `isshortdef` / `splitarg`)が利用する `OrBind` struct が
未解決 `StructRef` として変換 AST に漏れ、`utils.jl` の load 時に
`value_to_literal` の "macro expansion cannot quote value type Any" を誘発、
結果として `@capture` / `@match` が登録されず `macrotools::chunk_000` が red
だった。両 `vm.run()` 経路で primary path と同じ
`resolve_macro_result_struct_refs(value, vm.get_struct_heap())` を呼ぶよう修正。
`using MacroTools` 全体が読み込めるようになり、`@capture` / `@match` /
`splitdef` / `splitarg` / `isshortdef` が動作する。回帰 fixture:
`macrotools_package_load_capture_basic`,
`macrotools_nested_macrocall_structref_7856`。

### OrdinaryDiffEq README MVP の docs・parity gap・回帰テストを仕上げ ✅ (Issue #7366)

README MVP の最終到達点を `docs/vm/ORDINARYDIFFEQ.md` に固定した。supported API
matrix で `ODEProblem` / `ODESolution` fields、`solve(prob, Tsit5(); dt, saveat,
reltol, abstol)`、`plot(sol)` / `plot(sol, idxs=...)` / `plot!(sol.t, f)` の
対応範囲を明記し、upstream OrdinaryDiffEq との比較は source shape と workflow を
基準にしつつ、sjulia fixture では fixed-step RK4 代表値・plot shape・Plotly MIME を
固定する方針にした。completion fixture:
`packages_ordinarydiffeq_readme_mvp_7366`。follow-up: true Tsit5 #7367、その他
non-MVP parity gaps #7865。

### OrdinaryDiffEq README サンプルを iOS/Web/Flutter に追加 ✅ (Issue #7365)

`ordinarydiffeq_linear_ode` と `ordinarydiffeq_lorenz_attractor` を iOS sample
resources、web inline sample catalog、Flutter sample assets に追加した。linear sample
は README の `plot(sol, ...)` に `plot!(sol.t, t -> ...)` の解析解 overlay を重ね、
Lorenz sample は `plot(sol, idxs=(1,2,3))` で 3D Plotly path を返す。Flutter の
`CodeSample.loadSamples()` tests は 28 samples と新 ID 2 件を確認する。

### Plots.jl subset に ODESolution 可視化を追加 ✅ (Issue #7364)

`Plots` の Pure-Julia method として `plot(sol::SciMLBase.ODESolution)` /
`plot(sol; idxs=...)` を追加した。scalar solution は time series 1 本、vector
solution は各 component の time series、`idxs=(1,2)` は 2D phase path、
`idxs=(1,2,3)` は既存 `:path3d` series を返す。`plot!(sol.t, t -> ...)` は既存
overlay 経路で動作するため、線形 ODE README の解析解重ね描きも通常の `Plot`
値になる。fixture: `packages_ordinarydiffeq_plot_solution_7364`。artifact MIME
regression: `test_ordinarydiffeq_plot_solution_*_7364`。

### README MVP 向け `solve` / `ODESolution` を fixed-step RK4 で実装 ✅ (Issue #7363)

`solve(prob, Tsit5(); dt, saveat, reltol, abstol)` が `ODESolution` を返すように
した。`Tsit5()` は README 互換 algorithm object として受け取り、内部では
fixed-step RK4 を実行する。scalar out-of-place RHS と vector in-place RHS を
サポートし、`sol.t` / `sol.u` から保存 grid と各 state を取り出せる。in-place
RHS は各 stage で `du` を別 vector として確保し、元の `u0` と stage 入力を
mutate しない。`reltol` / `abstol` は accepted but ignored の parity gap として
`docs/vm/ORDINARYDIFFEQ.md` に明記。fixture:
`packages_ordinarydiffeq_linear_solve_7363`,
`packages_ordinarydiffeq_lorenz_solve_7363`。

### SciMLBase/OrdinaryDiffEq の最小 bundled package skeleton を追加 ✅ (Issue #7362)

Pure-Julia bundled packages として `SciMLBase` / `OrdinaryDiffEq` を追加した。
`SciMLBase` は README MVP に必要な `ODEProblem` / `ODESolution` の field surface
(`f`, `u0`, `tspan`, `p`, `kwargs`, `isinplace`; `u`, `t`, `prob`, `alg`,
`stats`, `retcode`)と `NullParameters`、`solve` hook を持つ。`OrdinaryDiffEq` は
SciMLBase constructor wrapper と `solve` wrapper を export し、`Tsit5()` algorithm
object を生成できる。fixture `packages_ordinarydiffeq_skeleton_7362` が
`using OrdinaryDiffEq`、scalar/vector `ODEProblem`、`Tsit5()` を固定する。

### OrdinaryDiffEq README 可視化 MVP の受け入れ条件を固定 ✅ (Issue #7361)

`docs/vm/ORDINARYDIFFEQ.md` に milestone 33 の MVP scope を追加した。
実装対象は OrdinaryDiffEq.jl README の線形 ODE と Lorenz in-place ODE に限定し、
必要 API surface、fixed-step RK4 backend 方針、MVP 外の upstream 機能、
fixture/sample 名と配置、Phase 1-5 の受け入れ gate を明文化した。これにより
後続 Phase は同文書を到達条件として `ODEProblem` / `solve` / `ODESolution` /
Plots 表示 / iOS-Web-Flutter sample を順に実装できる。

### @generated body の型引数返却と ntuple 風 unroll を修正 ✅ (Issue #7722, refs #5074)

generated body の引数名は runtime 値ではなく generated-time の型オブジェクトを指す。
untyped 引数を持つ generated method を通常の runtime value specialization から除外し、
generated method の call-site return refinement は `Any` に倒すことで、compiler が
`CallSpecialize` / typed consumer を生成して `DataType` 返却と衝突する問題を修正した。
`@generated function f(x); return x; end` は `f(1) == Int64`、vararg は
`(Int64, Float64)` のような型 tuple を返し、`Val{N}` から組み立てた ntuple 風 staged
`Expr(:tuple, ...)` も upstream Julia と一致する。fixture:
`generated/stage_type_args_ntuple_7722.jl`。

### マクロ返却の `where` 型が内側 TypeVar を束縛するよう修正 ✅ (Issue #7844)

ランタイム展開マクロが返す `Tuple{T,S} where S`(`Expr(:where, body, var...)`)を
正しく lowering できるようにした。`expr_heads.rs` の `ExprHead::Where` を
`macro_return_to_expr=true` にし、`macro_runtime.rs` に `where_expr_from_values` /
`where_typevar_binding` を追加。導入された内側型変数を `let S = TypeVar(:S[, lower,
upper])` として束縛し、本体を既存の curly/`DynamicTypeConstruct` 経路で lowering して
`UnionAll(S, body)` でラップする(最左変数が最外、upstream の
`A{T,S} where {T,S}` 規則に一致)。呼び出し側束縛の `T` は動的解決され、導入 `S` は
実行時 `TypeVar(:S)` になる。回帰:
`tests/fixtures/macros/macro_return_where_typevar_7844.jl`(8 アサーション、
`scripts/fixture_julia_parity.sh` で upstream 一致を確認)。
### メソッドの `where T` 型パラメータが同名グローバルにシャドウされるよう修正 ✅ (Issue #7847)

トップレベルに同名グローバル(`T = Int64`、sjulia では非パラメトリック型エイリアスと
して登録される)が存在すると、`function f(x::T) where T; Tuple{T, Int64}; end` の
パラメータ注釈 `x::T` が lowering 時にエイリアス対象 `Int64` へ凍結され、メソッドの
`where` パラメータ `T` が frame の type_bindings に束縛されず、`Tuple{T, Int64}` が
`UndefVarError: Unbound type parameter: T` を送出していた(upstream は
`Tuple{Int64, Int64}`)。原因は #7840 と同種(`type_alias::expand` が同名グローバルを
代入)だが、対象が構造体ではなくメソッドシグネチャ。pure-Rust パーサは
`function f(x::T) where T` を `[Identifier, ParameterList, WhereClause, Block]` の兄弟
ノードとして並べるため、パラメータ注釈は `WhereClause` より前に解析される。
`type_alias` にスレッドローカルのスコープ付き除外スタック(`ScopedExclusion` ガード)を
追加し、`expand` がそれを参照するようにした。`full_form.rs` で署名解析の前に
`where` パラメータ名を先読み(`collect_where_param_names`)して除外スコープを張り、
`where_clause.rs`/operator 短形式 (`short_form.rs`) でも制約を先に解析してから署名を
解析するよう並べ替えた。`where` パラメータが lexically に同名グローバルをシャドウする
upstream 挙動に一致。#7840 のエイリアス挙動(`where` の無い `U = Int64; h(x::U)`)は
保持。回帰: `tests/fixtures/where/global_collision_7847.jl`(長形式/短形式・無境界と
`S<:Real` 境界・エイリアス保持を網羅)と `lowering::type_alias::tests` のスコープ除外
ユニットテスト 3 件。

### quote の境界付き `where S<:Real` が `<:` を保持するよう修正 ✅ (Issue #7845)

`:(Tuple{T,S} where S<:Real)` を quote すると `S<:Real` がバラの 2 シンボル引数に
平坦化されていたのを、単一の `Expr(:<:, :S, :Real)` 引数を生成するよう修正。
`lowering/expr/quote/cst_to_constructor.rs` の `where_clause_args` に、値位置の境界付き
`where` が単一 `SubtypeConstraint`/`SupertypeConstraint` ノード(子は被演算子)で
届く場合はそのノードを丸ごと `where_param_constructor`→`subtype_constraint_constructor`
へ流すガードを追加(従来は子へ降りて各被演算子を別引数化していた)。あわせて
`subtype_constraint_constructor` が `SupertypeConstraint` ノードでは既定演算子を `>:`
にするよう修正(パーサは `<:`/`>:` を Operator 子として残さない)。これにより
`where S<:Real` は `Expr(:<:, :S, :Real)`、`where S>:Int` は `Expr(:>:, :S, :Int)`、
`where Int<:T<:Real` は `Expr(:comparison, :Int, :<:, :T, :<:, :Real)` を生成し、
無境界の `where S`(bare Symbol)は不変。関数シグネチャの
`f(x::T) where T<:Real` も同経路で正しくなる。回帰:
`tests/fixtures/metaprogramming/where_quote_bounded_constraint_7845.jl`(29
アサーション、`scripts/fixture_julia_parity.sh` で upstream 一致を確認)。

### statement-position macro block の末尾 `:function` / `:+=` を statement path で lowering ✅ (Issue #7805)

statement position の macro が返す outermost `Expr(:block, ...)` は、#7764 以降、
末尾値を捨てないため `value_to_branch_expr` 経由で value-preserving lowering している。
ただし末尾が `Expr(:function, ...)` や `Expr(:+=, ...)` のような statement-only head の場合、
`value_to_expr` が `macro_return_to_expr=false` として拒否していた。修正では outer block の
最後の non-`LineNumberNode` 要素を調べ、statement-only head のときだけ従来の
`value_to_stmt` path に戻す。あわせて esc された function name / `+=` target を
`macro_assignment_target` で unwrap し、`function $(esc(:adder))...end` と
`$(esc(x)) += 10` が statement lowering できるようにした。fixture:
`macro/macro_statement_tail_stmt_7805.jl`。

### 構造体の型パラメータが宣言親型で同名グローバルにシャドウされるよう修正 ✅ (Issue #7840)

`struct Wrap{T} <: AbstractVector{T}` のように、構造体の型パラメータと同名の
トップレベルグローバル(`T = Int64`)が存在すると、lowering で親型文字列を
`type_alias::expand` に通す際にグローバルの値が代入され、親テンプレートが
`AbstractVector{Int64}` に凍結されていた(`Wrap{Float64} <: AbstractVector{Float64}`
が誤って `false`)。`type_alias::expand_excluding` を追加し、`lowering/struct_.rs`
で構造体自身の型パラメータ名を除外集合として渡すことで、親テンプレートを
パラメトリックに保つ。upstream 同様に型パラメータが同名グローバルをシャドウする。
fixture: `struct/parent_typevar_shadows_global_7840.jl`。

### 式位置の `@sync` が本体値を返すよう修正 ✅ (Issue #7813)

式位置(代入 RHS / 戻り値)の `@sync` が本体の最後の式の値ではなく `nothing` を
返していた問題を修正。`lower_sync_block_expr` / `lower_sync_single_async_expr` が
本体の最後の式の値を span-unique な結果一時変数に束縛してから throw-if-failed
ガードを実行し、`Expr::LetBlock` がその一時変数を yield するようにした。末尾が
`@async` のときは upstream 同様に実 `Task` を生成して結果とし、待機による失敗集約
(`CompositeException`)も維持する。`r = @sync begin; @async 1; @async 2; end` /
`s = @sync @async 99` が共に `typeof == Task` を返すことを確認。fixture:
`concurrency/sync_expr_returns_value.jl`。
### ユーザー定義 outer constructor が field-count default constructor を隠す ✅ (Issue #7793)

ユーザー outer constructor を定義した struct で、bare / module-qualified な field-count
default-ctor 呼び出し(`Foo("hi", :t, :u)` 等)が `NoMethodFound` で失敗するバグを修正。
multi-arg / static-miss の `NoMethodFound` 回復アーム(`dispatch.rs`)と qualified 経路
(`compile_module_call_via_method_table`, `module_call.rs`)に、`struct_table` の struct で
arity が field 数と一致するとき field-count built-in default ctor へ fallback する処理を
追加(`try_struct_field_count_default_ctor_fallback`)。`has_inner_constructor` でガードし
inner ctor struct には合成 default ctor を作らない。回帰 fixture
`struct/outer_ctor_default_ctor_reachable_7793.jl`(#7729 と同根)。
### 文位置 @sync for ... @async ... end が await を握り潰して空結果を返す ✅ (Issue #7831)

文位置 `@sync` が for ループ本体を取る形(`@sync for i in 1:3; @async push!(results,
i^2); end`)は、PR #7811 以降 `lower_sync_macro_stmt` が `begin`/単独 `@async` 以外の
本体を plain-statement lowering に落としていたため、ループ内 `@async` が一つも await
されず空結果を返していた。ブロック子文の `@async` ラップを `lower_sync_body_stmts`
ヘルパに切り出して for 本体に再利用し、`control_for.rs` の
`lower_for_stmt_with_body`(binding/cartesian desugaring を共有しつつ本体ブロックだけ
差し替え)で for を再構築、共有例外アキュムレータに `sync_throw_if_failed` を発行する
よう修正。fixture: `concurrency/sync_stmt_for_async.jl`(range / 配列イテラブル / 非
async 文混在 / `t = @async` / CompositeException 集約)。upstream julia 1.12 と一致。

### 位置引数を持つ関数で global を参照する kwarg デフォルトが 0 になる ✅ (Issue #7774)

global / const-global を参照する keyword デフォルト(例 `close(a, b; atol=tol)`)が、
keyword 省略時に **位置引数を持つ関数でのみ** `0` と評価されるバグを修正した。
原因は specialize ディスパッチ経路(`execute_call_specialize_with_args`)が
fallback body 選択時に各省略 keyword を baked リテラル `kwparam.default`
(畳み込めないデフォルトは `I64(0)`)で直接束縛していたこと。この直接束縛ループを
keyword-only 経路と同じ `bind_kwargs_defaults` 呼び出しに置き換え、`default_expr`
を実フレーム(global フォールバック付き)で評価するようにした。fixture:
`kwargs/kwarg_default_global_with_positional_7774.jl`。
### ユーザ型のチェーンが AbstractArray{T,N} に届いても bare AbstractArray の subtype にならない ✅ (Issue #7787)

宣言した抽象親チェーンが `AbstractArray{T,N}` に届くユーザ型は、パラメータ付き
`AbstractArray{T}` の subtype には正しくなる(#7728 で修正済み)一方、**bare**
(パラメータ無し)の `AbstractArray` の subtype にはならなかった(upstream は `true`)。
原因は `struct_is_subtype_of_abstract_with_lookup` の array-family アーム
(`A::AbstractArray => array_family_dim(name).is_some()` など)が **組み込みの**
array-family 名しか見ず、供給された `hierarchy` を辿ってユーザの struct/abstract
チェーンを array 族まで遡らなかったこと。数値アーム(`A::Number =>
registered_struct_is_subtype_of_with_lookup(name, "Number", hierarchy)`)に倣い、
インスタンス化された親チェーンを array-family 祖先まで歩く
`user_struct_array_ancestor` ヘルパを追加し、bare `AbstractArray` /
`DenseArray` / `AbstractVector` / `AbstractMatrix` の各アームを、祖先の
abstractness / wrapper / rank 制約を尊重しつつ OR で拡張した。これにより
`MyArr{Float64} <: AbstractArray`(と rank-2 なので `<: AbstractMatrix`)が
`true`、`<: AbstractVector` / `<: DenseArray` は `false`、DenseArray 起点の
チェーンは `<: DenseArray` も `true` になる(upstream julia 1.12 と一致)。
fixture: `types/bare_abstractarray_user_chain_7787.jl`。

検証: upstream Julia と
`./target/release/sjulia subset_julia_vm/tests/fixtures/types/bare_abstractarray_user_chain_7787.jl`、
`bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/types/bare_abstractarray_user_chain_7787.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests -E 'test(types_tests)'`、
`timeout 1800 cargo nextest run --release --lib -E 'test(type_core)'`、
`cargo clippy -p subset_julia_vm --all-targets -- -D warnings`、
`bash scripts/check_fixture_test_names.sh`。
### for-head のインライン range リテラル(整数始点 + 非リテラル float step)が 0 回反復 ✅ (Issue #7800)

`for u in 0:(2π/12):2π` のような **整数始点 + 非リテラル float step** のインライン
range リテラルがトップレベル for-head で 0 回反復するバグを修正した。#3551 の
lowering 側リテラル検知(`is_non_integer_literal`)は `2π/12` のような `BinaryOp`
step を捕捉できなかった。`compile/stmt.rs` の `Stmt::For` codegen の
`needs_typed_range` ガードに `F64`/`F32`/`F16`/`BigFloat` を追加し、start/end/step の
推論型(`infer_expr_type`)が非整数なら `Stmt::ForEach` へ迂回するようにした。
fixture: `range/for_head_nonliteral_float_step_7800.jl`。

検証: upstream Julia と
`./target/release/sjulia` で 13 反復を確認、
`timeout 1800 cargo nextest run --release --test fixture_tests range:: control_flow::`、
`cargo nextest run --release --lib control_for`、
`cargo clippy -p subset_julia_vm --all-targets -- -D warnings` がパス。

### コンストラクタ本体の tuple リテラル splat が varargs をネストする ✅ (Issue #7741)

splat を含む tuple リテラル `(A, B, xs...)` が `xs` を展開せず 1 要素にネストする
バグを修正した。`lower_tuple_expr_impl` に splat 検出分岐を追加し、splat を含み
named field を含まない positional tuple リテラルを `tuple` builtin への splat-call
(per-element splat mask 付き)へ lower するようにした(upstream の
`Core.tuple(...)` / `Core._apply_iterate` lowering に対応)。`compile_splat_call` は
`tuple` の戻り値型を `Tuple` と報告し、`::Tuple` フィールド/引数への coercion 誤検知を防ぐ。
fixture: `splat/splat_tuple_literal_7741.jl`。

検証: upstream Julia と
`./target/release/sjulia subset_julia_vm/tests/fixtures/splat/splat_tuple_literal_7741.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests splat:: tuple:: struct_tests:: varargs:: kwargs_splat:: closures:: dispatch::`、
`timeout 1800 cargo nextest run --release --lib -p subset_julia_vm`、
`cargo clippy -p subset_julia_vm --all-targets -- -D warnings`、
`bash scripts/check_fixture_test_names.sh`。
### JSXGraph `parametricsurface3d` + 2 引数 JSFunction (トーラス曲面) ✅

- `JSXGraph.jl` に `parametricsurface3d(fx, fy, fz, urange, vrange; kwargs...)` を追加・export。座標写像は `(u, v)` の 2 引数 JSFunction。
- `JSFunction` に `var2::Symbol` を追加 (空シンボルで単一引数を表す)。`_jsf` (curve3d 用 `t`) は後方互換、`_jsf2` (`u`,`v`) を新設。
- `jsxgraph.rs`: 非空 `var2` を検出したら `{"jsfunc","vars":[...]}`、それ以外は従来どおり `{"jsfunc","var":...}` を出力。
- iOS `JSXGraphView.swift` / Web `web/app.js` の両レンダラが `new Function(...vars, body)` で多引数関数を生成。
- サンプル追加: 「Torus (Plots.jl)」(plot3d ワイヤーフレーム) と「JSXGraph Torus」(parametricsurface3d 曲面) を samples.json / Swift フォールバック / `web/samples_ir.js` に登録。Plots 版は最終評価値を `current()` で Plot にして描画させ、inline 整数始点+浮動ステップ範囲が for-head で 0 回反復するバグ (Issue #7800) を変数束縛で回避。
- テスト: `plot_artifact_mime_tests::test_jsxgraph_parametricsurface3d_emits_two_arg_jsfuncs`、fixture `packages_jsxgraph_parametricsurface3d`。
- 関連: 外側コンストラクタがデフォルトコンストラクタを bare 呼び出しでも隠す不具合を Issue #7793 として起票 (#7729 同系統)。

### LinearAlgebra forwarder の自己再帰 Stack overflow 修正 ✅ (Issue #7772)

stdlib forwarder を private compiler bridge から builtin へ直行させ、自己再帰を解消。
dispatch-first override (Issue #4020) は素の `LinearAlgebra.<fn>(A)` を非修飾 generic call
へ落として維持。`lu`/`det`/`inv`/`svd`/`qr`/`eigen`/`eigvals`/
`cholesky`/`cond` と `\` が再び動作。iOS サンプルを `matrix_decompositions_ios_7772.jl`
として fixture 追加し、linalg カテゴリ全 chunk が green。
### 値パラメータ AbstractArray 親チェーンの要素サブタイプ伝播 ✅ (Issue #7728)

`abstract type ... <: Parent{...}` の lowering でパラメトリックな親の型/値
パラメータが基底名に切り詰められていた問題を修正し、親の完全なパラメトリック
表記を保持するようにした(`subset_julia_vm/src/lowering/abstract_.rs`)。これにより
`StaticArray7458{S,T,N} <: AbstractArray{T,N}` 形の階層で
`SVector7458{3,Int64} <: AbstractArray{Int64,1}` が `true` になり、要素/次元
パラメータの不一致は invariant に `false` を返す。fixture:
`types/abstractarray_parent_param_chain_7728.jl`。bare `AbstractArray` への
ユーザ型サブタイプは既存 gap として #7787 に分離。

検証: upstream Julia と
`./target/release/sjulia` で
`subset_julia_vm/tests/fixtures/types/abstractarray_parent_param_chain_7728.jl`
が双方 8/8 pass。

### Distributions common univariate API ✅ (Issue #7324)

Bundled `Distributions` の既存 univariate 分布(連続8種・離散6種)で、
`modes`、`skewness`、`kurtosis`、`mgf`、`cf`、`isbounded` 系、
`cquantile` / `invlogcdf` / `invlogccdf`、`loglikelihood`、主要ペアの
`kldivergence` を利用可能にした。`mgf` / `cf` は closed form が自然な分布に限定し、
KL は Normal / LogNormal / Exponential / Poisson / Geometric / Bernoulli /
same-`n` Binomial / Categorical を閉形式で固定した。
fixture: `distributions/distributions_common_api_7324.jl`。

検証: upstream Julia
`subset_julia_vm/tests/fixtures/distributions/distributions_common_api_7324.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/distributions/distributions_common_api_7324.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests distributions::chunk_000 --no-fail-fast`、
`timeout 1800 cargo nextest run --release base_exports_do_not_exceed_upstream --no-fail-fast`、
`bash scripts/check_fixture_test_names.sh`、`git diff --check`。

### Distributions truncated univariate distributions ✅ (Issue #7325)

Bundled `Distributions` に `Truncated{D}` wrapper と `truncated(d, lower, upper)` /
`truncated(d; lower=..., upper=...)` を追加した。構築時に `lcdf` / `ucdf` / `tp` /
`logtp` を保存し、`pdf` / `logpdf` / `cdf` / `quantile` / `minimum` / `maximum` /
`mean` / `insupport` と inverse-CDF ベースの `rand` を提供する。`rand(td, n)` が
VM builtin の dimension path に吸われないよう、`truncated(...)` の戻り型を
`Distributions.Truncated` として保持する inference / dispatch override も追加した。
fixture: `distributions/distributions_truncated_7325.jl`。

検証: `cargo build --release --bin sjulia --features repl`、
`julia --project=/tmp/sjulia_distributions_check subset_julia_vm/tests/fixtures/distributions/distributions_truncated_7325.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/distributions/distributions_truncated_7325.jl`、
`timeout 1800 cargo nextest run --release -p subset_julia_vm --no-fail-fast julia::packages::tests::test_distributions_includes`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast distributions::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast exports::chunk_000`、
`timeout 1800 cargo nextest run --release -p subset_julia_vm --no-fail-fast rand_randn`、
`bash scripts/check_fixture_test_names.sh`、`git diff --check`。

### Distributions fit_mle / fit and suffstats expansion ✅ (Issue #7326)

Bundled `Distributions` の fitting API を `suffstats(D, x)` →
`fit_mle(D, ss)` の二段構成へ拡張した。Normal / Uniform / Exponential /
Gamma / Beta / LogNormal / Weibull / Cauchy / Bernoulli / Binomial / Poisson /
Geometric / Categorical / MvNormal を対象にし、Binomial / Categorical は
既知カテゴリ数・試行数を受ける `fit_mle(D, n, x)` も追加した。Gamma / Beta /
Weibull は Newton 反復、`fit(Beta, x)` と `fit(Cauchy, x)` は upstream の
`fit` surface に合わせる。
fixture: `distributions/distributions_fit_suffstats_7326.jl`。

検証: `cargo build --release --bin sjulia --features repl`、
`julia --project=/tmp/sjulia_distributions_check -e 'println(include("subset_julia_vm/tests/fixtures/distributions/distributions_fit_suffstats_7326.jl"))'`、
`julia --project=/tmp/sjulia_distributions_check -e 'println(include("subset_julia_vm/tests/fixtures/distributions/distributions_fit.jl"))'`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/distributions/distributions_fit_suffstats_7326.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/distributions/distributions_fit.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast distributions::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast exports::chunk_000`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`git diff --check`。

### Distributions classical test distributions ✅ (Issue #7327)

Bundled `Distributions` に `TDist(ν)`、`Chisq(ν)`、`FDist(ν1, ν2)` を追加した。
各分布で `params`、`mean`、`var`、`mode`、`minimum` / `maximum`、`pdf` /
`logpdf` / `cdf` / `quantile`、`rand` を提供する。`Chisq` は Gamma 経由、
`TDist` は Normal/Chisq 比、`FDist` は Chisq 比で sampling し、explicit RNG と
array sampling wrapper にも登録した。
fixture: `distributions/distributions_test_dists_7327.jl`。

検証: `cargo build --release --bin sjulia --features repl`、
`julia --project=/tmp/sjulia_distributions_check -e 'println(include("subset_julia_vm/tests/fixtures/distributions/distributions_test_dists_7327.jl"))'`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/distributions/distributions_test_dists_7327.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast distributions::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast exports::chunk_000`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`git diff --check`。

### Distributions continuous univariate expansion 1 ✅ (Issue #7328)

Bundled `Distributions` に `Laplace(μ, θ)`、`Logistic(μ, θ)`、`Rayleigh(σ)`、
`Pareto(α, θ)`、`Gumbel(μ, θ)`、`Frechet(α, θ)`、`Levy(μ, σ)` を追加した。
各分布で `params`、該当する `location` / `scale` / `shape`、`mean` / `var` /
`median` / `mode`、`entropy`、support bounds、`pdf` / `logpdf` / `cdf` /
`quantile`、`rand` を提供し、explicit RNG と array sampling wrapper に登録した。
fixture: `distributions/distributions_continuous_expansion_7328.jl`。

検証: `cargo build --release --bin sjulia --features repl`、
`julia --project=/tmp/sjulia_distributions_check -e 'println(include("subset_julia_vm/tests/fixtures/distributions/distributions_continuous_expansion_7328.jl"))'`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/distributions/distributions_continuous_expansion_7328.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast distributions::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast exports::chunk_000`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`git diff --check`。

### Distributions continuous univariate expansion 2 ✅ (Issue #7329)

Bundled `Distributions` に `Chi(ν)`、`Erlang(α, θ)`、`InverseGamma(α, θ)`、
`InverseGaussian(μ, λ)`、`Arcsine(a, b)`、`TriangularDist(a, b, c)`、
`SymTriangularDist(μ, σ)`、`Cosine(μ, σ)`、`Semicircle(r)`、`Kumaraswamy(a, b)` を
追加した。各分布で `params`、該当する `location` / `scale` / `shape`、
`mean` / `var` / `median` / `mode`、`entropy`(upstream が提供するもの)、
support bounds、`pdf` / `logpdf` / `cdf` / `quantile`、`rand` を提供し、
explicit RNG と array sampling wrapper に登録した。
fixture: `distributions/distributions_continuous_expansion_7329.jl`。

検証: `cargo build --release --bin sjulia --features repl`、
`julia --project=/tmp/sjulia_distributions_check -e 'println(include("subset_julia_vm/tests/fixtures/distributions/distributions_continuous_expansion_7329.jl"))'`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/distributions/distributions_continuous_expansion_7329.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast distributions::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast exports::chunk_000`、
`bash scripts/check_fixture_test_names.sh`、
`git diff --check`。

### Distributions discrete univariate expansion 1 ✅ (Issue #7330)

Bundled `Distributions` に `NegativeBinomial(r, p)`、`Hypergeometric(s, f, n)`、
`BetaBinomial(n, α, β)` を追加した。各分布で `params`、主要 statistics、
support bounds、`pdf` / `logpdf` / `cdf` / `quantile`、`rand` を提供し、
finite-support 分布は `support` も提供する。3 型を explicit RNG と integer array
sampling wrapper に登録した。
fixture: `distributions/distributions_discrete_expansion_7330.jl`。

検証: `cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia -e 'using Distributions; println(pdf(NegativeBinomial(5.0, 0.4), 3)); println(cdf(Hypergeometric(30,20,10), 6)); println(quantile(BetaBinomial(12,2.0,5.0), 0.5))'`、
`julia --project=/tmp/sjulia_distributions_check -e 'println(include("subset_julia_vm/tests/fixtures/distributions/distributions_discrete_expansion_7330.jl"))'`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/distributions/distributions_discrete_expansion_7330.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast distributions::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast exports::chunk_000`、
`bash scripts/check_fixture_test_names.sh`、
`git diff --check`。

### Distributions discrete univariate expansion 2 ✅ (Issue #7331)

Bundled `Distributions` に `Skellam(μ1, μ2)`、`Dirac(value)`、
`PoissonBinomial(p)` を追加した。各分布で `params`、主要 statistics、support bounds、
`pdf` / `logpdf` / `cdf` / `quantile`、`rand` を提供する。Skellam は整数次数 Bessel I
級数と有限窓 PMF scan、PoissonBinomial は再帰 DP の PMF 計算で実装し、3 型を
explicit RNG と integer array sampling wrapper に登録した。
fixture: `distributions/distributions_discrete_expansion_7331.jl`。

検証: `cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia -e 'using Distributions; println((mode(Skellam(4.0,1.5)), support(Skellam(4.0,1.5)), params(Dirac(3))))'`、
`julia --project=/tmp/sjulia_distributions_check -e 'println(include("subset_julia_vm/tests/fixtures/distributions/distributions_discrete_expansion_7331.jl"))'`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/distributions/distributions_discrete_expansion_7331.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast distributions::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast exports::chunk_000`、
`bash scripts/check_fixture_test_names.sh`、
`git diff --check`。

### Distributions parity and samples polish ✅ (Issue #7332)

Milestone 31 の仕上げとして、`distributions_parity_7332.jl` を追加し、Test.jl
summary を `fixture_julia_parity.sh` で upstream Julia / sjulia 間比較できるようにした。
既存の `Distributions.jl` サンプルは `StatsPlots` の pdf plot、`truncated`、
`fit_mle`、`PoissonBinomial` / `Skellam` の離散分布例を含む内容に更新し、
iOS `.jl` / mobile `.jl` / Swift fallback / web sample を同期した。
サポート範囲は `docs/vm/DISTRIBUTIONS.md` に一覧化した。
fixture: `distributions/distributions_parity_7332.jl`。

検証: `cargo build --release --bin sjulia --features repl`、
`JULIA_PROJECT=/tmp/sjulia_distributions_check bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/distributions/distributions_parity_7332.jl`、
`./target/release/sjulia SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/distributions_package.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast distributions::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast statsplots::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast exports::chunk_000`、
`timeout 1800 cargo nextest run --release --test code_samples_tests --no-fail-fast`、
`bash scripts/check_fixture_test_names.sh`、
`git diff --check`。

### Base macro expansion uses macro_runtime ✅ (Issue #7721)

Base registry macro expansion を legacy static template substitution から
`macro_runtime` の expansion-time VM 実行へ移した。expression / statement context の
Base macro は arity dispatch 後に user macro と同じ synthetic macro function として実行し、
returned AST を共通 value-to-IR path で戻す。旧 `substitute_params_in_macro_expr`
helper は削除済み。

意図的に残す Rust kernel は Base bootstrap 前に必要な構造 macro と AST 変換 macro
(`@inline` / `@noinline` / `@inbounds` / `@boundscheck` / metadata wrappers /
`@view` / `@views` / multi-argument `@show`) に限定した。macro-expanded call target の
`Symbol` / `Expr` / `QuoteNode` などは source lowering と同じ builtin mapping を使い、
macro argument string literal の escape 処理も通常 literal lowering と一致させた。
fixture: `macros/base_macro_runtime_path_7721.jl`。

full fixture validation で露出した runtime-expanded Base macro の AST 形も同じ path に
集約した。`Expr(:tuple, Expr(:(=), ...))` の named tuple、indexed assignment tail、
`Expr(:parameters, Expr(:..., opts))` keyword splat、`Expr(:where, ...)` type operand、
matrix `:hcat`/`:vcat`/`:row` constructors、`Base.:∈` の quoted operator field、unicode
identity / set operators、module literals (`Meta` 等)、static `Val{...}` type/call operands、
Big numeric literals、nested stdlib/package macro calls を lower できる。
fix coverage: #7763/#7765/#7767/#7769/#7771/#7773/#7775/#7778/#7779/#7780/#7786/#7790/#7794/#7798。
関連 regression として #7761/#7764/#7768/#7772 も fixture/full suite で固定した。

検証: upstream Julia fixture、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/base_macro_runtime_path_7721.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/var_string_macro_arg_7676.jl`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_metaprogramming_roundtrip.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`、
`timeout 1800 cargo nextest run --release --test fixture_tests macro_tests::chunk_001`、
`timeout 1800 cargo nextest run --release --test fixture_tests metaprogramming::chunk_000 operators::chunk_000 macrotools::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests types_tests::chunk_003`、
`CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=cc RUSTFLAGS= timeout 1800 cargo nextest run --release` (3963 passed)。
`bash scripts/test_aot.sh` は AoT codegen/types unit の既存 issue-named failures 6 件で
clippy step 前に停止した。iOS cross builds は local Linux 環境に `xcrun` / Apple SDK が無いため
link step で未検証。

### Runtime-expanded `@lock` accepts `Expr(:try)` tails ✅ (Issue #7806)

Runtime macro-return expression lowering now accepts `Expr(:try)` by lowering it
through the existing try-as-expression `LetBlock` conversion. This keeps #7764's
value-preserving macro block path working while allowing `@lock` expansions whose
tail is `try ... finally ... end`. Fixture `concurrency/lock_macro.jl` now checks
that `@lock` returns the body value and releases the lock.

検証: `./target/release/sjulia subset_julia_vm/tests/fixtures/concurrency/lock_macro.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests concurrency::chunk_000`、
`CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=cc RUSTFLAGS= timeout 1800 cargo nextest run --release` (3963 passed)。

### VM crate integration tests avoid moved C ABI symbols ✅ (Issue #7821)

FFI split 後に `compile_and_run_with_output` / `free_string` が
`subset_julia_vm_ffi` へ移ったため、`subset_julia_vm` crate の
`integration_compile_sample_tests` が full nextest compile で E0425 になっていた。
VM crate 側の tests から C ABI 直接呼び出しを外し、direct pipeline helper の戻り値に
`[result] ...` 行を追加する test helper を使うよう変更した。配列 result formatting は
`ffi_support::vm_format_value` を通すため、旧 FFI output と同じ `[result] [1, 2, 3]`
表示を維持する。

検証:
`CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=cc RUSTFLAGS= timeout 1800 cargo nextest run --release --test integration_compile_sample_tests`。

### Web Distributions sample test matches current sample output ✅ (Issue #7824)

`distributions_package.jl` の current sample は Normal stats、truncated distribution、
Binomial/PoissonBinomial/Skellam、sampling、`fit_mle` を表示する形に更新済みだったが、
web host-side test は削除された `Distribution: Normal{Float64}(2.0, 3.0)` 行を
期待したままだった。assertion を現行 sample の安定出力
(`Normal mean/std`、`truncated support`、`fit_mle mean/std`) へ更新し、
sample 実行 path を引き続き guard する。

検証:
`CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=cc RUSTFLAGS= timeout 1800 cargo nextest run --release -p subset_julia_vm_web test_web_sample_distributions_package_runs`。

### Macro-returned parametric types keep caller typevars dynamic ✅ (Issue #7830)

macro-returned `Expr(:curly, :Vector, :T)` が caller の `where T` を参照する場合に、
`static_curly_type_name` が `Vector{T}` を静的 type literal として文字列化し、
method instantiation の `T` を失っていた。function body lowering 中の active
`where` type parameter 名を `LambdaContext` に積み、macro-returned parametric type の
static fast path はその名前を含むとき `DynamicTypeConstruct` へ落とすようにした。
fixture: `macros/macro_return_typevar_curly_7830.jl`。

検証: upstream Julia と
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/macro_return_typevar_curly_7830.jl`。

### Runtime-expanded macros accept catch-only `Expr(:try)` ✅ (Issue #7832)

macro-returned `try ... catch ... end` は upstream Julia AST では `Expr(:try, ...)` の
3 引数形になるが、runtime macro-return lowering が 4 引数以上を要求していた。
`try_stmt_from_values` が 3 引数形を受け取り、`finally_block = None` として
`Stmt::Try` へ戻すよう修正した。fixture: `macros/macro_return_try_catch_only_7832.jl`。

検証: upstream Julia と
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/macro_return_try_catch_only_7832.jl`。

### Macro-spliced parametric type args evaluate caller bindings ✅ (Issue #7835)

macro-returned `Expr(:curly, :Vector, :T)` の `:T` が ordinary caller binding の場合も、
static type-name fast path が `Vector{T}` と文字列化して `T` を読めていなかった。
`Value::Symbol` type argument は既知 static type symbol の場合だけ `TypeOf` へ残し、
それ以外は `DynamicTypeConstruct` へ落として runtime binding を評価するようにした。
fixture: `macros/macro_spliced_typearg_binding_7835.jl`。

検証: upstream Julia と
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/macro_spliced_typearg_binding_7835.jl`。

### Metaprogramming roundtrip gate ✅ (Issue #7720)

`scripts/check_metaprogramming_roundtrip.sh` を追加し、upstream `julia` と
`target/release/sjulia` で同一の generated seed program を実行して Test.jl summary を比較する。
gate は `Meta.parse` source printing、`Meta.parse`→runtime `eval`、macro-returned
`Meta.parse` values→lowering IR→run の 3 経路を固定する。`check_*.sh` perimeter 用に
shellcheck registration と `docs/vm/CODE_AUDITS.md` 登録も追加した。

検証: `cargo build --release --bin sjulia --features repl`、
`bash scripts/check_metaprogramming_roundtrip.sh`、
`bash scripts/check_audit_scripts_bash3_compat.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests metaprogramming::`。
local 環境に `shellcheck` が無いため、`shellcheck scripts/check_metaprogramming_roundtrip.sh`
は未実行。

### Meta.parse roundtrip の parser-internal head 修正 ✅ (Issue #7753)

`Meta.parse(src)` / `string(::Expr)` の CST→Expr 変換 (`builtins_macro/parse.rs`
の `cst_to_value`) を upstream 形に正規化した。
(a) `var"@q"` → `Symbol("@q")` (表示 `var"@q"`)、`Expr(:prefixedstringliteral, ...)` を廃止。
`var` 以外の prefixed string は upstream の
`Expr(:macrocall, Symbol("@x_str"), LineNumberNode, "content")` 形にした
(`r"abc"` → `r"abc"`、`big"123"` → `big"123"`)。
(b) keyword 引数 → `Expr(:kw, name, value)` (表示 `name = value`)、
`Expr(:keywordargument, ...)` を廃止。
(c) `:a` → `QuoteNode(:a)`。`Dict(:a => 1)` が `:a` の `:` を保持し upstream 通り
`Dict(:a => 1)` と表示される(従来は `Dict(a => 1)`)。serialization 側
(`format_symbol_name` の `var"..."` quoting #7676、`format_kw`、`QuoteNode` 表示)は
既に upstream 形だったため変更不要。

検証: `cargo build --release --bin sjulia --features repl`、issue MWE を sjulia/julia で diff、
fixture `metaprogramming/metaparse_roundtrip_internal_heads_7753.jl` を julia/sjulia で実行、
`timeout 1800 cargo nextest run --release --test fixture_tests metaprogramming:: io:: macros:: macro_tests:: meta:: parse::`、
`cargo clippy -p subset_julia_vm --all-targets -- -D warnings`。

### Meta.parse keyword call の eval / macro-return lowering を修正 ✅ (Issue #7755)

`Meta.parse("f(a=2)")` は #7753 以降 upstream と同じ `Expr(:call, :f, Expr(:kw, :a, 2))`
を返すが、runtime `eval` は call argument 中の bare `Expr(:kw, ...)` を普通の expression
として評価し、`eval: unsupported Expr head 'kw'` で拒否していた。`eval_call_arguments` が
bare `Expr(:kw, name, value)` を `Expr(:parameters, ...)` 内の keyword と同じ helper で
kwargs map へ移すようにし、`eval(Meta.parse("kw(a=2,b=3)"))` が keyword call として
dispatch する。macro-return lowering は既存の `call_expr_from_values` の `ExprHead::Kw`
path で同形を扱えるため、同 fixture で固定した。roundtrip gate に eval / macro-return の
keyword call seed を追加し、#7755 を除外リストから外した。fixture:
`metaprogramming/metaparse_keyword_eval_macro_return_7755.jl`。

### Meta.parse let/else の parser-internal head を正規化 ✅ (Issue #7754)

`Meta.parse("let ... end")` が `Expr(:letexpression, ...)`、`if ... else ... end` が
tail に `Expr(:elseclause, ...)` / `Expr(:elseifclause, ...)` を返し、macro-return lowering が
unsupported Expr head として拒否していた。`builtins_macro/parse.rs` の CST→Value 変換で
`LetExpression` / `LetBindings` を upstream 形の `Expr(:let, bindings, body)` にし、`IfStatement`
は branch body を `Expr(:block, ...)` で包んだ `Expr(:if, cond, then[, else])` へ正規化する。
`elseif` tail は upstream と同じ `Expr(:elseif, Expr(:block, cond), then[, else])` にし、
macro-return lowering 側でも `ExprHead::ElseIf` を statement / expression tail として扱えるようにした。
roundtrip gate へ let / if-else / if-elseif-else の eval・macro-return seed を追加した。fixture:
`metaprogramming/metaparse_let_else_macro_return_7754.jl`。

### Runtime macro-return converter: named tuple & value-in-statement ✅ (Issues #7765 / #7764)

runtime macro-expansion 経由 (`subset_julia_vm/src/lowering/macro_runtime.rs`) の
value→IR 変換を修正した。#7765: `expr_value_to_expr` の `ExprHead::Tuple` arm が
`tuple_expr_from_values` 経由で named-tuple 形を検出し、全要素が `Expr(:(=), :name, value)`
形なら `NamedTupleLiteral` を生成する。これで `@timed` 風 NamedTuple のフィールドアクセスが
通る (mixed/plain tuple は従来どおり `TupleLiteral`)。#7764: `expand_macro_to_stmt` で
macro が返した outermost block を `value_to_branch_expr` 経由の value-preserving な
`Stmt::Expr` に変換し、top-level の `@show f(3)` / `@time result = f(10)` が最終値を保つ。
再帰的な `value_to_stmt` の block arm は据え置き (nested の statement-only 末尾を壊さない)。
fixture: `macro/macro_named_tuple_return_7765.jl`、`macro/macro_value_in_statement_position_7764.jl`。

検証: upstream `julia` で両 fixture を確認、`cargo build --release --bin sjulia --features repl`、
`timeout 1800 cargo nextest run --release --test fixture_tests macro:: macrotools:: timing:: tuple:: macros:: metaprogramming:: do_block::`、
`timeout 1800 cargo nextest run --release --test integration_array_tests --test integration_string_type_tests`、
`timeout 1800 cargo nextest run --release --lib`、
`cargo clippy -p subset_julia_vm --all-targets -- -D warnings`、
`bash scripts/check_fixture_test_names.sh`。

### 深いネスト closure からの global/const/builtin 参照 ✅ (Issue #7600)

深さ 2 以上の do-block / arrow lambda から `pi`・ユーザ `const K`・非 const global `G`
を参照すると `UndefVarError: Cannot capture undefined variable: <name>` で失敗していたバグを
修正した。(1) `Instr::CreateClosure` で capture 名が現 frame に無い場合に global frame
(frame 0) へ fallback、(2) flat に lift される `__lambda_N` の親子関係を参照から復元し、
外側 do-block の param/local を内側 lambda へ bottom-up に capture 伝播。
fixture: `closures/nested_closure_global_capture_7600.jl`。

検証: upstream Julia と sjulia でケース E/F/D/G/H・π・3 段ネストの出力一致を確認、
`bash scripts/fixture_julia_parity.sh ...` で 7 passed 一致、
`cargo nextest run --release --test fixture_tests closures:: do_block:: hof:: comprehension:: iteration:: iterators:: modules::`、
`cargo clippy -p subset_julia_vm --all-targets`(変更ファイルに warning 無し)。

### Expr head registry for quote/macro/eval dispatch ✅ (Issue #7719)

quoted AST construction、macro-return lowering、runtime `eval` の `Expr` head
dispatch を `ExprHead` enum と coverage registry に集約した。registry は
CST→`Expr` value、macro return→statement/expression、runtime eval の対応範囲を明示し、
dispatcher 側の support set と debug assert で同期する。既存の `Expr(:try)` /
`Expr(:parameters)` regression を維持しつつ、#7696 / #7676 の bug tail を fixture で固定した。
file: `subset_julia_vm/src/expr_heads.rs`。

検証: `cargo fmt --check`、
`cargo build --release --bin sjulia --features repl`、
`timeout 1800 cargo nextest run --release -p subset_julia_vm expr_head`、
`timeout 1800 cargo nextest run --release --test fixture_tests metaprogramming::`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### Expr printing keeps QuoteNode Symbol syntax ✅ (Issue #7696)

`Expr` source printing は `QuoteNode(:a)` を `:a` syntax として出力する。
`string(:(Dict(:a => 1)))` と `sprint(print, :(Dict(:a => 1)))` は upstream と同じ
`Dict(:a => 1)` になり、非 Symbol payload は expression context で
`$(QuoteNode(...))` として保持する。fixture:
`metaprogramming/expr_quotenode_symbol_print_7696.jl`。

検証: upstream Julia fixture、
`./target/release/sjulia subset_julia_vm/tests/fixtures/metaprogramming/expr_quotenode_symbol_print_7696.jl`、
`bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/metaprogramming/expr_quotenode_symbol_print_7696.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests metaprogramming::`。

### var-string identifiers pass to macros as Symbols ✅ (Issue #7676)

macro argument context の `var"@q"` / `var"@qq"` は string literal ではなく
`Symbol("@q")` / `Symbol("@qq")` として quoted AST に入る。`Expr` source printing も
invalid identifier symbols を `var"..."` form へ戻す。fixture:
`metaprogramming/var_string_macro_arg_symbols_7676.jl`。

検証: upstream Julia fixture、
`./target/release/sjulia subset_julia_vm/tests/fixtures/metaprogramming/var_string_macro_arg_symbols_7676.jl`、
`bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/metaprogramming/var_string_macro_arg_symbols_7676.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests metaprogramming::`。

### eval'd Expr(:try) with side-effecting catch/finally no longer StoreSlot OOB ✅ (Issue #7687)

`eval(:(try error() catch; push!(log, :caught); 123 finally push!(log, :finally) end))`
が `InternalError: StoreSlot: slot out of bounds: 63` で失敗していた問題を修正
（upstream julia は `123` と `Any[:caught, :finally]`）。eval 駆動の dispatch では
try body の raise が bytecode 例外ハンドラを通らず Rust `Err` として伝播するため、
失敗した callee の frame・operand stack・return address・try ハンドラが残置され、
後続の catch/finally body の `StoreSlot`（`push!`/`x = 1`）が stale な callee frame の
slot table を書いていた。`eval_dispatch_call` / `eval_dispatch_call_with_kwargs` の
error path で frame/stack/return_ips/handlers の深さを dispatch 前 snapshot まで
巻き戻すよう修正。
fixture: `metaprogramming/eval_expr_try_storeslot_7687.jl`。

検証: upstream Julia と sjulia の出力一致確認（MWE + 多変種）、
`cargo build --release --bin sjulia --features repl`、
`timeout 1800 cargo nextest run --release --test fixture_tests metaprogramming::`（chunk_000 PASS、
my fixture を含む）、`timeout 1800 cargo nextest run --release --lib eval`（eval unit tests PASS）、
`bash scripts/check_fixture_test_names.sh`、`cargo clippy -p subset_julia_vm`（eval.rs に新規 warning なし）。
（chunk_001 の 7029 / free_vars unit test の失敗は #7029 / #7618-7619 の pre-existing で本件と無関係）
### Distributions univariate sampler API ✅ (Issue #7323)

Bundled `Distributions` の既存 univariate 分布(連続8種・離散6種)で、
明示RNG scalar sampling、vector/matrix sampling、in-place `rand!`、`sampler(d)` を
使えるようにした。`rand` builtin の dimension fallback と衝突しないよう、
具体分布 wrapper と `_rand_scalar` helper 経由で package methods に逃がす。
fixture: `distributions/distributions_sampler_api_7323.jl`。

検証: `cargo build --release --bin sjulia --features repl`、
`julia --project=/tmp/sjulia_distributions_check subset_julia_vm/tests/fixtures/distributions/distributions_sampler_api_7323.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/distributions/distributions_sampler_api_7323.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/distributions/distributions_sampling.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/distributions/distributions_discrete_sampling.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests distributions::chunk_000 --no-fail-fast`。

### RNG values share mutable state across user calls ✅ (Issue #7751)

`RngInstance` を shared mutable handle にし、RNG を user function に渡しても
caller-visible state が進むようにした。これにより Distributions の
`rand(rng, d)` / `rand(rng, d, n)` でも同じ RNG object の stream が継続する。
fixture: `stdlib/random_rng_user_function_state_7751.jl`。

検証: upstream Julia `stdlib/random_rng_user_function_state_7751.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/stdlib/random_rng_user_function_state_7751.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests stdlib::chunk_000 --no-fail-fast`。

### Clippy all-targets gate is clean again ✅ (Issue #7623)

`cargo clippy --all-targets -- -D warnings` の失敗要因だった MacroTools WIP 周辺の
mechanical lint を修正した。対象は redundant closure call、`Return` statement の
collapsible match、`esc(...)` / keyword argument conversion の collapsible if、
Dict slot flag の sign-loss cast、destructuring/lambda lowering の needless borrow。

検証: `cargo fmt --all`、
`cargo clippy --all-targets -- -D warnings`。

### Pure-Julia Dict values dispatch to bare Dict annotations ✅ (Issue #7632)

Pure-Julia `Dict{K,V}` StructRef を bare `::Dict` annotation から除外しないよう、
runtime dispatch の Dict carrier mismatch guard を no-op stub にした。`Value::Dict`
carrier removal 後は bare `Dict` が upstream と同じ `Dict{K,V}` family として振る舞うため、
`f(d::Dict)` と `Any[d][1]` 経由の calls が concrete `Dict{K,V}` に dispatch できる。
MacroTools `combinedef` は workaround を外し、upstream と同じ
`combinedef(dict::Dict)` signature へ戻した。
fixtures: `dict/bare_dict_annotation_dispatch_7632.jl`,
`macrotools/combinedef_bare_dict_dispatch_7632.jl`,
`macrotools/upstream_split.jl`。

検証: upstream Julia `dict/bare_dict_annotation_dispatch_7632.jl`、
`macrotools/combinedef_bare_dict_dispatch_7632.jl`、bundled MacroTools upstream `split.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/dict/bare_dict_annotation_dispatch_7632.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/combinedef_bare_dict_dispatch_7632.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/upstream_split.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests dict:: macrotools::`。

### Any-typed GlobalRef field access uses VM GlobalRef projection ✅ (Issue #7743)

`GlobalRef` が `Expr.args` や `Any[...]` 経由で compile-time `Any` になっても、
runtime `GetFieldByName("mod" | "name")` が専用 `GlobalRef` projection を使うようにした。
`mod` は `Module` value、`name` は `Symbol` を返すため、MacroTools `rmdocs` は
upstream predicate `m.mod == Core && m.name == Symbol("@doc")` へ戻せる。
fixture: `macros/globalref_any_field_7743.jl`。

検証: upstream Julia `macros/globalref_any_field_7743.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/globalref_any_field_7743.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/upstream_utils.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros:: macrotools::`。

### Partial parametric constructor calls ✅ (Issue #7734)

`M{2,2}(args...)` のように parametric callable 側で一部または全部の
`where` parameter を指定する constructor method を compile できるようにした。
compiler は `M{A,B}(...) where {A,B}` 型の method table を検出し、callee frame に
`A=2` / `B=2` などの static callable parameter を明示 bind してから constructor body を
実行する。これにより constructor body 内の validation / conversion / tuple wrapping を
default field constructor でバイパスしない。StaticArrays / StaticArraysCore には
`SVector{N}(...)`、`SVector{N,T}(...)`、`SMatrix{M,N}(...)`、
`SMatrix{M,N,T}(...)` の pure-Julia constructor methods を追加した。
fixture: `types/partial_parametric_constructors_7734.jl`、
`static_arrays/static_arrays_constructors_7459.jl`。

検証: `cargo fmt --check`、
`cargo build --release --bin sjulia --features repl`、
`julia subset_julia_vm/tests/fixtures/types/partial_parametric_constructors_7734.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/types/partial_parametric_constructors_7734.jl`、
`JULIA_LOAD_PATH="$(pwd)/subset_julia_vm/packages:@stdlib" julia subset_julia_vm/tests/fixtures/static_arrays/static_arrays_constructors_7459.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/static_arrays/static_arrays_constructors_7459.jl`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests types_tests::`、
`timeout 1800 cargo nextest run --release --test fixture_tests static_arrays::`、
`timeout 1800 cargo nextest run --release --lib packages`。

### Quoted docstrings lower to Core.@doc inside quote blocks ✅ (Issue #7712)

`quote ... end` 内の newline docstring を upstream Julia と同じ
`Expr(:macrocall, GlobalRef(Core, :@doc), line, doc, stmt)` に畳むようにした。
これにより MacroTools `stripdocs` が block 内 standalone docstring を
`rmdocs` 経由で除去できる。semicolon-separated `quote; "doc"; stmt; end` は
ordinary string statement のまま残す regression も fixture で固定した。
fixtures: `macros/quoted_docstring_core_doc_7712.jl`, `macrotools/upstream_utils.jl`。

検証: upstream Julia `macros/quoted_docstring_core_doc_7712.jl`、
bundled MacroTools upstream `utils.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quoted_docstring_core_doc_7712.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/upstream_utils.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros:: macrotools::`。

### Quoted bare where clauses keep the where parameter ✅ (Issue #7714)

`:(f(a::T) where T)` の quote constructor が bare `T` を落とさず、
upstream Julia と同じ `Expr(:where, :(f(a::T)), :T)` を構築するようにした。
container を持たない where parameter leaf は `where_clause_args` で直接
`where_param_constructor` に渡し、MacroTools `gatherwheres` の bare-where
parameter collection も fixture で固定した。MacroTools 側は tuple literal splat
gap (#7741) を避けるため、where-parameter concatenation を `tuple(...)` call に通す。
fixtures: `macros/quote_where_expr_7553.jl`,
`macros/quote_where_expression_7553.jl`, `macrotools/upstream_utils.jl`。

検証: upstream Julia `macros/quote_where_expr_7553.jl`、
`macros/quote_where_expression_7553.jl`、bundled MacroTools upstream `utils.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia -e 'w = :(f(a::T) where T); wb = :(f(a::T) where {T}); println(length(w.args)); println(w.args[2]); println(length(wb.args)); true'`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quote_where_expr_7553.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quote_where_expression_7553.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/upstream_utils.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros:: macrotools::`。

### MacroTools.prettify resolves interpolated Function values ✅ (Issue #7711)

`prettify(:($sin(2)))` / `prettify(:($cos(x)))` が upstream MacroTools と同じ
`:(sin(2))` / `:(cos(x))` へ正規化されるよう、`unresolve1(::Function) = nameof(f)`
を追加し、sjulia の higher-order generic-function dispatch gap を避けるため
`unresolve` の `prewalk` lambda に `Function` leaf branch を inline した。
fixture: `macrotools/upstream_utils.jl`。

検証: bundled MacroTools fixture の upstream Julia 実行、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia -e 'using MacroTools; println(MacroTools.prettify(:($sin(2))) == :(sin(2))); println(MacroTools.prettify(:($cos(x))) == :(cos(x))); true'`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/upstream_utils.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### Value type parameter arithmetic in method bodies ✅ (Issue #7736)

`SM{M,N,T}` のような user-defined parametric struct method body で、`N` を
`(i - 1) * N + j` や `N == n` の value context に使う形を compile できるようにした。
binary operand に bare `where` type parameter が出る場合は runtime dispatch に委譲し、
callee frame に bind 済みの integer value parameter を実行時に読む。これにより
StaticArrays の `SMatrix{M,N,T}` indexing は `size(x)[2]` workaround ではなく直接 `N`
を使う形へ戻した。fixture: `types/value_type_param_arithmetic_7736.jl`。

検証: `cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`julia subset_julia_vm/tests/fixtures/types/value_type_param_arithmetic_7736.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/types/value_type_param_arithmetic_7736.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'struct SM{M,N,T}; data::Tuple; end; function Base.getindex(x::SM{M,N,T}, i::Int64, j::Int64) where {M,N,T}; return x.data[(i - 1) * N + j]; end; println(getindex(SM{2,2,Int64}((1,2,3,4)), 2, 1))'`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/static_arrays/static_arrays_constructors_7459.jl`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`timeout 1800 cargo nextest run --release --lib compile::expr::binary`、
`timeout 1800 cargo nextest run --release --lib packages`、
`timeout 1800 cargo nextest run --release --test fixture_tests static_arrays::`、
`timeout 1800 cargo nextest run --release --test fixture_tests types_tests::`。

### Module-qualified inner constructors and escaped Base-extension calls ✅ (Issue #7631)

`M.S()` が module-qualified syntax で inner constructor を呼ぶ場合、qualified method table が
無くても short-name method table へ fallback し、`Plots.Animation()` などの package
constructors が動くようにした。macro hygiene の module member set から `Base` extension
methods を除外し、`esc` された caller loop body の `push!(ps, p)` が `Plots.push!` ではなく
caller scope の配列 `push!` に dispatch する。
fixture: `module/module_qualified_inner_constructor_7631.jl`。

検証: upstream Julia `module_qualified_inner_constructor_7631.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/module/module_qualified_inner_constructor_7631.jl`、
`timeout 1800 cargo nextest run --release -p subset_julia_vm --lib repl::tests::test_repl_gif_with_global_accumulator_7151`。

### Assignment free vars respect same-statement local shadowing ✅ (Issue #7685)

`Stmt::Assign` / `Expr::AssignExpr` の free-var analysis で、関数 hard scope 内の単純 target を
事前 local set へ登録し、`x = rhs` の RHS や do-block local assignment が outer/global 同名
binding を誤 capture しないようにした。closure boxing の assigned-capture 判定も同じ
hard-scope local を除外し、`board(...) do ...; v = view3d(...); push!(v, ...); end` が
top-level `v` と同じ capture box に rewrite されないようにした。

検証: `timeout 1800 cargo nextest run --release -p subset_julia_vm --lib compile::free_vars::tests::test_same_name_assign_rhs_shadows_outer_var compile::free_vars::tests::test_local_assign_shadows_outer_var compile::free_vars::tests::test_rhs_evaluated_before_assign`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/packages/packages_jsxgraph_view3d_7373.jl`。

### Symbolics sqrt dispatch for Any-typed Num values ✅ (Issue #7702)

`sqrt(x)` の argument が compile-time `Any` でも、runtime 値が `Symbolics.Num` なら
`Symbolics.sqrt(::Num)` を選べるよう、`CallTypedDispatchOrBuiltin(Sqrt, "sqrt", ...)` を
emit する。`BuiltinId::Sqrt` は method candidate が無い primitive numeric の fallback 専用で、
public `sqrt` は引き続き dispatch-first route のままにした。

検証: `./target/release/sjulia --dump-bytecode -e 'using Symbolics; @variables x; operation(value(sqrt(x))) === :sqrt'`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/packages/symbolics_arithmetic.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests packages:: statsplots::`。

### Stdlib macro loading skips in-progress self imports ✅ (Issue #7735)

`using LinearAlgebra` の lowering 中に、early stdlib macro registration が
`LinearAlgebra.LAPACK` 内の `import ..LinearAlgebra: inv, lu, LU` から同じ
`LinearAlgebra` の macro scan へ再入して stack overflow しないようにした。
fixture: `stdlib_loader::tests::test_ensure_stdlib_macros_loaded_linear_algebra_no_recursion_7735`。

検証: `./target/release/sjulia -e 'using LinearAlgebra; true'`、
`timeout 1800 cargo nextest run --release -p subset_julia_vm --lib stdlib_loader::tests::test_ensure_stdlib_macros_loaded_linear_algebra_no_recursion_7735`。

### StaticArrays constructors and @SVector literal macro ✅ (Issue #7459)

`StaticArraysCore` / `StaticArrays` の Phase 3 対応として、`SVector(...)`、
fully-applied tuple constructors、`@SVector [1, 2, 3]`、`Tuple`、および最小
`getindex` を pure Julia package 側に追加した。`using StaticArrays` seed fixture は
この tranche で有効化した。当初 defer した `@SMatrix [1 2; 3 4]` /
`@SArray [1 2; 3 4]` は #7733 で有効化し、`SMatrix{2,2}(...)` 型の
partial parametric constructor は #7734 で有効化済み。
fixture: `static_arrays/static_arrays_constructors_7459.jl`。

検証: `cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`JULIA_LOAD_PATH="$(pwd)/subset_julia_vm/packages:@stdlib" julia subset_julia_vm/tests/fixtures/static_arrays/static_arrays_constructors_7459.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/static_arrays/static_arrays_constructors_7459.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/static_arrays/using_staticarrays_7456.jl`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`timeout 1800 cargo nextest run --release --lib packages`、
`timeout 1800 cargo nextest run --release --test fixture_tests static_arrays::`。

### Quoted matrix literal macro arguments and StaticArrays matrix macros ✅ (Issue #7733)

macro 引数の `[1 2; 3 4]` は upstream Julia と同じ
`Expr(:vcat, Expr(:row, 1, 2), Expr(:row, 3, 4))` として macro に渡るようになっている。
この shape を直接 fixture で固定し、StaticArraysCore / StaticArrays の pure-Julia
`@SMatrix` と matrix-form `@SArray` から #7733 blocker error を外した。両 macro は
quoted matrix AST から `(M,N,args)` を取り出し、現在の MVP row-major tuple layout に合わせて
`SMatrix{M,N}(args...)` へ展開する。fixture:
`macros/macro_arg_matrix_literal_7733.jl`、
`static_arrays/static_arrays_matrix_literal_macros_7733.jl`。

### StaticArraysCore static type and trait foundation ✅ (Issue #7458)

`StaticArray` / `StaticVector` / `StaticMatrix` / `StaticVecOrMat` / `StaticScalar`
と、tuple-backed `SArray` / `SVector` / `SMatrix` の Phase 2 基礎を pure Julia package
側に追加した。`SVector(1,2,3)`、`SVector{3,Int64}((1,2,3))`、
`SMatrix{2,2,Int64}((1,2,3,4))` が構築でき、`Size` / `Length` / `size` / `length` /
`eltype` / `ndims` / `Tuple` / tuple-size utility を static_arrays fixture で固定した。
`StaticArray{S,T,N}` の upstream 形 `AbstractArray{T,N}` は sjulia の既存 subtype gap
(#7728) に当たるため、`AbstractArray{Any,N}` parent と明示 `eltype` に留めた。
fixture: `static_arrays/static_arrays_core_basics_7458.jl`。

検証: `cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`JULIA_LOAD_PATH="$(pwd)/subset_julia_vm/packages:@stdlib" julia subset_julia_vm/tests/fixtures/static_arrays/static_arrays_core_basics_7458.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/static_arrays/static_arrays_core_basics_7458.jl`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests static_arrays::`。

### StaticArrays package skeleton loads through @packages ✅ (Issue #7457)

`StaticArraysCore` / `StaticArrays` / `PrecompileTools` を bundled package として追加し、
`using StaticArrays` と `using StaticArraysCore` が default `@stdlib:@packages` load path から
解決できるようにした。各 package source は `include(...)` 構造を保ったまま
`packages/mod.rs` に個別登録し、loader cache hash が included source を見る経路も固定した。
`PrecompileTools` は sjulia が package precompile hook を実行しないため、pure-Julia no-op
macro shim として提供する。`SVector` constructor / `@SMatrix` / indexing などの seed API は
後続 phase の対象なので Phase 0 fixture は skip のまま、ロード専用 fixture を追加した。
fixture: `static_arrays/package_load_7457.jl`。

検証: `cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'using StaticArrays; println("loaded")'`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'using StaticArraysCore; println("core loaded")'`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/static_arrays/package_load_7457.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --lib loader packages`、
`timeout 1800 cargo nextest run --release --test fixture_tests static_arrays::`。

### StaticArrays upstream audit and seed baseline ✅ (Issue #7456)

StaticArrays.jl 1.9.18 / StaticArraysCore を参照し、MVP 対象ファイル、
dependency 方針、明示 deferral、Phase 1-5 の対応表を `STATICARRAYS.md` に整理した。
`static_arrays/` に `using StaticArrays`、`SVector`、`@SMatrix`、`Size`、indexing/shape の
seed baseline fixture を追加した。Phase 0 では未 bundle が期待状態なので fixtures は
`skip = true` とし、Phase 1 以降で有効化する。
fixtures: `static_arrays/using_staticarrays_7456.jl`,
`static_arrays/seed_api_baseline_7456.jl`。

検証: upstream Julia with StaticArrays 1.9.18 for both static_arrays fixtures、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/static_arrays/using_staticarrays_7456.jl`
が `Unknown package: StaticArrays` で失敗すること、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests static_arrays:: --no-tests=pass`。

### MacroTools core matcher works across module-local imports ✅ (Issue #7451)

`module M; using MacroTools: @capture; ... @capture(...) ... end` のような module-local
selective import を lowering context に反映し、別 module から imported `@capture` を使えるようにした。
同じ regression fixture で top-level `using MacroTools: @match` の matcher branch も固定した。
`macrotools::` は upstream `match.jl` smoke を含めて通る。
fixture: `macrotools/selective_import_module_capture_7451.jl`。

検証: upstream Julia `macrotools/selective_import_module_capture_7451.jl` with bundled package load path、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/selective_import_module_capture_7451.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### MacroTools AST and macro-expansion substrate is covered ✅ (Issue #7450)

MacroTools が要求する `Expr` / `QuoteNode` / `LineNumberNode` / `GlobalRef`、
quoted `Expr(:block, ...)` / `Expr(:try, ...)` / `Expr(:macrocall, ...)`、
`Base.isexpr` / `Meta.isexpr`、`@q` / macro-definition 内 `@qq` の基礎動作を
既存 regression fixture 群で固定した。`macrotools::` fixture は bundled MacroTools の
upstream `match` / `split` / `destruct` / `utils` / `flatten_try` smoke を含めて通る。
残る full upstream parity gap は #7634/#7647 に分離済み。
fixtures: `macros/*7450*.jl`, `metaprogramming/*`, `reflection/base_isexpr_qualified_7527.jl`,
`macrotools/upstream_*.jl`。

検証: upstream Julia bundled MacroTools smoke for macro-definition `@qq`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'using MacroTools; macro m(ex); MacroTools.@qq begin value = $ex; value end end; println(@m(1 + 2)); true'`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### @__DIR__ works as a call argument ✅ (Issue #7494)

`joinpath(@__DIR__, "file_dir_macros.jl")` のように source-location macro を
別 call の argument position で使う形が parse/lower できることを既存 regression fixture で固定した。
MacroTools の `joinpath(@__DIR__, "..", "animals.txt")` 型の package data path construction を塞がない。
fixture: `macro/file_dir_macros.jl`。

検証: upstream Julia `macro/file_dir_macros.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macro/file_dir_macros.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macro_tests::`。

### Unicode ≤ and ≥ comparisons lower as standard comparisons ✅ (Issue #7500)

`2 ≥ 1` / `1 ≤ 2` が `UnsupportedOperator` にならず、ASCII `>=` / `<=`
と同じ `BinaryOp::Ge` / `BinaryOp::Le` として lower/execution されることを既存 regression fixture で固定した。
MacroTools matcher sources の Unicode comparison operator を書き換えずに扱える。
fixture: `operators/unicode_comparison_7500.jl`。

検証: upstream Julia `operators/unicode_comparison_7500.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/operators/unicode_comparison_7500.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests operators::`。

### Ternary branch macro calls expand with context ✅ (Issue #7503)

`true ? @m() : false` / `false ? @m() : false` のような ternary branch
position の macro call が active macro context で展開されることを既存 regression fixture で固定した。
MacroTools `isslurp(p) ? @trymatch(...) : @nomatch(...)` 型の matcher branch を塞がない。
fixture: `macros/ternary_macro_branch_7503.jl`。

検証: upstream Julia `macros/ternary_macro_branch_7503.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/ternary_macro_branch_7503.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Unary operands keep macro context for ternary branches ✅ (Issue #7505)

`!(false ? @m() : false)` のように unary expression の operand 配下にある
ternary branch macro call が active macro context で展開されることを regression fixture に追加した。
MacroTools validation 中の nested expression-position macro expansion を塞がない。
fixture: `macros/unary_ternary_macro_context_7505.jl`。

検証: upstream Julia `macros/unary_ternary_macro_context_7505.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/unary_ternary_macro_context_7505.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Nested quote interpolation in call arguments parses ✅ (Issue #7507)

`Expr(:$, :($TypeBind($(Expr(:quote, name)), Set{Any}([$(ts...)]))))` のような
nested quote interpolation が parse error にならず、call argument と nested vector splat
interpolation を含む Expr tree を構築できることを regression fixture に追加した。
MacroTools TypeBind matcher construction を塞がない。
fixture: `macros/nested_quote_call_interpolation_7507.jl`。

検証: upstream Julia `macros/nested_quote_call_interpolation_7507.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/nested_quote_call_interpolation_7507.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Short-form function bodies keep macro context ✅ (Issue #7509)

`f(x) = x ? true : @m()` のような short-form function body 内の expression-position
macro call が active macro context で展開されることを既存 regression fixture で固定した。
MacroTools `match_inner(...)= ... : @nomatch(...)` 型の short-form helper を塞がない。
fixture: `macros/short_form_function_macro_body_7509.jl`。

検証: upstream Julia `macros/short_form_function_macro_body_7509.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/short_form_function_macro_body_7509.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### var"name" identifiers passed to macros are Symbols, not Strings ✅ (Issue #7676)

macro 引数 tuple に渡した `var"@q"` が `String` ではなく `Symbol` として AST に
積まれるよう quote lowering (`cst_to_expr_constructor` の `prefixed_string_literal`
`var` arm) を修正。Julia-source 整形 (`format_symbol_name`) も valid-identifier でも
operator でもない symbol を `var"name"` で出力するようにし、`string(ex)` が
`(var"@q", var"@qq", postwalk)` と upstream 一致。
fixture: `macros/var_string_macro_arg_7676.jl`、
unit: `vm::formatting::tests::test_format_symbol_name_var_string_issue_7676`。

検証: upstream Julia `macros/var_string_macro_arg_7676.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/var_string_macro_arg_7676.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Quoted semicolon blocks construct Expr(:block) ✅ (Issue #7511)

`:($line;$yes)` が parse error にならず、interpolated values を含む `Expr(:block, ...)`
として構築されることを regression fixture に追加した。MacroTools match macro の
quoted block construction を塞がない。
fixture: `macros/quoted_semicolon_block_7511.jl`。

検証: upstream Julia `macros/quoted_semicolon_block_7511.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quoted_semicolon_block_7511.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Quoted let expressions construct Expr(:let) ✅ (Issue #7512)

`:(let x = 1; x end)` が quote lowering で拒否されず、upstream Julia と同じ
`Expr(:let, binding, body)` shape になることを regression fixture に追加した。
MacroTools match macro の quoted `let` construction を塞がない。
fixture: `macros/quoted_let_expression_7512.jl`。

検証: upstream Julia `macros/quoted_let_expression_7512.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quoted_let_expression_7512.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Prime-suffixed identifiers work in ternary branches ✅ (Issue #7513)

`s′` のような prime-suffixed identifier が ternary branch position で parse/lower できることを
regression fixture に追加した。MacroTools `replace(ex, s, s′)` 型の helper 定義を塞がない。
fixture: `ternary/prime_identifier_7513.jl`。

検証: upstream Julia `ternary/prime_identifier_7513.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/ternary/prime_identifier_7513.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests ternary::`。

### 子モジュールの無修飾メソッドが親モジュールの型付きメソッドに負けるのを修正 ✅ (Issue #7575)

子モジュール内の無修飾呼び出しが、自前で定義した generic 関数を共有 bare プールではなく
module-qualified table へ解決するようにした (`A.B.g(1)` が `:outer` ではなく `:inner` を返す)。
`using`/`import` 取り込み名・Base 拡張・builtin forward wrapper は除外し、#7468 LinearAlgebra
ディスパッチを壊さない。regression fixture `modules/module_child_method_shadows_parent_7575.jl` を追加。

### Quoted single tuple interpolation parses ✅ (Issue #7514)

`:($arg,)` が parse error にならず、upstream Julia と同じ single-element
`Expr(:tuple, value)` として構築されることを regression fixture に追加した。
MacroTools `longdef1` の `:($arg,)` signature construction を塞がない。
fixture: `macros/quoted_single_tuple_interpolation_7514.jl`。

検証: upstream Julia `macros/quoted_single_tuple_interpolation_7514.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quoted_single_tuple_interpolation_7514.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### eval handles Expr(:try) basics ✅ (Issue #7683)

`eval(:(try ... catch ... finally ... end))` が `unsupported Expr head 'try'` にならないよう、
eval interpreter に `Expr(:try, try_block, catch_var_or_false, catch_block_or_false[, finally])`
の基本実行を追加した。catch return と finally 実行後の try value 維持を
regression fixture で固定した。catch/finally 内の外側配列 mutation は #7687 に分離。
fixture: `metaprogramming/eval_expr_try_7683.jl`。

検証: upstream Julia `metaprogramming/eval_expr_try_7683.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/metaprogramming/eval_expr_try_7683.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests metaprogramming::chunk_000`。

### eval Expr(:try) preserves else branch values ✅ (Issue #7727)

Quoted `try ... catch ... else ... finally ... end` は upstream Julia と同じ
`Expr(:try, try_block, catch_var_or_false, catch_block_or_false, finally_or_false, else_block)`
shape を構築する。`eval` は try body が例外を投げなかった場合に else block を評価して返し、
例外時は catch value を返す。これにより MacroTools upstream `flatten_try.jl` の
`eval(flatten(... else ...))` cases が restored fixture として通る。
fixture: `metaprogramming/eval_expr_try_else_7727.jl`, `macrotools/upstream/flatten_try.jl`。

検証: upstream Julia MWE for `Expr(:try)` else value、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/metaprogramming/eval_expr_try_else_7727.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/upstream_flatten_try.jl`。

### eval catch path unwinds nested frames before caller stores ✅ (Issue #7730)

`eval(:(try error() catch; value ... end))` が catch value を返した後、nested dispatch の
callee frame を target depth まで巻き戻す。これにより `x = eval(...)` や `@test eval(...) == ...`
の caller-side `StoreSlot` が callee frame 上で実行される slot bounds regression を防ぐ。
fixture: `metaprogramming/eval_expr_try_else_7727.jl`, `macrotools/upstream/flatten_try.jl`。

検証: `./target/release/sjulia -e 'x = eval(:(try error() catch; 123 else 234 finally end)); println(x)'`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/metaprogramming/eval_expr_try_else_7727.jl`。

### Quoted function-definition Pair expressions parse ✅ (Issue #7517)

`:(begin function f_(args__) body_ end => rhs end)` が parse error にならず、
`Expr(:call, :=>, Expr(:function, ...), :rhs)` として構築されることを regression fixture に追加した。
MacroTools `@match` clause の function-definition Pair pattern を塞がない。
fixture: `macros/quoted_function_pair_7517.jl`。

検証: upstream Julia `macros/quoted_function_pair_7517.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quoted_function_pair_7517.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Anonymous block function expressions are callable values ✅ (Issue #7518)

`function (x) ... end` を expression position で拒否せず、匿名名つき nested
`FunctionDef` と function value へ lower するようにした。代入、直接呼び出し、
outer variable capture を regression fixture で固定し、MacroTools の
value-position anonymous function pattern を塞がない。
fixture: `functions/anonymous_function_expression_7518.jl`。

検証: upstream Julia `functions/anonymous_function_expression_7518.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/functions/anonymous_function_expression_7518.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests functions::`。

### Quoted parenthesized operator function heads parse ✅ (Issue #7519)

`:(function (fcall_ | fcall_) body_ end)` が parse error にならず、
operator function signature `Expr(:call, :|, :fcall_, :fcall_)` として構築されることを
regression fixture に追加した。MacroTools capture pattern の operator function head を塞がない。
fixture: `macros/quoted_operator_function_head_7519.jl`。

検証: upstream Julia `macros/quoted_operator_function_head_7519.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quoted_operator_function_head_7519.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Quoted function definitions interpolate names ✅ (Issue #7520)

`:(function $fname(x) ... end)` が parse error にならず、interpolated function name を
signature の callee に splice できることを regression fixture に追加した。
MacroTools `combinedef` 形に合わせ、`$fname($(args...); $(kwargs...))` の
parameter splat 併用も確認する。
fixture: `macros/quoted_function_name_interpolation_7520.jl`。

検証: upstream Julia `macros/quoted_function_name_interpolation_7520.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quoted_function_name_interpolation_7520.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Quoted function definitions splice parameter lists ✅ (Issue #7522)

`:(function f($(args...)) ... end)` と
`:(function g($(args...); $(kwargs...)) ... end)` が parse error にならず、
interpolated positional/keyword parameter list を function signature の
`Expr(:call, ...)` に splice できることを regression fixture に追加した。
MacroTools `combinedef` の quoted function reconstruction を塞がない。
fixture: `macros/quoted_function_param_splat_7522.jl`。

検証: upstream Julia `macros/quoted_function_param_splat_7522.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quoted_function_param_splat_7522.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Quoted interpolated field assignments parse ✅ (Issue #7523)

`:($x.$f += $v)` / `:($x.$f = $v)` のように receiver と field name の両方を
interpolation した quoted field assignment が、parse error にならず
`Expr(:+=, Expr(:., obj, QuoteNode(field)), ...)` /
`Expr(:(=), Expr(:., obj, QuoteNode(field)), ...)` として構築されるようにした。
MacroTools `resyntax` の field rewrite branch を塞がない。
fixture: `macros/quoted_interpolated_field_7523.jl`。

検証: upstream Julia `macros/quoted_interpolated_field_7523.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quoted_interpolated_field_7523.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Whitespace macro comma arguments stay one tuple arg ✅ (Issue #7526)

`@m alpha, beta` のような whitespace macro call が、macro varargs 2個ではなく
upstream Julia と同じ `Expr(:tuple, :alpha, :beta)` 1引数として渡ることを
regression fixture に追加した。MacroTools `@public a, b, ...` 型の
single-argument macro を `macro @m not found (with 2 args)` に落とさない。
`var"@q"` identifier AST parity は別 issue #7676 として分離した。
fixture: `macros/whitespace_macro_tuple_7526.jl`。

検証: upstream Julia `macros/whitespace_macro_tuple_7526.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/whitespace_macro_tuple_7526.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### VersionNumber comparison operators are covered ✅ (Issue #7529)

`VersionNumber` 同士の `<` / `<=` / `>` / `>=` と、MacroTools `@public` が使う
`VERSION >= v"..."` 形を regression fixture に追加した。既存の pure-Julia
comparison methods が `MethodError` にならず、major/minor/patch の順に比較される。
fixture: `version/version_comparison.jl`。

検証: upstream Julia `version/version_comparison.jl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/version/version_comparison.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests version::`。

### MacroTools TypeBind splats Set head patterns ✅ (Issue #7670)

`Set{Any}([:call])...` を pure-Julia `Set{T}` wrapper の backing Dict keys として
splat 展開できるようにした。これにより MacroTools `TypeBind` の `b.ts...` が
`isexpr(ex, :call)` 相当へ展開され、Expr head match 成功時に capture env へ対象 Expr を
束縛できる。
fixture: `macrotools/typebind_set_splat_7670.jl`。

検証: upstream Julia `typebind_set_splat_7670.jl`、
`cargo check -p subset_julia_vm --bin sjulia --features repl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/typebind_set_splat_7670.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`、
`timeout 1800 cargo nextest run --release --test fixture_tests sets::`。

### MacroTools upstream utils fixture restored ✅ (Issue #7647)

MacroTools upstream `test/utils.jl` v0.5.16 の checks を restored fixture に戻した。
bare function values now preserve resolved method candidates, so
`map(flatten, ex.args)` dispatches to `MacroTools.flatten` instead of
`Base.Iterators.flatten`. Quoted assignment lowering preserves `where` LHS
expressions, macro-expanded blocks propagate `LineNumberNode` spans into nested
function definitions, and tail-position nested function definitions return their
function object, matching the `@qq` line-number check.
fixture: `macrotools/upstream_utils.jl`。

検証: upstream Julia `macrotools/upstream_utils.jl`、
`cargo check -p subset_julia_vm --bin sjulia --features repl`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/upstream_utils.jl`。

### Top-level begin assignment RHS keeps global bindings ✅ (Issue #7667)

`x = begin ... end` を assignment statement lowering で inline block に展開し、
最後の式または単純代入の値を `x` に代入するようにした。Julia の `begin` は
スコープを作らないため、top-level RHS block 内の `y = ...` は global binding として残り、
function 内では同じ local scope に残る。
fixture: `scope/top_level_begin_rhs_assignments_7667.jl`。

検証: upstream Julia `scope/top_level_begin_rhs_assignments_7667.jl`、
`cargo check -p subset_julia_vm --bin sjulia --features repl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/scope/top_level_begin_rhs_assignments_7667.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests scope::`、
`timeout 1800 cargo nextest run --release --test fixture_tests control_flow::`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### Any-typed infix equality reaches user struct methods ✅ (Issue #7643)

`Any` 経由の infix `==` が user-defined `==(::S, ::S)` を runtime binary
dispatch 候補へ渡すようになった。MacroTools upstream destruct smoke の
`==(x, S("foo"))` workaround は upstream-compatible な `x == S("foo")` に戻した。
fixture: `macrotools/infix_eq_after_macro_block_7643.jl` と upstream destruct smoke。

検証: upstream Julia `infix_eq_after_macro_block_7643.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/infix_eq_after_macro_block_7643.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/upstream_destruct.jl`。

### MacroTools selective `striplines` import visibility ✅ (Issue #7645)

`using MacroTools: striplines` で selective import した helper が、unqualified
`striplines(ex)` として呼べることを fixture で固定した。MacroTools upstream utils fixture の
`MacroTools.striplines(...)` workaround を upstream-compatible な bare call に戻した。
fixture: `macrotools/striplines_selective_import_7645.jl` と upstream utils rmlines smoke。

検証: upstream Julia `striplines_selective_import_7645.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/striplines_selective_import_7645.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/upstream_utils.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### MacroTools animals module-qualified constant lookup ✅ (Issue #7646)

`MacroTools.animals` が `function MacroTools.animals` ではなく、package data 由来の
`Vector{Symbol}` constant として解決されるようにした。`Module.name` の値参照では、
using visibility 用に `module_functions` へ混ぜている module constants を function ref より
先に `LoadGlobalAny("Module.name")` として扱う。
fixture: `macrotools/animals_constant_7646.jl` と upstream utils animals test。

検証: upstream Julia `animals_constant_7646.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/animals_constant_7646.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/upstream_utils.jl`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### MacroTools destruct_key handles QuoteNode key patterns directly ✅ (Issue #7637)

`MacroTools.destruct_key(QuoteNode(:a), :tmp, MacroTools.getkeym)` が
`atoms(i -> getm(val, i), pat)` の closure 経路に入り、captured callable argument
`getm` を `Unknown function: getm` として失敗していた。bundled MacroTools の
`destruct_key` で atomic pattern を `getm(val, pat)` へ直接流す path を使い、
QuoteNode key pattern が postwalk closure を経由しないことを fixture で固定した。
fixture: `macrotools/destruct_key_quotenode_7637.jl`。

検証: `JULIA_LOAD_PATH=$(pwd)/subset_julia_vm/packages:@stdlib julia subset_julia_vm/tests/fixtures/macrotools/destruct_key_quotenode_7637.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/destruct_key_quotenode_7637.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'using MacroTools; println(MacroTools.destruct_key(QuoteNode(:a), :tmp, MacroTools.getkeym)); true'`、
`bash scripts/check_fixture_test_names.sh`。
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::` は既存
`macrotools_match_clauses_7546` の include path failure で失敗し、本 fixture は
direct 実行で通過。

### MacroTools @destruct captures array patterns ✅ (Issue #7636)

`MacroTools.@destruct [a, b] = Dict(:a => 1, :b => 2)` が upstream MacroTools と同様に
array-style destructuring pattern を capture し、`a` と `b` へ key lookup 結果を
束縛できることを fixture で固定した。bundled MacroTools の structural
`Expr(:vect, ...)` path により、`Unrecognised destructuring syntax [a, b]` を回避する。
fixture: `macrotools/destruct_array_pattern_7636.jl`。

検証: `JULIA_LOAD_PATH=$(pwd)/subset_julia_vm/packages:@stdlib julia subset_julia_vm/tests/fixtures/macrotools/destruct_array_pattern_7636.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/destruct_array_pattern_7636.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'using MacroTools; d = @destruct [a, b] = Dict(:a => 1, :b => 2); println(d); println((a, b)); true'`、
`bash scripts/check_fixture_test_names.sh`。

### Macro expansions can return function definitions ✅ (Issue #7634)

macro expansion が返す `Expr(:function, Expr(:call, :foo, :x), body)` を
statement-position の `Stmt::FunctionDef` に戻せるようにした。これにより
MacroTools `combinedef` / `@splitcombine` 系の macro-generated function definition が
`macro expansion returned unsupported Expr head :function` で止まらず、通常の関数定義として
登録される。
fixture: `macros/macro_expr_function_7634.jl`。

検証: upstream Julia `macro_expr_function_7634.jl`、
`cargo fmt`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/macro_expr_function_7634.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'macro m(); esc(Expr(:function, Expr(:call, :foo, :x), Expr(:block, :(x + 2)))); end; @m; println(foo(10)); true'`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Typed full-form methods keep nested arrow lambdas callable ✅ (Issue #7545)

`function rmlines(x::Expr)` のような typed full-form method 内で
`filter(x -> !isline(x), ...)` に渡す nested arrow lambda が、nested-function qualification
後も runtime で解決できることを既存 fixture で固定した。MacroTools `utils.jl` の
`rmlines(x::Expr)` 相当の形で `Function '...#__lambda_nested_...' not found` を回避する。
fixture: `closures/nested_arrow_in_typed_method_7545.jl`。

検証: `JULIA_LOAD_PATH=$(pwd)/subset_julia_vm/packages:@stdlib julia subset_julia_vm/tests/fixtures/closures/nested_arrow_in_typed_method_7545.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/closures/nested_arrow_in_typed_method_7545.jl`、
`bash scripts/check_fixture_test_names.sh`。

### Nested MacroTools-style quote macros preserve function locals ✅ (Issue #7542)

関数内で `esc(Expr(:quote, ex))` 型の nested quote macro を呼び、`@q $y` 相当の
interpolation が function-local `y` を macro expansion 時ではなく runtime/caller context
で解決することを既存 fixture で固定した。MacroTools `@match` branch template 内の
nested `@q` が `UndefVarError: f not defined` / `args not defined` になる早期評価を回避する。
fixture: `macros/nested_quote_macro_local_interpolation_7542.jl`。

検証: `JULIA_LOAD_PATH=$(pwd)/subset_julia_vm/packages:@stdlib julia subset_julia_vm/tests/fixtures/macros/nested_quote_macro_local_interpolation_7542.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/nested_quote_macro_local_interpolation_7542.jl`、
`bash scripts/check_fixture_test_names.sh`。

### MacroTools shortdef typed branches avoid nested @q splat failure ✅ (Issue #7541)

MacroTools `shortdef1` の `function f_(args__)::rtype_ body_ end` 系 branch が
`@q $f($(args...))::$rtype = ...` を load/lowering 時に評価して function-local capture を
見に行かないよう、upstream と同じ `Expr(:(=), Expr(:(::), Expr(:call, f, args...), rtype), ...)`
shape を明示構築する。`shortdef(:(function f(x)::Int ... end))` が `Expected Function or
Closure, got Symbol(:f)` で失敗しないことを fixture で固定した。
fixture: `macrotools/shortdef_splatted_q_7541.jl`。

検証: upstream Julia `shortdef_splatted_q_7541.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/shortdef_splatted_q_7541.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'using MacroTools; ex = :(function f(x)::Int x + 1 end); println(MacroTools.shortdef(ex)); true'`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### Quoted typed/where function signatures construct Expr(:function) ✅ (Issue #7540)

`:(function f(x::T) where T ... end)` が upstream Julia と同様に
`Expr(:function, Expr(:where, ...), body)` を構築できることを fixture で固定した。
quote constructor が typed parameter / where signature を拒否して MacroTools `utils.jl`
の `shortdef` / `combinedef` 系 helper を止める問題を回避する。
fixture: `macrotools/quoted_typed_where_function_7540.jl`。

検証: upstream Julia `quoted_typed_where_function_7540.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/quoted_typed_where_function_7540.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'ex = :(function f(x::T) where T; x; end); println(ex isa Expr); println(ex.head); true'`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### MacroTools @capture quotes runtime Any payloads ✅ (Issue #7539)

`using MacroTools` と basic `@capture(ex, f_(args__))` が、heterogeneous runtime payload を
含む macro return value の quote conversion で `macro expansion cannot quote value type Any`
にならないことを fixture で固定した。MacroTools package load が `utils.jl` の
`@capture` / matcher helper expansion を通過できる。
fixture: `macrotools/package_load_runtime_any_7539.jl`。

検証: upstream Julia `package_load_runtime_any_7539.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSET_JULIA_VM_DISABLE_CACHE=1 SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1 SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE=1 ./target/release/sjulia -e 'using MacroTools; println("loaded"); true'`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/package_load_runtime_any_7539.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### Quoted typed expressions construct Expr(:(::), ...) ✅ (Issue #7537)

`:(x::Int)` が upstream Julia と同様に `Expr(:(::), :x, :Int)` を構築できることを
fixture で固定した。MacroTools `@capture` / matcher patterns が typed expression を含む
quoted AST を扱う際に `quote for typed_expression not yet supported` で止まらない。
fixture: `macrotools/quoted_typed_expression_7537.jl`。

検証: upstream Julia `quoted_typed_expression_7537.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/quoted_typed_expression_7537.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'ex = :(x::Int); println(ex isa Expr); println(ex.head); true'`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### MacroTools @capture splats generated binding assignments ✅ (Issue #7536)

MacroTools `@capture` の `quote ... $([:($(esc(b)) = nothing) for b in bs]...) ... end`
形が、binding 名の comprehension を評価して複数 assignment を quoted block に splice
できることを fixture で固定した。match 成功時の captured values と、match 失敗時の
`nothing` 初期化の両方を確認し、`Undefined variable: [:($(esc(b)) = nothing) for b in bs]`
にならないことを押さえる。
fixture: `macrotools/capture_splatted_comprehension_7536.jl`。

検証: upstream Julia `capture_splatted_comprehension_7536.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSET_JULIA_VM_DISABLE_CACHE=1 SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1 SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE=1 ./target/release/sjulia -e 'using MacroTools; println("loaded"); true'`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/capture_splatted_comprehension_7536.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### MacroTools allbindings handles QuoteNode.value guard ✅ (Issue #7535)

MacroTools `allbindings(QuoteNode(:x_), bs)` が `isa(pat, QuoteNode)` guard 後の
`pat.value` を動的 field access として扱い、`type Expr has no field value` で
macro helper compilation を止めないことを fixture で固定した。`@capture` expansion が
`allbindings` dependency を含んでも package load を通過できる。
fixture: `macrotools/allbindings_quotenode_value_7535.jl`。

検証: upstream Julia `allbindings_quotenode_value_7535.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSET_JULIA_VM_DISABLE_CACHE=1 SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1 SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE=1 ./target/release/sjulia -e 'using MacroTools; println("loaded"); true'`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/allbindings_quotenode_value_7535.jl`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### MacroTools TypeBind @nomatch fallback lowers ✅ (Issue #7534)

MacroTools `match_inner(b::TypeBind, ex, env)` の mismatch branch が `@nomatch(b, ex)` を
short-form method body から展開し、`return MatchError(...)` として実行できることを
fixture で固定した。`match/types.jl` lowering が `type Expr has no field value` で
止まらず、package load が `@nomatch` dependency を通過する。
fixture: `macrotools/nomatch_typebind_7534.jl`。

検証: upstream Julia `nomatch_typebind_7534.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSET_JULIA_VM_DISABLE_CACHE=1 SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1 SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE=1 ./target/release/sjulia -e 'using MacroTools; println("loaded"); true'`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/nomatch_typebind_7534.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### Package-internal MacroTools @capture resolves defining module ✅ (Issue #7533)

`using MacroTools` 中の package-internal `@capture` expansion が、macro 定義元 module
`MacroTools` の helper を expansion-time compilation で解決できることを既存 fixture で
固定した。`Unknown module: MacroTools` で `utils.jl` の local macro expansion が止まらず、
bundled package path から basic `@capture` call を実行できる。
fixture: `macrotools/package_load_capture_basic.jl`。

検証: upstream Julia `package_load_capture_basic.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSET_JULIA_VM_DISABLE_CACHE=1 SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1 SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE=1 ./target/release/sjulia -e 'using MacroTools; println("loaded"); true'`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/package_load_capture_basic.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`。

### Short-form arrow closures capture outer parameters ✅ (Issue #7531)

`makepred(x) = y -> y == x` のような short-form function body から返る arrow closure が、
outer parameter `x` を capture して runtime で callable になることを既存 fixture で固定した。
VersionNumber comparison helper などの `isequal(x) = y -> isequal(y, x)` 形が
`Undefined variable: x` で compile 失敗しない。
fixture: `closures/short_form_arrow_capture_7531.jl`。

検証: upstream Julia `short_form_arrow_capture_7531.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/closures/short_form_arrow_capture_7531.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'makepred(x) = y -> y == x; p = makepred(2); println(p(2)); true'`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests closures::`。

### Macro-expanded Pair calls restore to Pair expressions ✅ (Issue #7639)

macro expansion が返す quoted code 内の `:a => 1` が
`Expr(:call, :=>, :a, 1)` のまま generic call として実行され、
`Unknown function: =>` になる問題を修正した。macro result conversion の
`call_expr_from_values` で callee が `=>` の通常 2 引数 call を `Expr::Pair` に戻し、
source の `Dict(:a => 1)` lowering と同じ path に通す。
fixture: `macros/macro_pair_call_7639.jl`。

検証: upstream Julia `macro_pair_call_7639.jl`、
`cargo fmt`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/macro_pair_call_7639.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'macro m(); esc(:(Dict(:a => 1))); end; println((@m())[:a]); true'`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### MacroTools @match block macro arguments keep clause shape ✅ (Issue #7547)

`@match ex begin ... end` の macro argument が、clauses 全体を包む余分な
`Expr(:block, ...)` として渡らず、MacroTools `clauses(lines)` が各 clause を期待形で
処理できることを existing regression fixture で固定した。これにより
`Invalid match clause Expr(:block, ...)` を回避できる。
fixture: `macrotools/match_block_macro_arg_7547.jl`（Issues #7547, #7548）。

検証: `cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/match_block_macro_arg_7547.jl`、
`bash scripts/check_fixture_test_names.sh`。
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::` は既存
`macrotools_match_clauses_7546` の include path failure で失敗し、本 fixture は
direct 実行で通過。

### MacroTools @match helper splat dependencies are available ✅ (Issue #7548)

MacroTools `@match` expansion の `foldr((clause, body) -> makeclause(clause..., body), ...)`
形で、lifted lambda 内の splat call から必要になる `makeclause` などの helper が
expansion-time program に含まれることを existing regression fixture で固定した。
`macro_dependency_functions` の full dependency retry path により、`Cannot find function
'makeclause' for splat call` を回避できる。
fixture: `macrotools/match_block_macro_arg_7547.jl`（Issues #7547, #7548）。

検証: `cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/match_block_macro_arg_7547.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'using MacroTools; ex = :value; out = @match ex begin x_ => x end; println(out); true'`、
`bash scripts/check_fixture_test_names.sh`。
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::` は既存
`macrotools_match_clauses_7546` の include path failure で失敗し、本 fixture は
direct 実行で通過。

### Quote interpolation splats generator values into Expr construction ✅ (Issue #7549)

quote 内の `$((expr for x in xs)...)` が runtime `Expr` construction 時に generator
値を splat できることを existing regression fixture で固定した。MacroTools `bindinglet`
が capture bindings を quoted body に展開する形で `Cannot splat value of type Generator`
にならず、`Expr(:block, ..., Expr(:(=), ...))` を構築できる。
fixture: `macros/quote_generator_splat_7549.jl`。

検証: upstream Julia `quote_generator_splat_7549.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quote_generator_splat_7549.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Macro-returned Expr(:let, ...) lowers to LetBlock ✅ (Issue #7550)

macro expansion が `Expr(:let, bindings, body)` を返したときに `LetBlock` として
lowering できることを existing regression fixture で固定した。single binding と
multi binding の両方を扱い、MacroTools `@match` の `bindinglet` が返す let AST を
expression position で評価できる。fixture: `macros/macro_return_let_expr_7550.jl`。

検証: upstream Julia `macro_return_let_expr_7550.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/macro_return_let_expr_7550.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### MacroTools nested macro dependencies avoid unrelated include helpers ✅ (Issue #7551)

`using MacroTools` の package load が、nested `@q` expansion 時に同じ include file の
無関係な helper（`MacroTools.*` を参照するもの）まで compile-time program に含めて
`Unknown module: MacroTools` で失敗しないことを regression fixture で固定した。
`macro_dependency_functions` による transitive dependency filtering の範囲で
MacroTools `utils.jl` の local macro expansion を通過できる。
fixture: `macrotools/package_load_nested_macro_deps_7551.jl`。

検証: `cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'using MacroTools; println(true); true'`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/package_load_nested_macro_deps_7551.jl`、
`bash scripts/check_fixture_test_names.sh`。
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::` は既存
`macrotools_match_clauses_7546` の include path failure で失敗し、本 fixture は
direct 実行で通過。

### Macro expansion dependencies include higher-order function arguments ✅ (Issue #7552)

macro expansion 用の compile-time dependency filtering が、`prewalk(rmlines, ex)` の
ように関数値として渡される helper も含めて展開できることを regression fixture で固定した。
MacroTools `@q` の `striplines(ex) = prewalk(rmlines, ex)` 形で `rmlines` が欠落せず、
expansion-time program を構築できる。fixture: `macros/macro_hof_dependency_7552.jl`。

検証: upstream Julia `macro_hof_dependency_7552.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/macro_hof_dependency_7552.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Quoted where expressions construct Expr(:where) values ✅ (Issue #7553)

`:(x where {T})` の quote construction が `Expr(:where, :x, :T)` を生成できることを
regression fixture で固定した。MacroTools `@q` helper が function/arrow signature を
再構築するときに使う `where` expression AST の head/args を upstream Julia と同じ形で
保持する。fixture: `macros/quote_where_expr_7553.jl`。

検証: upstream Julia `quote_where_expr_7553.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quote_where_expr_7553.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Quoted adjoint expressions construct Expr(:') values ✅ (Issue #7554)

`:(x')` の quote construction が `Expr(:', :x)` を生成できることを regression
fixture で固定した。MacroTools `resyntax` の `adjoint(x_) => :($x')` 形を支える
adjoint expression AST の head/args を upstream Julia と同じ形で保持する。
fixture: `macros/quote_adjoint_expr_7554.jl`。

検証: upstream Julia `quote_adjoint_expr_7554.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quote_adjoint_expr_7554.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Macro-generated Expr(:public, ...) lowers as a declaration no-op ✅ (Issue #7625)

macro expansion が `esc(Expr(:public, :foo))` を返しても、
`macro expansion returned unsupported Expr head :public` で落ちずに lower できるようにした。
statement context の macro result で `escape` / `hygienic-scope` を unwrap し、
`Expr(:public, symbols...)` を source の `public` statement と同じ compile-time-only
no-op として扱う。fixture: `macros/macro_public_expr_7625.jl`。

検証: upstream Julia `macro_public_expr_7625.jl`、
`cargo fmt`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/macro_public_expr_7625.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Quoted field assignment from macro expansion is lowerable ✅ (Issue #7630)

macro expansion が `QuoteNode(:($x.$f = $v))` のような quoted field assignment
を返しても、`Expr(:(=), Expr(:., ...), value)` の assignment target を Symbol
前提で拒否せず Expr object として扱えることを regression fixture で固定。
MacroTools `resyntax` の `:($x.$f = $v)` 形を担保する。
fixture: `macros/quoted_field_assignment_7630.jl`。

検証: upstream Julia `quoted_field_assignment_7630.jl`、
`cargo fmt`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quoted_field_assignment_7630.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### Macro expansion Expr(::) values lower as typeassert ✅ (Issue #7628)

macro expansion が `Expr(:(::), value, type)` を value position に返しても
lowering できるようになった。2 引数の `::` Expr を `typeassert(value, type)` call
に変換し、通常 CST の `expr::T` と同じ runtime 型検査に通す。
fixture: `macros/macro_return_typed_expr_7628.jl`。

検証: upstream Julia `macro_return_typed_expr_7628.jl`、
`cargo fmt`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/macro_return_typed_expr_7628.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'macro m(); esc(:(x::Int)); end; function f(x); @m; end; println(f(1)); try; f(1.5); catch e; println(typeof(e)); end'`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`。

### index/field 代入の値に現れるネストした lambda を登録 ✅ (Issue #7615)

`xs[1] = map(x -> x + 1, xs[1])` のように index/field/Dict 代入の値（や index 位置）に
現れる lambda が登録されず実行時 `Function '...__lambda_nested_...' not found` で落ちていた
不具合を修正。`compile/collect.rs` の `collect_stmt_functions` に `Stmt::IndexAssign`
（indices + value）・`Stmt::FieldAssign`（value）・`Stmt::DictAssign`（key + value）の分岐を
追加（AoT `call_graph` の走査と一致）。MacroTools upstream `split.jl` を unblock。
fixture: `closures/nested_lambda_index_assign_7615.jl`（julia とパリティ一致、4/4）。

### eval handles Expr dotted callees ✅ (Issue #7616)

`eval` が `Expr(:call, Expr(:., :MacroTools, QuoteNode(:trymatch)), ...)` を実行できる
ようになった。`eval_expr_ast` の call callee 解決に dotted callee 分岐を追加し、
`Expr(:., module, QuoteNode(name))` を module-qualified call として既存 dispatch
path に渡す。fixture: `macros/eval_dotted_callee_7616.jl`。

検証: upstream Julia `eval_dotted_callee_7616.jl`、
`cargo fmt`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/eval_dotted_callee_7616.jl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia -e 'using MacroTools; eval_dotted_callee_7616_ex = :(foo(1)); body = Expr(:call, Expr(:., :MacroTools, QuoteNode(:trymatch)), Expr(:quote, :(foo(x_))), :eval_dotted_callee_7616_ex); println(eval(body))'`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::chunk_000`。

### Quoted typed assignment LHS preserves Expr(::) ✅ (Issue #7622)

`:(x::Int = nothing)` が `Expr(:(=), :x::Int, :nothing)` ではなく、upstream Julia と
同じ `Expr(:(=), Expr(:(::), :x, :Int), :nothing)` を返すようになった。
quote constructor の assignment LHS 判定で `TypedExpression` を複合 LHS として
再帰変換する。`:(x = nothing)` の plain identifier LHS は引き続き `:x`。
fixture: `macros/quote_assign_typed_lhs_7622.jl`。

検証: upstream Julia `quote_assign_typed_lhs_7622.jl`、
`cargo fmt`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quote_assign_typed_lhs_7622.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::chunk_000`。

### Expr 値に対する getfield/getproperty を Expr 対応 ✅ (Issue #7614)

`getfield(ex, :head)` / `getfield(ex, :args)` / `getproperty(ex, :head)` および整数添字版
`getfield(ex, 1)`=head, `getfield(ex, 2)`=args が `Expr` 値で実行時エラーになっていた不具合を
修正。`.head`/`.args` のプロパティ構文だけがコンパイル時に `GetExprField` へ特殊化され、
明示的な `getfield`/`getproperty` 呼び出し（`Any` 型引数経由。例: `MacroTools.splitdef`）は
汎用リフレクション getfield（`vm/builtins_reflection/mod.rs`）に流れて `Value::Expr` 分岐が
無かったのが原因。Symbol 添字・整数添字の両分岐と out-of-bounds 用 `field_count`=2 を追加。
`args` は共有バッキング配列を返すため `getfield(ex, :args) === ex.args` の参照同一性も一致。
fixture: `reflection/expr_getfield_through_any_7614.jl`（julia とパリティ一致、10/10）。

## 最新対応 (2026-06-24)

### Complex/Real の順序比較をエラー化 ✅ (Issue #7605)

`Complex` × `Real` の `< <= > >=`（左右両方向）が、エラーではなく実部比較の `Bool` を
返していた不具合を修正（`complex(1.0, 2.0) < 3` が `true`）。`base/complex.jl` に
parametric `Complex{T} where {T<:Real}` の順序メソッドを明示追加し、
`error("Complex numbers are not orderable")` を送出（upstream は MethodError、sjulia は
ErrorException だが「エラーになる」点で一致）。総称 `<(::Real, ::Real)`（promotion.jl）が
specialization 下で Complex に緩マッチして実部比較していた穴を塞ぐ。`Complex × Complex` は
どちらも `Real` に一致しないため従来どおり MethodError（不変）。fixture
`complex/complex_ordering_error_7605.jl`（`@test_throws Exception` で julia と parity一致）と
既存 Rust テスト `test_complex_ordering_error`（main で赤→緑）で担保。

### MacroTools splitarg macro-expanded unbound refs ✅ (Issue #7556)

`@match` 展開の `let` clause で新規 binding が compiler locals に残り、後続
clause が未初期化値を shadow-save して `UndefVarError` になる問題を修正。
`let` 導入 locals を body 後に compiler state から外し、ネストした block tail も
`let` の値として返すようにした。

`MacroTools.splitarg` は default 引数分解を `Expr(:(=), ...)` から直接行い、
`MacroTools.splitarg(:(x::Int))` / `:(::Int)` / `:(x)` / `:(args...)` が upstream
Julia と同じ結果を返す。fixture: `macrotools/splitarg_unbound_refs_7556.jl`。

検証: upstream Julia `splitarg_unbound_refs_7556.jl`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`SUBSETJULIA_CACHE_DIR=$(mktemp -d) ./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/splitarg_unbound_refs_7556.jl`、
`bash scripts/check_fixture_test_names.sh`。
`timeout 1800 cargo nextest run --release --test fixture_tests macrotools::chunk_000` は
既存 fixture `macrotools_match_clauses_7546` の include path failure で失敗し、
本 fixture は direct 実行で通過。

### Float64/Float32 × Int64 混合算術の高速化 ✅ (Issue #7587)

`base/float.jl` に混合 `Float64×Int64` / `Float32×Int64`（両方向、`+ - * / ^`）の
concrete メソッドを追加。第一級 `^(::Float64, ::Int64)` 等が `::Number,::Number` の
promote() フォールバックではなく同型 intrinsic に直結するようになり、`x .^ 2` /
`x .+ 2`（Float64 配列 × Int スカラー）のブロードキャストが ~8–14倍、スカラー
`s + 2` が ~240倍高速化。`exp.(-(x .- t) .^ 2)` は 12.25s→3.30s（結果同一）。スコープは
`Int64` 限定で、`BigInt`/`Bool`/他整数幅は従来の promote 経路（型不変）。fixture:
`broadcast/mixed_float_int_arith_7587.jl`（値・型パリティ）と
`broadcast/broadcast_perf_mixed_float_int_7587.jl`（性能回帰）、bench:
`vm_broadcast_mixed_float_int_benchmark`。

### BigInt ^ BigInt exponent support ✅ (Issue #7608)

`big(2) ^ big(3)` が `BigInt(8)` を返すようになった。`PowBigInt` intrinsic の
exponent pop を `I64` 限定から `BigInt` coercion に広げ、関数引数などで
`DynamicPow` に落ちる `BigInt` base + integer exponent も同じ BigInt power path で
処理する。

検証: upstream Julia `bigint_bigint_pow_7608.jl`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/bigint/bigint_bigint_pow_7608.jl`、
`./target/release/sjulia -e 'println(big(2) ^ big(3)); println(typeof(big(2) ^ big(3))); println(big(2) ^ big(64)); println(typeof(big(2) ^ big(64)))'`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests bigint::chunk_000`。

### JSXGraph 3D view rotates on single-finger drag ✅ (Issue #7592)

View3D を載せた board が単指ドラッグで回転せず平行移動する不具合を修正。
emitter (`plotting/jsxgraph.rs`) が View3D を含む board に `pan.needTwoFingers =
true` を注入し、単指=回転 / 二指=pan / pinch=zoom にした(web/iOS/Flutter 共通の
JSON spec なので 1 箇所修正で全フロントエンド反映、2D board は従来どおり単指 pan)。
回帰テスト 2 件を `plot_artifact_mime_tests` に追加。MWE の `2*π` が踏む別不具合は
#7600 として分離起票。

### AbstractFloat ^ BigInt type-preserving power ✅ (Issue #7602)

`2.0 ^ big(3)` / `2.0f0 ^ big(3)` / `Float16(2) ^ big(3)` が upstream Julia と同じく
`Float64` / `Float32` / `Float16` のまま結果を返すようになった。VM の dynamic pow
に `AbstractFloat` base + `BigInt` exponent の inline path を追加し、`Float64` は
#7308 の補正済み `pow_f64` を使う。`tfunc_pow` もこの形を base float 型として推論する。

検証: upstream Julia `abstractfloat_bigint_pow_7602.jl`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/arithmetic/abstractfloat_bigint_pow_7602.jl`、
`./target/release/sjulia -e 'println(2.0 ^ big(3)); println(typeof(2.0 ^ big(3))); println(2.0f0 ^ big(3)); println(typeof(2.0f0 ^ big(3))); println(Float16(2) ^ big(-3)); println(typeof(Float16(2) ^ big(-3)))'`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests arithmetic::chunk_000`。

### MacroTools @capture/@match expansion helper visibility ✅ (Issues #7569/#7603/#7604)

Bundled package macro definitions now carry their package helper functions,
struct definitions, and macro hygiene member set into expansion-time execution.
This lets `MacroTools.@capture` and `MacroTools.@match` resolve package-private
helpers such as `allbindings`, `TypeBind`, and `trymatch` even when the macro is
expanded from user code after `using MacroTools: @capture`.

Macro-produced identity operator calls (`Expr(:call, :===, ...)` / `:!==`) are
converted back to `BinaryOp::Egal` / `BinaryOp::NotEgal`, and macro-produced
multi-statement block Expr values keep their tail expression value. This preserves
the boolean result of MacroTools `@capture` blocks whose tail is an `if`.

検証: upstream Julia `macro_expanded_egal_call_7603.jl` /
`macro_block_tail_if_7604.jl`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/package_load_capture_basic.jl`
with fresh `SUBSETJULIA_CACHE_DIR`、
`./target/release/sjulia -e 'using MacroTools; true'` with fresh cache、
Issue #7569 destruct include probe with fresh cache、
`./target/release/sjulia -e 'using MacroTools: @capture; ex = :(f(42)); ok = @capture(ex, f(x_)); println(ok); println(x)'`
with fresh cache、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/macro_expanded_egal_call_7603.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/macro_block_tail_if_7604.jl`。

### MacroTools @forward quoted generator support ✅ (Issues #7572/#7599)

MacroTools `examples/forward.jl` の quoted generator/splat method definition が
parse/lower できるようになった。`[expr\n for x in xs]` 型の array comprehension、
quoted comprehension constructor、quoted keyword/splat call constructor を追加し、
`using MacroTools` は `examples/forward.jl` を越えて package load まで完了する。

併せて、package root lowering と include lowering が `@__DIR__` を実ソース/virtual source
directory に合わせるようにし、bundled package file I/O は `/embedded_packages/...`
の `..` を正規化して registry file を読む。MacroTools `animals.txt` 初期化に必要な
`eachline(joinpath(@__DIR__, "..", "animals.txt"))` は bundled/実パッケージの両方で通る。

検証: upstream Julia `quote_forward_generator_7572.jl` / `package_animals_dir_7599.jl`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/macros/quote_forward_generator_7572.jl`、
fresh cache で `./target/release/sjulia subset_julia_vm/tests/fixtures/macrotools/package_animals_dir_7599.jl`、
fresh cache で default bundled `./target/release/sjulia -e 'using MacroTools; true'`、
fresh cache + `SUBSETJULIA_LOAD_PATH` real package path で `using MacroTools`、
`cargo test -p subset_julia_vm_parser --test corpus_collections test_comprehension_newline_before_for -- --nocapture`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::chunk_001`。
`macrotools::chunk_000` は既存の Issue #7569 (`allbindings`) で失敗するため、この PR の
package-load 回帰は fresh cache の direct `sjulia` 実行で検証した。

### Nested closure module scope propagation ✅ (Issue #7591)

Module function 内の do-block / closure からさらに lifted された nested closure が、
親 module path を失わないようにした。#7180 で追加された first-level closure の
module scope 継承を、`parent#child` qualified parent 名にも伝播する。

これにより、MacroTools `gensym_ids` のように nested lambda から module-private
helper を呼ぶ pattern で `function 'hidden' is not imported` にならない。

検証: upstream Julia `module_nested_closure_scope_7591.jl`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/modules/module_nested_closure_scope_7591.jl`、
`./target/release/sjulia -e 'module M; hidden(x) = x + 1; function f(xs); map(xs) do x; thunk = () -> hidden(x); thunk(); end; end; end; @assert M.f([1]) == [2]; println("ok")'`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/modules/module_closure_hof_helper_7180.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests modules::chunk_000`。

### Base.eachline(filename) package initialization support ✅ (Issue #7593)

`eachline(filename)` を compile-time routed file I/O builtin として追加した。
現在は `readlines(filename)` と同じくファイル全体を `Vector{String}` に materialize し、
`collect(eachline(path))` と `map(Symbol, eachline(path))` をサポートする。

MacroTools の bundled `animals.txt` を読む fixture を追加し、package initialization
で使われる line iteration を検証した。

検証: upstream Julia `eachline_filename_7593.jl`、
`cargo check -q -p subset_julia_vm --lib`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia -e 'lines = collect(eachline("subset_julia_vm/packages/MacroTools/animals.txt")); @assert length(lines) == 214; @assert lines[1] == "wombat"; println("ok")'`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/filesystem/eachline_filename_7593.jl`、
`../target/release/sjulia tests/fixtures/filesystem/eachline_filename_7593.jl` from `subset_julia_vm/`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests filesystem::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests exports::chunk_000`。

### Nested module relative named imports ✅ (Issues #7574/#7594)

Nested module 内の `import ..Parent: name` / `import ..Sibling: name` が、
module-local imported-function set に反映されるようになった。`UsingImport` は
relative import の leading dot count を保持し、compile phase は現在 module path
から相対 module を解決する。

`LinearAlgebra.LAPACK` の workaround を削除し、`import ..LinearAlgebra: inv, lu, LU`
を使う upstream-style source に戻した。

Issue #7574 の元MWEが使う `x() = 1` についても、user module function `x` が
Base/prelude closure capture `x` を function ref として shadow しないようにした
(Issue #7594)。

検証: upstream Julia `relative_named_import_parent_7574.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/module/relative_named_import_parent_7574.jl`、
`./target/release/sjulia -e 'module A; x() = 1; end'`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`cargo test -p subset_julia_vm_parser --test corpus_statements test_import_relative -- --nocapture`、
`timeout 1800 cargo nextest run --release --test fixture_tests module_tests::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests modules::chunk_000`、
`timeout 1800 cargo nextest run --release --test fixture_tests linalg::chunk_000 linalg::chunk_001 linalg::chunk_002`。

### MacroTools macro-runtime and closure lowering progress ✅ (Issues #7566/#7569/#7541/#7542/#7554)

MacroTools `utils.jl` / `examples/destruct.jl` の load blocker を進めた。do-block の
expression-position `if` / assignment が outer capture を正しく mutate するようにし、
macro body 内の user macro call は ctx 付き lowering で解決する。macro expansion から
戻る `Expr(:$)`, `Expr(:...)`, `Expr(:')`, `Expr(:curly, ...)` と expression/DataType
call target は caller-side IR に再構成する。

検証: direct `using MacroTools` は Issue #7572 の `examples/forward.jl` parser blocker
まで到達。`timeout 1800 cargo nextest run --release --test fixture_tests closures::`、
`timeout 1800 cargo nextest run --release --test fixture_tests macros::`、
`timeout 1800 cargo nextest run --release --test fixture_tests generated::`、
`timeout 1800 cargo nextest run --release --test fixture_tests operators::`、
`bash scripts/check_fixture_test_names.sh`。

### Macro body lifted lambda visibility ✅ (Issue #7584)

macro body 内の macro call により ctx-aware lowering される macro definition で、同じ body 内に
lifted arrow helper が生成された場合、その helper を macro 定義直後に compile-time 関数として
登録するようにした。これにより、同一 source/include 内の後続 macro expansion が
`__lambda_0` を見つけられない、または別 arity の同名 helper に dispatch してしまう回帰を防ぐ。

検証: `macro_body_foldr_lambda_7584.jl`、および direct `using MacroTools` が Issue #7572 まで到達。

### LinearAlgebra Sylvester array unary minus workaround removal ✅ (Issue #7577)

`sylvester(A, B, C)` の RHS 構築を `_neg_colvec(C)` の明示ループから
`-_colvec(C)` に戻した。array unary minus は既存 broadcast materialization 経路で
compile できるため、`docs/vm/WORKAROUNDS.md` の active workaround W-13 を
Resolved に移動した。

検証: upstream Julia `sylvester_array_unary_minus_7577.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/sylvester_array_unary_minus_7577.jl`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests linalg::`。

### MacroTools forward.jl, @capture, and package data load path ✅ (Issues #7494/#7535/#7572/#7591/#7593)

MacroTools `examples/forward.jl` の multi-line quoted generator method definition を
parser / quote lowering で扱えるようにした。改行後の `for` を array comprehension として
認識し、quote constructor lowering は interpolated call / block の semicolon token を
式として扱わない。

併せて、深く lift された module closure が module-private helper を解決できるよう module path を
親チェーンへ伝播し、`eachline(filename)` / `EachLine`、`@__DIR__` source context、
embedded package data file registry read を追加した。`using MacroTools; true` は新規/default
package cache の両方で成功する。

MacroTools `@capture` は bundled package macro の helper functions / hygiene context を
macro runtime へ登録して評価し、返却された quoted block の末尾式値を保持する。
`Expr(:call, :===, ...)` / `Expr(:call, :!==, ...)` も `BinaryOp` に戻す。

検証: `macrotools_package_load_capture_basic`、`macrotools_forward_quote_7572.jl`、
`modules_module_deep_closure_helper_7591`、`filesystem_eachline_7593`、`macro_file_dir`。

### MacroTools upstream fixture smoke expansion ✅ (Issues #7614/#7615/#7617/#7621/#7625/#7636/#7637/#7639/#7641)

MacroTools upstream fixture smoke を `destruct.jl` / `utils.jl` / `flatten_try.jl` まで
進めた。macro-expanded `Expr(:call, :=>, ...)` は Pair IR に戻り、quoted
assignment の vector LHS は `Expr(:vect, ...)` として保持される。`Expr(:public, ...)`
は statement/value 位置で no-op 化し、`Expr` の `getfield` は `Any` 経由でも
`head` / `args` を読める。

`@destruct` は sjulia の現行 `@match` capture gap を避けるため、array/ref/field
pattern の構造分岐を MacroTools 側に持たせた。atomic key path は captured callable
closure を避けて `getm(val, pat)` へ直接進む。

検証: `timeout 1800 cargo nextest run --release --test fixture_tests macrotools::`、
`macros::`、`reflection::`、`closures::`、`control_flow::`。

### Function-local `size(A)` tuple comparison ✅ (Issue #7578)

関数 body 内の `size(A) == size(B)` / `size(A) != size(B)` が、数値比較
fallback の `DynamicToI64` ではなく tuple comparison として compile されるようにした。
`BinaryOp::Eq` / `BinaryOp::Ne` の tuple 早期処理で、1引数 `size` / `Base.size`
/ `BuiltinOp::Size` call を tuple-producing expression として扱う。

検証: upstream Julia `size_tuple_comparison_7578.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/comparison/size_tuple_comparison_7578.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests comparison::`。

### Base matrix array addition/subtraction ✅ (Issue #7579)

`Matrix + Matrix` / `Matrix - Matrix` を `base/arraymath.jl` の Pure Julia
dispatch で扱うようにした。dense matrix 同士の shape を次元ごとに検証し、
結果は `size(A)` を保った行列として elementwise に生成する。混合数値型では
最初の要素結果から結果 element type を決め、`Int64` + `Float64` なども
`Float64` matrix になる。

検証: upstream Julia `arraymath_matrix_add_sub_dispatch_7579.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/array/arraymath_matrix_add_sub_dispatch_7579.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests array::`。

### JSXGraph 3D/do-block artifact integration ✅ (Issues #7373, #7374, #7375)

`JSXGraph` Pure-Julia model を 3D に拡張した。`board(...) do` /
`view3d(...) do`、`View3D` の nested element storage、`curve3d` / `point3d` /
`line3d`、raw JS 式を構造化して運ぶ `JSFunction` を追加。

Rust の JSXGraph artifact writer は nested `view3d.elements` と
`{"jsfunc": code, "var": "t"}` を JSON として出力し、web/iOS renderer は同じ spec を
再帰的に `board.create` / `view.create` へ変換する。3D Lissajous sample を
web/iOS サンプルへ追加した。

検証: `packages_jsxgraph_doblock_7373` / `packages_jsxgraph_view3d_7373` /
`packages_jsxgraph_jsfunction_7373`、および
`test_jsxgraph_view3d_emits_nested_elements_and_jsfunc_7374`。

### Persistent prelude Program cache compiler fingerprint ✅ (Issue #7544)

persistent prelude Program cache の compatibility key を、prelude source hash と
build-time compiler/VM source fingerprint の combined SHA-256 に変更した。lowering 変更後に
古い Program cache を再利用して `sjulia -e '42'` が `Undefined variable: x` で落ちる回帰を
防止する。

併せて、macro context を関数 body lowering へ伝播するのは body に macro call を含む関数定義に
限定し、通常の関数内 arrow closure capture を従来経路に戻した。

### LinearAlgebra factorization result objects ✅ (Issue #7463)

`lu` / `qr` / `cholesky` / `eigen` / `svd` の stdlib wrapper が、既存 builtin の
numeric result を `Factorization` subtype (`LU`, `QR`, `Cholesky`, `Eigen`, `SVD`)
として返すようになった。既存 field access と LU/SVD destructuring は維持し、
dispatch-first user override が tuple/NamedTuple 以外を返す場合は wrapper せずそのまま返す。

検証: upstream Julia `factorization_result_types_7463.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/factorization_result_types_7463.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/user_snippet_inv_svd.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/det_lu_module_dispatch_first_4020.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests linalg::`。

### LinearAlgebra in-place and values-only factorization APIs ✅ (Issue #7464)

`lu!` / `qr!` / `cholesky!` / `eigen!` / `eigvals!` / `svd!` / `svdvals` /
`svdvals!` / `isposdef!` を `LinearAlgebra` の export に追加した。既存の
`Factorization` wrapper を再利用し、戻り値は `LU` / `QR` / `Cholesky` / `Eigen` /
`SVD` または values vector とする。`*!` API は sjulia が扱える factorization work
form を入力行列へ書き戻す。

検証: upstream Julia `factorization_inplace_values_7464.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/factorization_inplace_values_7464.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests linalg::`。

### LinearAlgebra diagonal/copy and mutating transpose helpers ✅ (Issue #7466)

`diagind` / `diagview` / `transpose!` / `adjoint!` / `triu!` / `tril!` /
`copy_transpose!` / `copy_adjoint!` / `copytrito!` と、`Diagonal` から dense
matrix への `copyto!` を追加した。`diagview` は親行列の diagonal に alias する
軽量 view として実装し、`dv[i] = x` が元行列へ反映される。

検証: upstream Julia `diagonal_copy_transpose_helpers_7466.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/diagonal_copy_transpose_helpers_7466.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests linalg::`。

### LinearAlgebra matrix division operator calls ✅ (Issue #7467)

operator function-call lowering が `\` を通常の callable operator target として扱うようにした。
これにより `\(A, b)` は infix `A \ b` と同じ dispatch path に到達する。
`LinearAlgebra` 側では `\` / `/` を export し、dense matrix left/right division と
`LU` / `QR` / `Cholesky` / `SVD` の vector RHS left division を追加した。

検証: upstream Julia `division_operator_calls_7467.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/division_operator_calls_7467.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/ldiv_basic.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/ldiv_dispatch_first_4020.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests linalg::`。

### LinearAlgebra Givens rotations and reflection helpers ✅ (Issue #7469)

`LinearAlgebra.Givens` / `givens` / `rotate!` / `reflect!` を追加した。
`givens(f, g, i1, i2)` は upstream と同じ `Givens{T}` と radius を返し、vector/matrix overload は指定 entry から
scalar pair を取り出す。`lmul!(G::Givens, A)` と `G * A` は dense vector / matrix に
rotation を適用する。

検証: upstream Julia `givens_rotate_reflect_7469.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/givens_rotate_reflect_7469.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/checksquare_axpy_rmul.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests linalg::`。

### LinearAlgebra BLAS/LAPACK module subset ✅ (Issue #7468)

`LinearAlgebra.BLAS` / `LinearAlgebra.LAPACK` を公開し、sjulia が VM 上で扱える
Pure Julia subset を追加した。`BLAS` は `dot` / `dotu` / `dotc` / `axpy!` /
`scal!` / `gemv!` / `gemm!`、`LAPACK` は `gesv!` / `getrf!` を提供する。

実装は native BLAS/LAPACK binding ではなく、dense array loop と既存の `inv` /
`lu` factorization wrapper を使う。これにより、decomposition code が upstream と同じ
module 名を参照しつつ、no-JIT VM / iOS path で安定して実行できる。

検証: upstream Julia `blas_lapack_modules_7468.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/blas_lapack_modules_7468.jl`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests linalg::`。

### LinearAlgebra matrix equations and low-rank updates ✅ (Issue #7470)

`condskeel` / `lyap` / `sylvester` と、`Cholesky` wrapper 用の
`lowrankupdate` / `lowrankupdate!` / `lowrankdowndate` /
`lowrankdowndate!` を追加した。`lyap` / `sylvester` は dense small-matrix
subset を `kron` と matrix division で解き、`condskeel` は upstream と同じ
`abs(inv(A))*abs(A)` ベースの Skeel condition number を返す。

low-rank update/downdate は `Cholesky` の public `L` / `U` fields から
dense matrix を再構成し、`L*U ± v*v'` を再 factorization して `Cholesky`
object を返す。`*!` 版は既存 wrapper の `L` / `U` matrix field へ結果を書き戻す。

検証: upstream Julia `matrix_equations_lowrank_7470.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/matrix_equations_lowrank_7470.jl`、
`bash scripts/check_fixture_test_names.sh`、
`bash scripts/check_workarounds_documented.sh`、
`bash scripts/check_workarounds_sync.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests linalg::`。

### LinearAlgebra structured matrix wrappers and UniformScaling ✅ (Issue #7462)

`UniformScaling` / `I` と、`Symmetric` / `Hermitian` / triangular wrappers /
`UpperHessenberg` / `Bidiagonal` / `Tridiagonal` / `SymTridiagonal` /
`Transpose` / `Adjoint` を追加した。wrapper は upstream と同名の public
fields を保持し、dense subset の `size` / `getindex` / matrix multiplication
と `I * A` / `A * I` をサポートする。

検証: upstream Julia `structured_uniform_scaling_7462.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/structured_uniform_scaling_7462.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests linalg::`。

### LinearAlgebra remaining decomposition family objects ✅ (Issue #7465)

`Schur` / `GeneralizedSchur` / `Hessenberg` / `LQ` / `LDLt` /
`BunchKaufman` / `GeneralizedEigen` / `GeneralizedSVD` と、`schur` /
`schur!` / `ordschur` / `ordschur!` / `hessenberg` / `hessenberg!` /
`lq` / `lq!` / `ldlt` / `ldlt!` / `bunchkaufman` / `bunchkaufman!` を追加した。

`schur` は既存 `eigen` wrapper を再利用して symmetric dense matrix の
`Z*T*transpose(Z)` reconstruction を満たす。その他の families は upstream-shaped
result fields と small dense wrapper surface を提供し、specialized LAPACK work
storage は future performance/decomposition scope に残す。

検証: upstream Julia `decomposition_families_7465.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/linalg/decomposition_families_7465.jl`、
`bash scripts/check_fixture_test_names.sh`、
`timeout 1800 cargo nextest run --release --test fixture_tests linalg::`。

### Module-local macro visibility and qualified Base.isexpr ✅ (Issues #7525, #7527)

同一 module body 内で先に定義された macro を後続 statement macro call から参照できるようにした。
macro call を含む module body statement だけ active macro context 付きで lower し、通常の
module statement / closure helper resolution は従来経路を維持する。

また、qualified `Base.isexpr(...)` は bare `isexpr(...)` として import guard にかけず、
Base method table 経由で dispatch するようにした。

検証: `cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/modules/module_macro_visible_same_body_7525.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/reflection/base_isexpr_qualified_7527.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests modules::`、
`timeout 1800 cargo nextest run --release --test fixture_tests reflection::`。

### Include macro context sharing ✅ (Issue #7510)

`include("a.jl"); include("b.jl")` のような sequential include で、先に include
された file が定義した macro を後続 include の lowering 中に参照できるようにした。
function body と ternary branch の expression lowering に macro context を渡す一方、
macro call を含まない module function は従来経路のままにして module-local closure
helper resolution を維持する。

検証: upstream Julia equivalent include sequence、
`../target/release/sjulia -e 'include("tests/fixtures/modules/include_macro_shared_7510_a.jl"); include("tests/fixtures/modules/include_macro_shared_7510_b.jl"); include_macro_shared_7510_result() == false'`、
`timeout 1800 cargo nextest run --release --test fixture_tests modules::`。

### Persistent Base cache compiler fingerprint ✅ (Issue #7515)

Base bytecode cache の compatibility hash を `get_base()` source hash と
build-time Rust source fingerprint の combined SHA-256 に変更した。persistent cache
filename と serialized header validation の両方で同じ hash を使うため、compiler/VM
変更後に stale Base bytecode を再利用しない。

検証: stale pre-fix cache が残った状態で
`./target/release/sjulia -e 'f(re::Regex, s)=occursin(re,s); x=f(r"\\d+", "abc123"); true'`
が pass。`timeout 1800 cargo nextest run --release --lib compile::precompile`、
`timeout 1800 cargo nextest run --release --test fixture_tests regex::`。

### VM regex match arity dispatch guard ✅ (Issue #7502)

`match` の compile-time regex builtin handler を 2-arg call のみに制限した。
`match(a, b, c)` のような user-defined non-regex arity は handler error ではなく
通常の method dispatch に fall through する。

検証: `julia --startup-file=no --history-file=no
subset_julia_vm/tests/fixtures/regex/regex_match_user_method_7502.jl`、
`cargo build --release --bin sjulia --features repl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/regex/regex_match_user_method_7502.jl`、
`bash scripts/check_fixture_test_names.sh`。`fixture_tests regex::` は既存の
`regex_occursin_needle_5705` failure で blocked (Issue #7515)。

### Plots Aizawa attractor push! hot path ✅ (Issue #7431)

Plots の `push!(plt, x, y[, z])` が毎回 series vector 全体をコピーしていた問題を修正した。
`plot` / `plot3d` / `scatter` などで series data を plot-owned buffer にコピーし、
以後の point append は buffer へ直接 `push!` する。これにより Aizawa attractor の
3000-6000 step animation は linear growth になり、元の `xs` / `ys` / `zs` 配列を
変更しない copy semantics も維持する。

検証: `packages_plots_push_xy_point_7271` に元配列非破壊 regression を追加。
`vm_aizawa_plot_push_benchmark` で Plotly push hot path を継続計測する。

### AoT call/control-flow contract sync ✅ (Issues #7032, #7043, #7047, #7053, #7054, #7055)

AoT の varargs / splatting、broadcast fusion、first-class functions、do-block、
closures / lambdas、`try` / `catch` / `finally` の milestone 29 parent scope を
`docs/aot/CALL_CONTROL_FLOW_CONTRACTS.md` と support matrix に固定した。
static native path と runtime helper boundary を分け、未接続 helper が必要な箇所は
span diagnostic gate のままにする。

検証: docs/support matrix sync。runtime tuple packing、broadcast helper、callable handle、
closure environment call shim、exception status propagation の実体は未有効化で、
既存 gate を維持。

### AoT let-binding and HOF inference regression fixes ✅ (Issue #7495)

`convert(Int64, x)` の inference が `Int64` constructor function の型を返し、
typed local slot を `Function{(Any)->Int64}` にしていた regression を修正した。
concrete type-name target はその static type として扱い、`convert(Any, x)` は従来どおり
value type を保持する。

`reduce(f, arr)` は reducer return type が `Any` の場合に static collection element type
へ fallback するようにし、inline lambda reduce の `Any` → concrete slot 代入エラーを
解消した。

検証: `timeout 1800 cargo nextest run --release -p subset_julia_vm --features aot
convert_call_inference_uses_type_name_not_constructor_function_issue_7495
reduce_return_type_falls_back_to_collection_element_issue_7495
test_aot_e2e_multiple_dispatch_nested_calls test_aot_e2e_reduce_with_lambda
test_aot_e2e_static_function_call_nested`。

### AoT lowered operator-call inference fix ✅ (Issue #7504)

`%` / `mod` と `÷` / `div` の lowered call inference を binary operator typing に接続した。
Collatz pipeline の modulo comparison は `Bool`、integer division assignment は `Int64`
として推論される。

検証: `operator_call_inference_types_collatz_condition_issue_7504`、
`test_e2e_pipeline_collatz_sequence`。

### AoT fresh let binding slot type fix ✅ (Issue #7506)

fresh local declaration の slot type では converted value type を優先し、同名の
global/inference env entry が local `let` binding を汚染しないようにした。
`Box{Int64}` / `Box{Float64}` の parametric struct constructor local は stale `Set{Any}`
slot ではなく concrete struct slot で生成される。

検証: `test_aot_parametric_struct_codegen_7040`。最終確認として
`timeout 1800 cargo nextest run --release --test aot_e2e_tests --features aot`
は 225/225 pass。

### AoT top-level for-loop compound assignment fix ✅ (Issue #7416)

AoT DCE が nested `for` body 内の `total += x` を dead store と誤判定し、
generated Rust の loop body を空にしていた問題を修正した。nested block の
dead-store elimination を outer liveness なしに走らせないようにし、outer block 側の
conservative liveness で loop body mutation を保持する。

あわせて array `for` codegen を `.iter().cloned()` に変更し、loop variable が
AoT element type と同じ owned Rust value になるようにした。

検証: `cargo run --bin juliars --features aot -- --minimal-prelude -o /tmp/aot_array_for_7416_fixed.rs -e ...`、
`timeout 1800 cargo nextest run --release -p subset_julia_vm --features aot
dce_keeps_loop_body_mutation_read_after_loop_issue_7416 test_aot_codegen_for_each_array
issue_7416_top_level_for_compound_assign_preserves_body`。

### AoT C ABI and runtime numeric contracts ✅ (Issues #7077, #7056)

AoT C ABI export の non-scalar return shape と BigInt / BigFloat / Rational /
Irrational の runtime numeric contract を
`docs/aot/ABI_AND_NUMERIC_CONTRACTS.md` に固定した。String / Array view、owned
handle、struct out-param、opaque `SjuliaValue*`、runtime numeric handle の境界を
明文化し、silent narrowing を拒否する gate 方針を support matrix に反映した。

検証: docs/support matrix sync。runtime helper 実体と non-scalar export codegen は未有効化で、
既存 gate を維持。

### AoT map/filter generated Rust expectation refresh ✅ (Issue #7421)

#7070 の AoT E2E regression tests と codegen unit test の filter 期待文字列を、
現在の owned iterator based codegen に合わせた。`map` / `filter` の generated Rust
surface は non-Copy `String` element を `Vec<String>` のまま保持し、filter predicate
には cloned value を渡すことを引き続き検証する。

検証: `timeout 1800 cargo nextest run --release --test aot_e2e_tests --features aot
issue_7070_named_hof_functions_survive_dce_and_keep_types
issue_7070_string_map_filter_keep_non_copy_element_types`、
`timeout 1800 cargo nextest run --release -p subset_julia_vm --features aot
map_filter_clone_non_copy_elements_issue_6957_6958`。full `aot_e2e_tests` は
#7421 対象 test 通過後、別件の type-unstable let binding regression で失敗
(Issue #7495)。

### Pure Julia exp(::Real) VM hot-loop fix ✅ (Issue #7455)

`exp(::Float64)` の Pure Julia scale-back を `2.0 ^ k` から integer exponent
bit reinterpret helper に変更し、VM hot loop の generic power dispatch を除去した。
`exp(-745.0)` の subnormal 境界は upstream Julia と同じ `5.0e-324` に修正し、
`exp(::Bool)` は Real forwarding として `Float64` 経由で動くようにした
(Issue #7484)。

検証: `julia --startup-file=no --history-file=no
subset_julia_vm/tests/fixtures/math/exp_real_upstream_shape_7455.jl`、
`./target/release/sjulia subset_julia_vm/tests/fixtures/math/exp_real_upstream_shape_7455.jl`、
`timeout 1800 cargo nextest run --release --test fixture_tests math::`、
`cargo bench -p subset_julia_vm --bench vm_exp_real_benchmark -- --quick`。

### Cranelift milestone-29 parent surface sync ✅ (Issues #7081, #7080, #7079)

Cranelift milestone 29 の親 issue を support matrix / README / VM docs に同期した。
`--emit-binary --backend cranelift` は Cranelift object emission と system linker driver
を通じて native executable を生成する。runtime `Value` rooting / safepoint は
`CRANELIFT_GC_ROOTING_CONTRACT.md` の managed runtime value contract として固定済み。
globals / struct / enum lowering は scalar initialized globals、scalar-field stack
struct、Int32-backed enum metadata/member lowering の範囲で matrix に紐付け済み。

検証: docs/support matrix sync。heap/runtime-shaped Cranelift codegen は既存 gate を維持。

### Cranelift varargs / kwargs call adapter contract ✅ (Issue #7118)

Cranelift varargs / kwargs call adapter contract を
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に固定した。static splat expansion、
true varargs tuple packing、keyword canonicalization、keyword adapter symbol key、
dynamic keyword splat の NamedTuple packing、exception propagation rule を定義した。

検証: docs/support matrix sync。adapter generation、runtime tuple / NamedTuple helper
実体、Cranelift helper import/binding は未有効化で、既存 gate を維持。

### Cranelift Array / Vector heap lowering contract ✅ (Issue #7098)

Cranelift Array / Vector lowering contract を
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に固定した。`SjuliaArray*` の runtime
ABI header、element carrier、allocation hook、`length` / `size`、1-based /
column-major indexing、bounds / overflow failure の exception transition を定義した。

検証: docs/support matrix sync。runtime helper 実体、Cranelift helper import/binding、
managed element write barrier は未有効化で、既存 gate を維持。

### Cranelift exception / unwinding model ✅ (Issue #7108)

Cranelift exception propagation strategy を
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に固定した。native unwinding /
landing pad / `setjmp`-`longjmp` ではなく、`SjuliaCallStatus` と
`SjuliaGcContext` の pending exception state を使う Result-style ABI とする。
`try` / `catch` / `finally` / `throw` の lowering 形、exception helper ABI、C ABI
export boundary の扱いも同じ contract に定義した。

検証: docs/support matrix sync。exception helper 実体と Cranelift codegen は未有効化で、
既存 gate を維持。

### Cranelift runtime Value / Any / Union boundary ✅ (Issue #7102)

Cranelift runtime `Value` boundary contract を
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に追加した。`Any` / multi-variant
`Union` は opaque GC-managed `SjuliaValue*` handle とし、boxing/unboxing helper、
runtime tag check、rooting rule、Union narrowing / re-boxing rule を固定した。

検証: docs/support matrix sync。runtime `Value` codegen は未有効化で、既存 gate を維持。

### Cranelift String / Array ownership model ✅ (Issue #7107)

Cranelift non-Copy heap value ownership model を
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に固定した。heap String / Array は
GC-managed pointer handle、handle copy は非 owning reference copy、borrowed
byte/buffer pointer は safepoint をまたがない、という規則にした。read-only String
literal payload は object/JIT data section 所有として GC root 対象外に分離した。

検証: docs/support matrix sync。heap String / Array lowering は未有効化で、既存 gate を維持。

### Cranelift stack map / precise safepoint contract ✅ (Issue #7106)

Cranelift precise safepoint metadata contract を
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に追加した。function-scoped
safepoint ID、root slot descriptor、frame-base offset、Cranelift stack map または
explicit root-stack fallback の許可条件を固定した。

検証: docs/support matrix sync。managed-value lowering は未有効化で、既存 gate を維持。

### Cranelift heap allocation hook ABI ✅ (Issue #7105)

Cranelift heap allocation runtime hook ABI を
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に固定した。raw allocation、
array allocation、string allocation の C-ABI import symbol、fixed-width
parameter carriers、null-on-failure、runtime symbol binding、allocating safepoint
classification を明文化した。

検証: docs/support matrix sync。heap allocation call emission は未有効化で、既存
gate を維持。

### Cranelift GC/rooting and safepoint contract ✅ (Issue #7104)

Cranelift backend の GC/rooting design contract を
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に追加した。contract は
`SjuliaGcContext*`、allocation hooks、root stack slots、safepoints、stack maps、
GC-managed ownership を前提にし、native scalar / native stack aggregate /
read-only data pointer と managed runtime pointer の境界を固定する。

検証: docs/support matrix sync。runtime heap lowering は未有効化で、既存 gate を維持。

### Cranelift Complex aggregate arithmetic lowering ✅ (Issue #7099)

Cranelift backend で local `Complex` / `ComplexF64` / `Complex{Float64}` と
`ComplexF32` / `Complex{Float32}` を stack aggregate pair として扱えるようにした。
constructor は builtin Complex layout の `StructNew`、`real` / `imag` は field
offset load、`abs2` と同一 element type の `+` / `-` / `*` は scalar field
arithmetic へ lower する。

検証: #7099 unit regression、Cranelift JIT execution regression、minimal source-level
`--check` / `--jit-run` smoke。

### Cranelift String constant data payload lowering ✅ (Issue #7094)

Cranelift backend で local `String` literals を read-only data section の
length-prefixed UTF-8 payload へ lower できるようにした。`LoadConst(String)` は
payload 先頭の symbol pointer を生成し、JIT / object backend の両方で同じ data
declaration path を使う。`length(::String)` は payload 先頭の byte length を
native `Int64` として読む。

検証: #7094 unit regression、object payload regression、Cranelift verifier/codegen
regression、minimal source-level `--check` / `--jit-run` smoke。

### Cranelift struct field layout lowering ✅ (Issue #7095)

Cranelift backend で non-parametric scalar-field struct definitions を受け入れ、
Julia-compatible な field offset / size / alignment を計算して stack slot に lower
できるようにした。`StructNew`、byte offset 付き `GetFieldOffset` /
`SetFieldOffset` を low-level IR に追加し、construction / field load / mutable
field store を Cranelift memory ops へ接続した。

検証: #7095 unit regression、Cranelift verifier/codegen/JIT execution regression。

### Cranelift multiple return / destructuring lowering ✅ (Issue #7117)

Cranelift backend で tuple-returning scalar functions を Cranelift multi-result
signature と low-level IR の `ReturnMany` / `CallMulti` に lower できるようにした。
destructuring assignment は既存の temp tuple + constant tuple index lowering を使い、
tuple-returning static call の各 field carrier へ直接接続する。

検証: #7117 unit regression、Cranelift verifier/codegen/JIT execution regression。

### Cranelift DWARF debug info output ✅ (Issue #7090)

`juliars --backend cranelift --debug-info` を追加し、Cranelift object / binary /
library output へ DWARF debug sections を emit できるようにした。object path は
`gimli` で `.debug_abbrev` / `.debug_info` / `.debug_line` などを生成し、Core IR span
由来の function-level line を compile unit line table と subprogram DIE に記録する。

CLI は `--debug-info` を Cranelift native artifact output に限定し、Rust backend、
`--check`、JIT-only 実行との誤用を usage error にする。

検証: #7090 CLI parse regression、Cranelift object DWARF section regression、
Core-span-to-object pipeline regression。

### Cranelift static/shared library output ✅ (Issue #7085)

`juliars --backend cranelift --emit-library <path>` を追加し、Cranelift object
output から library artifact を生成できるようにした。既定の
`--library-kind static` は `ar crs` で static archive を作り、`--library-kind shared`
は `aot::linker` の shared-library mode で `.so` / `.dylib` / `.dll` 相当の artifact
を system linker に生成させる。

`--emit-library` は `--emit-binary` / `--emit-object` / `--check` / `--jit-run` /
`-o` との併用を usage error にする。外部公開 symbol は既存の Cranelift
`--export-c-abi` wrapper surface を使い、現行 scalar/object subset の C-stable symbol
として library に含まれる。

検証: #7085 CLI parse / linker plan regression、Cranelift release nextest、host CLI
static/shared library smoke。

### Cranelift `--emit-binary` object-to-link path ✅ (Issue #7083)

`juliars --backend cranelift --emit-binary <path>` が Cranelift object output を
一時 object file へ書き、`aot::linker` で native executable を生成するようになった。
`--target` は object target と linker target family の両方へ渡す。Cranelift binary
path では Rust source 出力がないため `-o/--output` と `--check` の併用は usage error
にする。

検証: #7083 parse regression、Cranelift release nextest、host CLI binary smoke。

### Cranelift standalone executable entry wrapper ✅ (Issue #7084)

Cranelift lowering が `__juliars_main` に加えて C ABI の `main() -> Int32`
wrapper を生成するようにした。wrapper は `__juliars_main()` を呼び出し、終了コード
`0` を返すため、object output が standalone executable 用の entry symbol を持つ。

検証: #7084 standalone main wrapper regression、Cranelift release nextest。

### Cranelift system linker / lld driver planning ✅ (Issue #7089)

`aot::linker` を追加し、Cranelift object output を system linker へ渡すための
再利用可能な invocation planner / executor を用意した。C driver、Unix `ld` /
`ld.lld`、MSVC `link.exe` / `lld-link` を分類し、object、runtime library、libc/libm
などの system library 順序を platform family ごとに固定する。

検証: Linux cc/lld、Darwin cc、Windows MSVC link の linker plan regression。

### AoT `Dict` construction / lookup / iteration codegen ✅ (Issue #7034)

Rust backend で `Dict("a" => 1)` などの静的 Pair 由来 Dict と `Dict{K,V}()` を
`std::collections::HashMap<K,V>` carrier へ生成できるようにした。`d[k]` /
`get(d, k, default)` / `haskey(d, k)` / `d[k] = v` は native `HashMap` API へ
lower し、`length(d)` / `isempty(d)` / `collect(d)` / `for kv in d` は collection
iterator 経路を使う。

検証: upstream Julia / VM MWE stdout 比較、AoT analyze/type/builtin unit、AoT E2E
generated Rust warning-deny check。

### Cranelift ELF / Mach-O / COFF object smoke coverage ✅ (Issue #7088)

Cranelift object output に representative triple ごとの object format regression を追加。
`x86_64-unknown-linux-gnu` は ELF、`x86_64-apple-darwin` は Mach-O、
`x86_64-pc-windows-msvc` は COFF の header magic と export symbol を確認する。

検証: #7088 ELF/Mach-O/COFF ObjectModule regression、代表 triple CLI smoke。

### AoT `Set` construction / membership / iteration codegen ✅ (Issue #7035)

Rust backend で `Set([1, 2, 2])` などの静的 iterable 由来 Set と
`Set{T}()` を `std::collections::HashSet<T>` carrier へ生成できるようにした。
`push!` は `insert`、`in` は `contains`、`collect(s)` / `length(s)` /
`isempty(s)` / `for x in s` は native iterator 経路を使う。

検証: VM / generated native binary MWE stdout 比較、`juliars --minimal-prelude
--check`、AoT analyze/type/builtin unit、AoT E2E generated Rust warning-deny check。

### Cranelift object target triple selection ✅ (Issue #7087)

Cranelift object output が `--emit-object --backend cranelift --target <triple>` を
受け、`target-lexicon` の triple を Cranelift ISA lookup に渡して object target を選ぶ
ようになった。host 固定の ObjectModule 生成から、明示 target 指定可能な object emission
へ広げた。

検証: #7087 explicit host target ObjectModule regression、CLI parse regression。

### Cranelift C ABI object export symbols ✅ (Issue #7086)

Cranelift object output が C-stable scalar / `Nothing` signature の
`--export-c-abi` を受理し、指定 symbol の wrapper を ObjectModule に export するように
なった。alias export は同じ Cranelift low-level signature の wrapper から対象関数を
forward する。

検証: #7086 Cranelift ObjectModule export symbol regression、CLI parse regression。

### Cranelift relocatable object output path ✅ (Issue #7082)

Cranelift backend が `cranelift-object::ObjectModule` による relocatable object
bytes の出力経路を持つようになった。`juliars --backend cranelift --emit-object
<path>` は既存の AoT preparation / Cranelift lowering を通した後、リンクせず `.o`
を直接書き出す。

検証: #7082 ObjectModule smoke regression、CLI `--emit-object` parse regression。

### Cranelift scalar global constant lowering ✅ (Issue #7103)

Cranelift backend が initialized scalar top-level globals を受理し、関数や
`__juliars_main` 内の global `Var` 参照を initializer の read-only constant として
lower するようにした。heap/runtime `Value` global は導入せず、現在の scalar Cranelift
surface に限定して扱う。

検証: #7103 scalar global constant Cranelift JIT regression、未初期化 global gate。

### Cranelift tuple local field projection ✅ (Issue #7097)

Cranelift AoT lowerer が local tuple literal を scalar field carrier に分解し、
定数 tuple index (`t[2]`, `(x, y)[1]`) を選択 field の `VarRef` へ投影するようにした。
tuple object / tuple ABI は導入せず、現在の scalar Cranelift surface 内で field access
を扱う。

検証: #7097 tuple local / literal field access Cranelift JIT regression。

### AoT parametric struct definition/codegen ✅ (Issue #7040)

Rust backend で user parametric struct を generic Rust struct として生成し、
explicit constructor (`Box{Int64}(...)`) と default constructor inference (`Box(...)`)
を concrete instantiation へ lower できるようにした。field access は type parameter
substitution 後の concrete field type を返す。

DCE は `Box{Int64}` のような parametric constructor call から bare `Box` 定義を保持し、
未使用 Base parametric structs は引き続き skip する。

検証: upstream Julia / VM / generated native binary MWE stdout 比較、`juliars
--minimal-prelude --check`、AoT analyze/call-graph unit、AoT E2E generated Rust
warning-deny check。

### Cranelift `@enum` Int32-backed scalar lowering ✅ (Issue #7096)

Cranelift backend で AoT enum definitions を metadata として許可し、enum member
参照を `Int32` backing value の `LitI32` へ fold するようにした。`@enum Color red
green blue` の `green` は Cranelift scalar codegen では `1_i32` carrier として扱われる。

検証: #7096 source-level Cranelift enum member regression、既存 AoT enum E2E。

### Cranelift short-circuit `&&` / `||` CFG lowering ✅ (Issue #7115)

Bool `&&` / `||` を Cranelift AoT lowerer で eager logical op にせず、
branch + short-circuit constant + join phi の CFG として lower するようにした。
RHS は Julia の短絡規則で評価される path に入った場合だけ lower される。

検証: #7115 AOT lowerer CFG regression、Cranelift Branch/Phi truth-table JIT regression。

### Cranelift Float16 widened scalar lowering ✅ (Issue #7093)

`StaticType::F16` を Cranelift `F32` carrier に投影し、Rust backend と同じ
Float16 widened codegen surface に揃えた。F16 typed parameter / return / scalar
binop は Cranelift verifier/codegen を通り、JIT では `fn(f32, ...) -> f32` として
実行できる。`sqrt` などの libm 経路も F16 を `*f` symbol へ流す。

検証: #7093 F16 widened carrier Cranelift JIT regression、AOT lowerer regression。

## 最新対応 (2026-06-23)

### AoT parameterized `Complex{T}` arithmetic ✅ (Issue #7041)

Rust backend の `Complex` を default type parameter 付き `Complex<T = f64>` として
生成し、既存 Float64 `Complex` に加えて `Complex{Float32}` / `Complex{Int64}` などの
primitive numeric parameterized Complex constructor と `+` / `-` / `*` を扱えるようにした。
`real` / `imag` / `abs2` も generic helper で element type を保持する。

検証: upstream Julia / VM / generated native binary MWE stdout 比較、`juliars
--minimal-prelude --check`、AoT codegen unit、AoT E2E、既存 mandelbrot complex/broadcast
regression。

### AoT `rand` / `randn` RNG codegen ✅ (Issue #7036)

`rand()` / `randn()` と `rand(dims...)` / `randn(dims...)` を Rust backend で
生成できるようにした。生成 Rust は runtime crate 経由で VM の `StableRng` と
`randn` helper を共有し、thread-local `StableRng::new(42)` を進めるため、bare RNG
stream は VM CLI と一致する。次元付き form は同じ stream から nested `Vec` carrier を
構築する。

検証: VM/AoT MWE stdout 比較、`juliars --minimal-prelude --check`、生成 native binary、
AoT codegen unit、AoT E2E。

### Cranelift I128 / U128 scalar lowering ✅ (Issue #7092)

`StaticType::I128` / `StaticType::U128` を Cranelift `I128` carrier に投影し、
AOT lowering gate でも 128bit integer を scalar subset として許可した。U128 は
unsigned integer 判定に含め、logical shift / unsigned div-rem 経路へ流す。x64 JIT の
i128 args/return ABI には Cranelift LLVM ABI extension が必要なため、ISA flags で有効化。

検証: #7092 i128/u128 Cranelift scalar ops JIT regression、AOT lowerer regression。

### Cranelift Char scalar lowering ✅ (Issue #7101)

`Char` を Cranelift 側で i32 codepoint carrier として lower する regression を追加。
`StaticType::Char` は Cranelift `I32`、`AotExpr::LitChar` は `ConstValue::Char` 経由の
`iconst.i32` になり、`Char` parameter / return を持つ static call も verifier/codegen
まで通る。

検証: #7101 Char scalar lowering Cranelift regression。

### AoT lazy Range / Char range codegen ✅ (Issue #7039)

Range literal を `Vec<T>` へ即時 materialize せず、生成 Rust の
`SjuliaRange<T>` / `SjuliaCharRange` carrier と iterator へ下ろすようにした。
`collect(r)`、range 入力の comprehension、`sum` / `map` / `filter` / `reduce` /
`mapreduce` は reusable な range binding を clone して走査する。

Char unit range (`'a':'c'`) は AoT codegen で扱えるようになった。step 付き Char range
は Julia `Char` と Rust `char` の表現差が残るため gate を維持する。

検証: upstream Julia MWE、`juliars --minimal-prelude --check`、生成 native binary、
AoT codegen unit、AoT E2E。

### AoT generator expression codegen ✅ (Issue #7046)

`f(x) for x in xs` を AoT IR の generator carrier として扱い、Rust backend で
typed boxed iterator (`Box<dyn Iterator<Item = T>>`) へ下ろすようにした。
`collect(generator)` / `sum(generator)` は lazy iterator を直接消費し、`if` filter 付き
generator は `filter_map` を生成する。range source は lazy range のまま、array source は
cloned iterator として扱う。

検証: upstream Julia MWE、`juliars --minimal-prelude --check`、生成 native binary、
AoT E2E。

### AoT 3D+ array codegen ✅ (Issue #7033)

Rust backend の static-rank array codegen を 3D 以上へ拡張した。`zeros(dims...)` /
`ones(dims...)` と 3D+ array literal は nested `Vec` carrier を生成し、`length` /
`size` / `size(A, dim)` / `ndims` は static rank から shape を計算する。
`A[i,j,k,...]` と `A[linear]` は 1-based bounds check と column-major linear index
decomposition を行う。

検証: upstream Julia MWE、`juliars --minimal-prelude --check`、生成 native binary、
AoT E2E。

### Cranelift unsupported gate diagnostics ✅ (Issue #7129)

低レベル Cranelift codegen の `CraneliftError::Unsupported` を backend 境界で
`AotError::UnsupportedInstruction` に変換し、CLI diagnostic kind と workaround を
AoT gate と揃えた。source span は現行 AoT IR / low-level IR が保持する範囲に制約される。

検証: #7129 unsupported diagnostic mapping regression。

### Cranelift backend benchmark helper ✅ (Issue #7127)

`scripts/aot_cranelift_backend_benchmark.sh` を追加し、fixture ごとに Rust backend
`--check`、Rust `--emit-binary`、生成バイナリ runtime/size、Cranelift `--check`、
Cranelift `--jit-run` の timing を TSV で比較できるようにした。Cranelift standalone
binary の runtime/size は object/linker path 完了後に拡張する。

検証: shell syntax check、空 stdout scalar fixture smoke (`ITERATIONS=1`)。

### Cranelift differential stdout harness ✅ (Issue #7126)

`scripts/aot_cranelift_fixture_differential.sh` を追加し、fixture stdout を upstream
Julia、Rust backend generated binary、Cranelift JIT の 3 経路で exact diff できるようにした。
Cranelift 側は #7131 の `--jit-run` を使い、unsupported feature は通常の Cranelift gate
として失敗を見せる。

検証: shell syntax check、空 stdout scalar fixture smoke。

### Cranelift desktop opt-in JIT execution path ✅ (Issue #7131)

`juliars --backend cranelift --jit-run` を追加し、Cranelift の in-process JIT module
から `__juliars_main` を明示的に呼べるようにした。`--jit-run` は `--check` /
`--emit-binary` / `-o` と排他で、Rust backend では拒否する。成功時は余計な stdout
を出さず、`--stats` / `--time-passes` / `--dump-aot-stage` は従来の CLI surface と
同じ形で利用できる。

検証: #7131 JIT main regression、CLI parser regression、`juliars --jit-run` smoke。

### AoT array comprehension codegen ✅ (Issue #7045)

`[f(x) for x in xs]` / `[f(x) for x in xs if cond]` /
`[f(i, j) for i in xs, j in ys]` を AoT IR の comprehension carrier として
扱い、Rust backend で concrete `Vec<T>` build の block expression へ下ろすようにした。
body/filter/iterator は comprehension-local な loop variable type を使って変換するため、
静的 element type を保ったまま `Value::from(())` placeholder へ落ちない。

検証: upstream Julia MWE、`juliars --minimal-prelude --check`、AoT E2E。

### Cranelift lowering property / fuzz regression ✅ (Issue #7128)

Cranelift scalar subset 向けに deterministic pseudo-random AoT IR generator を追加。
生成した `Int64` / `Bool` expression program を Rust backend codegen と Cranelift
lowering の両方へ通し、Cranelift 側は verifier/codegen まで実行して invalid CLIF を
早期検出できるようにした。

検証: #7128 Cranelift lowering property regression。

### AoT NamedTuple construction / field access ✅ (Issue #7049)

`(a=1, b=2)` を `StaticType::NamedTuple` / `AotExpr::NamedTupleLit` として
推論・AoT IR・optimizer/rooting/verifier/codegen へ通し、Rust backend では
field-ordered tuple carrier に投影するようにした。`.field` access は AoT IR 変換で
tuple field index へ静的変換されるため、`nt.a + nt.b` は dynamic dispatch なしで
生成される。

検証: upstream Julia MWE、`juliars --minimal-prelude --check`、AoT E2E。

### AoT tuple destructuring rest/splat tail ✅ (Issue #7391)

Tuple destructuring の top-level final rest target `a, rest... = xs` を AoT E2E で固定。
lowering は RHS temp から内部 `#__sjulia_tuple_tail__` を作り、AoT 変換で静的 tuple 型に
基づく tuple tail field reads へ展開する。`rest` は concrete Rust tuple になり、
`rest[1]` / `rest[2]` の後続 access も静的 field access で codegen される。

検証: tuple rest destructuring AoT E2E。

### AoT tuple return / nested destructuring codegen ✅ (Issue #7048)

Tuple return と destructuring assignment を AoT E2E で固定。`(a, b) = f()` の basic
shape に加え、`a, (b, c) = f()` を lowering で recursive indexed reads に展開し、
Rust tuple return / field access として codegen できるようにした。rest/splat target は
Issue #7391 に分離。

検証: nested tuple return destructuring AoT E2E。

### Cranelift bit operations / shift lowering ✅ (Issue #7120)

Cranelift backend で整数 `&` / `|` / `xor` / `~` / `<<` / `>>` を低レベル
Cranelift IR へ下ろす regression を追加し、右 shift は signed/unsigned で
`sshr` / `ushr` を使い分けるようにした。mixed-width shift count は left/result
幅へ extend/reduce して verifier-safe にした。

検証: #7120 bitwise / shift / bitnot Cranelift JIT regression。

### AoT typed overload source signature projection ✅ (Issue #7387)

同一 arity の typed overload で `IrConverter` が常に最初の typed method を選び、
Float64 overload まで Int64 signature として AoT IR 化される問題を修正。明示 annotation
付き関数では宣言側 parameter signature に一致する `TypedFunction` を優先する。

検証: typed overload source の AoT E2E generated Rust regression。

### AoT C ABI export overload signature resolution ✅ (Issue #7078)

`--export-c-abi` に `symbol=function(Int64,Float64)` 形式を追加し、overload された
AoT method を original name + 引数型で C ABI export できるようにした。top-level comma
区切りで複数 specs も一括指定できる。既存の direct symbol / `symbol=function` /
generated method 名 alias は維持。Julia source typed overload projection の別 bug は
Issue #7387 で修正済み。

検証: C ABI codegen unit、CLI parser unit、AoT E2E generated Rust check。

### Cranelift libm 数学組込みを拡張 ✅ (Issue #7122)

Cranelift backend の libm 宣言と lowering を拡張し、`sqrt` / `sin` / `cos` /
`exp` / `log` / `abs` を `Instruction::Call` から libm または Cranelift scalar
IR へ下ろせるようにした。`Float64`/`Float32` の libm symbol を登録し、
AoT `CallBuiltin` も同じ call lowering 経路へ接続。未知 runtime-checked call の
Issue #7111 gate は維持した。

検証: #7122 libm unary / abs / AoT builtin lowering regression。

### Cranelift IR verifier を compile 前に統合 ✅ (Issue #7125)

Cranelift backend で `FunctionBuilder::finalize()` 後、`define_function` 前に
`Context::verify(self.module.isa())` を実行するようにした。不正な CLIF は native compile
前に `FunctionCompilation` diagnostic として返る。Cranelift support matrix も対応済みに更新。

### AoT global state / redefinition policy gate ✅ (Issue #7061)

AoT の静的 program 前提と衝突する top-level `const` marker / const 再代入、関数内
`global` mutable state、同一 signature の関数再定義(world-age 依存)を
Core IR -> AoT IR 変換で span 付き `UnsupportedInstruction` にした。これにより
`juliars --check` が成功した後に生成 Rust が未定義 `___sjulia_declare_const__` /
未定義 global 参照で失敗するケースと、関数再定義で古い method を選ぶ silent mismatch を
事前診断へ移した。

検証: #7061 AoT E2E diagnostics、`juliars --check` smoke、AoT `cargo check`。

### `examples/test_intrinsic.rs` の CompiledProgram 初期化漏れを修正 ✅ (Issue #7383)

`CompiledProgram` の新フィールド `runtime_specialization_map` を
`subset_julia_vm/examples/test_intrinsic.rs` の手動 initializer に追加し、Cranelift feature
nextest の example build が通るようにした。

### Cranelift opt-level mapping を settings に反映 ✅ (Issue #7091)

`CodegenConfig` に `OptLevel` を追加し、`compile_program` から Cranelift backend へ
`-O0..-O3` を伝搬。Cranelift settings は `O0 -> none`、`O1/O2 -> speed`、
`O3 -> speed_and_size` にマップする。Cranelift support matrix も対応済みに更新。

### AoT 相互再帰 codegen と TCO 境界を対応 ✅ (Issue #7060)

相互再帰関数は Rust backend の通常 `fn` 間 static call としてそのまま生成する方針に固定。
Rust は関数定義順に依存せず相互呼び出し可能なため、AoT 独自の forward declaration は不要。
一方、optimizer の tail-call elimination は direct self-tail recursion のみに限定し、
`is_even ⇄ is_odd` のような mutual tail call は通常 call のまま保持する。

検証: mutual recursion AoT E2E、mutual tail call 非 TCO optimizer unit、
`juliars --check` / `--emit-binary` smoke。

### AoT timing macros の実行時測定方針を対応 ✅ (Issue #7059)

AoT の `@time` / `@elapsed` を no-op ではなく、生成バイナリ実行時の wall-clock
measurement として扱う方針に固定。`time_ns` builtin を AoT builtin へ変換し、
`time_ns()::Int64` と推論するようにした。`target = @elapsed ...` で現れる
macro-lowered `LetBlock` は副作用を保ったまま outer target へ最終値を代入し、CLI の
top-level 複数文も statement-position `LetBlock` に正規化して `juliars --check` 経路を
E2E helper と揃えた。

検証: `@time` / `@elapsed` AoT E2E、`juliars` CLI path unit、`juliars --check`
smoke、AoT analyzer/inference unit。

### Web/WASM Apollonian Gasket の compile freeze を修正 ✅ (Issue #7357)

Apollonian Gasket の Web/WASM 実行が compile 中に約105〜108秒停止していた問題を修正。
原因は `recurse!` の自己再帰で、`Top` 引数の解析中に `Circ` へ具体化された引数で同じ method
を呼ぶと、従来の full `(method, arg types)` key の cycle guard をすり抜けて同一関数を別解析
として再帰的に広げていたこと。`InferenceEngine` に method identity 単位の
`active_function_estimates` を追加し、refined arg key の自己再帰では現在の推定型を返すように
した。さらに Base cache に `specializable_functions` と `runtime_specialization_map` を保存し、
cached Base の `CallSpecialize` metadata を warm compile で復元するようにした。WASM MWE は
`gasket(15.0)` が約113s→約1.5s、`gasket(120.0)` が約108s級→約1.6sまで短縮し、出力
61/889 を確認。

### Cranelift backend 専用 support matrix を追加 ✅ (Issue #7130)

`docs/aot/CRANELIFT_SUPPORT_MATRIX.md` を追加し、Cranelift backend 固有の
CLI surface、生成物、scalar 型、control-flow、heap/runtime boundary、quality gate、
milestone issue 順 roadmap を Rust backend の一般 matrix から分離。`README.md` と
`SUPPORT_MATRIX.md` から参照するよう更新した。

### AoT generated Rust の `rustc -D warnings` clean gate を nextest 化 ✅ (Issue #7076)

generated Rust を実際の downstream Cargo crate として `RUSTFLAGS=-Dwarnings cargo check`
に通す `aot_e2e_tests` 回帰テストを追加。warning 誘発 source は `Float64 + Int64` の
redundant parens と top-level binding path を踏む。`scripts/test_aot.sh` の
generated-Rust clippy smoke も同じ source に更新し、`cargo clippy -- -D warnings` で監視。
`docs/aot/SUPPORT_MATRIX.md` の `rustc -D warnings clean guarantee` を対応済みに更新。

### マクロ展開まわりの複数バグを修正 ✅ (Issue #7350)

`@manipulate` 実装中に発見したマクロまわりの 4 件を修正(いずれも本家 julia では正常動作)。

- **(A1) 引数位置の三項演算子が `nothing` に評価される**: マクロが返す `if`/三項式が式
  位置で使われたとき、値を生まない `Stmt::If` に lowering されサイレントに `nothing` を
  返していた。修正=`macro_runtime.rs` の `expr_value_to_expr` の `:if` を値を生む
  `Expr::Ternary` に変更(各分岐は `value_to_branch_expr` で `block` の最終文の値を生む
  形へ)。文位置の `:if` は従来通り `Stmt::If`。
- **(A2) 添字代入 `a[i] = v` が無効**: quote 構築(`cst_to_constructor.rs`)が代入 LHS の
  `IndexExpression` を `Expr(:ref, ...)` でなく不正な `Symbol("a[i]")` にしていた。修正=
  代入ターゲットの再帰条件に `IndexExpression` を追加。併せて `macro_runtime.rs` の
  `value_to_stmt` の `=` で `:ref`→`Stmt::IndexAssign`、`:.`→`Stmt::FieldAssign` を処理。
- **(A3) 修飾呼び出し `Mod.f(...)` が呼び出しターゲットにできない**: `call_expr_from_values`
  が `Expr(:., Mod, QuoteNode(:f))` の callee を「unsupported call target Expr」で拒否して
  いた。修正=`:.` callee を `call_named_expr(Some(module), name, ...)` に振り分け。
- **(B5) `nothing` 初期化アキュムレータのループ内三項累積が壊れる**: `acc = nothing` →
  ループ内 `acc = acc === nothing ? x : g(acc, x)`(分岐が異種型→join が `Any`)で、
  pre-pass の `mixed_type_vars` マーキングが `ty != Any` で弾かれ acc が動的スロットに
  ならず、`acc = nothing` がスロットを `Nothing` に narrow → ループ本体で `acc === nothing`
  が常時 true に const-fold され、後続イテレーションの値が捨てられていた(最後の要素のみ
  残る)。修正=`inference.rs` で「旧型が具象の非数値型(`Nothing` 等)かつ新型が `Any` に
  widening」する場合も mixed としてマーク。同種型の三項は従来通り(既に動作)。

fixture: `macros/macro_if_expr_value_7350.jl` / `macro_indexed_assign_7350.jl` /
`macro_qualified_call_target_7350.jl` / `macro_loop_nothing_ternary_7350.jl`。
**(A4) は #7355 として切り出し、本日対応済み(下記)**。

### モジュール内マクロの解決＋ハイジーンを対応 ✅ (Issue #7355 / #7350 A4)

`module M ... macro m ... end end` で定義したマクロが、`using .M` 後の `@m(...)` でも
修飾呼び出し `M.@m(...)` でも `unknown macro @m` で解決できなかった(本家 julia は両方
動作)。さらに解決後も、マクロの非 esc 識別子が**呼び出し側スコープ**で解決されるため、
モジュールの未 export ヘルパを参照する `:(helper($v))` が `helper is not imported` で失敗
していた(本家は定義モジュールで解決)。

- **(1) 解決**: モジュール内マクロが `lambda_ctx` のマクロレジストリに登録されていな
  かった。修正=`lower_module_definition` に `macro_ctx: Option<&LambdaContext>` を追加し、
  モジュール本体の `MacroDefinition` を呼び出し側コンテキストに登録(パーサは `M.` 接頭辞を
  落とすため、`using` 経由・修飾呼び出しのどちらも bare 名で引ける)。sjulia はモジュール
  関数をフラットに hoist しているので、これで両形が解決する。
- **(2) ハイジーン**: 定義モジュールのメンバ名集合と「esc 深度」を `LambdaContext` の
  ハイジーンフレーム(`register_module_macro_hygiene` / `begin_macro_hygiene` /
  `enter_macro_esc`)で保持。`macro_runtime.rs` の `call_expr_from_values` で、esc 外かつ
  名前が定義モジュールのメンバなら呼び出しターゲットを `M.name` に修飾(修飾アクセスは
  visibility ゲートを迂回するため未 export メンバも解決)。`esc(...)` 部分木は深度カウンタ
  で修飾を抑止し、呼び出し側スコープ解決を維持。

fixture: `macros/module_macro_using_resolution_7355.jl`(export 経由+未 export ヘルパ)/
`module_macro_qualified_call_7355.jl`(`M.@m`)/ `module_macro_esc_7355.jl`(esc は呼び出し
側で解決)。いずれも本家 julia と出力一致。フルスイート 3929 passed / 0 failed。

### inner constructor 本体が無視されるバグを修正 ✅ (Issue #7345)

ユーザー定義 struct の inner constructor 本体が実行されず常に合成 default 構築へ
フォールバックしていたバグを修正(`Bar(x)=new(x*10)` で `Bar(5).x` が `5`、バリデーション
`Foo(x)=(x>0||error(...); new(x))` も無視)。原因=`compile/expr/call/constructors.rs` の
`try_compile_struct_table_constructor_call` の「フィールド数・型一致なら default constructor」
高速パスが inner constructor を持つ struct でも発動。修正=この高速パスを「宣言 constructor が
呼び出しにマッチしない場合のみ」に限定し、マッチすれば inner constructor のメソッド
ディスパッチへ回す。inner ctor を持たない struct(`Year` 系)は条件が常に false で従来通り。
REPL グローバル再構築(`Animation(frames)` 全フィールド再注入, #7151)は「マッチ無し→合成
default」経路で維持。fixture `struct/inner_constructor_untyped_field_body_7345.jl` を追加。

### Interact `@manipulate` の複数同時コントロール対応 ✅ (Issue #7344)

`@manipulate for a = …, b = … end` が動作。リアクティブ実行が無いため、選択肢の**直積を
1 つの結合ドロップダウン**で近似(ラベル `a=<va>, b=<vb>`、最内変数が最速)。マクロが
`forloop.args[1].head == :block` を検出してネストループに展開し、既存 dropdown レンダラを
再利用(Rust 変更不要)。fixture `packages/interact_manipulate_multi_control_7344.jl` +
artifact テスト。本家の N 独立コントロールとは異なる意図的近似(詳細は STATUS.md)。

### quote の複数 for-binding 対応 ✅ (Issue #7343)

`quote`/`:( … )` 内の `for a = …, b = …` を `Expr(:for, Expr(:block, :(a=…), :(b=…)),
body)`(本家表現)で構築できるようにした。`cst_to_constructor.rs` の `ForStatement` が
全 `ForBinding` を収集し複数なら block でラップ。汎用 `quote` 機能で、複数コントロール
`@manipulate`(#7344)の前提。fixture `metaprogramming/quote_for_multiple_bindings_7343.jl`。

### Interact `@manipulate` のレンジをスライダー描画 ✅ (Issue #7338)

本家 `widget()` の `AbstractRange → slider` に倣い、レンジ選択肢を Plotly スライダーで、
配列など離散選択肢を従来どおりドロップダウンで描画。`Manipulate` に `control::Symbol`
(`:slider`/`:dropdown`)フィールドを追加(2 引数の後方互換 outer constructor 併設)、
`@manipulate` 展開で export 済み実関数 `manipulate_control(choices)` により種別を判定し、
Rust `generate_plotly_manipulate_json` が `:slider` のとき `sliders`、それ以外で
`updatemenus` を出力。詳細(sjulia マクロ展開の制約と回避)は STATUS.md 参照。
`widget()` の checkbox/textbox/spinbox/Dict/Date/Color は Phase 3 据え置き(#7275)。

### Interact `@manipulate` の非プロット本体を明確にエラー化 ✅ (Issue #7338)

`@manipulate` の本体がプロット以外の値(数値・文字列など)を返すと、従来は
`Manipulate(Any[1,4,9], …)` を黙って構築し exit 0 で何も描画しない無言の失敗だった。
修正=`@manipulate` 展開時に各キャプチャ値が `Plot` か検証し、違反時に明確な
`error(...)` を投げる(`packages/Interact/src/Interact.jl`)。検証は本来 `Manipulate` の
inner constructor が自然だが sjulia は inner constructor 本体を無視するため(Issue
#7345)展開側で実施。fixture `packages/interact_manipulate_nonplot_errors_7338.jl`。
本家 Interact はリアクティブ widget で値を表示するため意図的な非パリティ(#7275 の Phase 2+)。

### 2D `plot`/`scatter` が `legend` キーワードを受理 ✅ (Issue #7337)

bundled Plots の 2D `plot` / `plot!` / `scatter` / `scatter!` に `legend=...` を渡すと
`MethodError: ... unsupported keyword argument "legend"` になっていた。本家 Plots.jl では
`legend` は 2D/3D 共通の普遍属性で、sjulia でも 3D の `plot3d`/`plot3d!` は `kwargs...`
を持つため受理して無視していたが、2D パスのメソッドは `kwargs...` を欠いていた。修正=
`packages/Plots/src/api.jl` の 2D `plot`/`plot!`/`scatter`/`scatter!` 全オーバーロードに
`kwargs...` を追加し、表示専用の未モデル化キーワードを 3D パスと同様に受理して無視する
ようにした。回帰テスト `tests/fixtures/packages/plots_plot_legend_kwarg_7337.jl` を追加。

## 最新対応 (2026-06-22)

### `::AbstractMatrix` パラメータが `Function` を緩マッチしない ✅ (Issue #7334)

`::AbstractMatrix`(= `AbstractArray{T,2}`)パラメータが、配列でない関数シングルトン
(`typeof(sin)`)まで緩マッチし、しかも具体的な `::Function` メソッドより優先されて
いた(`h(sin)` が `h(::AbstractMatrix)` を選び、本家は `h(::Function)` を選ぶ)。根因は
コンパイル時 `struct_parents_fallback_match` のサブタイプ判定 `struct_is_subtype_of_abstract`
で、`typeof(...)` という struct 名が宣言済みユーザ struct でも組み込みファミリでもないため
**保守的 accept (`None => return true`)** に落ち、任意の抽象型のサブタイプと誤判定して
いたこと(#7266 と同クラス)。修正=関数シングルトン名 `typeof(...)` を「未知のユーザ抽象
型」ではなく既知の `Function` 上位型(`typeof(f) <: Function <: Any`)として扱い、`Function`/
`Any` 以外の抽象型に対しては `false` を返す。これにより `scatter(sin)` が本家通り
`scatter(f::Function)` に到達し、bundled Plots の `scatter(m::AbstractMatrix)` /
`scatter!(m::AbstractMatrix)` を本家通りの `::AbstractMatrix` に復元(#7322 で入れた
`::Matrix` workaround を解消)。#7275 Interact `@manipulate` サンプルのブロッカーも解消。
fixture `dispatch/dispatch_abstractmatrix_no_loose_match_function_7334.jl`(8 テスト, julia
parity 一致)。full 3928/0。

### `scatter(::Matrix)` を列ごと1系列で実装 ✅ (Issue #7322)

bundled Plots に `scatter(m::Matrix)` / `scatter!(m::Matrix)` を追加し、行列を
列ごとに1系列としてプロット(行インデックス `1:size(m,1)` を x 軸に共有)。本家
Plots.jl と同等。`scatter(rand(10,2))`(= #7275 Interact `@manipulate` の iOS
サンプル)が `MethodError: no method matching scatter(::Matrix{Float64})` で落ちて
いたのを解消。列は無注釈の2引数 `scatter(x,y)`/`scatter!(x,y)` 経由で流すため列
ごとの `::Vector` dispatch に依存しない。当初は bare abstract 注釈が `Function` を
緩マッチする問題(Issue #7334)を避けるため具象 `::Matrix` を採用していたが、#7334 を
修正したため本家通りの `::AbstractMatrix` に復元(同セッションの #7334 エントリ参照)。
fixture `packages/plots_scatter_matrix_7322.jl`(20 テスト)。

### 行列スライス carrier が rank を復元して `::Vector`/`::Matrix` に dispatch ✅ (Issue #7333)

`m[:, 1]` / `m[1, :]`(および `collect(m[:, 1])`)が `typeof`/`isa` では
`Vector{Float64}` を名乗るのに、推論では rank 不明の bare `Array` に落ちて
`::Vector` メソッドに `MethodError: no method matching f(::Array)` を出していた問題を
修正。`Expr::Index` 推論(`compile/expr/infer/julia_type.rs` の JuliaType チャネルと
`mod.rs` の ValueType チャネル両方)で、スライス次元数(colon/range/整数ベクトル
インデックスの個数)から結果 rank を復元: `m[:,1]`→Vector, `m[:,:]`→Matrix。要素型は
受け側から取り、不明なら bare `Vector`/`Matrix` alias(既存 `Expr::Var` の rank
ロジックと一致)。受け側が配列族でない(タプル/レンジ等)場合は従来の `Array` fallback を
維持。#7307/#7317 の rand/randn rank 復元と同系統。fixture
`getindex/getindex_slice_carrier_dispatch_7333.jl`(7 テスト)、本家 1.12.6 パリティ確認。

### wasm32 で `::Int` / `::UInt` パラメータの dispatch を修正 ✅ (Issue #7310)

`::Int` を持つユーザ関数が wasm32 でのみ `MethodError` になっていた問題を修正。
`types/native_word.rs` が `usize::BITS` に応じて `Int`→`Int32`(wasm32)を返して
いたが、VM の整数キャリアは常に Int64 なので `Int`/`UInt` を常に `Int64`/`UInt64`
へ解決するよう変更。fixture `tests/fixtures/dispatch/int_alias_param_dispatch_7310.jl`、
wasm32 実機(node)で MWE が `206.0000000001` を返すことを確認。

### `Vector{Any}` の `show` で `Any[` 接頭辞を保持 ✅ (Issue #7303)

`println(Any[1, 2, 3])` が `[1, 2, 3]` だったのを本家どおり `Any[1, 2, 3]` に。本家の
`typeinfo_prefix` は型駆動で `Vector{Any}` は常に `Any[` を付ける。`_array_show_prefix`
(`base/io.jl`)/`array_show_prefix`(`vm/formatting/mod.rs`)の値駆動導出を修正し、`Any`
タグの同型 implicit ランは **要素がすべて推論 widen 対象の複合型(`Pair`/`Tuple`/ネスト配列)
のときのみ** 接頭辞を落とすようにした(スカラ implicit 配列の `Any` は明示 `Any[...]` として
接頭辞を保持)。`Int[1,2]`→bare / `Real[1,2]`→`Real[` / `[1=>1]`→bare 不変。fixture
`array/show_any_vector_prefix_7303.jl`(14+11 テスト)、本家 1.12.6 パリティ確認。

### `Vector{T}(undef, n)` / `T[...]` がユーザ struct eltype を保持 ✅ (Issue #7304)

`Vector{PP}(undef, 1)` / `PP[PP(1)]` が `Vector{Any}` だったのを `Vector{PP}` に
(ユーザ struct / `mutable struct`)。構築時に struct 名を `StructOf(type_id)` へ解決
(`array_element_type_from_julia_type_resolved` を `NewMemoryDynamicTyped` に、
compile 側 `heap_julia_type_array_element_type_resolved` を typed リテラル経路に配線)し、
`typeof`/`eltype` 表示境界で `StructOf` を `struct_defs` で逆引きする `self` メソッド
(`memory_element_type_name`/`array_element_type_to_julia_type_resolved`/
`array_wrapper_julia_type_resolved`)を追加。組み込みプリミティブと `Vector{Any}` は回帰なし。
fixture `array/typed_struct_array_eltype_7304.jl`(4+8+9 テスト)、本家 1.12.6 パリティ確認。

### 修飾付き型アクセス `Module.Type` ✅ (Issue #7302)

`Module.Type`(`Plots.Plot` / `M.Circle`)を `isa`・`<:`・`===`・`Module.T(args)` 構築・
素の型値・`::Module.T` 注釈・`Type{Module.T}` ディスパッチで解決可能にした。これまでは
修飾アクセスを**関数ルックアップ**へ誤ルートし `Module X has no function named Type` で失敗。

- `compile/expr/call/module_call.rs::compile_module_function_ref`: モジュールに同名関数が
  無いとき、型テーブルを**短名**で引いて `PushDataType` を発行(struct/parametric/abstract/
  enum/primitive)。関数を先にチェックするので関数束縛は優先。
- `compile/collect.rs::resolve_abstract_type`: 修飾抽象型注釈 `f(s::M.Shape)` が `Struct("M.Shape")`
  のまま抽象レジストリ(短名キー)に当たらず `MethodError` になっていたのを、ルックアップ前に
  モジュール接頭辞を除去して `AbstractUser` 再分類するよう修正。
- テスト: `fixtures/modules/module_qualified_type_access_7302.jl`(13/13、julia パリティ)、
  `fixtures/packages/plots_qualified_type_access_7302.jl`。
### `MersenneTwister(seed)` の構築に対応 ✅ (Issue #7306)

`MersenneTwister(seed)` を構築可能にした(従来は `Unknown function: MersenneTwister`)。
型注釈・`isa`・ディスパッチ (#7231) は既に RNG 扱いだったので、欠けていた**構築**を補完。

- 決定的 **MT19937-64** エンジン (`subset_julia_vm/src/rng.rs::MersenneTwister`) でバック。
  同一 seed → 同一系列(再現可能)、異なる seed → 異なる系列、`rand`/`randn`/`rand(m,n)`/
  `randn(m,n)` が有限値、`isa AbstractRNG` 成立、`typeof` = `MersenneTwister`、無型 /
  `::MersenneTwister` / `::AbstractRNG` 引数スレッディング対応。
- **upstream は dSFMT バック**のため生成ストリームは**ビット一致しない**(要件外と明記)。
- 配線: `BuiltinOp::MersenneTwisterRNG`(列挙子末尾、serialized IR discriminant 互換)→
  `Instr::NewMersenne` → `RngInstance::Mersenne(Box<MersenneTwister>)`(`Value` 肥大化回避の
  ため Box 化)。Xoshiro/StableRNG と同じ構築・スレッディング経路をミラー。
- fixture (13 件): `subset_julia_vm/tests/fixtures/stdlib/random_mersenne_twister_ctor_7306.jl`
  (sjulia/upstream 両方で 13/13 pass を `fixture_julia_parity.sh` で確認)。
### `rand(n)` / `randn(n)` carrier を `scatter`/`plot` にディスパッチ ✅ (Issue #7307)

`scatter(rand(5))` / `plot(rand(5))` が `MethodError: no method matching
scatter(::Float64)` で失敗していた(`scatter(collect(rand(5)))` は別経路で
`scatter(::Array)`)。`typeof(rand(5))` も `isa Vector` も `Vector{Float64}` を
返すのに**ディスパッチだけ**が外れる症状。根因は推論:`rand`/`randn` は
`Expr::Builtin(BuiltinOp::Rand/Randn)` ノードで、`infer_julia_type` の Builtin
アームが引数の有無に関わらず常に `JuliaType::Float64` を返していた(`rand(5)` の
引数型 = `Float64` → 静的ディスパッチが `scatter(::Float64)` を探して失敗)。
ValueType 側も `rand(n)` → 未パラメータ化 `Array`(rank 不明)で、これは
バンドル Plots の `scatter(y::Vector)` / `plot(y::Vector)` に静的マッチしない
(`zeros(n)` は pure-Julia 関数呼び出しとして `Call` 経路を通り `Vector{Float64}`
にランク付けされるので動いていた)。

修正(`compile/expr/infer/`):
- `expr_tfuncs.rs` に `infer_rand_array_julia_type_for` / `infer_rand_array_value_type_for`
  を追加。スカラ整数次元引数からランクを復元し `rand(n)`→`Vector{Float64}`、
  `rand(n,m)`→`Matrix{Float64}`(`zeros`/`ones` と同じ `dims_rank_from_args` ロジック)。
- `julia_type.rs` の `BuiltinOp::Rand | Randn` アーム、`mod.rs` の同アーム(ValueType)
  をこのヘルパー経由に変更。
- 要素型は **Float64 固定**:`rand(Int, n)` は意図的に defer(未パラメータ化 `Array`
  維持)。`RandIntArray` ランタイムが現状 `Vector{Float64}` を返す別バグがあり、
  ここで `Vector{Int64}` と推論すると推論↔ランタイムが食い違い `x[1]` で
  `expected I64, got Float64` を起こすため(下記の発見済みバグ参照)。
- RNG/コレクション形(`rand(rng, ...)` / `rand(itr)`)は先頭が非整数なので従来通り defer。

検証: `rand(5)`/`rand(2,3)`/`rand(Float64,4)`/`randn(6)`/`randn(2,2)`/`rand()` が
全て upstream(julia 1.12.6)と一致。`scatter`/`plot`/`scatter!` 等の Vector 系
ディスパッチが `collect(...)` と同等に成立。fixture:
`packages/plots_scatter_rand_7307.jl`(16 アサーション)。

**スコープ**: Vector carrier(`scatter(rand(n))`)は完全解決。Matrix carrier
(`scatter(rand(n,m))`、列ごとに 1 series)はバンドル Plots に `scatter(::Matrix)`
メソッドが無い別機能 — 本修正で型は正しく `Matrix{Float64}` になり、エラーも
`scatter(::Float64)` から `scatter(::Matrix{Float64})` に正常化した(未対応のまま)。

**発見した既存乖離(本修正の対象外)**: `rand(Int, n)` がランタイムで
`Vector{Float64}` を返す(upstream は `Vector{Int64}`)。`RandIntArray` ハンドラ側の
別バグで main にも存在。
### AoT 生成 Rust の冗長な括弧で `clippy -D warnings` が落ちる ✅ (Issue #7311)

AoT の二項演算エミッタは優先順位保持のため**全ての**二項演算を括弧で包む
(`(__sjulia_global_a + (__sjulia_global_b as f64))`、ローカル `p+q` も `(p + q)`)。
トップレベル(および関数の唯一の引数)ではこれが冗長になり、生成 Rust に対する
`clippy -D warnings` が `unused_parens`(rustc)と `clippy::double_parens`(clippy)で
2 件警告していた。`--emit-binary` と実行は無影響(rustc の `unused_parens` は warn-by-default)で、
#7242 でトップレベルグローバルがエミッタまで到達するようになって初めて観測された。
- 修正: 生成 Rust のヘッダ(`aot/codegen/aot_codegen/program.rs::emit_prelude`)に
  `#![allow(unused_parens)]` / `#![allow(unused_braces)]` / `#![allow(clippy::double_parens)]`
  を追加。エミッタを優先順位依存にする(正しさを損なうリスク)より、無害な括弧を
  ヘッダで黙らせる方が安全。
- 検証: MWE(`a=3.0; b=2; println(a+b)`)を `juliars` で生成 → 生成クレートに対する
  `cargo clippy -- -D warnings` が修正前は 2 エラー → 修正後 0(プログラムは `5.0` を正しく出力)。
- テスト: `aot_e2e_tests` に `test_aot_header_allows_unused_parens_7311`(グローバル binop で
  3 つの allow + 冗長括弧の存在を確認)と `test_aot_header_allows_unused_parens_for_local_binop_7311`
  を追加(`--features aot` でのみビルド)。
### `Float64 ^ Integer` を上流の補償付き power-by-squaring に一致 ✅ (Issue #7308)

`10^-2` / `10.0^-2` が `0.01` を返していた(上流 Julia 1.12.6 は
`0.010000000000000002`、~1 ULP 差)。VM の `Float64^Int` が Rust `powf`/`powi`
(`inv(10.0^2)` 相当)を使い、上流 `base/special/pow.jl` の
`pow_body(x::Float64, n::Integer)`(補償付き power-by-squaring、`inv(10.0)^2` 相当)と
リダクション順序が違っていたのが原因(`#7233` の整数底負リテラル指数広げで表面化)。

- `intrinsics_exec.rs` に `pow_body_f64_int` / `two_mul_f64` を上流から移植し、整数値かつ
  `|n| < 2^20` の指数を補償経路へ送る `pow_f64(base, exp)` を追加(`n==0` は `1.0` 短絡、
  非整数/範囲外は正しく丸められた `powf` フォールバック)。
- typed 経路(`Instr::PowF64` / `Intrinsic::PowFloat`)と `dynamic_pow` の F64 結果アームを
  すべて `pow_f64` に統一。厳密表現ケース・非負指数・NaN/±Inf/±0.0 エッジは上流一致を確認。
- F32/F16 の `^`(上流は別アルゴリズム)と AoT 定畳み込み経路は本件スコープ外(VM 優先)。
- テスト: `arithmetic/float_pow_int_compensated_7308.jl`(38 アサーション、期待値は julia 1.12.6)。
### quote 内 colon インデックスの `Colon()` 往復 ✅ (Issue #7312)

`:(a[:, 1])` の colon インデックスが `eval` 時に `Colon()` へ解決されず MethodError に
なっていた問題を修正。`eval_expr_value`(`vm/builtins_macro/eval.rs`)の `Value::Symbol`
アームで、ローカルにシャドウされていない `Symbol(":")` を `Value::SliceAll`(= `Colon()`)
へ解決(本家の大域 `: = Colon()` 束縛に一致)。`eval(:(m[:, j]))` / `eval(:(m[i, :]))` /
`eval(:(m[:]))` / `eval(Expr(:ref, :m, Symbol(":"), 1))` が列・行スライスを返すようになった。

- fixture: `metaprogramming/eval_expr_ref_colon.jl`(本家と 10/10 一致)。
- 付随ギャップ(未対応): スタンドアロン `Colon()` コンストラクタ / `Colon` 型名 /
  `typeof(::Colon)`。fixture はこれらに依存しない。
### `~(::Bool)` ビット否定 ✅ (Issue #7305)

`~(x::Bool) = !x`(上流 `base/bool.jl`)を `subset_julia_vm/src/julia/base/bool.jl`
に追加。`~true === false` / `~false === true`、ブロードキャスト `.~[true,false]` =
`Bool[0, 1]`。fixture: `tests/fixtures/bool/bitnot_bool_7305.jl`(sjulia/julia 双方 6/6)。

### `inv(::BigInt)` を BigFloat 化 ✅ (Issue #7309)

`inv(big(2))` が整数除算で `0` を返していたのを `0.5 :: BigFloat` に修正。
`subset_julia_vm/src/julia/base/gmp.jl` に `inv(x::BigInt) = inv(BigFloat(x))` を追加
(上流 `inv(x::Integer) = float(one(x))/float(x)` 相当)。`inv(2)` / `inv(2.0)` は
Float64 のまま不変。fixture: `tests/fixtures/bigint/inv_bigint_7309.jl`(sjulia/julia
双方 8/8。二進値のみビット一致を確認、非二進は astro-float↔MPFR で最終 ULP 差あり)。

## 最新対応 (2026-06-21)

### Web Playground: iOS 専用サンプル 5 件を有効化 ✅ (Issue #7286)

`webUnsupported: true` だった iOS 専用サンプルのうち 5 件を Web Playground で動作可能に。
- `primes_package` / `symbolics_package` / `barnsley_fern`: バンドルパッケージ
  (Primes/Symbolics/Distributions)は `include_str!` で WASM バイナリ同梱済みのため
  `using` で解決済み。`web/samples_ir.js` のフラグを落とすだけ。
- `jsxgraph_demo` / `apollonian_gasket`: iOS `JSXGraphView.swift` の描画ロジックを
  `web/app.js::renderJsxgraph` に移植 + `web/jsxgraph.min.js`(1.12.2)同梱。
- 既存バグ修正: Plotly/JSXGraph の UMD を Monaco loader(`define.amd`)より前に読み込み、
  グローバル `Plotly`/`JXG` が設定されずプロット/ボードが描画されなかった問題も解消。
- `distributions_package` のみ未対応維持: `cdf(::Binomial,…)` 経由の `_beta_inc_cf`
  末尾デフォルト引数が **wasm32 限定**でディスパッチ失敗(CLI/native は成功)。
- 検証: cache 埋め込み `wasm-pack` ビルド成功、Playwright で 25 pass/1 skip/0 fail、
  実 DOM 描画確認、`subset_julia_vm_web` 回帰テスト 6 件追加。
### 引数昇格する外部コンストラクタの戻り値型推論 ✅ (Issue #7284)

`Foo(3).x`(`function Foo(x::Real); v = float(x); Foo{typeof(v)}(v); end`)等、引数を
`float()`/`promote()` で昇格するユーザ定義外部コンストラクタを持つパラメトリック構造体の
**インラインフィールドアクセス**が `Type error: expected I64, got Float64` で失敗していた。
呼び出し点の戻り値型推論がデフォルト内部コンストラクタをモデル化し生の引数型から
`Foo{Int64}` と束縛、フィールドを `Int64` 型付けしていた(実行時は `Foo{Float64}` で正)。
`compile/expr/infer/mod.rs` の `parametric_structs` 分岐で、ユーザ外部コンストラクタ
(`function_ir_by_global_index` に本体 IR を持つメソッド)があれば本体経由で戻り値型を
再推論(`infer_user_outer_constructor_return_type`)し、無ければ従来の型引数推論へフォール
バック(デフォルトコンストラクタの整数フィールド型を維持)。`using Distributions` の
`Normal(2, 3)`/`mean(Normal(2, 3))`/`Normal(2, 3).μ` も解消。fixtures:
`struct/inline_parametric_ctor_promote_7284.jl`,
`distributions/distributions_normal_integer_args_7284.jl`。#7240(インラインブレース
lowering)の姉妹で、こちらは呼び出し点の推論経路。

### Interact `@manipulate` の Plotly ドロップダウン MVP ✅ (Issue #7275)

- バンドルパッケージ `Interact`(`Manipulate` 構造体 + `@manipulate` マクロ)を追加。
  `using Interact` で到達し、`@manipulate for var = choices … end` は離散選択ごとに本体を
  評価して per-choice `Plot` + ラベルを `Manipulate` に集約。
- `plotting/plotly.rs::generate_plotly_manipulate_json` が `Manipulate` を検出し、全選択の
  トレース(選択 0 のみ可視)+ `"type":"dropdown"` の `updatemenus` を持つ静的 Plotly 図を
  生成(`AnimatedGif` frames アニメの dropdown 版)。`try_value_to_artifact` に配線。
- 付随: マクロ `quote` 本体中の添字式(`a[i]` / `Any[]`)を `Expr(:ref, …)` に往復させる arm を
  `cst_to_constructor.rs` に追加(上流一致)。`@manipulate` の `datasets[dataset]` と
  `@animate`/`@gif` の添字本体を同時に解決。
- テスト: `plot_artifact_mime_tests`(dropdown JSON 構造の e2e 2 件)、
  `packages::interact_manipulate_7275`(Julia レベルの収集/ラベル)、
  `metaprogramming::quote_index_expr_7275`(quote→ref 往復, upstream julia とパリティ)、
  plotly/packages の Rust unit テスト。MVP のみ(Phase 2 はリアクティビティ/連続スライダー/
  複数コントロール/非プロット本体 → UNIMPLEMENTED.md)。

### 抽象パラメトリック型まわりのディスパッチ修正 3 件 ✅ (Issue #7235)

Distributions.jl 移植中に見つかった抽象パラメトリック型ディスパッチ不具合 3 件を修正
(全て julia 1.12.6 とパリティ)。
- sub1: `const CUD = Dist{Uni,Cont}; struct Norm{T} <: CUD` の `isa`/`<:`/dispatch
  (`lowering/struct_.rs` で親型を `type_alias::expand`)。
- sub2: `abstract type Dist{F,S} <: Sampleable{F,S} end` への `::Dist` メソッド/`isa Dist`
  (`lowering/abstract_.rs` のトップレベル `ParametrizedTypeExpression` アームが抽象型名を親で
  上書きしていたのを修正)。
- sub3: `M.onearg(M.Norm(0.0))` のクロスモジュール qualified ネスト呼び出し
  (`infer/julia_type.rs` の `ModuleCall` で qualified コンストラクタの戻り値型を推論)。
  モジュールローカル抽象注釈部分は #7265 で解決済み。

fixtures: `abstract/const_alias_parametric_supertype_7235.jl`,
`abstract/parametric_abstract_supertype_dispatch_7235.jl`,
`dispatch/qualified_nested_constructor_arg_7235.jl`。

### パラメトリック型 + ユーザ定義外部コンストラクタの `::Type{Foo}` ディスパッチ ✅ (Issue #7247)

`struct Foo{T<:Real}` がユーザ定義外部コンストラクタも持つとき、ベア型 `Foo` を
`ff(::Type{Foo}, v)` に渡すと `typeof(Foo)` に誤解決され MethodError になっていた問題を修正。
`infer/julia_type.rs` の識別子推論にパラメトリック struct アーム(`parametric_structs`)を
`struct_table` アームの直後に追加し `Type{Foo}` を返すよう修正。コンストラクタ呼び出し/インスタンス
ディスパッチ/型同一性は不変。fixtures: `dispatch/type_param_struct_custom_ctor_7247.jl`。

### Aizawa attractor 3D アニメーション iOS サンプル ✅ (Issue #7273)

Lorenz 系で整備した Plots 機能を組み合わせた **Aizawa strange attractor** の 3D アニメーション
サンプルを追加。`Base.@kwdef mutable struct` + `step!`(Euler 積分)で軌道を進め、`plot3d(1)` の
空 3D パスに `push!(plt, x, y, z)`(#7271)で 1 点ずつ追加、`@animate ... for i in 1:3000 ... end every 20`
(#7272)で 150 フレームの GIF を生成する。サンプルは
`SubsetJuliaVMApp/.../Samples/intermediate/aizawa_attractor.jl` + `samples.json` + Swift フォールバック。
回帰テスト `subset_julia_vm/tests/fixtures/packages/plots_aizawa_attractor_7273.jl` で `step!` の
Float64 厳密値(upstream julia 1.12.6 とビット一致)と `length(anim.frames) == 150` を検証。Web は
3D アトラクタ + `push!`/`every` の wasm 動作未検証のため見送り(`plots_animation`/`plotting_3d` は web 対応済み)。

### `Random.default_rng()`/`GLOBAL_RNG` + 明示 RNG 引数スレッディング ✅ (Issues #7230/#7231)

- **#7230**: `Random.default_rng()` / `Random.GLOBAL_RNG` が VM グローバル RNG ハンドル
  (`RngInstance::Global`)を返す。`rand(default_rng())`/`randn(default_rng())` は素の
  `rand()`/`randn()` と同一ストリーム。`typeof`→`TaskLocalRNG`、`isa AbstractRNG`→true、
  `println`→`TaskLocalRNG()`(upstream 1.12.6 一致)。
- **#7231**: 無型 / `::Xoshiro` / `::AbstractRNG` の RNG 引数を取るユーザ関数で
  `randn(rng)`/`rand(rng)` がスカラを返す。RNG 型注釈→`ValueType::Rng`(`type_helpers`/
  `core_compiler`)、無型単一引数の実行時分岐命令 `RandArg`/`RandnArg`、`Value::Rng` の
  ディスパッチ型を具体名に揃え `check_subtype` で `<: AbstractRNG` を解決。
- fixtures: `stdlib/random_default_rng_7230.jl`(7)・`stdlib/random_rng_param_threading_7231.jl`(10)
  が sjulia/upstream parity。
### ユーザモジュール内 `using LinearAlgebra` の名前衝突修正 ✅ (Issue #7245)

`module D; using LinearAlgebra; ddet(S)=det(S); end` のように、ユーザモジュール名が
LinearAlgebra のローカル変数名(`D`/`D1` — `Diagonal` メソッドの `D.diag` フィールド
アクセス)と衝突するとロードが失敗(`Module D has no function named diag`)していた。
`compile/expr/struct_.rs::compile_field_access` で、スコープ内のローカル束縛が同名モジュールを
シャドウするよう修正(Julia のスコープ規則準拠; ローカルが `ValueType::Module` を保持する
場合のみモジュールアクセス)。fixtures: `linalg/linalg_in_user_module_7245.jl`,
`linalg/linalg_in_user_module_cholesky_7245.jl`,
`modules/module_name_shadows_local_var_7245.jl`,
`modules/module_using_statistics_regression_7245.jl`(回帰ガード)。
### AoT: global 名衝突 (E0530) / 大きな Float64 の科学表記 ✅ (Issues #7242/#7256)

`--features aot` 専用(デフォルトテストでは未ビルド)。`bash scripts/test_aot.sh` で検証。

- **#7242**: トップレベルのスカラ global を衝突しない `__sjulia_global_<name>` static として
  出力(同名の関数引数による E0530 を回避)。参照側も書き換え、関数引数が shadow する
  場合は接頭辞を付けない。`mod.rs` (`global_static_ident` / `current_function_param_names`),
  `program.rs` (`emit_global`), `expressions.rs` (`AotExpr::Var`)。
- **#7256**: `__sjulia_format_float64` を runtime クレートの
  `subset_julia_vm_runtime::intrinsics::format_float64_julia` へ委譲し、`1e30`→`1.0e30` の
  科学表記(本家 julia 1.12.6 / VM `format_float_julia` 一致)に。InexactError の埋め込み値も
  同経路で `InexactError: Int64(1.0e30)` に。
- テスト: runtime 単体テスト(30 値)+ aot codegen string-assert + e2e。AoT full は既存 7 fail のみ
  (新規 0)、default lib 2940 green。
### 型パラメータ波括弧内のネストした関数呼び出し `T{typeof(f(x))}(...)` ✅ (Issue #7240)

`Foo(x::Real) = Foo{typeof(float(x))}(float(x))` が
`Compilation error: "Undefined variable: float(x)"` で失敗していたのを修正。

- 根本原因: 実行時型引数文字列 `typeof(float(x))` を式に戻す `lower_expr_from_text`
  が手書き簡易パーサ(`Name(args)` を `,` 分割)で、ネストした呼び出し `float(x)` を
  1 個の識別子 `Var("float(x)")` として取り込んでいた。
- 修正: `lower_expr_from_text` を本物の `Parser` + `lower_expr` 経由
  (`try_lower_expr_via_parser`)に置換(`subset_julia_vm/src/lowering/mod.rs`)。
  ネストした呼び出し・ブロードキャスト・演算子・文字列リテラルが通常の式ロワリングと
  同一経路で処理され、`Symbol(s)`/`Symbol("foo")`(MIME 等)も従来どおり動作。
- fixture: `struct/typeparam_nested_call_7240.jl`(MWE・深いネスト `typeof(g(h(x)))`・
  2 型パラメータ・回帰用 `typeof(var)`)。上流 julia 1.12.6 とパリティ一致。
  lib 2938 green / full fixture suite 153 chunks green。

### Plots: `plot3d` / `push!(plt, x, y[, z])` / `@animate ... every N` ✅ (Issues #7270/#7271/#7272)

upstream Plots.jl の Lorenz アトラクタ・アニメーションサンプルを動かすための 3 件(`packages/Plots/`)。

- **#7270**: `plot3d`/`plot3d!` を export。`plot3d(x,y,z;kw)` ≡ `plot(...; seriestype=:path3d)`、
  `plot3d(n::Integer;kw)` は `n` 個の空 `:path3d` series で `Plot` を初期化。
- **#7271**: `push!(plt, x, y)` / `push!(plt, x, y, z)`(および index 付き)で第1 series に 1 点を追加
  (upstream `extend_series!` 準拠)。
- **#7272**: `@animate`/`@gif ... every N` / `... when cond` 修飾子。パーサがブロック引数の後の
  同一行追加引数を収集するよう修正し、単一可変長マクロメソッドで判定式を `frame(_anim, ::Bool)`
  へ splice。付随して、マクロランタイムが `obj.field`(`Expr(:.)`)を変換できるようにし、バンドル
  マクロ展開プログラムにユーザ型定義を渡すよう `LambdaContext` を拡張(`step!(l::Lorenz)` の
  `Unknown field` を解消)。
- fixtures: `plots_plot3d_alias_7270` / `plots_push_xy_point_7271` / `plots_animate_every_7272`。
  Lorenz サンプルが end-to-end(150 フレーム)で通ることを確認。full suite 3905 green。

### 整数の負リテラル指数 `2^-3` → `literal_pow` ✅ (Issue #7233)

`2^-3` が DomainError を投げていた(本家は `0.125`)。リテラル整数指数を本家同様
`Base.literal_pow(^, x, Val(p))` に下げる。

- lowering (`expr/binary.rs`): `^` の右が「単項 `-` + 整数リテラル」のときだけ
  `literal_pow(^, base, Val(p))` を生成。正リテラル指数は `Pow` 経路維持、非リテラル
  `n=-3; 2^n` も `Pow` 経路で本家同様 DomainError。
- pure Julia (`base/intfuncs.jl`): `literal_pow` 追加。整数ベースの負指数は Float64 へ
  拡幅(`2^-3 == 0.125`)、それ以外は `inv(x)^(-p)` で型安定(`(1//2)^-2 == 4//1`)。
- fixture: `arithmetic/literal_pow_negative_exponent_7233`(15 assertions, julia parity)。

### 前置ブロードキャスト単項 `.-v` / `.+v` ✅ (Issue #7234)

前置 dotted 単項 `.-v` / `.+v`(本家 `broadcast(-, v)` 相当)のパースエラーを解消。

- parser (`expressions/mod.rs`): `try_parse_dotted_unary_broadcast_prefix` を追加し、
  `.+`/`.-`(paren 無し)と二トークン `.~` を `BroadcastCallExpression` に変換(既存
  lowering が `broadcast(op, x)` へ)。`.+(x,y)` の関数呼び出しと `.*`/`./` の前置は不変。
  `.^` の優先順位(`.-x .^ 2` = `.-(x .^ 2)`)も `.!` 同様に処理。
- fixture: `broadcast/prefix_unary_dot_7234`(8 assertions, julia parity)。

### StatsPlots 分布プロット ✅ (Issue #7262)

- 同梱パッケージ `StatsPlots` を追加し、`using Distributions, StatsPlots;
  plot(Normal(0, 1))` で pdf 釣鐘曲線を描画。連続分布は pdf の `:line`、離散分布は
  pmf の `:bar` を、いずれも `quantile(d, 0.0001) … quantile(d, 0.9999)` 範囲で
  サンプリング。具象型ごとの typed wrapper → untyped ヘルパー委譲で #7235 の
  クロスモジュール抽象ディスパッチ問題を回避。
- 副産物: comma-form `using A, B` / `import A, B` の lowering を修正(1 path =
  1 `UsingImport`)。`modules/using_comma_multiple` 回帰 fixture を upstream julia と
  パリティ確認のうえ追加。
- `tests/fixtures/statsplots/`(normal_pdf / continuous / discrete / using_comma)+
  `mod.rs` 登録テスト 3 件。フル suite 3909/3909 green、clippy 0 警告、
  base_exports 監査グリーン。
### 配列リテラル内の位置スプラット ✅ (Issue #7255)

- `Any[pts...]` / `[a, xs..., b]` / `Float64[pts...]` / `Int[0, pts..., 99]` /
  `Complex{Float64}[pts...]` / `UserStruct[pts...]` を upstream と同じ値・型で
  サポート(以前は `unsupported expression: splat_expression`)。
- untyped は `Base._array_splat_literal(vals...)`(= upstream `Base.vect`)、typed は
  `Base._array_splat_literal_typed(T, vals...)`(= upstream `getindex(::Type{T},
  vals...)`)へスプラット呼び出しで降ろす。要素型は splat 展開後の値の
  `promote_typeof`(untyped) / 指定型 `T`(typed)。
- 変更: `lowering/expr/collection.rs`(untyped vcat 経路の置換 + typed index 経路へ
  splat 検出を追加、パラメトリック/ユーザ型ターゲットも対象)、`base/array.jl`
  (3 ヘルパー追加)。fixture `splat/splat_array_literal_7255`(25 アサーション、
  julia とパリティ一致)。
### 複合代入を式として使用 ✅ (Issue #7269)

- `x += y`(`-=`/`*=`/`/=`/`^=`/`%=`/`÷=`/`.=`/`.+=` など)を式の位置(`return`
  値・別代入の RHS・関数引数)で使えるよう lowering に式経路を追加。値は新しく
  代入された値(upstream Julia 準拠)。`return p.z += 1.0` → `2.0`、`y = (x += 1)`
  → `1`、`a[i] += 5` → 新値、Lorenz `return l.z += l.dt * dz` が動作。
- 単純変数・フィールド・添字・ネストフィールド・複雑な添字
  (`obj.field[i] +=`)・broadcast(`Z .+= …`)の各 LHS 形をサポート。文形式の
  降ろしを再利用し、新値を一時変数に束縛して返すことで対象の再読み込み/binop
  再評価を回避。`control_flow` に 5 fixture を追加(sjulia↔julia パリティ確認済み)。
### モジュール内 abstract 型注釈ディスパッチ修正 ✅ (Issues #7263 / #7265)

- モジュール/バンドルパッケージ内で宣言された abstract 型がコンパイラの
  abstract 型レジストリへ届かず、`f(d::Distribution)` のような注釈が具象
  `Struct("Distribution")` 扱いになって untyped generic に負けていた不具合を
  修正。`compile/mod.rs` に `collect_module_abstract_types` を追加し
  (`collect_module_structs` / `collect_module_primitive_types` と対):
  `pipeline_ctx.rs` の `abstract_types` / `abstract_type_names` /
  `abstract_type_parents` がモジュール abstract 型を bare 名で取り込む。
- `dispatch_resolver/core_match.rs` の `CoreType::Named(expected)` アームに
  `Named(actual)` 対応(`strip_module_prefix` で family 名比較)を追加。
  パラメトリック package struct の bare↔モジュール修飾名がマッチするようになり、
  メソッド本体内のクロスメソッド呼び出し(`ncategories(d)` 等)も解決する。
- `Distributions.Categorical` を upstream 準拠の **パラメトリック**
  `Categorical{T<:Real}` / `p::Vector{T}` に戻した(#7263 回避策の撤去)。
  `Categorical(k::Integer)` はパラメトリックデフォルトコンストラクタ直呼びで
  #7266 を回避。
- fixture: `distributions_median_dispatch` (#7265) /
  `distributions_categorical_parametric` (#7263)。upstream Distributions.jl と
  パリティ確認済み。副産物として #7284(整数引数 `Normal(2,3)` の
  パラメトリック型誤推論)を起票。

### Distributions.Categorical ✅ (Issue #7260)

- バンドル `Distributions` の離散分布に `Categorical` を実装。`Categorical(p)` /
  `Categorical(k::Integer)` と `params`/`probs`/`ncategories`/`support`/`mean`/
  `var`/`mode`/`entropy`/`minimum`/`maximum`/`pdf`/`cdf`/`quantile`/`rand` を提供。
  `distributions_discrete` fixture に upstream 一致の参照値テストを追加。
- iOS サンプル `Barnsley Fern` を `Categorical` ベースの写像選択に改修
  (`.jl` / `samples.json` / Swift フォールバック同期)。
- VM ディスパッチ不具合を分離して issue 化(回避策込み): #7263(型付き `var`
  メソッドが untyped generic に負ける), #7266(`::Integer` ctor 内からの
  `::AbstractVector` ctor 呼び出しが loose-match), #7265(`median(d::Distribution)`
  が dispatch されない既存不具合)。

### JSXGraph.jl 統合 backend ✅ (Issue #6357)

- Pure-Julia サブセット `packages/JSXGraph` を追加。`board`/`point`/`line`/
  `segment`/`circle`/`polygon`/`text`/`functiongraph`/`push!`/`html` を実装。
- Rust 側 `src/plotting/jsxgraph.rs` で `application/vnd.jsxgraph+json` artifact
  を生成。要素参照は `{"ref": id}`、数値/配列/Tuple はそのまま JSON 化。
- バンドルパッケージ登録（`src/julia/packages/mod.rs`）と補完登録
  （`src/repl/completions.rs`）を追加。
- fixture tests (`tests/fixtures/packages/packages_jsxgraph_*.jl`) と Rust MIME
  unit test (`tests/plot_artifact_mime_tests.rs`) を追加。
- ドキュメント `docs/vm/JSXGRAPH.md` を追加。
- iOS/Web frontend 描画分岐は後続タスク。

### Apollonian gasket サンプル (JSXGraph) ✅ (Issue #6357)

- iOS サンプル `apollonian_gasket.jl` を追加。Descartes Circle Theorem の線形
  swap `b₄′ = 2(b₁+b₂+b₃) − b₄`（中心は複素数 `b·z` で同形に更新）を再帰し、
  根四つ組 `(−1,2,2,3)` のアポロニウスのガスケットを `circle` で描画。
- 円の中心を point 要素ではなく座標タプル `(x,y)` で渡す経路を確認
  （`parents == [[x,y], r]`）。
- `samples.json` 登録 + Swift フォールバック（`CodeSamples+Intermediate.swift`）。
- fixture `packages_jsxgraph_apollonian` と MIME unit test
  `test_jsxgraph_circle_with_coordinate_center_emits_array_parent` を追加。

### Distributions.jl サポート Phase 1.5〜5 ✅ (Issue #7178)

- **明示 RNG 配列サンプリング ✅ (Issue #7227)**: `rand(rng[, dims...])` /
  `rand(rng, Int, dims...)` / `randn(rng, dims...)` を実装。
- **Distributions パッケージ(連続分布)✅**: `Normal`/`Uniform`/`Exponential`/
  `Gamma`/`Beta`/`Cauchy`/`LogNormal`/`Weibull` と共通 API
  (`pdf`/`logpdf`/`cdf`/`logcdf`/`ccdf`/`quantile`/`mean`/`var`/`std`/`median`/
  `mode`/`entropy`/`params`/`minimum`/`maximum`/`insupport`/`rand`)。
- **Distributions パッケージ(離散分布, Phase 3)✅**: `Bernoulli`/`Binomial`/
  `Poisson`/`Geometric`/`DiscreteUniform`。pmf(`pdf`)/`cdf`(`beta_inc`・
  `gamma_inc` 利用)/scan ベース `quantile`/`succprob`/`failprob`/`ntrials`/
  `span` ほか。サンプリングは Knuth(Poisson)・ベルヌーイ列(Binomial)等。
- **Distributions パッケージ(多変量, Phase 4)✅**: `MvNormal(μ, Σ)`。
  `mean`/`cov`/`var`/`pdf`/`logpdf`/`insupport`/`dim`/`rand`。`using
  LinearAlgebra` がモジュール内で失敗する(#7245)ため、Cholesky 下三角分解と
  前進代入を mvnormal.jl 内に純 Julia 実装(サンプリングの行列・ベクトル積は
  ビルトイン演算子)。サンプリングは μ + L·randn(k)。
- **Distributions パッケージ(MLE フィッティング, Phase 5)✅**: `fit`/`fit_mle`
  を `Normal`/`Bernoulli`/`Exponential`/`Poisson`/`Geometric`/`Uniform`/
  `MvNormal` に対して実装。`::Type{T}` ディスパッチがコンストラクタ付き
  パラメトリック型に効かない(#7247)ため、`D === Normal` 等の型同一性分岐で
  実装。
- **SpecialFunctions 拡張 ✅**: `gamma_inc`(正則化下側不完全ガンマ)を実装、
  `erf`/`erfc` を `gamma_inc` ベースで ~1e-13 精度に改善。
- フィクスチャ: `tests/fixtures/distributions/`(10 ファイル、upstream 参照値)。

### 単項マイナスと冪乗 `^`/`.^` の優先順位 ✅ (Issue #7232)

- `-x^2` が `(-x)^2`(符号反転→二乗で**正**)とパースされ、本家の `-(x^2)`
  (二乗→符号反転で**負**)と逆符号になっていた問題を修正。`-2^2` も `4`(本家 `-4`)、
  broadcast の `-v .^ 2` も `[1,4,9]`(本家 `[-1,-4,-9]`)と誤っていた。実害として
  ガウシアン `exp.(-(x .- t) .^ 2)` が減衰せず発散(max 5.7e5)していた。
- Julia では `^`/`.^` が前置単項演算子より強く結合する(`julia/src/julia-parser.scm`
  `parse-unary`/`parse-factor`: "-2^3 is parsed as -(2^3)")。`subset_julia_vm_parser`
  の Pratt パーサが単項オペランドに後続 `^` を吸収しておらず、先に単項を包んでから
  `^` を適用していたのが原因。
- 修正: `parser/expressions/mod.rs` に `absorb_power_into_unary_operand` を追加し、
  単項オペランドのパース直後に後続 Power 精度演算子(`^ .^ ↑ ↓`)を右結合で折り込む。
  `parse_prefix` / `parse_prefix_with_postfix` / `.!`前置 の3経路に適用。`^` の右辺の
  符号(`2^-3`→`2^(-3)`)は別経路のため不変。
- テスト: parser `test_unary_minus_binds_looser_than_power` /
  `test_power_rhs_keeps_unary_sign`、fixture
  `operators/unary_power_precedence_7232.jl`(sjulia/julia 両方 12/12)。
- 付随発見: `2^-3` の `literal_pow`(#7233)、broadcast 前置単項 `.-v`(#7234)を
  `unsupported-feature` で起票。

### `let` ブロック内の `@test` マクロ展開 ✅ (Issue #7189)

- `using Test; let a = 1; @test a == 1 end` が `@test macro requires \`using Test\``
  で失敗していた問題を修正。`lower_expr_with_ctx` の `let` 分岐を ctx 伝播版
  `lower_let_expr_with_ctx` に切り替え、`let` 本体へ `using` 情報を持つ
  `LambdaContext` を引き継ぐようにした。`@testset`/`@test_throws`/`@test_broken`、
  `let a=1, b=2 ...` の複数束縛、`@testset` 内ネスト `let` も同様に解消。
- 回帰テスト: `subset_julia_vm/tests/fixtures/macros/test_macro_in_let_7189.jl`。

### 内包表記引数が `::Integer` メソッドに loose-match ✅ (Issue #7266)

- 内包表記 `[expr for ...]`(要素型が静的に不明 → ベアな `JuliaType::Struct("Vector")`
  と推論)を `::Integer`/`::AbstractVector{<:Real}` 両メソッドを持つ関数・コンスト
  ラクタへ直接渡すと、誤って `::Integer` メソッドに dispatch されていた問題を修正。
- 真因: ディスパッチの struct-parents フォールバック `struct_is_subtype_of_abstract`
  が、ユーザ宣言階層に無い組み込み `Vector` ファミリを「保守的に accept」して
  `Vector <: Integer` を真と判定していた(#5966 系の loose-abstract-annotation)。
- 修正: (1) 組み込み struct ファミリ(`Vector`/`Matrix`/`Array`/`Dict`/`Set`/range 系)
  は組み込み上位型チェーンを辿って判定し保守的 accept を回避、(2) 要素型不明の単一
  配列引数で静的一致が無い場合は runtime dispatch にルーティング。
- 回帰テスト: `dispatch/comprehension_arg_abstract_array_7266.jl`(julia 1.12 と一致)
  + `method_table.rs` ユニットテスト。バンドル `Distributions.Categorical(k::Integer)`
  を自然形 `Categorical([1.0/k for _ in 1:k])` に戻した。

## 最新対応 (2026-06-20)

### Symbolics 微分のコンパイル推論爆発(~7–17 秒)を解消 ✅ (Issue #7215)

- `Differential(x)(cos(x))` / `using Symbolics` の初回コンパイルが ~7–17 秒(`compile.method_table_setup` 内の `infer_function`、特に `Symbolics._apply_diff`)→ **0.28s**(`method_table_setup` 6967ms → 95ms)。
- 原因: 抽象解釈エンジンの呼び出し側補間推論が、呼び出し先の宣言済み戻り値型(`f(...)::T`)を無視してボディを毎回再展開していた。相互再帰する `_deriv ⇄ _deriv_*` family では tentative cycle 結果が outer fixpoint iteration ごとに破棄され long-lived cache に届かず、同じ `(callee, arg_types)` 解析が `depth × iterations × branching` 回繰り返され組合せ爆発。
- 修正(両面): (1) コンパイラ — `abstract_interp/engine` の補間推論で、呼び出し先が戻り値型を宣言していればボディ再展開せずその型へ短絡(`convert(T,…)::T` 保証で健全、top-level `infer_function` と一致)。(2) Symbolics — 再帰ハブ `_deriv(node, x)` に `::Any` 注釈(`convert(Any,…)` 恒等で実行時無変更、`_apply_diff` は `Num` 精度維持)。
- テスト: unit `interprocedural::test_issue_7215_declared_return_type_short_circuits_call_site` + 既存 fixture `packages/symbolics_derivative.jl`。
### モジュール内クロージャから module-private ヘルパへの名前解決 ✅ (Issue #7180)

- module 内の Base HOF へ渡したクロージャ/関数値から module-level ヘルパを参照すると
  `function 'help' is not imported` で失敗していたのを修正。
- `compile/pipeline_ctx.rs`: module 関数名→module_path のマップを作り、module 関数本体から
  lift された inline/nested 関数(クロージャ)に親の module_path を継承させ、`function_imports`
  に module の関数集合が含まれるようにした。
- fixture `modules/module_closure_hof_helper_7180.jl` を追加(julia と 3/3 パリティ)。

### モジュール内 callable struct (functor) のディスパッチ (Issue #7185)

- モジュール内で定義した `(obj::T)(args...)`(callable struct/functor)が呼べず `Function '__callable_M.Foo' not found` で失敗していた。トップレベルでは動作。
- 原因: `vm/exec/call_function_variable.rs` の `callable_method_name` がモジュール修飾名(`M.Foo{Int}`)を `__callable_M.Foo` として登録名と不一致にしていた。`{` で head を取った後 `rsplit('.')` で module 接頭辞を落とし `__callable_<bare>` に解決するよう修正(#7171/#7172 の show 名整合と同方針)。
- テスト: fixture `module/module_callable_struct_7185.jl`(julia 1.12.6 と 7/7 パリティ: 内部/外部呼び出し・parametric `Scale{T}`・匿名 functor・converting ctor)。
### Broadcasted unary minus on array values ✅ (Issue #7212)

- `-A` / `Base.:-(A)` の operand が Array 型に推論できる場合は、scalar `Neg` opcode ではなく既存の
  `materialize(Broadcasted(-, (A,)))` 経路へコンパイルするようにした。
- `DynamicNeg` も array-like runtime 値を要素ごとに negation するため、`-((x .- t) .^ 2)` のような
  broadcast result が `Array{Any, Any}` として渡る経路でも失敗しない。
- fixture `broadcast_unary_minus_array_7212` で Float64/Int64 配列、broadcast result、qualified `Base.:-` 形式を固定。

### Context-aware `let` lowering re-export build fix ✅ (Issue #7218)

- `lower_let_expr_with_ctx` が `expr/mod.rs` から re-export/call されていた一方で `misc.rs` に実装がなく、
  release build が unresolved import で失敗していた問題を修正。
- ctx 版 `let` lowering を追加し、binding・block statement・single-expression body へ `LambdaContext` を伝播する。

### 行列/hcat/vcat リテラル中の配列・範囲要素のフラット化 ✅ (Issue #7203)

- 行列リテラルの行要素が配列/範囲のとき、本家どおり列/行方向へフラット化されるようにした
  (`[g 4]`→`[1 2 3 4]`、`[1:2 3:4]`→`[1 3; 2 4]`、`[[1 2] [3 4]]`→`[1 2 3 4]`、`[g; row]`、
  `[A B; C D]`)。従来は `Any` ボックス化要素として残るかクラッシュしていた。
- lowering(`lowering/expr/collection.rs`)で非スカラー要素を含むリテラルのみ `hcat`/`vcat`/`hvcat`
  へ振り分け、全スカラーは従来の `ArrayLiteral` 高速パスを維持。型付き行列 `T[...]` は
  `lower_matrix_expr_raw` でフラット要素列を生成。base(`julia/base/array.jl`)にブロック形状ベースの
  `_block_hcat`/`_block_vcat`/`hvcat` を追加(1 次元ベクトルの型保存高速パス #3588 は維持)。
- フィクスチャ `arrays/hcat_vcat_flatten_elements_7203.jl`(julia 1.12.6 と 28/28 パリティ一致)。

### インライン `Dict(...)[key]` の `<: Real` 構造体キー誤ルーティング ✅ (Issue #7173)

- 束縛せず生成した `Dict(...)` をそのまま `[key]` 添字し、キーがモジュール修飾されたユーザ構造体
  （`<: Real`、例 `Symbolics.Num`）だと `getindex` が numeric array-index（`IndexLoad`）へ落ち
  `Type error: expected I64 or CartesianIndex, got <Struct>` で失敗していた（束縛後の `d[key]` は正常）。
- 原因は `is_dict_struct_name` がパラメトリック名全体に `rsplit('.')` を適用し、型パラメータ内のドット
  （`Dict{M.R, Int64}`）で誤分割していたこと。`{` で base を切り出してから接頭辞を剥がすよう修正
  （`compile/expr/mod.rs`・`compile/stmt.rs`）。インライン・束縛のどちらも `CallSpecialize getindex` へ。
- fixture `dict_inline_dict_real_struct_key_7173`（依存なしのモジュール内 `<: Real` 構造体）。julia 1.12.6 とパリティ。
### マクロ注入 `QuoteNode(:sym)` → `::Symbol` フィールド ✅ (Issue #7163)

- マクロ展開で `QuoteNode(:sym)` を構造体コンストラクタ(`::Symbol` フィールド)や `::Symbol` 仮引数へ
  差し込むと `Cannot convert Any to Symbol` でコンパイル失敗していた問題を修正。
- 根本原因: `compile/expr/mod.rs` の `Literal::Symbol` アームが `PushSymbol`(本物の `Value::Symbol`)を
  emit しながら静的型を `ValueType::Any` と返していた。ソース直書き `:sym`(`QuoteLiteral(SymbolNew)`)は
  既に `ValueType::Symbol` を返すため直接呼び出しは成功していた。`ValueType::Symbol` を返すよう修正。
- 検証: 上流 `julia` 1.12.6 とバイト一致。fixture `macros/quotenode_symbol_typed_field_7163.jl`。

### Macro `Expr(head, args...)` splat in no-context lowering ✅ (Issue #7162)

- macro definition body の no-context call lowering で `Expr` builtin 変換時に `splat_mask` を落としていたため、
  `Expr(:vect, names...)` が `names` Vector を 1 個の AST 引数として残していた問題を修正。
- no-context path も context-aware path と同じく splatted `Expr` constructor args を `SplatInterpolation`
  marker に包み、macro expansion runtime の `ExprNewWithSplat` で要素展開する。
- fixture `macros_expr_splat_macro_7162` で escaped Symbol の macro-local Vector splat が upstream Julia と同じ
  `[7, 8]` へ展開されることを固定。

### AoT generic `::Any` method dispatcher integration ✅ (Issue #7158)

- AoT IR converter が同名 overload の先頭 typed signature を全 method に再利用していたため、
  explicit `::Any` を含む overload set が同一 Rust signature に潰れ、generated dispatcher から欠落していた問題を修正。
- converter は関数名ごとの occurrence で `TypedProgram` の対応 signature を選び、`pick(::Int64, ::Any)` と
  `pick(::Any, ::Int64)` を別 method として Rust backend に渡す。
- static call codegen は single-method でも method table の dispatch 解決を通し、ambiguous/no-method を invalid Rust
  call ではなく AoT diagnostic として報告する。
- 検証: #7158 E2E regressions、既存 AoT dispatch resolver unit tests。

### Plots plot(p::Plot) copy semantics ✅ (Issue #7149)

- `plot(p::Plot)` が source plot の `series` 配列を共有していたため、`plot(p); scatter!(...)` などの current plot
  追記が保存済み `p` にも反映される aliasing を修正した。
- replot 時は `Series` と x/y/z データを snapshot 化した独立 series list を作り、current plot と戻り値に登録する。
  `frame(anim, plt)` も同じ helper を使う。
- fixture は `plot!` 追記と `push!(plt, i, y)` の両方で元 plot が変化しないことを固定。
### 再帰コンストラクタ walker の load 時 PartialStruct 推論ハング ✅ (Issue #7186)

- 再帰関数の一分岐が「第2引数で再帰 + 複数ヘルパをネストしたコンストラクタ式」を含むと、
  `using <package>` が load 時推論でハング(関数は未実行)。`infer_function_partial_struct_return`
  に再入ガードと負キャッシュが無く、各ヘルパが `Union{Number, Struct}` を返して partial 復元が
  毎回 `None`(=未キャッシュ)になり、ネスト呼び出しサイト毎に深さ上限まで指数的に再解析していた。
- 修正: `analyzing_partial_structs` 再入ガード + `CachedConstructorPartial.partial` の
  `Option` 化による**負キャッシュ**(world-stamp 付き)。PartialStruct は精度最適化に過ぎず
  `None`→通常推論型への widening なので健全。これで Symbolics の一般冪則 (`_deriv_genpow`,
  `(a^b)' = a^b·(b'·log a + b·a'/a)`) を workaround から復活。
- 検証: `using Symbolics` 即時 load、`x^x`/`2^x` 導関数の数値一致、
  `test_recursive_constructor_walker_partial_struct_terminates_7186`、
  `packages/symbolics_derivative.jl`、フルスイート。

### Cranelift float comparison NaN parity ✅ (Issue #7124)

- Cranelift の Float64 comparison lowering が Julia の NaN semantics と一致することを JIT regression で固定した。
  `NaN == NaN` は false、`NaN != NaN` は true、`<` / `<=` / `>` / `>=` は NaN を含むと false。
- `fcmp`/`icmp` result は ABI の `Bool` carrier (`I8`) へ `bmask & 1` で 0/1 正規化し、Float comparison return が
  Cranelift verifier に落ちないようにした。
- 検証: upstream Julia NaN comparison smoke、Cranelift NaN comparison regression、Cranelift release nextest。

### 行列リテラルの空白依存 `-`/`+` 要素分割 ✅ (Issue #7196)

- `[0.20 -0.26; 0.23 0.22]` の行列行で `0.20 -0.26` が二項減算と解釈され `MalformedMatrix`
  になっていたのを、上流 Julia の空白規則に合わせて修正。行列/`hcat` 行では「前に空白・後ろに空白
  無し」の `-`/`+` が新しい単項符号付き要素を開始する(`[1 -2]`=2要素 / `[1 - 2]`=二項1要素 /
  `[1 *2]`=`1*2`)。カンマ配列・通常の減算・呼び出し引数は不変。
- パーサに `in_matrix_row` フラグ(grouping で解除)+ `peek_next_start` を追加し、
  `parse_expression_with_precedence` で空白条件を満たす `+`/`-` の二項化を中断。型付き行列
  `T[...]` も同規則。
- フィクスチャ `arrays/matrix_literal_negative_element_7196.jl`(julia パリティ 41/41)+ パーサ
  corpus 4 件。

### Cranelift numeric conversion parity gate ✅ (Issue #7123)

- Cranelift adapter の non-identity `AotExpr::Convert` と `sitofp` / `fptosi` builtin を、
  Julia の range / rounding / InexactError semantics 実装まで `Issue #7123` diagnostic として明示拒否する。
- Low-level `TypeAssert` conversion gate も #7123 を併記し、runtime type assertion と numeric conversion parity の
  境界を分けて追跡できるようにした。
- 検証: Cranelift numeric conversion gate regression と Cranelift release nextest。

### Cranelift display runtime parity gate ✅ (Issue #7121)

- Cranelift adapter が `print` / `println` / `string` display builtin を generic unsupported へ落とさず、
  Julia の `print`/`show` formatting runtime 未接続として `Issue #7121` diagnostic で明示拒否するようにした。
- VM/Rust backend 側の Float64 表示 (`1.0`、`Inf`、`NaN`) は既存 formatter が担う。Cranelift は runtime helper
  bridge が入るまで、Rust/Cranelift のデフォルト表示を使った不一致バイナリを生成しない。
- 検証: Cranelift display builtin gate regression と Cranelift release nextest。

### REPL/FFI 結果エコーを user `show` 経由に ✅ (Issue #7168)

- 対話 REPL / iOS-Web の結果表示が user struct を struct dump していたのを、登録済み `show` 経由に修正。
  Symbolics の `x^2+2x+1` が REPL で `x^2 + 2*x + 1` と表示される。
- VM `render_value_via_user_show`(eval 後に `start_sprint_call`+`run_until_frame_return` で show 実行)→
  `REPLResult.value_display` → CLI/FFI フォーマッタが優先。Complex/Rational/LinRange/array-wrapper は
  専用 Rust 整形を維持(除外ガード)。`repr` は別経路で未対応。
- テスト: `repl::tests::test_repl_value_display_uses_user_show_7168`。フル 3889 green。

### Cranelift integer division/remainder parity gates ✅ (Issue #7119)

- Cranelift integer `BinOpKind::Div` / `Rem` に zero-divisor `TrapCode::INTEGER_DIVISION_BY_ZERO` を明示し、
  signed `sdiv`/`srem` の負数挙動が Julia `div`/`rem`/`%` と一致することを JIT regression で固定した。
- unsigned integer carrier は `sdiv`/`srem` ではなく `udiv`/`urem` を使うようにし、`UInt64` high-bit 値の
  division/remainder を検証した。
- `mod`/`fld`/`cld` など builtin division family は Cranelift adapter では未実装 semantics として
  `Issue #7119` diagnostic にする。
- 検証: Cranelift division-family regression と Cranelift release nextest。

### Cranelift nested break/continue target coverage ✅ (Issue #7116)

- Cranelift の loop control lowering を、continue edge が loop latch に入り induction variable を更新する CFG と、
  nested inner break が outer loop 全体ではなく inner exit / outer latch へ分岐する CFG で検証した。
- どちらも phi block args を伴う target なので、break/continue target と SSA value merge が同時に崩れないことを
  JIT 実行 regression で固定した。
- 検証: Cranelift break/continue regression と Cranelift release nextest。

### Cranelift switch coverage and type gate ✅ (Issue #7114)

- `Terminator::Switch` の lowering を empty case default jump、Bool key、case/default から phi merge へ流れる
  block args で検証した。
- Cranelift switch は現状 integer/Bool/Char tag を `icmp` chain として lower するため、Float64 など非整数 key は
  NaN/`isequal` semantics 実装まで `Issue #7114` 付き unsupported diagnostic として止める。
- 検証: switch default/Bool/phi/Float gate regression と Cranelift release nextest。

### Cranelift phi placeholder removal ✅ (Issue #7113)

- `Instruction::Phi` が block parameter mapping を持たない場合に typed zero placeholder を生成する経路を廃止し、
  malformed phi として明示 diagnostic を返すようにした。
- phi parameter を持つ target block へ incoming value が無い edge や、incoming 個数が phi destination 個数と
  一致しない edge も `get_phi_args` で検出する。
- 検証: malformed phi regression、既存 phi/back-edge regression、Cranelift release nextest。

### Symbolics サブセット: 微分 — 中核セット完成 ✅ (Issue #6572)

- `diff.jl`: `derivative(expr, var)` / `Differential(x)`(eager 微分。和/積/商/冪/連鎖律 + 初等関数)。
  `derivative(x^2,x)==2x`、`Differential(x)(sin(x))==cos(x)`。非簡約なので 2 階微分は
  `simplify(derivative(...))`。
- **Issue #6572 中核セット完成**: `@variables`/型/四則・冪・初等関数/`show`/`substitute`/`simplify`・
  `expand`/微分が `using Symbolics` で動作。
- subset-VM バグ #7185(モジュール内 struct call operator 未ディスパッチ → クロージャ設計)・
  #7186(複雑再帰関数のロード時コンパイルハング → 小ヘルパ分割)を起票・回避。
- テスト: fixture `packages/symbolics_derivative.jl`(manifest、21 アサーション)。

### Cranelift CFG loop/back-edge coverage ✅ (Issue #7112)

- Cranelift `compile_function_body` の block parameter based phi lowering が、loop header への back-edge、
  nested loop、複数 latch からの multi-back-edge を正しく扱うことを JIT 実行 regression で固定した。
- low-level IR の `Terminator::Jump` / `Branch` が phi destination block へ渡す block args を検証し、
  placeholder や stale value ではなく latch 側の SSA value が使われることを確認した。
- 検証: Cranelift loop/back-edge regression と Cranelift release nextest。

### Cranelift runtime-checked call/conversion gates ✅ (Issue #7111)

- Cranelift low-level `Call` が未解決関数を typed placeholder `0` として生成する経路を廃止し、
  runtime check が必要な call として明示 diagnostic にした。
- `TypeAssert` も source/destination/target 型が一致しない変換では pass-through せず、InexactError 等の
  runtime check 実装まで unsupported にする。
- 検証: Cranelift unknown call / conversion regression と Cranelift release nextest。

### Symbolics サブセット: `simplify` / `expand` ✅ (Issue #6572)

- `simplify.jl`: `simplify`(同類項/同因子結合・定数畳み込み・正準順序)と `expand`(積/小整数冪の分配
  → simplify)。`(x+y)^2 → x^2 + 2*x*y + y^2`、`2x+3x → 5x` 等。検証は substitute 数値評価。
- 実装中に subset-VM バグ #7180(モジュール private ヘルパを Base HOF にラムダ経由で渡すと未解決)を
  起票し、明示ループで回避。
- テスト: fixture `packages/symbolics_simplify_expand.jl`(manifest、21 アサーション)。

### Cranelift integer overflow wrapping parity ✅ (Issue #7110)

- Cranelift scalar integer `+` / `-` / `*` が Julia / Rust backend と同じ two's-complement wrapping
  semantics になることを JIT 実行 regression で固定した。
- `typemax(Int64)+1 == typemin(Int64)`、`typemin(Int64)-1 == typemax(Int64)`、
  `typemax(Int64)*2 == -2` を Cranelift で確認。
- 検証: upstream Julia smoke、Cranelift overflow regression、Cranelift release nextest。

### Cranelift array indexing bounds metadata gate ✅ (Issue #7109)

- Cranelift low-level IR の `GetIndex` / `SetIndex` が bounds metadata なしに unchecked pointer
  load/store を生成しないよう、明示 unsupported diagnostic にした。
- 高レベル Cranelift adapter は配列型を引き続き未対応として gate し、bounds-aware array carrier が入るまで
  unsafe な配列アクセス JIT を出さない。
- 検証: Cranelift `GetIndex` / `SetIndex` regression と Cranelift release nextest。

### Symbolics サブセット: `substitute` + 構造的 hash ✅ (Issue #6572)

- `substitute.jl`: `substitute(expr, dict/pair)` が記号変数を置換し数値部分を畳み込む。共有
  `_rebuild`/`_applyelem` を `arithmetic.jl` に追加(初等関数も DRY 化)。`Base.hash(::Num)` を構造的に
  定義し `Num` を `Dict` キーに使える(`d=Dict(x=>3); d[x]`)。
- インライン連鎖 `Dict(x=>3)[x]` の getindex 誤ルートを Issue #7173 で起票(回避: 変数束縛)。
- テスト: fixture `packages/symbolics_substitute.jl`(manifest)。

### Cranelift Bool result / Bool-as-integer operand parity ✅ (Issue #7100)

- Cranelift backend の scalar binary op で、`Bool` operand を mixed numeric/comparison の相手型へ
  zero-extend / float convert するようにした。
- `Bool * Bool` は `Bool` ABI carrier のまま保持し、`Bool + Int64` や `Bool < Int64` は verifier error
  ではなく Julia-compatible promotion 型で lower される。
- 検証: Cranelift JIT unit regression、`juliars --backend cranelift` smoke、Cranelift release nextest。

### AoT type-unstable local Value boxing boundary ✅ (Issue #7075)

- type-unstable local の slot 型を再代入全体の join 型として収集し、converter が初期化と再代入で同じ
  `Value` boundary を使うようにした。
- `x = 1; x = "s"` は invalid Rust の `i64` slot ではなく、`Value::from(...)` で boxing される。
- `typeof(x)` は `Union{...}` slot の static 名ではなく runtime `Value::type_name()` を参照するため、
  実値に合わせて `String` などを返す。
- 検証: upstream Julia smoke、`juliars --minimal-prelude --emit-binary` smoke、
  `--pure-rust --check` runtime-boundary diagnostic、AoT release nextest、`cargo check`。

### Symbolics サブセット: `show`(中置プリティプリント) ✅ (Issue #6572)

- `show.jl`: `Sym`/`Term`/`Num` を演算子優先順位カッコ付けで中置表示。`string`/`print`/`println`/
  `show(io,·)` が `x^2 + 2*x + 1` のような整形出力を返す(VM の `user_show_method_for` 経由)。
- bare REPL / iOS・Web 結果エコーと `repr` は Rust フォーマッタで struct dump になる(全 user struct
  共通の表示経路制約)。汎用修正を Issue #7168 で起票。当面は `println`/`string` で整形表示。
- テスト: fixture `packages/symbolics_show.jl`(manifest)。

### AoT abstract return Any boundary validation ✅ (Issue #7074)

- 抽象 return annotation (`::Real`) の AoT `Any`/`Value` boundary を検証し、未定義 `Real` 識別子や
  `Value + i64` の invalid Rust を生成しないようにした。
- lowered `convert(Real, value)` は、静的に subtype と分かる戻り値なら `Value::from(value)` として box する。
- `Any` を含む binary operation は runtime `dynamic_binop` に接続し、`--pure-rust --check` では dynamic
  operation として明示 diagnostic を返す。
- 検証: upstream Julia smoke、`juliars --minimal-prelude --emit-binary`、`--pure-rust --check`、
  AoT release nextest。

### AoT Any-boxed ternary branch boxing ✅ (Issue #7166)

- `Any`/`Value` boundary へ flow する mixed ternary を、式全体ではなく branch ごとに boxing するようにした。
- `Value::from(if flag { 1i64 } else { 2.5_f64 })` の invalid Rust を防ぎ、
  `if flag { Value::from(1i64) } else { Value::from(2.5_f64) }` を生成する。
- 検証: AoT codegen regression、#7074 integrated regression、`juliars --emit-binary` smoke。

### AoT Bool power DomainError / Float64 boundary parity ✅ (Issue #7073)

- `Bool ^ signed integer` の AoT inference を `Any` ではなく `Bool` にし、Rust backend が boxed Float64 を返さないようにした。
- `true^-1 == true`、`false^0 == true`、`false^positive == false` を Bool として生成し、
  `false^-1` は Julia-compatible DomainError message を維持する。
- `Bool ^ Float64` は Float64 `powf` 経路として、`true^-1.0 == 1.0` / `false^-1.0 == Inf` を維持。
- 検証: upstream Julia smoke、`juliars --minimal-prelude --emit-binary` Bool power smoke、
  AoT inference/codegen/E2E release nextest。

### Symbolics サブセット基盤 + `@variables` ✅ (Issue #6572)

- バンドルパッケージ `Symbolics`(Pure Julia)を新設。`using Symbolics; @variables x y` で
  `Num`-ラップした記号変数を caller スコープへ束縛しベクトルを返せるようになった。
- 型: `Sym` / `Term` / `Num <: Real` + `unwrap`/`value`(`packages/Symbolics/src/types.jl`)。
- 登録は `src/julia/packages/mod.rs` のみ(Primes 同形)。微分・simplify 等は後続 PR。
- 詳細は [SYMBOLICS.md](./SYMBOLICS.md)。

### Symbolics サブセット: `Num` 算術・初等関数・等価性 ✅ (Issue #6572)

- `arithmetic.jl`: `Num` の `+ - * / ^`・単項 `-`・`==`/`isequal`・`zero`/`one` を**混合型
  メソッド網羅**で実装(`Num <: Real` の #5966 promote 再帰を回避)。初等関数 sin/cos/tan/exp/log/sqrt。
- 浅い正規化(定数畳み込み + 0/1 恒等)。TermInterface 風アクセサ `operation`/`arguments`/`iscall`/
  `issym`/`isterm` を追加・export。`x^2 + 2x + 1` 等が sjulia 上で構築・比較できる。
- 作業中に subset-VM バグ #7162(マクロ内 Expr スプラット)/#7163(QuoteNode→Any)を MWE 付きで起票。
- 詳細は [SYMBOLICS.md](./SYMBOLICS.md)。

### AoT print / println collection display parity ✅ (Issue #7072)

- AoT Rust backend の `print`/`println`/`string(...)` 表示境界で、配列・タプルを Julia `show` 風の
  typed formatting expression に通すようにした。
- `Vector{T}` は `[a, b]`、`Vector{String}` は `["a", "b"]`、nested vector は `[[...], [...]]`、
  `Array{T,2}` は `[1 2; 3 4]` 形式で表示される。
- タプルは `(1, "x")` と singleton comma を保持し、内部 String/Char の引用と Float 表示 helper を再利用する。
- 検証: upstream Julia smoke、`juliars --minimal-prelude --emit-binary` collection display smoke、
  AoT codegen unit。

### AoT static dispatch ambiguity / no-method diagnostics ✅ (Issue #7071)

- AoT Rust backend の static dispatch resolver が unique most-specific method だけを選ぶようになった。
  同スコアで互いに具体性を支配しない候補は Julia と同じ ambiguity diagnostic として拒否する。
- no-method static call は存在しない mangled Rust symbol へ流さず、`no method matching f(::T, ...)` の
  codegen diagnostic にする。
- 明示 `::Any` parameter は call-site specialization で concrete type に潰さず、Julia の fallback method として保持する。
- generated runtime dispatcher も overlapping fallback arms より前に ambiguity guard を出し、
  dynamic `Value` 経路の先勝ち silent mismatch を防ぐ。
- method table は同一 mangled signature を dedupe し、既存 multiple-dispatch E2E の通常 specialization は維持。
- user-level `::Any` generic methods が dispatcher を bypass する高位 pipeline gap は Issue #7158 へ分離。
- 検証: upstream Julia ambiguity/no-method smoke、AoT codegen unit、AoT multiple-dispatch E2E。

### AoT HOF function-value codegen and non-Copy element parity ✅ (Issue #7070)

- `map`/`filter`/`reduce`/`foldl`/`sum(f, xs)`/`mapreduce(f, op, xs)` が named function / operator function value を
  AoT Rust backend で静的に解決し、必要な callee を DCE 後も保持するようになった。
- `String` など非 Copy element の `map`/`filter` は clone-based iterator lowering を使い、
  result element type を `Any`/`Value` に落とさず `Vec<String>` として生成する。
- generated struct constructor parameter は `__sjulia_field_*` に分離し、global `const im` と
  `Complex::new(..., im)` の Rust pattern 衝突を解消(Issue #7154)。
- AoT E2E の array index assertion は BoundsError guard 付き checked indexing を期待する形に更新(Issue #7155)。
- 検証: upstream Julia smoke、`juliars --minimal-prelude --emit-binary` Int/String HOF smoke、
  full `aot_e2e_tests` release nextest。

### AoT `zeros`/`ones` type-argument dimension handling ✅ (Issue #7069)

- `zeros(Int64, 3)` / `ones(Bool, 2)` の先頭型引数を dims と誤解していた AoT inference/conversion を修正。
  既知の Julia 型名は element type として消費し、残りの引数だけを builtin codegen の dims に渡す。
- `zeros(Int64, (2, 3))` のような literal tuple dims は converter で次元引数に展開し、
  既存の 2D `Vec<Vec<T>>` 生成に載せる。
- これにより `zeros(Int64,3)` は `Vector{Int64}`、`ones(Bool,2)` は `Vector{Bool}` として生成される。
- 検証: upstream Julia smoke、`juliars --minimal-prelude` 生成バイナリ smoke、
  AoT inference/converter unit。

### AoT DataType field access gate ✅ (Issue #7068)

- AoT `typeof(x)` は compact な `Value::DataType(String)` carrier で型名表示を提供するが、
  Julia の full `DataType` identity / `parameters` / reflection object model はまだ表せない。
- `typeof(x).parameters` などの `DataType` field access を generated Rust の不正 field access に流さず、
  Issue #7068 を指す AoT unsupported diagnostic として gate するようにした。
- 単純な `typeof(1)` 表示は維持し、生成バイナリで `Int64` を確認。
- 検証: `juliars --minimal-prelude --check` unsupported smoke、
  `juliars --minimal-prelude --emit-binary` typeof smoke、AoT codegen unit。

### AoT integer division/remainder sign parity ✅ (Issue #7067)

- AoT integer `÷`/`div` は Rust の checked guard 付き truncating division を生成し、
  `div(1,0)` と `div(typemin(Int64), -1)` が Julia と同じ `DivideError` になるようにした。
- `%`/`rem` は truncated remainder として維持しつつ、`rem(typemin(Int64), -1) == 0` を
  Julia parity として明示。`mod` は builtin 経路へ分離し、divisor sign の floored remainder を生成する。
- `fld`/`cld` を AoT builtin に追加し、整数では quotient/remainder から floor/ceil division を計算する。
  Bool mixed static case は `true`/`false` を整数へ cast して処理する。
- 検証: upstream Julia smoke、`juliars --minimal-prelude` 生成バイナリ smoke、
  AoT analyzer/codegen unit。

### AoT integer power/abs overflow parity ✅ (Issue #7065)

- AoT integer `^` を `.wrapping_pow` 生成に変更し、`typemax(Int64)^2 == 1` /
  `typemin(Int64)^2 == 0` の Julia wrapping parity を明示的に保つようにした。
- `abs(::Signed)` は `.wrapping_abs()`、`abs(::Unsigned)` は identity を生成し、
  `abs(typemin(Int64)) == typemin(Int64)` になるよう修正。
- `AotBuiltinOp::Abs.return_type` を引数型 preserve に修正し、`abs(x::Int64)` が `Float64` 表示経路へ
  誤って流れないようにした。
- `div`/`fld`/`cld` の特殊ケースは #7067 で別途解消済み。検証: upstream Julia smoke、
  `juliars --minimal-prelude` 生成バイナリ smoke、AoT codegen unit。

### Plots.jl: `plot(...; title=...)` とフレーム毎タイトル ✅ (Issue #7030)

- `plot`/`plot!`/`scatter`/`bar`/`histogram`/`surface`/`heatmap` 等の全公開メソッドに
  `title=""` キーワードを追加(`_new_plot`/`_append_to_current` 系ヘルパへスレッド)。`Plot`
  構造体に `title` フィールドを追加(4 番目。Rust 側 `values[0]`/`values[2]` 参照は不変、`values[3]`
  を読む)。`_CURRENT_TITLE` で current plot のタイトルを追跡し `current()`/`frame` がフレームへ伝播。
- Rust `plotting/plotly.rs`: 静的レイアウト(`render_axis_layout_2d`/`render_scene_layout`)と
  アニメーション(各フレームの `layout.title`、ベースはフレーム 0)へタイトルを出力(JSON エスケープ込み)。
- これで `@gif for t in -π:0.1:π; plot(x, sin.(x .- t), title="t=$t"); end` が iOS/Web で
  フレーム毎にタイトル更新付きアニメーションとして描画される。
- fixture: `packages/plots_gif_title_7030.jl`、Rust: `plot_artifact_mime_tests::{test_gif_per_frame_title_7030,test_plot_title_static_7030}` + `plotly` 単体テスト。iOS/Web の Animation サンプルを更新。

### quote→code: ブロードキャスト/キーワード引数/文字列補間の往復 ✅ (Issue #7029)

- マクロの quote 経路(`cst_to_constructor`)がブロードキャスト呼び出し(`f.(x)` / `x .- y`)、
  キーワード引数(`f(x; k=v)` のカンマ/セミコロン両形式)、文字列補間(`"…$x…"`)を扱えるように
  なった。従来は `quote for {broadcast_call_expression,keyword_argument} not yet supported` や
  補間がリテラル `t=$t` のまま凍結 → `@gif`/`@animate` の本体に `plot(x, sin.(x .- t), title="t=$t")`
  を書くと失敗していた。
- 実装: ブロードキャストは `materialize(Broadcasted(fn, (args...)))` 形を構築(`make_broadcasted_call`
  と同形で往復)。キーワードは `Expr(:kw, :name, value)` を構築し、再低位化側
  (`macro_runtime::call_expr_from_values`/`call_named_expr`)が `:kw` を kwargs として取り出す。
  文字列補間は `Expr(:string, parts...)` を構築し、`expr_value_to_expr` の `:string` アームが
  `StringConcat` に再低位化(内側式は再パースして constructor 化)。
- fixture: `metaprogramming/quote_broadcast_kwarg_interp_7029.jl`。

### AoT 2D array indexing column-major parity ✅ (Issue #7063)

- `Array{T,2}` の `A[k]` が nested `Vec<Vec<T>>` の row を返して generated Rust compile error に
  なる問題を修正し、Julia の column-major linear indexing で scalar を返すようにした。
- runtime shape (`rows`/`cols`) から `row = (k - 1) % rows`, `col = (k - 1) / rows` を計算し、
  `A=[1 2; 3 4]` で `A[1]...A[4] == 1,3,2,4` になることを確認。
- `A[i,j]` も index 式を先に評価してから row/column bounds guard を行う生成式へ整理し、
  cartesian indexing と linear indexing の双方で Julia と同じ scalar order を保つ。
- 検証: upstream Julia smoke、`juliars --minimal-prelude` 生成バイナリ smoke、
  AoT codegen unit。

### AoT 1D array indexing BoundsError parity ✅ (Issue #7062)

- AoT 生成コードの 1D 配列 indexing に runtime bounds guard を追加し、範囲外 access が Rust の
  `index out of bounds` panic ではなく `BoundsError([1, 2], (3,))` 形式で停止するようにした。
- `index < 1` も明示的に検査し、`a[0]` は `BoundsError([1, 2], (0,))` になる。
- runtime array helper の `bounds_error` は Julia の 1-based index を保持する signed index に変更。
  2D indexing も direct nested Rust indexing から guard 付き生成へ移行した(完全な 2D 表示 parity は #7063)。
- 検証: upstream Julia smoke、`juliars --minimal-prelude` 生成バイナリ smoke、
  AoT codegen unit、runtime array/error unit。

### Generator reduction `init` keyword forwarding ✅ (Issue #7133)

- `sum(x for x in itr; init=v)` / `prod(...)` / `maximum(...)` / `minimum(...)`
  が generator 引数 + trailing keyword の形でも実行時 `MethodError` にならないよう、
  `Base.Generator` 向け reduction method に `init` keyword を追加。
- `prod` は no-init の既存 identity/型挙動を保つため、空 generator では既存配列 reduction と同じ
  `_prod_empty_value` を使い、`init` 付きは generator を直接 reduce する。
- 検証: upstream Julia fixture smoke、direct `target/release/sjulia` fixture smoke、
  `timeout 1800 cargo nextest run --release --test fixture_tests generator::chunk_000`、
  full `timeout 1800 cargo nextest run --release` pass。

### iOS: エディタ／REPL のフォントサイズ変更(ピンチ + Cmd ショートカット)✅ (Issue #7008)

- iOS アプリのコードエディタと REPL のフォントサイズを 2 本指ピンチ・Cmd `+`/`=`/`-`/`0` で
  動的に変更できるようにした。エディタと REPL は `@AppStorage("editorFontSize")` で設定を共有し永続化。
- ピンチ実装はエディタが `UIPinchGestureRecognizer`、REPL ログが SwiftUI `MagnifyGesture`。
  フォントサイズ方針(整数 pt 丸め + `10...24` クランプ)は `AppConfiguration.Editor.clampFontSize(_:)`
  に集約し、`FontSizeConfigurationTests` でカバー。

### iOS: REPL 入力欄のフォントサイズ変更を即時反映 ✅ (Issue #7025)

- #7008 のフォローアップ。テキスト入力済みの REPL 入力欄で Cmd `+`/`-`/`0` を押したとき、
  編集中テキストが次の編集まで旧サイズのままだったバグを修正。
- `SyntaxHighlightedTextField` / `SyntaxHighlightedTextEditor` の `updateUIView` に
  `uiView.font?.pointSize != fontSize` でフォントのみの変更を検出して `applyFontSizeChange` を
  呼ぶ分岐を追加(エディタ `MonospacedTextEditor` と同じパターン)。`REPLFontSizeUpdateTests` を追加。

### Plots.jl アニメーション `@animate` / `@gif` / `Animation` / `gif` ✅ (Issue #6355)

- `using Plots; p = plot(1); @gif for x = 0:0.1:5; push!(p, 1, sin(x)); end` が iOS/Web で
  自動再生+ループ+スライダー付きの Plotly `frames` アニメーションとして描画される。
- ファイル I/O / FFmpeg 不使用。各フレームを Plot スナップショット(in-memory)で蓄積し、
  Rust 側 `generate_plotly_animation_json` が各フレームを既存 trace 生成(`extract_series`/`render_trace`)で
  再利用して `frames` JSON を生成。MIME は既存 `application/vnd.plotly+json` のまま(C ABI 変更なし)。
- 追加 API(`packages/Plots/`): `Animation`/`AnimatedGif`、`current()`、`plot(::Number)`、
  `Base.push!(::Plot,i,y)`、`frame`、`gif`、`@animate`/`@gif`。fixtures: `packages/plots_{push_point,animate,gif_artifact,gif_macro}_6355.jl`、
  Rust: `plot_artifact_mime_tests::test_animate_gif_emits_plotly_frames_animation_6355`。

### バンドルパッケージのマクロを `using` で公開 ✅ (Issue #6355)

- `include` されたファイル内のマクロが `Module.macros` に集約される(`IncludedContent::merge_into`)。
- `ensure_bundled_package_macros_loaded` でバンドルパッケージのマクロを登録し、式/文の両ディスパッチで
  `macro_runtime` 経由展開(ユーザ定義マクロと同じ完全展開)。`@testset` 等 stdlib 経路は不変。

### `esc` した 3 引数ステップレンジ `a:b:c` の平坦化 ✅ (Issue #7020)

- マクロで `esc`/補間したステップレンジが `(a:b):c` の入れ子のまま再低位化され
  `expected numeric value, got Range` で落ちていたのを、`call_named_expr`/`handle_call` で平坦化して修正。
  regression: `metaprogramming/esc_step_range_7020.jl`。

### VM キーワードディスパッチ不具合の起票と回避 (Issue #7021)

- 単一位置引数+同一キーワードのメソッドが 3 つ以上だと選択メソッドがキーワードを取りこぼす不具合を起票。
- `#6355` の `plot(1)` 追加で発覚。`plot(y::Vector)`+`plot(y::Number)` を untyped `plot(y)` 1 つに畳む回避を
  `WORKAROUNDS.md` に登録(`plot(sin, aspect_ratio=:equal)` 回帰を解消)。根本修正は #7021。

### VM キーワードディスパッチ根本修正 ✅ (Issue #7021)

- runtime dispatch now preserves keyword argument payloads by emitting the
  keyword/splat VM call opcode when kwargs are present.
- ambiguous single-argument overload dispatch and dynamic no-method fallback
  paths now select the same method and see the same keyword values as Julia.
- 検証: upstream Julia fixture smoke、direct `target/release/sjulia` fixture smoke、
  `fixture_tests kwargs::` pass。

### Float step range endpoint length ✅ (Issue #7024)

- floating-point stepped ranges account for inclusive endpoint roundoff, so
  `0:0.1:0.3` reports length 4 and indexes the rounded final value.
- VM collection helpers, range indexing, and `last(range)` now delegate to
  centralized `RangeValue` length/last behavior.
- 検証: upstream Julia fixture smoke、direct `target/release/sjulia` fixture smoke、
  focused range unit test、`fixture_tests range::` pass。

### AoT implicit/boxed return regressions ✅ (Issues #7010, #7012)

- converter regression coverage fixes the implicit full-form function body path
  that previously could surface an `Undef` AoT expression.
- inliner regression coverage fixes the runtime-boxed return path so unit main
  bodies keep the call instead of invalidly inlining it.
- 検証: focused AoT converter/inliner tests pass。

### Plots existing Plot replot support ✅ (Issue #7026)

- `plot(p::Plot)` now restores the existing plot's series/current state and
  returns a Plot, avoiding the old `length(::Plot)` MethodError fallback.
- keyword-preserving runtime dispatch now covers dynamic no-method fallback
  paths as well, so Plot replot keyword overrides are not dropped.
- 検証: upstream Julia with local package load path、direct `target/release/sjulia`
  fixture smoke、`fixture_tests packages::` pass。

### REPL inline suggestion font refresh ✅ (Issue #7028)

- font-size-only updates in the single-line field and multi-line editor refresh
  completion state after re-highlighting.
- active inline suggestion labels are recreated through `showInlineSuggestion`
  with the new font size instead of staying at the old point size until the next
  keystroke.
- 検証: source-level review; local environment lacks `xcodebuild`, so iOS
  simulator build is deferred to CI。

### AoT throw helper Display error text ✅ (Issue #7018)

- AoT runtime `aot_throw` now panics with `Display` text, so
  `RuntimeError::DivisionByZero` reaches generated binary stderr as
  `DivideError: integer division error` instead of the enum debug name.
- generated prelude `throw` uses the same `Display` bound, and generated
  `ErrorException` implements `Display` for its message.
- 検証: runtime panic-message regression test、release `juliars --emit-binary`
  divide-by-false smoke pass。

### AoT `typeof` DataType display parity ✅ (Issues #6973, #7015)

- AoT runtime `Value::DataType(String)` を追加し、`typeof` codegen が Julia type name を
  持つ DataType value を返すようにした。
- static 型は `StaticType::julia_type_name()`、dynamic `Value` は runtime
  `type_name()` から DataType carrier を生成し、Rust carrier 名 (`i64`) が stdout に出る
  regression を解消した。
- 検証: focused codegen/runtime regression tests、upstream Julia vs generated binary
  stdout diff pass。

### juliars Cranelift backend CLI reachability ✅ (Issue #6927)

- `--backend cranelift` selection を `CompileConfig` へ渡し、early usage error ではなく
  backend codegen boundary で処理するようにした。
- `cranelift` feature 付き build では scalar / straight-line subset を低レベル
  `IrModule` へ落として既存 `CraneliftCodeGenerator` を呼び出す。feature なし
  build は rebuild 手順付き `UnsupportedInstruction` として分類する。
- 検証: focused no-feature / feature-gated Cranelift regression tests、release
  `juliars --backend cranelift --check` / `-o -` smoke pass。

### AoT parametric struct constructor gate ✅ (Issue #6975)

- parametric struct definition と、`Box(1)` のような未解決 constructor-like call を
  converter boundary で分類し、Rust backend へ無効な constructor call が漏れる経路を止めた。
- full parametric struct codegen が入るまでは、span 付き `UnsupportedInstruction` として
  exit 5 で拒否する。既存 `Complex` special-case は維持する。
- 検証: focused converter regression tests、release `juliars --check` repro smoke pass。

### AoT converter/inference panic-free cleanup ✅ (Issue #6933)

- function conversion の parameter env setup から不要な `params.iter().find(...).unwrap()`
  を削除し、loop で保持している type を直接使うようにした。
- multi-argument operator unfold の初期値取得を explicit `InternalError` にし、
  call-site specialization / single return inference の single-element vector は
  `.unwrap()` ではなく `remove(0)` で取り出す。
- 検証: focused function conversion / inference regression tests pass。

### AoT expression-position begin/let side-effect gate ✅ (Issue #7014)

- expression-position `begin` / `let` block を最後の expression だけに落として
  preceding side effect を消す fallback を停止した。
- bindings なし・単一 expression の block は維持し、bindings あり / multi-statement /
  side-effecting block は sequence expression support まで span 付き
  `UnsupportedInstruction` として拒否する。
- 検証: focused converter regression tests、release `juliars --check` repro smoke pass。

### AoT Float print/string display parity ✅ (Issues #7013, #7017)

- generated Rust の `print` / `println` / `string(...)` 境界で static
  `Float64` / `Float32` を Julia-style helper に通し、whole float を `3` ではなく
  `3.0`、`-0` ではなく `-0.0` として表示する。
- unshadowed global `Inf` / `Inf32` / `Inf64` / `NaN` / `NaN32` / `NaN64` は
  converter で float literal へ変換し、generated Rust の bare `Inf` / `NaN`
  compile error を解消した。local binding による shadowing は維持する。
- 検証: upstream Julia float display smoke、focused converter/codegen regression
  tests、release `juliars --emit-binary` stdout diff pass。

### AoT local `Dict` / `Set` construction gate ✅ (Issue #7016)

- local `Dict(...)` / `Set(...)` construction を converter boundary で分類し、Pure
  Julia collection body 由来の `Any` condition が Rust backend まで漏れて exit 6
  になる経路を止めた。
- `Dict` は Issue #6971、`Set` は Issue #6972 の full codegen 実装まで
  span 付き `UnsupportedInstruction` として exit 5 で拒否する。
- 検証: focused converter regression test、release `juliars --check` local Dict/Set
  repro smoke pass。

### AoT Complex arithmetic Float64 layout gate ✅ (Issue #6965)

- `Complex{Float64}` / `ComplexF64` / legacy `Complex64` の Rust type projection を既存 monomorphic `Complex` layout に揃えた。
- `Complex{Float32}` / `Complex{Int64}` など non-`Float64` parameterized Complex の static `+` / `-` / `*` codegen は、誤 Rust を出さず diagnostic gate にする。
- 検証: focused Complex arithmetic codegen regression test pass。

### AoT Char literal Unicode boundary ✅ (Issue #6967)

- valid Unicode scalar の Char literal は Rust `char` literal として escape し、quote/backslash/control/non-ASCII scalar を regression test で固定。
- Julia の invalid-codepoint `Char` は Rust `char` で表現できないため、conversion-to-Char は diagnostic gate にする。
- 検証: focused Char literal/codepoint boundary codegen regression test pass。

### AoT Complex `im` lexical shadowing ✅ (Issue #6966)

- `im` 参照を内部 alias `IM` へ一律変換する workaround を撤去し、Julia global `im` は generated Rust の lowercase `const im` として emit。
- function parameter / local named `im` は通常の Rust lexical shadowing で global `im` を隠すようにした。
- 検証: focused Complex `im` shadowing codegen regression test pass。

### AoT random builtin codegen gate ✅ (Issue #6964)

- `rand` の undeclared `rand::random::<f64>()` emission と、`randn` の constant `0.0` fallback を削除。
- VM-compatible RNG contract / seed control が実装されるまで `rand` / `randn` は codegen diagnostic で拒否。
- 検証: focused random builtin codegen regression test pass。

### AoT range step / float / empty parity ✅ (Issue #6969)

- `AotExpr::Range` は Rust `..=` / `step_by(step as usize)` を直接出さず、`Vec<T>` materialization へ変更。
- integer / `Float32` / `Float64` range の positive step、negative step、方向不一致 empty range、zero-step diagnostic を生成コードで固定。
- range inference / converter は `step` 型も含めて element type を決め、mixed `Int64` bounds + `Float32` step では bounds を `f32` へ cast する。
- 検証: upstream Julia range smoke、focused codegen/inference/converter regression tests pass。

### AoT checked numeric conversion gates ✅ (Issue #6968)

- `AotExpr::Convert` の Rust `as` fallback を整理し、float->integer、integer narrowing、符号境界、numeric->Bool は Julia `InexactError` parity 用 runtime check が入るまで codegen error にする。
- lossless integer widening、integer->float、Float32/Float64 間、Bool->numeric、Char->wide integer は引き続き Rust cast を生成。
- Core IR `fptosi` も direct `as i64` ではなく diagnostic で拒否。
- 検証: focused checked-conversion codegen regression test pass。

### AoT tuple `first` / `last` field access ✅ (Issue #6963)

- tuple-specific `TupleFirst` / `TupleLast` は Rust tuple field access (`.0`, `.N`) を生成し、array-style indexing 前提を撤去。
- empty tuple / non-tuple argument は codegen error として拒否。
- 検証: focused tuple first/last codegen regression test pass。

### AoT tuple dynamic index gate ✅ (Issue #6962)

- static tuple + constant in-range index のみ `.N` field access へ lower するよう固定。
- dynamic `t[i]` と literal out-of-bounds index は invalid Rust を出さず diagnostic gate にする。
- 検証: focused tuple dynamic/out-of-bounds index codegen regression test pass。

### AoT 2D array shape builtins use static rank ✅ (Issue #6959)

- `length(::Matrix)` は rows*cols を返す generated code へ変更し、outer `len()` / row-length sum の簡易実装を撤去。
- `size` / `ndims` も inferred array rank から生成し、3D+ は一般 N-D codegen まで diagnostic gate にする。
- 検証: focused array shape builtin codegen regression test pass。

### AoT 3D+ arrays are explicitly gated ✅ (Issue #6960)

- 3D+ array literal / indexing / `zeros` / `ones` の 3 dimension 以上指定を Rust nested `Vec` へ黙って lower しない diagnostic gate にした。
- 1D/2D の現行 `Vec` / `Vec<Vec<_>>` surface は維持し、一般 N-D Array carrier は [UNIMPLEMENTED.md](./UNIMPLEMENTED.md) に残す。
- 検証: focused 3D array gate codegen regression test pass。

### AoT array shape rank selection is static ✅ (Issue #6961)

- `length` / `size` / `ndims` の 1D vs 2D codegen branch は `StaticType::Array.ndims` で決め、source spelling や runtime nested-`Vec` probe に戻らないよう regression test 化した。
- `ndims: None` は従来互換の 1D fallback、3D+ は Issue #6960 の diagnostic gate に分離する。
- 検証: focused static-rank shape selection codegen regression test pass。

### AoT Bool/Int arithmetic and condition boundary ✅ (Issue #6980)

- `Bool` を含む static `+` / `-` / `*` と mixed comparison は generated Rust の型不一致を避ける numeric cast を入れ、inferred result width を保持する。
- `Bool * Bool` は Bool result として `&&` へ lower し、`if` / `elseif` / `while` / ternary の non-Bool condition は diagnostic で拒否する。
- `Bool`/`Bool` と mixed `Bool`/integer の `÷` / `%` / `^` は Julia result surface
  に合わせ、signed integer exponent の `Bool ^ n` は必要に応じて `Value` boundary
  で `Float64` / `DomainError` を表す。
- strength reduction は Bool operand / Bool result を shift や integer literal へ
  変換しないため、`true ÷ 2` や `false ^ 0` の source smoke が generated Rust
  でも upstream stdout と一致する。
- 検証: upstream Julia Bool arithmetic/condition/error smoke、focused Bool codegen /
  converter / optimizer / inference regression tests pass。

### AoT Nothing and nullable return codegen ✅ (Issue #6979)

- `LitNothing` / `Nothing` return function は Rust unit `()` として生成し、explicit `return nothing` も `return ();` になる。
- `Union{T,Nothing}` の nullable return は runtime `Value` boundary として、ternary arms と explicit `return nothing` を `Value::from(...)` へ boxing する。
- 検証: focused Nothing / nullable union return codegen regression test pass。

### AoT Union value representation uses runtime Rust enum ✅ (Issue #6977)

- multi-variant `Union{...}` は `subset_julia_vm_runtime::Value` Rust enum boundary として表現する。
- generated Rust は `Union{Int64,String}` return を `Value` return type にし、branch values を `Value::from(...)` で boxing する。
- 検証: focused multi-variant Union return codegen regression test pass。

### AoT struct definitions emit in dependency order ✅ (Issue #6974)

- generated Rust の struct definitions は field type dependency を topological sort して、入力順に関係なく依存先を先に出す。
- nested container / tuple / union / function type 内の `StaticType::Struct` dependency も走査し、循環 struct dependency は diagnostic gate にする。
- 検証: focused struct dependency ordering / cycle diagnostic codegen regression tests pass。

### AoT type-unstable bindings use Value boundary ✅ (Issue #6978)

- `Any` / `Union` slot の `let` / assignment は native value を `Value::from(...)` へ boxing し、分岐ごとの型変化を runtime `Value` boundary で保持する。
- fixed native slot へ incompatible value を入れる path は、型不安定 local は `Any` / `Union` boundary が必要という diagnostic にする。
- 検証: focused type-unstable binding codegen regression tests pass。

### AoT static dispatch picks the most-specific method ✅ (Issue #6976)

- static `CallStatic` resolution は matching methods を specificity score で比較し、program order が broad `Any` method を先に持っていても concrete/exact method を選ぶ。
- 同 specificity の場合は従来どおり source order を保つ。
- 検証: focused broad-first static dispatch regression test pass。

### AoT inliner tracks pure static callees ✅ (Issue #6981)

- inliner の purity analysis は known-pure function set を fixed point で求め、pure `CallStatic` callee を含む wrapper も pure として inline score に反映する。
- `CallDynamic` は world-age / dispatch side effect safety が未整備なため impure のまま維持する。
- 検証: focused inliner purity regression tests pass。

### AoT LICM uses dominator-based back edges ✅ (Issue #6982)

- low-level CFG LICM の loop detection は block order ではなく dominator analysis を使い、target が source を支配する edge だけを back edge として扱う。
- earlier-block cross edge は loop と誤認せず、header が source より後ろに並ぶ natural loop も検出する regression coverage を追加。
- 検証: focused #6982 regression tests と LICM unit-test filter pass。

### AoT LICM refines loop-control dependency hoisting ✅ (Issue #6983)

- loop-control condition 定義は通常の loop-invariant dependency check を通し、loop-carried state に依存しない scalar condition 定義を preheader へ hoist できるようにした。
- induction-variable dependent condition と、no-alias / no-throw proof が未整備の memory/assertion condition 定義は loop 内に残す。
- 検証: focused #6983 regression test と LICM unit-test filter pass。

### AoT DCE removes overwritten dead stores ✅ (Issue #6986)

- high-level AoT DCE に backward liveness scan を追加し、読み出し前に後続 `Assign` で上書きされる plain variable store を削除する。
- `Let` declaration、branch / loop boundary、effectful / throwing 可能性のある RHS は conservative に残し、Rust codegen の宣言形と Julia の評価順を壊さない。
- 検証: focused #6986 regression tests と DCE unit-test filter pass。

### AoT CSE reuses dominating structured expressions ✅ (Issue #6985)

- high-level AoT CSE は `if` branch body に親 scope の available expressions を渡し、branch を支配する計算を branch 内の重複式へ再利用する。
- loop body も親 scope を seed するが、loop variable / body-modified variable を含む expression は invalidation して iteration 間の値変化を誤用しない。
- availability merge は子 block 方向だけに限定し、branch / loop exit への global merge は未導入。
- 検証: focused #6985 regression tests と CSE unit-test filter pass。

### AoT direct self-tail recursion to loop ✅ (Issue #6987)

- explicit `return f(args...)` が direct self `CallStatic` の場合、argument temps → parameter assignments → `continue` へ変換する high-level AoT optimizer pass を追加。
- constant-true transformed loop は Rust `loop { ... }` として emit し、reassigned parameter scan が `mut` parameter signature を生成する。
- nested existing loop bodies は `continue` target safety のため対象外。
- 検証: focused #6987 optimizer/codegen regression tests と AoT optimizer unit-test filter pass。

### AoT optimizer Criterion benchmark gate ✅ (Issue #6945)

- `aot_optimizer_benchmark` を追加し、constant folding / strength reduction / CSE / DCE / loop optimization / inlining / direct self-tail recursion TCO の synthetic AoT IR pass cost を Criterion で計測できるようにした。
- `subset_julia_vm/Cargo.toml` に `required-features = ["aot"]` の bench entry を登録。
- 検証: `timeout 1800 cargo bench -p subset_julia_vm --features aot --bench aot_optimizer_benchmark --no-run` pass。

### AoT fixture stdout parity helper ✅ (Issue #6954)

- `scripts/aot_fixture_julia_parity.sh` を追加し、`juliars --emit-binary` で作った native binary stdout と upstream `julia` stdout を exact diff する developer helper を用意。
- release `juliars` と `julia` on PATH を前提に、fixture 単位で AoT e2e parity を確認できる。
- 検証: shell syntax check と temporary `println(1 + 2)` fixture smoke pass。

### AoT VM-vs-generated-binary differential helper ✅ (Issue #6942)

- `scripts/aot_vm_differential.sh` を追加し、release `sjulia` VM stdout と `juliars --emit-binary` で作った native binary stdout を exact diff する developer helper を用意。
- release `juliars` と release `sjulia` を前提に、upstream Julia parity とは別の VM/AoT backend 差分確認に使える。
- 検証: shell syntax check と `fixtures/aot/builtin_stdout_parity_6999.jl` の VM-vs-AoT smoke pass。

### AoT supported builtin stdout parity fixture ✅ (Issue #6999)

- `fixtures/aot/builtin_stdout_parity_6999.jl` を追加し、supported scalar operators / builtins の AoT generated-binary stdout が upstream Julia と一致することを固定。
- `sqrt(9.0)` の Float64 whole-value stdout formatting mismatch は Issue #7013 として分離。
- 検証: AoT parity helper、fixture nextest `aot::`、fixture name audit pass。

### AoT fixture no-silent-mismatch property harness ✅ (Issue #7003)

- `scripts/aot_fixture_no_silent_mismatch.sh` を追加し、VM-passing fixture が generated AoT binary で original stdout と final value の両方を保つか、exit code 5 (`UnsupportedInstruction`) で明示拒否されることを検査可能にした。
- 引数なしでは manifest fixture corpus を列挙し、引数ありでは targeted subset を検査する。
- probing 中に begin-wrapper side effect gap を Issue #7014 として分離し、harness は original stdout と final-value last line を別々に比較する。
- 検証: shell syntax check、`fixtures/aot/builtin_stdout_parity_6999.jl` compiled+matched smoke、temporary `<:` unsupported smoke pass。

### AoT DataType carrier and `typeof` codegen gate ✅ (Issues #6973, #7015)

- `StaticType::DataType` を追加し、`typeof(x)` inference / builtin return type を `Any` / `String` ではなく DataType carrier へ分類。
- Rust backend の `std::any::type_name_of_val` codegen を止め、Julia DataType representation が実装されるまで `typeof` は `UnsupportedInstruction` として拒否する。
- `DataType` は runtime `Value` boundary / rooting-required carrier として ABI 分類する。
- 検証: focused #6973/#7015 tests と CLI unsupported smoke pass。

### AoT `map` / `filter` non-`Copy` element codegen ✅ (Issues #6957, #6958)

- named-function `map` は `iter().cloned()` 経由で要素を渡し、`String` など non-`Copy` element で `|&x|` destructuring しないよう修正。
- named-function / closure `filter` も predicate へ cloned value を渡し、retained element は `.cloned()` で result に集める。
- 検証: focused `Vector{String}` HOF codegen regression test pass。

### AoT typed `zeros` / `ones` fill literals ✅ (Issue #6956)

- `return_ty` の配列 element type から `zeros` / `ones` の fill literal を選び、`Int32` / `UInt8` / `Float32` / `Bool` などの concrete scalar 幅を保持。
- unsupported non-scalar element は誤った `f64` fallback ではなく codegen error にする。
- 検証: upstream Julia typed zeros/ones smoke、focused codegen regression test pass。

### AoT Cranelift string/unsupported type gates ✅ (Issues #6948, #6949)

- `ConstValue::String` は Cranelift の managed string/rooting contract 未充足として明示拒否する coverage を追加。
- Cranelift type mapper で未対応の scalar carriers (`I128` / `U128` / `F16` / `Missing`) を列挙し、`TypeConversion` unsupported として固定。
- 検証: focused Cranelift feature tests pass。

### AoT Cranelift rooting/safepoint contract gate ✅ (Issue #6947)

- Cranelift backend の signature / module generation boundary で runtime `Value` や rooting model 必須型を拒否する regression coverage を追加。
- Cranelift feature は native scalar / straight-line IR の実験経路に限定し、Rust
  backend より狭い CLI 接続として support matrix に明記。
- 検証: focused Cranelift feature tests pass。

### AoT Union/abstract return inference and fallback coverage ✅ (Issue #6939)

- 複数 concrete return の `Union{...}` signature、abstract return annotation の `Any`/`Value` boundary、static call が dynamic fallback に数えられないことを regression test 化。
- runtime-boxed return 関数は inliner が caller-context return rewrite を持つまで inline しない gate を追加し、`main` への不正 `return Value::from(...)` 混入を防止 (Issue #7012)。
- 検証: focused inference/codegen/optimizer tests pass。

### AoT Float32/Float64 type preservation coverage ✅ (Issue #6941)

- Float32 と整数の static arithmetic / division / comparison codegen で `f32` 幅を保持し、Float64 混在時のみ `f64` へ widen するように固定。
- Rust では無効な `bool as f32/f64` を避け、Bool numeric cast は integer 経由で generated Rust を出す。
- 検証: upstream Julia Float32 promotion smoke、focused inference/codegen regression tests。

### AoT low-level optimizer gaps documented ✅ (Issue #6944)

- low-level `IrFunction` strength reduction / inlining no-op を `UNIMPLEMENTED.md` と AoT design note に明示し、現行主経路が高レベル `AotProgram` optimizer であることを固定。
- 検証: documentation diff check、AoT clippy pass。

### AoT integer arithmetic uses wrapping codegen ✅ (Issue #6940)

- 同一 concrete integer 型の `+` / `-` / `*` と `+=` / `-=` / `*=` を Rust `wrapping_*` codegen に変更し、Julia の overflow wrap semantics を固定。
- 検証: upstream Julia overflow smoke、focused arithmetic/compound/snapshot codegen tests、AoT clippy pass。

### AoT lambda multi-statement body rejects with diagnostic ✅ (Issue #6938)

- multi-statement lambda body / single-expression でない lambda body を `LitNothing` に黙って変換する fallback を廃止し、body span と workaround 付き `UnsupportedInstruction` へ変更。
- 検証: focused converter diagnostic test pass。

### AoT CSE temp-var dead scaffold removed ✅ (Issue #6943)

- 未配線の `_cse_*` temp-var counter / helper を削除し、CSE は後続の重複式を先行 binding 参照へ置換する現在の実装形状に整理。
- 検証: focused CSE scaffold removal test pass。

### AoT DCE constant-condition simplification boundary ✅ (Issue #6984)

- `if` / `while` 条件の nested foldable constant expression と boolean `!` を DCE 単体で評価し、到達不能 branch / false loop を削除する regression test を追加。
- side-effect unknown な call / variable condition は畳み込まず保持する conservative boundary のまま。
- 検証: focused DCE tests pass。

### AoT loop optimizer boundary and alias correctness tests ✅ (Issue #6946)

- empty range / zero-step boundary と、mutated variable 依存式を LICM が hoist しない alias correctness を regression test 化。
- 検証: focused loop optimizer tests pass。

### AoT rooting conservative liveness coverage and cost checks ✅ (Issue #6989)

- heap-shaped `StaticType` が rooting model を要求する coverage test と、native scalar locals を safepoint obligation から除外する cost guard を追加。
- current Rust backend は owned runtime `Value` contract のまま、future native backend 向けの under-rooting 防止を固定。
- 検証: focused rooting tests pass。

### AoT many-function compile-time scaling stress test ✅ (Issue #7002)

- 128 typed functions の reachable call-chain を synthetic Core IR で組み、AoT pipeline 全体が bounded time で compile できることを regression test 化。
- #7010 により source-level function input は避け、Core IR 直接構築で DCE/inference/conversion/optimizer/codegen scaling を測定。
- 検証: focused stress test pass。

### AoT native-call boundary design and gate ✅ (Issue #6988)

- `ccall` / `llvmcall` / `Core.Intrinsics.llvmcall` を AoT native-call boundary として分類し、safe subset 未実装の現状では span 付き diagnostic で拒否。
- future supported path は typed `AotCallAbi` 必須にし、pass verifier でも backend 到達を backstop。
- 検証: classifier / converter span / verifier tests pass。

### AoT top-level heap global initializers reject before Rust static codegen ✅ (Issue #7011)

- top-level `String` など Rust `static` const initializer にできない global initializer を span 付き AoT diagnostic で拒否。
- API 経由の `AotGlobal<String>` も codegen backstop で拒否し、uncompilable `static x: String = "...".to_string();` を出さない。
- 検証: focused converter/codegen tests + generated `let` string concat compile smoke pass。

### AoT string concatenation audit ✅ (Issue #6970)

- `String` / `Char` の Julia `*` concat を generated Rust `format!` に落とし、literal concat は constant folding で畳み込み。
- `string(...)` / interpolation-shaped concat の `StringConcat` builtin 出力も regression test で固定。
- 検証: upstream Julia smoke + focused codegen/optimizer tests pass。

### juliars C ABI export entry generation ✅ (Issue #6990)

- `--export-c-abi` を追加し、C-stable scalar signature の generated function を `#[no_mangle] extern "C"` entry として出力。
- overload は generated method 名または alias 指定を要求し、非 C-stable 型 / runtime `Value` boundary は codegen error で拒否。
- 検証: focused C ABI codegen tests + CLI parser tests pass。

### juliars colored/structured span diagnostics ✅ (Issue #6996)

- `--diagnostic-format human|json` と `--color auto|always|never` を追加。
- unsupported AoT span diagnostics は source excerpt/caret/workaround を表示し、JSON 形式も出力可能。
- 検証: focused diagnostic bin tests pass。

### juliars `--target` for `--emit-binary` ✅ (Issue #6994)

- `--target <triple>` を `--emit-binary` の Cargo build に配線し、単独使用は usage error にした。
- 検証: parser tests + no-target `--emit-binary` smoke + AoT clippy pass。

### juliars `--emit-binary` ✅ (Issue #6928)

- `--emit-binary <path>` を追加し、generated Rust を runtime path dependency 付き一時 Cargo project で native binary 化。
- 検証: parser tests + release `juliars --emit-binary` smoke + AoT clippy pass。

### Uninitialized AoT globals reject with diagnostic ✅ (Issue #6937)

- 未初期化 global の `// TODO: static ...` emission を廃止し、workaround 付き `UnsupportedInstruction` に変更。
- 検証: focused uninitialized/initialized global codegen tests pass。

### Stable generated-Rust link helper ✅ (Issue #6951)

- `scripts/juliars_build_generated.sh` を追加し、generated Rust を runtime path dependency 付き一時 Cargo project で build。
- `docs/aot/README.md` の link 手順を helper-first に更新。

### Generated Rust / runtime ABI compatibility check ✅ (Issue #6952)

- `AOT_RUNTIME_ABI_VERSION` を compiler/runtime に追加し、generated Rust prelude に compile-time equality check を emit。
- 検証: focused ABI tests + generated Cargo clippy smoke pass。

### Generated Rust snapshot tests ✅ (Issue #7000)

- AoT Rust codegen の single function と multi-method program sections を inline snapshot tests で pin。
- 検証: targeted generated Rust snapshot nextest pass。

### Generated Rust cargo clippy smoke ✅ (Issue #7001)

- `scripts/test_aot.sh` に generated Rust Cargo project の `cargo clippy -- -D warnings` smoke を追加。
- generated prelude に必要な Clippy allow を追加。
- 検証: manual generated Cargo clippy smoke pass、`bash -n scripts/test_aot.sh` pass。

### AoT generated Rust `rustc -D warnings` clean ✅ (Issue #6950)

- generated prelude で unused runtime imports / discarded expression results を許可し、runtime root から `RuntimeResult` を再 export。
- 検証: generated sample を `rustc -D warnings` で compile。

### AoT 変更時に `scripts/test_aot.sh` を走らせる PR CI gate ✅ (Issue #6953)

- `.github/workflows/ci.yml` に path-aware `aot-gate` job を追加し、AoT 関連変更で `bash scripts/test_aot.sh` を実行。
- 検証: workflow YAML parse check pass。

### AoT `missing` literal support ✅ (Issue #6935)

- `AotExpr::LitMissing` を追加し、Core IR `Literal::Missing` から generated Rust `Value::Missing` まで接続。
- 旧 workaround #3343 を resolved に移動。
- 検証: missing literal focused AoT tests pass。

### AoT UnsupportedInstruction diagnostic に span/workaround を付与 ✅ (Issue #6992)

- `UnsupportedInstructionDiagnostic` を追加し、unsupported AoT boundary error が span と workaround を持てるようにした。
- 検証: native-call diagnostic / subtype gate / exit-code targeted tests pass。

### AoT Rust keyword identifier escaping coverage ✅ (Issue #6934)

- Rust strict/reserved/weak/future keyword を raw identifier へ escape し、`self` / `super` / `crate` / `Self` は `_...` rename するよう修正。
- 検証: targeted AoT codegen unit test pass。

### AoT subtype `<:` placeholder mapping の gate 化 ✅ (Issue #6936)

- `BinaryOp::Subtype` を `AotBinOp::Lt` に落とす誤 mapping をやめ、専用 `AotBinOp::Subtype` を codegen unsupported gate にした。
- 検証: targeted AoT IR/codegen tests pass。

### docs/aot README 全フラグ整合性レビュー ✅ (Issue #6955)

- 当時の `juliars` CLI と `docs/aot/README.md` の flag 記述を揃え、未実装 flag と
  Cranelift reserved gate を明示。現在の Cranelift CLI reachability は Issue #6927 の
  最新項目を参照。
- 対応サブセット / 既知の制限節を追加。

### AoT 対応サブセット・マトリクス ✅ (Issue #7004)

- `docs/aot/SUPPORT_MATRIX.md` を追加し、AoT Rust backend の対応/一部対応/gate/未対応を機能別に一覧化。

### Core IR -> AoT IR 2段変換と feature gate 設計ノート ✅ (Issue #7005)

- `docs/aot/DESIGN.md` に Core IR -> AoT IR の 2 段変換と stage ごとの feature gate/diagnostic boundary を追記。

### juliars `--pure-rust` 失敗診断の残存 runtime symbol 列挙 ✅ (Issue #6926)

- Pure Rust 出力に残った `subset_julia_vm_runtime` 参照行と residual dynamic dispatch diagnostics をユーザー向けに列挙。
- 検証: `scripts/test_aot.sh` 3821/3821 pass + clippy `--features aot --all-targets -D warnings` pass。

### juliars 入力 source の相互排他検証 ✅ (Issue #6929)

- `-e` / `--ir` / file / stdin の同時指定を usage error として拒否。
- 検証: bin tests + CLI smoke で exit code `2`。

### juliars `--stats` の AoT 品質情報拡張 ✅ (Issue #6930)

- 生成 Rust LOC、推定出力サイズ、動的 dispatch site 一覧を `--stats` に追加。
- 検証: CLI smoke `--stats --time-passes`。

### juliars `--check` dry-run mode ✅ (Issue #6931)

- Rust を書き出さずに AoT 可否と残存動的 dispatch を報告。
- 検証: CLI smoke `juliars -e '1 + 2' --check`。

### juliars stdin 入力 / stdout 出力 ✅ (Issue #6932)

- `juliars -` と `-o -` を追加。
- 検証: stdin/stdout CLI smoke。

### `compile_from_ir_bytes` 実装 ✅ (Issue #6991)

- serialized Core IR bytes を共有 Core IR -> AoT pipeline に接続し、旧 UNIMPLEMENTED stub 記載を削除。
- 検証: `aot::tests::compile_from_ir_bytes_compiles_serialized_core_ir` pass。

### juliars optimization level `-O` / `--opt-level` ✅ (Issue #6993)

- `O0..O3` の optimizer control を追加。
- 検証: bin parser tests + AoT full gate。

### juliars parse/lowering 診断の span/context 化 ✅ (Issue #6995)

- parser `Span { ... }` Debug dump を含まない CLI message に整形し、
  parser/lowering diagnostic と caret context を表示。
- 検証: bin diagnostic tests + parse CLI smoke exit code `4`。

### juliars エラー種別別 exit code ✅ (Issue #6997)

- usage/io/parse/unsupported/codegen/internal を exit code で分類。
- 検証: unit tests + CLI smoke。

### juliars `--time-passes` ✅ (Issue #6998)

- pass ごとの wall-clock timing を CLI 表示。
- 検証: CLI smoke `--time-passes`。

### web の WASM size 最適化 profile を有効化 (無視されていた `[profile.release]` 修正) ✅ (Issue #6922)

- `subset_julia_vm_web/Cargo.toml` の `[profile.release]`(`opt-level = "s"` / `lto = true`)は
  **ワークスペース非ルートのため Cargo に黙って無視**されており、web/WASM ビルドは意図に反してデフォルト release
  (`opt-level = 3` / `lto = false`)でビルドされていた(`cargo` 実行時に `profiles for the non root package will be ignored`
  warning)。体裁だけでなく **size 最適化が未適用**という実害。
- 対応: 全クレートへ波及させない/`cargo nextest run --release` の compile time を増やさないため、ルート `Cargo.toml` に
  専用 `[profile.web-release]`(`inherits = "release"` + `opt-level = "s"` + `lto = true`)を新設し、web 専用に限定。
  非ルートの `[profile.release]` ブロックは削除(warning 解消)。`lto` は per-package override 不可のため専用 profile が必要。
- ビルド経路を `--profile web-release` へ更新: `scripts/wasm_build_with_cache.sh`(profile/mode 未指定時に自動付与)・
  `Makefile`(`build-wasm`)・各 README/docs の `wasm-pack build --target web` を `--profile web-release` 付きに統一
  (wasm-pack 0.15 の `--profile` を使用)。
- 効果(raw `.wasm`, wasm-opt/embed cache 無しで cargo profile 効果を分離): `release` 17,920,826 B →
  `web-release` 14,218,760 B(**約 3.53 MiB / 20.7% 減**)。warning も解消(`cargo metadata` で確認)。

### `ValueType` 降格エピック開始: 変換面の primitive テーブル重複除去 ✅ (Issue #6916, Slice 1–2)

- #6720(`ConcreteType` 薄ラッパー化)を実質完了でクローズし、残った特大スコープ「`ValueType` 降格本体」
  (`ValueType::` 4539 uses)を独立エピック #6916 へ分離。以下は behaviour-preserving な変換面削減スライス。
- **Slice 1** (PR #6917): `compile/bridge.rs::concrete_type_to_julia_type` の 21 個の `Core(Primitive(_))` アーム
  (全 nullary primitive の手書きテーブル)を、共有ハブ `inference_core::core_type_to_julia_type` への単一委譲へ畳んで
  重複除去。primitive の像は両者バイト同一。委譲は `Primitive(_)` 限定で、abstract/`Any` は従来どおり catch-all で
  `Any` へ widen する(ハブは abstract を固有 `JuliaType` に写すため、委譲すると挙動変化になる)。不変条件を
  `concrete_type_to_julia_type_routes_primitives_through_core_hub_issue_6916` で pin。
- **Slice 2**: `impl From<&CorePrimitive> for ValueType`(`CorePrimitive → ValueType` の単一ソース、`CorePrimitive`
  全網羅で compiler が完全性を保証)を新設し、`From<&LatticeType> for ValueType` 逆ブリッジの 21 個のネストした
  primitive アームを単一の `Core(Primitive(p)) => ValueType::from(p)` へ collapse。abstract/`Any`/catch-all `Core(_)`
  アームとは disjoint なので挙動不変。`value_type_from_core_primitive_is_reverse_bridge_source_of_truth_issue_6916`
  で pin。
- 各スライスとも full `--release` 3866/3866 + AoT gate green、clippy 0 warnings。

### `ValueType` 降格エピック継続 Slice 3: `CoreType → ConcreteType` primitive 恒等 fold ✅ (Issue #6919)

- #6916 を「変換面削減スライス着地で区切り → CLOSE」した後の継続エピック #6919。`From<&CoreType> for ConcreteType`
  (`compile/lattice/types.rs`、#6599 Phase 3 のダウンエッジ)に残っていた 21 個の `CorePrimitive` アームは、全て
  `CorePrimitive::X => ConcreteType::Core(CoreType::Primitive(CorePrimitive::X))` の**純粋な恒等写像**だった。これを
  単一の束縛アーム `CoreType::Primitive(primitive) => ConcreteType::Core(CoreType::Primitive(primitive.clone()))` へ
  collapse。アップエッジ `From<&ConcreteType> for CoreType` が既に `ConcreteType::Core(c) => c.clone()`(#6720 Slice 2)
  で行っている fold の対称版で、挙動不変。
- 全 21 `CorePrimitive` を網羅する pin テスト
  `coretype_to_concretetype_maps_every_primitive_to_identity_issue_6919` を追加(collapse 前 green を確認 → collapse 後も green)。
  併せて impl doc の stale 記述(「no callers yet」)を、`julia_type_to_concrete_type_lossy`(#6599 Phase 3 Slice B)が
  この edge を経由している現状へ更新。
- full `--release` 3868/3868 + AoT gate(nextest `--features aot` 3808/3808 + clippy `--features aot`)green、
  clippy `--all-targets -D warnings` 0 warnings、fmt clean。
- 残: 本体(`ValueType::` 4539 uses の薄ビュー化、特大/高リスク)は引き続き多 PR スライス前提。

### `ValueType` 降格エピック継続 Slice 4: 重複 `ArrayElementType → ValueType` テーブルの単一化 ✅ (Issue #6919)

- `vm/specialize/expr.rs::value_type_from_array_element_type`(関連関数、2 caller)が、canonical な
  `ArrayElementType::to_value_type`(`vm/value/array_element.rs`)と **全 26 variant でバイト同一の重複テーブル**だった。
  重複関数を削除し、2 caller(`vm/specialize/expr.rs` / `stmt.rs`)を `elem_ty.to_value_type()` 直接呼び出しへ置換。
  `ArrayElementType → ValueType` の変換面を 1 ソースへ集約(挙動不変)。
- canonical 側を固定する pin テスト `to_value_type_maps_every_variant_issue_6919`(全 26 variant)を追加。
- full `--release` 3869/3869 + AoT gate green、clippy `--all-targets -D warnings` 0 warnings、fmt clean。

### ConcreteType 表現削減 Slice 2: nullary fold 完了 + container carrier 確定 ✅ (Issue #6720)

- epic #5916 §4 Phase 6。`ConcreteType` を `Core(CoreType)` + lattice-only carrier の薄い wrapper へ寄せる
  representation flip の **nullary 半分を完了**(全 primitive/abstract/`Any` を `Core(CoreType)` へ畳んだ)。
- 着地 PR: #6900(behaviour-preserving smart constructor 群 + #6817 Array rank ハザード発見)、
  #6901(abstract `Number`/`Integer`/`AbstractFloat`/`IO`→`Core`、9 ファイル)、
  #6902(21 primitive + `Any`→`Core`、61 ファイル)。各 full `--release` 3846/3846 + AoT 3786/3786 green。
- 手法: 一括 flip(60+ ファイル)は import 整合がカスケードして revert → **ファイル数で小バッチ化**し green 増分で着地。
  `ConcreteType::X`→`ConcreteType::Core(CoreType::Primitive(CorePrimitive::X))`(value/pattern 両方で valid、
  `&ConcreteType` パターンも通る; assoc-const は `&` パターン不可)。import は scope 別配置(lib=file-top /
  test 専用=`#[cfg(test)]` or `mod tests` 内)、`cargo fix --lib` は test 専用 import を誤削除するため不使用。
- container 系(`Array`/`Tuple`/`Dict`/`Set`/`Range`/`Generator`/`NamedTuple`/`UnionOf`)は **carrier 維持**で確定
  (子に `ConcreteType` を持ち lattice が再帰操作 → Core 化は per-access 変換で深い改修・利益薄。`Array` は #6817 rank)。
  doc 終点像「`CoreType` + lattice-only carrier の薄い wrapper」を実質達成し flip 実用完了。残(`Struct`/`DataType`/
  `Module`/`Named`-nullary の Core 化)は利益薄で deferred。詳細 `docs/vm/CONCRETETYPE_RETIREMENT.md` §1/§2.2/§5。
- 副次: type-stability レポートの `format_lattice_type` が ConcreteType の Debug を表示に使い `Core(Primitive(Int64))`
  をリークしていたのを `to_type_name` 経由へ修正(#6902、`type_stability_uses_global_types_for_const_reader`)。

## 最新対応 (2026-06-18)

### `Int(::BigFloat)` / `floor(Int, ::BigFloat)` 整数変換 ✅ (Issue #6890)

- `Int(big(2.0))` / `Int64(big(3.0))` と型付き丸め `floor(Int, x)` / `round(Int, x)` / `ceil(Int, x)` /
  `trunc(Int, x)`(= `T(round(x))`、`base/floatfuncs.jl`)が `Type error: Cannot convert BigFloat(...) to Int64` で
  失敗していた。#6801 で `floor(big(2.7))` 等が BigFloat を返せるようになった後の残ギャップ(BigFloat → Integer 変換)。
- 原因: 整数コンストラクタ `convert_to_iNN`/`convert_to_uNN`(`vm/type_ops/conversion.rs`)は `F64`/`BigInt`/`Rational`
  は扱うが `Value::BigFloat` アームが無く、catch-all の TypeError に落ちていた。
- 対応: (1) `RustBigFloat::to_bigint_exact()`(`vm/value/mod.rs`)を追加。非有限は `None`、整数値判定は `trunc(x) == x`
  (astro_float `int()` + `cmp`)、整数なら astro_float の十進 `Display`(正規化 `[-]D[.F…]e[+-]N`)を桁・指数から
  `num_bigint::BigInt` へ厳密復元(`bigfloat_decimal_to_bigint`)。(2) 全 10 変換(i8/i16/i32/i64/i128, u8/u16/u32/u64/u128)
  に `Value::BigFloat` アームを追加し、共有ヘルパ `bigfloat_to_exact_bigint` で BigInt 化 → `ToPrimitive` で各幅へ
  範囲チェック。非整数値・非有限・範囲外・符号不一致は `InexactError`(本家 `(::Type{<:Integer})(::BigFloat)` 準拠)。
- 結果: `Int(big(2.0))==2`、`floor(Int,big(2.7))==2`、`round(Int,big(2.5))==2`(ties-to-even)等が本家 1.12.6 一致。
  Float64 の正確整数範囲を超える `Int128(big(2.0)^70)` / `Int(big"123456789012345678")` も正確(f64 経由しない)。
  Rust のみで base cache 再生成不要。fixture `bigfloat/bigfloat_int_conversion_6890.jl`(parity OK)。

### 汎用 `div`/`divrem` を trunc 化 + 動的 Float `%` を truncated rem 化 ✅ (Issue #6891 / #6895)

- 汎用 `div(x, y)`(`base/math.jl`)が `floor(x / y)`(−∞ 方向)だったが本家 `div` は 0 方向(RoundToZero)。異符号の
  Float64 / BigFloat で `div(-7.0, 3.0)` が −3.0(本家 −2.0)。`trunc(x / y)` に修正。Int 経路(typed sdiv)は元から正しく、
  `fld`/`cld` は本来 floor/ceil なので不変。
- 連動して `divrem` の rem 部も誤り(#6895): 動的 Float `%`(`vm/exec/binary_both.rs` の `Intrinsic::SremInt` fallback ×2)が
  `a - floor(a/b)*b` = mod を計算していた。`%`/`rem` は truncated remainder(被除数の符号)なので `a - trunc(a/b)*b` に修正。
  typed/特殊化経路と BigFloat(#6796 `RemBigFloat`)は元から trunc で、plain-float の動的 fallback だけ floor のまま取り残されていた。
  `mod` は `base/math.jl` で `%` から符号調整して導出するため、`%` が真の剰余を返せば従来どおり正しい(本家一致を確認)。
- 結果: `divrem(-7.0, 3.0) == (-2.0, -1.0)`(Float64 / BigFloat とも)、`rem`/`%` の動的経路も本家 1.12.6 一致。
  `div` は math.jl 変更のため base cache 再生成。`%` 修正は Rust のみ。
- fixture: `math/div_trunc_negative_6891.jl`(新規、@testset + gating tail。harness は最終値しか見ないため bare `true` だと
  wrong-value 回帰がマスクされる → AND ゲートで実ゲート化)。既存 `math/divrem_fldmod.jl` の stale 誤期待値
  (`divrem(-7,3)`/`fld1`/`fldmod1`、typed メソッドで実際は正しかったが期待値が古かった)を本家一致へ修正し gating tail を追加。両 parity OK。

### tuple `==` の BigFloat 要素比較 ✅ (Issue #6892)

- `(big(2.0), big(1.0)) == (2.0, 1.0)` / `(big(2.0),) == (2.0,)` / `(big(2.0), big(1.0)) == (2, 1)` が、要素ごとの
  `==` は `true`(スカラ `big(2.0) == 2.0` も `true`)なのに tuple `==` だけ `false` を返していた。#6801 で `divrem` が
  tuple を返すようになった際に判明。
- 原因: tuple/named-tuple の `==` は Rust の `TupleEquals` builtin(`values_equal_tristate`)で要素比較を畳み込む。
  BigFloat を含む数値ペアにアームが無く、最後の `_ => values_isequal(...)` → `Debug` 文字列比較フォールバックに落ち、
  `BigFloat("2.0e+0", ...)` と `2.0` で表現差により不一致になっていた(スカラ `==` は `pop_bigfloat` で両者を BigFloat へ
  昇格してから比較するので一致)。
- 対応: `value_to_bigfloat`(`StackOps::pop_bigfloat` と同じ昇格規則: F16/F32/F64・全整数幅・Bool・BigInt → BigFloat)を
  `builtins_equality.rs` に追加し、`values_equal_tristate` に「どちらかが BigFloat」のガードアームを `_` の前に挿入。
  両者を BigFloat へ揃え `cmp` で値比較する。BigFloat 同士・BigFloat↔Float64/Int・混在/ネスト tuple・#6801 の `divrem`
  結果(`divrem(big(7.0),big(3.0)) == (2.0,1.0)`)で本家 1.12.6 一致、不一致ケースは `false` のまま。Rust のみで
  base cache 再生成不要。
- fixture `bigfloat/bigfloat_tuple_eq_6892.jl`(parity OK)。

### BigFloat の `floor`/`ceil`/`round`/`trunc` と `div`/`fld`/`cld`/`divrem`/`fldmod` ✅ (Issue #6801)

- `floor(big(2.7))` 等が `Type error: expected numeric value, got BigFloat(...)` で未対応だった。丸め経路が
  値を f64 へ変換していて(`value_to_f64` が BigFloat を弾く)、`div`/`fld`/`cld`(= `floor`/`ceil`(x/y))と
  `divrem`/`fldmod` も連鎖的に失敗していた。
- 対応: `RustBigFloat` に astro_float ネイティブ丸め(`floor`/`ceil`/`int`(=trunc, 0方向)/`round(0,ToEven)`)を
  追加し、共有ヘルパ `apply_unary_rounding_op_with_heap`(BigFloat を先に分岐、他型は従来 f64 経路で F16/F32 幅も保持)で
  全ての丸め実行点に配線:`Instr::FloorF64`/`CeilF64`、`BuiltinId::Round`/`Trunc`、`*Llvm` intrinsic 群
  (`floor_llvm`/`ceil_llvm`/`trunc_llvm`/`rint_llvm`)、動的 `CallDynamicOrBuiltin` 経路。
- 結果: 任意精度を保持(24 桁の値の `floor` も正確)、`round` は ties-to-even、`trunc` は 0 方向。`div`/`fld`/`cld`/
  `divrem`/`fldmod` は `floor`/`ceil`(x/y) から導出。Rust のみの変更で base cache 再生成不要。本家 1.12.6 一致。
  fixture `bigfloat/bigfloat_rounding_div_6801.jl`(parity OK 36 assert)。
- スコープ外(別 issue 起票): `floor(Int, ::BigFloat)` は `Int(::BigFloat)` 変換が未対応(#6890)、
  汎用 `div`/`divrem` が負値で floor(本来 trunc)になる既存バグ(#6891)、tuple `==` の BigFloat 要素比較(#6892)。

### `Value::NativeArray` → `Value::ExprArgs` 改名 + 封じ込め監査(accept & confine) ✅ (Issue #6807)

- #6807(native carrier 撤去キャンペーン)の最終方針。メンテナ判断で **option 1(accept & confine)** 採用。
- 背景: 本セッションの producer flip 群(compile-time/slice/HOF value-mode 等)後、plain 配列プログラムは native carrier
  を一切生成せず(100% wrapper 化)、FFI 境界も済。残る唯一の origin は `expr.args`(可変 `Vector{Any}` AST 引数)。
  `struct_heap` に per-value GC が無く Expr ノード毎に大量生成されるため、heap StructRef 化は struct_heap スロットの
  無限リーク。Rc carrier(`Value::NativeArray`)は drop 時 auto-free で transient args に**正しい表現** → 削除ではなく
  封じ込めが正解と結論。
- 実装: (1) variant `Value::NativeArray(ArrayRef)` を **`Value::ExprArgs(ArrayRef)`** に改名(役割を型名に反映、
  doc コメント追加)。`native_array_*` converter helper は汎用 carrier アクセサとして名称維持。
  (2) `scripts/check_value_array_allowlist.sh` Policy 2 を「zero へ ratchet」→ **恒久封じ込め allowlist**
  (`EXPR_ARGS_ALLOWLIST`、3 ファイル)へ。acceptance = 「`expr.args` 以外で carrier 無し」。
  (3) `CODE_AUDITS.md` / `ARRAY_MEMORY_MIGRATION.md` 更新。
- 検証: フル 3845/3845(改名は挙動中立)、AoT 3782/3782、clippy/fmt クリーン、carrier 監査 pass(3 ファイル封じ込め)。
- **完了(confined)**: これをもって #6807 はクローズ。移行キャンペーンの実用目標(no-JIT ランタイムを
  一般配列で MemoryRef-backed `Array{T,N}` ラッパーに統一)は達成。`Value::ExprArgs` carrier は `expr.args` に
  限定保持され、その auto-free な Rc 意味論はこの用途に正しい。完全な変種削除は `struct_heap` GC 実装を前提とする
  大規模な別作業(option 2)として保留。将来 GC 導入時に reopen / repurpose する。
- 後続クリーンアップ(#6889): #6807 由来の表示修正 #6882 で PR #6884 が既マージの #6883 と二重対応していたため、
  #6884 の footprint を revert し #6883 に一本化(到達不能デッドコード・不要再エクスポート・重複フィクスチャ/テスト/docs を削除)。

### 直積 `for x in xs, y in ys ... end` 複合形のネストループ脱糖 ✅ (Issue #6865)

- 複数イテレータをカンマ区切りで回す直積 `for`(`for x in xs, y in ys ... end`)が
  lowering で `UnsupportedForBinding` になり実行できなかった。
- 原因: `lowering/stmt/control_for.rs` が `for` ヘッドの `ForBinding` が複数あると
  一律 `UnsupportedForBinding` を返していた(`bindings.len() != 1`)。
- 修正: upstream Julia の `expand-for` に倣い、複数バインディングを**外側から内側へ
  ネストした `for` 文に脱糖**(最初の binding が最外ループ、最後の binding が最内ループ、
  本体は最内に配置)。各バインディングを個別に lower し、内側ループを外側ループの本体に
  詰める fold で構築するため、内側イテレータが外側の変数を参照する形(`for i in 1:3, j in 1:i`)も
  自然に動作する。Range / Iterable / TupleIterable・float ステップなど既存のバインディング種別を
  そのまま再利用。
- 内包表記側(`[f(x,y) for x in xs, y in ys]`)は既に `MultiComprehension` で多重バインディング
  対応済みのため変更不要。
- 効果: `for` 文・内包表記の両形が upstream Julia 1.12 と一致。
- テスト: フィクスチャ `control_flow/cartesian_for_6865.jl`(6 assert、2/3 イテレータ・
  内側が外側変数参照・配列×範囲混在・タプル分解・float ステップ内側範囲)、lowering 単体テスト
  `test_cartesian_for_desugars_to_nested_loops_issue_6865` / `test_cartesian_for_mixed_iterables_issue_6865`。

### パラメトリック制約メソッド `f(x::T) where T<:Real` の直接呼び出し特殊化 ✅ (Issue #6868)

- `f(x::T) where T<:Real` のようなパラメトリック制約メソッドが、型注釈なし generic 版や
  具象 `::Float64` 版より**遅い**問題。具象の約3.4倍、generic の約2.2倍(Issue 計測)。
- 原因: 直接呼び出し経路 `execute_direct_call_with_func_args` が、where-メソッド
  (`type_params` 非空)に対して**未特殊化の generic 本体**(`func.entry`、全パラメータが
  `Any` 束縛で内部の `==`/`*`/`sin`/`/` が動的ディスパッチ)へジャンプするだけだった。
  where-メソッドは型付き直接呼び出し fast path(`execute_direct_call_fast` が
  `!type_params.is_empty()` で bail)からも、`CallSpecialize`(`needs_specialization` が
  untyped param を要求)からも除外されており、specialization が一切効いていなかった。
- 修正: 直接呼び出し経路で where-メソッドを**実引数の具象型で特殊化**(既存の
  `try_specialized_entry_for_runtime_call` を再利用、`(spec_idx, arg_types)` でキャッシュ)し、
  特殊化済みエントリへ入る。メソッドの bound(`T<:Real` 等)は CallResolved を発行した
  静的ディスパッチで検証済みのため健全で、型変数 `T` は `bind_type_params` でフレームに
  束縛済み(`zero(T)`/`one(T)`/`Vector{T}` 本体も正しく動作)。
- 効果: where-メソッドが具象 `::Float64` とほぼ同等(~1.05-1.08倍)に。upstream Julia 1.12 と
  値・型・MethodError 挙動すべて一致。
- テスト: フィクスチャ `where/specialized_direct_call_6868.jl`(14 assert、値/型/制約/`T` 参照を
  検証)、ベンチ `vm_where_specialization_benchmark`(driver `benchmarks/vm_where_specialization.jl`)。

### 配列の逐次成長 (内包表記 / `push!`) の O(n²) → O(n) 修正 ✅ (Issue #6873, via #6846)

- iOS の surface plot `surface(x, y, (x,y) -> sinc(norm([x,y])))`(100×100)が ~1.6s かかる件
  (#6846)を系統的にプロファイル: 直列化 ~1.16ms、反復 ~9ms に対し **内包表記の配列構築だけで ~1.2s**。
  VM 外ではなく **VM 内の O(n²) 配列成長**が真因と特定。
- **原因**: 内包表記・`push!` はともに結果配列を Memory-backed `Array{T}` ラッパとして確保し、毎要素の
  `ArrayPush` → `push_array_wrapper` が `MemoryValue::undef_typed(new_len)` で**ぴったりサイズの Memory を
  毎回作り直し、全 len 要素をコピー**(+1 成長)していた。1 push が O(n)、n 要素で O(n²)。事前確保
  (`ReserveArray`, Issue #5186)は `Value::Memory` 限定で、配列の wrapper 化(#6649/#6807)後は no-op。
- **修正**: `push_array_wrapper` の `Value::Memory`/`Value::MemoryRef` 分岐に in-place 高速パスを追加。
  ラッパが親 Memory を先頭から論理長ぴったり連続所有する場合(`offset==1`/`memref.offset==0` かつ
  `mem.len()==len`)、親 Memory の償却 `push()`(Vec 幾何成長で容量を尊重)で **in-place 追記**し、
  size フィールドのみ更新。前方オフセット付きビュー等は安全のため従来の realloc にフォールバック。
- **効果**: 10000 要素の内包表記/`push!` = **1213ms → ~8ms(~140倍)**、スケーリングが O(n²)→O(n)。
  surface カーネルは ~1.4s → ~0.17s(#6846 の「後2倍」要望を超過達成)。値・型・順序・`c = d`
  エイリアス挙動は upstream Julia 1.12 と一致。
- **テスト**: 単体 `array_push_grows_wrapper_in_place_amortized`(realloc-per-push なら fail する Rc
  同一性アサート)、parity フィクスチャ `arrays/growth_amortized_6873.jl`(37 assert)、ベンチ
  `vm_array_benchmark::growth_comprehension_push_2048`。
- **既知の残件**: typejoin 版 `push_array_wrapper_typejoin`(Any 体の runtime-typejoin 内包表記)は今回の
  対象外で依然 realloc(別途検討)。

### Complex×Real 混在配列リテラルが `Vector{Any}` になる問題の修正 ✅ (Issue #6867)

- `[1.0 + 0.0im, 2.0]` のような **Complex と Real を混在**させた型注釈なし配列リテラルが、
  upstream の `Vector{ComplexF64}` に対し sjulia では `Vector{Any}` になっていた。結果、
  `Complex{Float64}` 特殊化メソッド(`norm` 等)にディスパッチされず、generic fallback の
  `xi*xi`(Complex を Float64 アキュムレータに加算)で実行時型エラーになっていた。
- 原因: コンパイル時の配列要素型 narrowing (`infer_array_element_type`) に **Complex×Real の
  ルールが無く**、`all_complex_scalar` でも `all_numeric` でもないため `Any` にフォールバック
  していた。同種 Complex 要素 (`[1.0+2.0im, 3.0+4.0im]`) は #6851 で既に修正済みで、本件は
  混在ケースの残ギャップ。
- 修正: `infer_mixed_complex_real_element_type` を追加し、各要素型を Julia の `promote_type` /
  Complex `promote_rule`(`promote_type(Complex{Float64}, Float64) == ComplexF64`)で畳み込み。
  結果が `Complex{Float64}`/`Complex{Float32}` ならインライン Complex ストレージへ、整数 Complex なら
  struct-backed パスへルーティング。emit 側は `compile_complex_array_element` で実数要素を
  `Complex{T}(x, 0)` コンストラクタで widen する。`[1.0+0.0im, 2]`, `[1+0im, 2.0]`,
  `[1.0+0.0im, 2.0, 3]` が `Vector{ComplexF64}`、`norm([1.0+0.0im, 2.0]) == 2.23606797749979` を
  fixture 化(`complex_real_mixed_array_literal_6867`)。
- 既知の残課題: `[1+0im, 2]` → upstream `Vector{Complex{Int64}}` に対し sjulia は依然 `Vector{Any}`。
  これは `1+0im`(整数 Complex リテラル)のコンパイル時 ValueType が `Any` で畳まれない別ギャップ
  (`[1+0im, 2+0im]` も同様に `Vector{Any}`)で、本件のスコープ外。

### `sinc(norm([x,y]))` サーフェスプロット・カーネルの高速化 (norm/sinc) ✅ (Issue #6846)

- iOS のサーフェスプロット `surface(x, y, (x,y) -> sinc(norm([x,y])))`(100×100 グリッド)が遅い件の
  追加対応。PR #6849(配列リテラルのネイティブ確保)後にプロファイルし直したところ、残りコストは
  **norm ~48% / 確保 ~26% / sinc ~26% / ループ ~3%** で、最大成分が `norm` に移っていた。
- **norm**: `LinearAlgebra.jl` の全 `norm` メソッド(Float64/Int64/Complex{Float64}/generic)の内側ループを
  インデックス走査(`for i in 1:n; xi = x[i]`)から**直接イテレーション**(`for xi in x`)へ変更。
  要素ごとの境界チェック付き `getindex` と `1:n` レンジ生成が消え、VM 上で **norm ~2.2倍速**
  (0.423s → 0.195s, 10000 点)。Issue #6846 のユーザー要望「内部ループの index 命令削減」。
- **sinc**: 具象型 `sinc(x::Float64)` のメソッドを追加。型注釈なし generic 版は引数型が不明なため
  `==`/`*`/`sin`/`/` を動的ディスパッチしていたが、具象型注釈で VM が静的特殊化し **~1.5倍速**
  (0.244s → 0.160s)。パラメトリック `sinc(x::T) where T<:Real` は逆に generic より遅かったため
  (VM のパラメトリック・ディスパッチ overhead, → #6868 として起票)具象型を採用。
- 合計でフルカーネル `sinc(norm([x,y]))` は **1.055s → 0.603s(−43%, ~1.75倍速)**。値は upstream
  Julia 1.12 と一致(224.198001738587…, 1万回累積で末尾 ~1 ULP 差)。
- フィクスチャ `linalg/norm_iter_sinc_6846.jl`(norm 全 p 分岐 × Float64/Int/Complex/generic + sinc 速経路)
  を追加し upstream parity を固定。プロファイル中に発見した別件を Issue 化:
  #6865(直積 `for x in xs, y in ys` 未対応)、#6866(float `range` 反復が整数比 ~10倍遅い)、
  #6867(Complex×Real 混在リテラルが `Vector{Any}`、bug)、#6868(パラメトリック制約メソッドが遅い)。
- **追補**: 同じ具象型特殊化テクニックを cos 系の型注釈なし generic ラッパーにも適用。
  `cosc(x::Float64)`(~1.4倍, sinc の導関数で対)、`sincos(x::Float64)`(~2.3倍, 2 transcendental +
  タプル生成が特殊化)、`tanpi(x::Float64)`(~1.1倍, `isinteger`/`copysign` 主体で控えめ)を `math.jl`
  に追加。本体は generic と同一で挙動不変、既存 fixture(`math/sinc_cosc.jl`, `math/sincos_basic.jl`,
  `operators/tanpi_test.jl`)で parity 担保。cospi/cosd 等の薄いラッパーは特殊化対象が少なく ~1.1倍
  止まりのため見送り。

### キーワード引数の配列/タプル/内包表記リテラルのデフォルトが `0` になるバグ修正 ✅ (Issue #6876)

- #6807(`Value::NativeArray` carrier 撤去)の compile-time injector 調査中に発見・起票したバグ。
- 症状: 省略可能キーワード引数のデフォルトが**配列リテラル**(`f(; x=[1,2])`)、**タプルリテラル**
  (`f(; t=(1,2))`)、**型付き空配列**(`f(; v=Float64[])`)、**内包表記**(`f(; c=[i for i in 1:3])`)の
  いずれの場合も、キーワード省略時に `0`(`Int64`)に束縛され、本家の値(`[1, 2]` / `(1, 2)` / …)と乖離。
  `push!` するとさらに `ArrayPush: expected Array or Set, got "Int64"` を送出。スカラ/文字列リテラルや
  関数呼び出しデフォルト(`zeros(2)`)は正常だった。
- 原因(2点の結合):
  1. `compile/utils.rs::eval_literal_default`(非 body-eval デフォルトの事前評価 fast path)は畳み込み済みの
     `Expr::Literal(Literal::Array|…)` のみ対応。ソースの `[1,2]` は `Expr::ArrayLiteral`、`(1,2)` は
     `Expr::TupleLiteral` としてパースされるため `_ => Value::I64(0)` フォールバックに落ちていた。
  2. `lowering/function/kw_defaults.rs::default_needs_body_eval` は配列/タプルリテラルを「要素に call を含む時のみ」
     body 再評価へ回しており、全要素が定数のリテラルは壊れた fast path に留まっていた。
- 修正: `Expr::ArrayLiteral` / `Expr::TupleLiteral` / `Expr::TypedEmptyArray` / `Expr::Comprehension` /
  `Expr::MultiComprehension` のデフォルトを**無条件で per-call body 再評価**へ。これらは事前評価 path で
  materialize 不能であり、本家の「省略時は毎回フレッシュな配列を生成する」per-call セマンティクスとも一致する
  (`push!` するデフォルトが呼び出し間で漏れない)。`Literal::*` 畳み込み済みデフォルトの fast path は不変。
- 検証: フィクスチャ `kwargs/kwarg_literal_default_6876.jl`(15 アサーション、upstream Julia 1.12.6 とパリティ一致)。
  フルスイート 3842/3842、AoT 3782/3782、clippy/fmt クリーン。
- #6807 への寄与: 配列リテラルデフォルトが実行時 VM コンテキストで materialize されるようになり、コンパイル時の
  ネイティブ配列キャリア注入(`compile/utils.rs::literal_array_value` → `native_array_value_from_array`)へ
  到達しなくなる(carrier 撤去キャンペーンの compile-time injector を縮退)。

### `Value::NativeArray` carrier 撤去: HOF value-mode result producer を wrapper 化 ✅ (Issue #6807, campaign 進行中)

- #6882(表示修正)で解錠された最大の fresh-build root injector(実測 sweep で ~307 発火)を flip。
  `hof_exec/value_mode.rs::create_typed_array_from_values` が native carrier ではなく `Array{T,N}` wrapper を
  返す(`array_value_to_wrapper`)。同時に **ネスト配列ラッパー要素を native carrier に materialize していた
  line 855 の #5229 対策ループを撤去** — #6882 で `value_show_type` がラッパー要素の typeinfo を正しく扱うため、
  ネスト `map(x->map(...),v)` 結果は wrapper-of-wrapper のまま `[[10,20],...]` と bare 表示され、indexing/mutation も正常。
- 検証: フィクスチャ `hof/value_mode_nested_wrapper_result_6807.jl`(11 アサーション、1.12.6 パリティ)。
  フル 3845/3845、AoT 3782/3782、clippy/fmt クリーン。`eltype` が `Any`(本家 `Vector{Int64}`)は本変更前から
  の既存の推論幅差で本変更非起因。
- 残: `value_mode.rs` の他 `array_value` サイト(empty 結果・`wrap_array_result==false` 経路・`FindAll` empty Int64)、
  `expr.args` 表現(`ExprContainer.args: ArrayRef`)、FFI `normalize_host_return_value` 境界。

### 配列ラッパー要素の Vector が誤った `Array{T,N}[...]` typeinfo プレフィックスを出すバグ修正 ✅ (Issue #6882)

- #6807 の HOF value-mode injector flip 調査中に発見・起票したバグ。型付き `T[...]`/`T[]` 由来の配列ラッパーを
  要素に持つ `Vector` が `Array{Int64, 1}[[1], [2]]` と表示(本家 `[[1], [2]]`)。素のリテラル要素は正常。
- 原因: `vm/formatting.rs::value_show_type`(上流 `typeinfo_implicit` 相当)のネスト配列アームが
  ネイティブキャリア専用で、inline `Array{T,N}` ラッパー `Value::Struct` 要素が汎用 struct アームに落ち
  `("Array{Int64, 1}", false)` を返し外側に非 implicit プレフィックスを出していた。
- 修正: `value_show_type` の `Value::Struct` アームで `array_wrapper_julia_type().is_some()` のラッパーを検出し、
  自己完結な Memory ストレージ(`array_wrapper_value_to_array_value(v, &[])`)から要素型/implicit を算出して
  ネイティブアームと同一表示に。フィクスチャ `arrays/nested_array_wrapper_typeinfo_prefix_6882.jl`(8 assert、1.12.6 パリティ)。
  フル 3843/3843、AoT 3782/3782、clippy/fmt クリーン。
- #6807 への寄与: HOF value-mode producer の flip を阻んでいた表示問題を解消。残課題=非 implicit 内側型のネスト
  typeinfo 伝播(`Int8[1]`→`[1]`、別件)。

### `Value::NativeArray` carrier 撤去: 実測 injector マップ + compile-time/slice producer flip ✅ (Issue #6807, campaign 進行中)

- B方針確定 + 実測 live-injector マップ取得 + producer flip 2件。詳細は `docs/vm/ARRAY_MEMORY_MIGRATION.md`
  「#6807 — variant removal」節。
- **B1(全 producer を heap-StructRef 化)が上流忠実な正解**と確定: `push!(a::Array,item)` は
  `a._mem=mem; a._size=(new_len,)` と `a` のフィールドを再代入する(base/array.jl)ため、ラッパーは参照セマンティクスの
  heap `StructRef` 必須(上流の可変 `jl_array_t` と一致)。B2(共有 Memory から長さ導出)は authoritative な `size`
  フィールドから乖離するため却下。
- **実測マップ**: 2 つの converter helper を `#[track_caller]` で instrument し 2533 フィクスチャを `sjulia` で sweep。
  native carrier はレガシーでなくホットパスで現役と判明。root injector(fresh build)= `hof_exec/value_mode.rs`
  (HOF value-mode 結果; #5229 のネスト配列 leak 防止で load-bearing)+ `exec/array_index_slice.rs`(slice 結果)。
  再ラップ伝播 = `locals.rs:691`(typed-slot LoadSlotArray)・`container.rs:1565`(`expr.args`=`ArrayRef` 格納)等。
  iteration 行列/`value_enum`/`deep_copy`/test helper はレガシー・テスト専用(実プログラム未発火)。
- **flip 2件(本 PR)**: (1) `compile/utils.rs` の compile-time injector 除去(#6876 で配列リテラル kw デフォルトが
  body 再評価になり `eval_literal_default` の配列アームが dead → `literal_array_value`+import 削除)。(2)
  `exec/array_index_slice.rs` の slice 結果 producer(`a[range]`/`a[idxvec]`/`m[rows,cols]`/n-dim)を
  `array_value_to_wrapper` で wrapper 化。slice は独立可変な fresh 配列(`push!`/`setindex!` が親に漏れない)。
  内部 `range.collect()` 一時(`load_selected_array_elements` が native 読み)は carrier 据え置き。
- フィクスチャ `arrays/slice_producers_wrapper_6807.jl`(18 アサーション、1.12.6 パリティ)。フル 3843/3843、
  AoT 3782/3782、clippy/fmt クリーン。
- 残: HOF value-mode(#5229 load-bearing)・`expr.args` 表現(`ExprContainer.args: ArrayRef`)・FFI host-return 境界
  = いずれも深く multi-session。全 root injector flip 後に再ラップループが dead 化 → ~187 consumer wrapper-arm が
  full suite で証明され variant を機械削除可能。

### iOS/Web/Flutter の `bar` プロットが折れ線で表示される問題を修正 ✅ (Issue #6850)

- iOS アプリで `bar`(および `heatmap`)プロットが折れ線グラフとして描画されていた。原因は VM ではなく
  ホスト同梱の Plotly バンドル: 3 ホスト(iOS `SubsetJuliaVMApp/.../Resources/`, `web/`,
  Flutter `mobile/assets/plotly/`)が `plotly.min.js` の **`gl3d` 部分バンドル**を同梱しており、
  cartesian の `bar`/`heatmap` トレースモジュールを欠いていた。Plotly は未登録トレース型を無言で
  `scatter` にフォールバックするため、`bar` が線になっていた。
- VM 側(`src/plotting/plotly.rs`)は `:bar` を正しく `{"type":"bar",...}` に変換しており
  (`tests/plot_artifact_mime_tests.rs` で確認済み)、コード変更は不要。3 ホストの `plotly.min.js` を
  3D + cartesian 両方を含む**フルバンドル**(`plotly.js v2.35.2`)に差し替えて解消。
- 退行防止 `scripts/check_plotly_bundle.sh`(CI 登録)を追加。各ホスト同梱 `plotly.min.js` が VM の emit
  する全トレース型(`scatter`/`bar`/`heatmap`/`scatter3d`/`surface`)を登録しているか検証する。
  ポリシーは `docs/vm/CODE_AUDITS.md` の「Bundled Plotly.js trace coverage」に記載。

### 虚数算術要素の配列リテラル `[1.0 + 2.0im, ...]` が `Vector{Any}` になる問題を修正 ✅ (Issue #6851)

- 型注釈なし配列リテラル `[1.0 + 2.0im, 3.0 + 4.0im]` の要素型推論が `Any` にフォールバックし
  `Vector{Any}` になっていた。要素 `1.0 + 2.0im`(= `1.0 + 2.0 * im`)の ValueType 推論が
  `*`/`+` Base メソッド(宣言戻り型 `Any`)へのディスパッチで Complex 要素型を落としていたのが原因。
- `infer_expr_type` の `BinaryOp` struct 分岐で、ディスパッチが `Any` を返したときに `infer_julia_type`
  の Complex 昇格結果を `julia_type_to_value_type_with_ctx` で ValueType に変換して回収するよう修正。
  `1.0 + 2.0im` → `ComplexF64`、`1.0f0 + 2.0f0im` → `ComplexF32` に畳まれ、配列リテラルが
  `Vector{ComplexF64}` / `Vector{ComplexF32}` を確保する。コンストラクタ形式 `[Complex(...)]` と
  型付きリテラル `ComplexF64[...]` は元から正しく動作。フィクスチャ
  `complex/complex_imag_arith_array_literal_6851.jl` を追加。フル test suite + clippy 通過。

### 動的呼び出し毎の `FunctionInfo` clone を `Rc<FunctionInfo>` で除去 ✅ (Issue #6853)

- VM の関数呼び出しパスは、`self.functions[idx]` の借用を解放して `&mut self` を取るために、選択した
  `FunctionInfo` 全体を毎回 clone(borrow-checker 駆動)していた。`FunctionInfo` は多数の `Vec`/`String`
  を持つため複数 heap 確保を伴い、`sinc(norm([x,y]))` のような call-heavy ループで steady-state コストの
  ~5〜8% を占めていた。
- `Vm.functions: Vec<FunctionInfo>` → `Vec<Rc<FunctionInfo>>` に変更。clone サイト
  (`get_function_cloned_or_raise` / `start_function_call` / `execute_direct_call_with_func_args`)は
  `Rc` の refcount bump(O(1))に。`CompiledProgram.functions` は `Vec<FunctionInfo>` のままで serde 非影響、
  `Vm` 構築時に `into_iter().map(Rc::new).collect()`。whole-vec を借用するヘルパ
  (`bind_kwargs_defaults`/`bind_kwargs_with_map`/`KwDefaultEvalCtx`/`try_predecode_i64_function` 群)は
  `&[Rc<FunctionInfo>]` にシグネチャ調整、フィールド読み取りは deref で素通り。
- ベンチ `benches/vm_dynamic_dispatch_benchmark.rs` を追加(`sinc(norm([x,y]))` 10000 点、出力
  `335.7282850538752` は upstream Julia と一致)。フル test suite + clippy 通過。

### 型名パースのメモ化で配列ディスパッチを高速化 ✅ (Issue #6846 follow-up)

- #6849 で配列リテラル構築を native 化した後の追加高速化要望(「後2倍速くしてほしい」)。`sample` プロファイルで
  steady-state の支配コストを **型名文字列 `"Array{Float64, 1}"` の再パース** と特定:動的ディスパッチの
  `value_matches_param`→`engine_subtype`/`type_matches`→`CoreType::from_julia_name` が、O(n) の
  `split_trailing_where` 状態機械・`parse_parametric_name`(`parse_named_tuple_type_name` 経由で **2 回**)・
  `parse_core_value_param` の `format!` をディスパッチ毎に走らせていた。
- 修正:
  - `CoreType::from_julia_name` を thread-local `String → CoreType` キャッシュでメモ化
    (`inference_core/type_core.rs`)。`name` の純関数(唯一の外部入力 `native_int_type_name()` はビルド定数)なので
    キャッシュヒットは常に正しい。再帰サブパースもキャッシュ経由。安全弁として 100k エントリ超で clear。
  - `array_wrapper_julia_type` の補助を軽量化(`vm/value/struct_instance.rs`): `is_array_wrapper_name` を
    base 名直接判定(Vec 確保除去)、`array_wrapper_memory_element_type` を `array_element_type_to_julia_type`
    直写像(要素名 `String` + パーサ往復の 2 確保除去)に。
- 効果(10000点 `sinc(norm([x,y]))` カーネル): compute 約 **−27%**(本セッション計測)。L2 ディスパッチキャッシュは
  既に機能(399995/400000 hit)。残る 2× ギャップは配列ラッパ表現の per-literal alloc 数(`[x,y]` 1個につき ~8 heap
  alloc)と動的呼び出し毎の `FunctionInfo` clone であり、前者は #6807/#6723 表現エピック、後者は `Rc<FunctionInfo>`
  化(別 PR 候補)。
- fixture `arrays/type_name_memoization_6846.jl`(16 assert, julia 1.12.6 parity)。発見した pre-existing bug を
  #6851 で起票(`[1.0+2.0im, ...]`→`Vector{Any}` 誤推論、`Complex(...)`/`ComplexF64[...]` 形式は正常)。

## 最新対応 (2026-06-17)

### 配列リテラル `[...]` 構築の per-literal pure-Julia `wrap` 呼び出しを native 化 ✅ (Issue #6846)

- iOS の `surface(x, y, (x, y) -> sinc(norm([x, y])))`(100×100=10000点)が 4 秒かかる perf 劣化。原因は配列
  リテラル `[x, y]` の構築が **リテラルごとに pure-Julia `wrap(::Type{Array}, mem, dims)` を呼ぶ**ことで、
  `wrap`→`_array_wrap_check`→`memoryref`→`_array_construct`→`Array{T,N}(ref,dims)` の ~5 Julia フレームを毎回
  張っていた(配列 wrapper 移行 #6649/#6653 で導入された回帰)。
- 修正: `emit_array_wrapper_from_memory_on_stack`(`compile/expr/mod.rs`)が `NewTuple`+`PushFunction("wrap")`+
  `CallFunctionVariable(3)` でなく native な **`FinalizeArray(shape)`** を emit。`wrap` の第1引数だった
  `PushDataType("Array")` を `emit_array_wrapper_memory_start` と `Vector{T}()`(collection.rs)から除去。
- `finalize_memory_build_buffer`(`vm/exec/array_basic.rs`)は、バッキング `Memory` の `MemoryRef` を **zero-copy で
  そのまま `Array{T,N}` wrapper(`{ref, size}`)に包む**方式に変更(`wrap` = `_array_construct(T, memoryref(m), dims)`
  と同型)。従来は `ArrayValue` 経由で storage を要素ごとに再マテリアライズしており、**ComplexF64(インターリーブ)/
  struct(AoS)** 要素型で length 不整合 → `arr[1]` out-of-bounds を起こしていた(全リテラルをこの経路に通したことで露呈、
  13 fixture が一時 RED)。直接 wrap は全レイアウト(interleaved Complex / AoS struct / boxed Any)で正しく、comprehension
  build buffer(#6807)とも共有。
- 効果(同条件 before/after、10000点カーネル): `sinc(norm([x,y]))` 全体 **0.844s→0.493s(−42%)**、
  `[x,y]` 確保のみ 0.702s→0.356s(−49%)。標準 `vm_array_benchmark` は非回帰(construction −1.0%、他 ±0.5% ノイズ)。
  bench ケース `literal_alloc_2elem_128`(16384 確保)を追加して追跡。
- `array_construction_routing_6649_tests` の `is_native_array_carrier_builder` から `FinalizeArray`/
  `FinalizeArrayTyped` を除外し、`wrap` 呼び出し前提の assert を `FinalizeArray` 前提に更新(#6807 Slice 4 で
  これらは de-variant 済み=wrapper 構築であり、#6846 でリテラルが native finalize を使うため。stale ガードの是正)。
- 正しさ: カーネル結果 `224.198001738587…` は upstream と末尾 ~1ULP の FP 丸め差のみ。full green。

### `const` global の `name[]` 空インデックス読みが `Any[]` になるバグ修正 ✅ (Issue #6839)

- `const LOG = Ref(0); LOG[]` が `getindex(LOG)` でなく空 `Vector{Any}`(`Any[]`)にコンパイルされていた。parser は
  `LOG[]` を `Int64[]` と同じ `TypedExpression`(Identifier + 空 VectorExpression)としてパースし、lowering が
  `Expr::TypedEmptyArray{element_type:"LOG"}` を生成、compiler の `TypedEmptyArray` arm
  (`compile/expr/mod.rs`)が未知名 `LOG` を struct 型でも parametric 型でもないとして `ArrayElementType::Any` に
  フォールバック → 空配列を emit していた。issue 本文の `setindex!` override は赤鯡(無関係)。
- 修正: catch-all で `get_struct_type_id` が None かつ名前が **値バインディング**(`self.locals` /
  `shared_ctx.global_types` / `global_const_structs` / `captured_vars` のいずれか、Var arm と同じ判定)の場合は
  typed-empty-array でなく `getindex(Var(name))` にルート。`getindex(::Ref)` は ref を読み、`getindex(::Type{T})`
  は空 `Vector{T}` を作るので、Ref 値でも型を束ねた変数(`T=Int; T[]`)でも正しく動く。リテラル型名(`Int[]`/
  ユーザ struct)は上の arm と struct-id 分岐で先に捕捉されるため不変。
- write 側(`LOG[] = v`)は元から正しく `IndexStore(0)` にコンパイルされていた(read のみのバグ)。
- fixture `essentials/const_ref_empty_index_6839.jl`(11 assert, julia 1.12 parity): const Ref read/write/関数内
  アクセス + `T=Int; T[]` + リテラル `Int[]`/`Float64[]` 回帰ガード。full green・clippy/fmt clean。

### `Value::NativeArray` carrier 撤去 Slice 9: linalg result producer の wrapper 化 ✅ (Issue #6807)

- 線形代数の結果 producer(`builtins_linalg.rs`)を carrier → wrapper 化。file-local `linalg_array_value`
  free fn(`native_array_value_from_array` の薄ラッパ)を `Vm::linalg_wrapper(&mut self, ArrayValue)` メソッドに
  置換(`array_wrapper_value_from_array_value` で MemoryRef-backed `Array{T,N}` を構築)。各分解は入力行列を
  nalgebra に取り込んでから結果を生成するため producer サイトで `self` は自由。`lu`/`inv`/`\`/`svd`/`qr`/
  `eigen`/`eigvals`/`cholesky` の 19 サイトが wrapper を返す。
- consumer 側(`with_linalg_array`/`linalg_value_to_array_value`)は既に `linalg_array_wrapper_value` 経由で
  wrapper を受理済みなので新規 consumer 修正は不要。blast radius ゼロ: full **3842/3842**・AoT green・
  clippy/fmt clean・allowlist 5 files 不変。bench なし(linalg は全 `vm_array_benchmark` path 外)。
- fixture `linalg/decomposition_wrapper_producers_6807.jl`(23 assert, julia 1.12 parity): 各分解の出力 wrapper を
  downstream(indexing/size/matmul/equality)で再利用して回帰を検出。残り carrier: deep_copy(再帰)、formatting
  (FFI境界)、`Mark*`/`Reshape`(override/shared_parent 保持の copy-free ctor 要)、hot な binary_both/array_index*。

### `Value::NativeArray` carrier 撤去 Slice 8: scattered non-hot producer の wrapper 化 ✅ (Issue #6807)

- 非hot散在 producer を wrapper 化: `builtins_io.rs`(file readers)、`builtins_macro/mod.rs`(eval)、
  `builtins_reflection/mod.rs`(return_types/methods)。未使用 alias/import も削除。後回し: linalg(tuple free-fn)、
  deep_copy(再帰)、formatting(FFI境界)。
- blast radius ゼロ: full **3842/3842**・AoT green・clippy/fmt clean・allowlist 不変、新規 consumer 修正なし。

### `Value::NativeArray` carrier 撤去 Slice 7: native zeros/ones/undef constructor の wrapper 化 ✅ (Issue #6807)

- `builtins_arrays.rs` の純粋コンストラクタ 10 サイト(`zeros*`/`ones*`/`AllocUndef{F64,I64,Bool,Any}`)を
  wrapper 化(`push_array_value_as_wrapper`)。`Mark{BitVector,BitArray}`(array_type_override+BitPackedBool)と
  `Reshape`(shared_parent)は carrier のまま(copy-free path が unpack/detach するため)。
- blast radius ゼロ: full **3842/3842** 新規 consumer 修正なし(Slice 5 `length` fallback + Base ロード)。
  AoT green・clippy/fmt clean・allowlist 不変。bench: 構築 copy-free baseline 比 ~+0.3-0.6%(大半ノイズ・許容内)、他 neutral。

### `Value::NativeArray` carrier 撤去 Slice 6: wrapper 構築の copy-free 化 ✅ (Issue #6807)

- `array_wrapper_value_from_array_value`(全 wrapper producer の変換ハブ)の O(n) 要素コピーを、単純配列では
  `ArrayData` を move する fast path に置換。move するのは `undef_typed(element_type)` が同 storage variant を
  選ぶ場合のみ(override/array_type_override/shared_parent 無し・`raw_len==element_count`・primitive backing)で
  BitPackedBool/StructRefs/Any 系は従来コピー → byte-identical。
- full **3842/3842**・AoT green・clippy/fmt clean・allowlist 不変。bench: `construction_undef_zeros_128` −0.53%、他 neutral。
  後続の native constructor(zeros/ones/undef)flip を構築 bench 退行なしで可能にする前提整備。

### `Value::NativeArray` carrier 撤去 Slice 5: array constructor producer wrapper 化 第1バッチ ✅ (Issue #6807)

- VM producer 第1バッチ(range 実体化 `MakeRange`/`MakeRangeF64`、RNG 配列 `Rand*Array`、行列演算結果)を
  native carrier → `Array{T,N}` wrapper へ(`push_array_value_as_wrapper` を `pub(crate)` 化)。hot dispatch
  経路外の fresh constructor 結果で、既に wrapper の `zeros`/`collect`(#6653)と同類なので最初に選択。
- consumer-readiness: native `length` builtin に **dispatch-miss 時のみ** wrapper 要素数を native に数える
  fallback を追加(ユーザ override は dispatch 優先で温存、Base ロード済みは挙動不変)。
- full **3842/3842**・AoT green・clippy/fmt clean・allowlist 5 files 不変・bench neutral。fixture
  `arrays/constructor_producers_wrapper_6807.jl`(21 assert, julia 1.12 parity)。残り ~36 producer は後続バッチ。

### `Value::NativeArray` carrier 撤去 Slice 4: build buffer de-variant ✅ (Issue #6807)

- 増分 build buffer(`NewArray*`/`PushElem*`/`ReserveArray`/`Finalize*`、`exec/array_basic.rs`)を
  `Value::NativeArray` → flat growable `Value::Memory`(`NewMemory`/`MemorySet` と同表現)へ移行し、VM に
  残る**最後の生きた `Value::NativeArray` producer** を撤去。build buffer = lazy specializer の型付き配列
  リテラル(`[1,2,3]`)+ 空 `Vector{String}` 定数(`ARGS`/`DEPOT_PATH`/`LOAD_PATH`)。
- `ArrayValue::push` の要素ロジックを共有 `push_into_array_data` へ抽出、`MemoryValue::push`/`push_f64`/
  `reserve`/`with_capacity`/`is_struct_ref_array` から再利用。`Finalize*` は native buffer の `ArrayValue` を
  厳密再構成(`memory_first_with_capacity` の derive + storage/shape 差し込み)し `array_wrapper_value_from_array_value`
  で変換 → wrapper byte-identical。dead化した共有 converter `native_array_value_mut_ref` も撤去。
- full **3842/3842**・AoT **3782/3782**・clippy(通常/aot)・fmt clean、allowlist 5 files 不変、bench neutral。
  fixture `arrays/build_buffer_devariant_6807.jl`(35 assert, julia 1.12 parity)。

### `Value::NativeArray` carrier 撤去 Slice 3c (PR B): IndexStore write fast path ✅ (Issue #6806)

- 単一整数 index + numeric `Array{T}` wrapper への `a[i]=v` を `Memory` へ直接書く native write fast path
  (`exec/array_index.rs`)。新フラグ `disable_array_setindex_specialization`(#6657 を write 側にミラー、
  cache.rs+pipeline_ctx.rs、cache 再計算で version bump 不要)で user `setindex!` override 時は dispatch。
  安全制約: `set_value` == `convert(T,v)` のペアのみ(完全一致 + int→float widening; **float→int は defer**)。
- 効果: `construction_undef_zeros_128` **−46%**・`hof_broadcast_filter_reduce_128` **−49%**(#6805 比)。
  full **3842/3842**・AoT **3782/3782**・clippy/fmt clean。fixtures: `arrays/setindex_wrapper_untyped_param_6806.jl`
  + `dispatch/setindex_any_user_method_6806.jl`(gate, julia parity)。pre-existing バグ #6839 発見・報告。

### `Value::NativeArray` carrier 撤去 Slice 3 (PR B 起点): IndexLoad rank-1 wrapper fast path ✅ (Issue #6806)

- raw `IndexLoad` で rank-1 MemoryRef-backed `Array{T}` を単一整数で引くケースを Memory から直接読む native
  fast path に(`exec/array_index.rs::rank1_memoryref_wrapper_element`)。従来は wrapper をインデックス毎に
  Base `getindex` dispatch していた(untyped-param `f(a)=a[i]` が該当)。#6657 フラグ
  `disable_array_getindex_specialization` でゲート(ユーザ override 時は dispatch)。挙動同一・O(1)・bounds は
  論理 `shape[0]`(view 対応)。
- 効果: `hof_broadcast_filter_reduce_128` **−38%**、`view_subarray_parent_share_64` **−57%**(#6805 baseline 比)。
  full **3842/3842**・AoT **3782/3782**・clippy/fmt clean。fixture `arrays/rank1_wrapper_index_untyped_param_6806.jl`
  (12 assert, julia parity)。次: IndexStore/多次元拡張 + consumer 移行(PR B 継続)。

### `Value::NativeArray` carrier 撤去 Slice 2: 配列 producer を wrapper 化 ✅ (Issue #6806)

- 配列リテラル(`PushArrayValue`)・comprehension(`FinalizeArray`/`FinalizeArrayTyped`)・undef ctor
  (`push_undef_typed_array`)を MemoryRef-backed `Array{T,N}` wrapper 直生成へ flip
  (`exec/array_basic.rs`、helper `push_array_value_as_wrapper` / `finalize_top_array_to_wrapper`、
  既存 `array_wrapper_value_from_array_value` 再利用)。増分 build buffer は #6807 まで一時 native。
- raw `IndexLoad` に wrapper native fallback(`exec/array_index.rs`、Base getindex 未ロードの bare VM 用、
  dispatch 優先で実プログラム挙動不変)。host 戻り値は `normalize_host_return_value` が NativeArray へ正規化(FFI 境界)。
- 回帰 test 追加(`test_array_literal_emits_memoryref_wrapper_issue_6806`、struct heap で内部表現を検証)、
  `test_typed_container_slot_ops_roundtrip_issue_5081` を新表現に更新(wrapper は generic slot 経由)。
- full suite **3842/3842**、AoT gate green、clippy 0、perf #6805 baseline 同等。docs:
  `ARRAY_MEMORY_MIGRATION.md` に Slice 2 追記。残: typed-slot/accessor 移植(PR B)+ variant 撤去(#6807)。

### 推論型システムに配列階数(ndims)を追加し comprehension の rank dispatch を修正 ✅ (Issue #6817)

- `ValueType::ArrayOf(ArrayElementType, Option<usize>)` と `ConcreteType::Array { element, ndims }` に
  階数フィールドを追加(default `None`→従来通り Vector 投影)。多イテレータ comprehension の lowering /
  `infer_expr_type` が階数=節数をセットし、`infer_julia_type`・`concrete_type_to_julia_type`・
  `lattice_to_parametric_julia_type`・bridge を rank 対応化。2次元 comprehension が `::Matrix` へ正しく
  dispatch し、`view(matrix_comprehension, …)` が動作。要素未知時は bare alias で要素特化メソッドは実行時解決。
- ~360 箇所のフィールド追加(cache 再生成)。fixture `comprehension/multidim_comprehension_dispatch_6817.jl`
  (15 assert, julia parity)。full 3841/3841・AoT green・clippy 0。#6814 の `<:` 修正・#6806 carrier 撤去とは別経路。

### `ConcreteType` retirement Slice 1: 分類 pin + `type_id` reader 監査 ✅ (Issue #6720, Phase 6)

- representation flip(Slice 2)前の behaviour-preserving 足場固め。
- **分類 characterization test** `concretetype_coretype_roundtrip_classification_issue_6720`
  (`compile/lattice/types.rs`)を追加。`ConcreteType ↔ CoreType` round-trip の golden snapshot で、
  どの variant が CoreType-faithful(round-trip 恒等)か、type_id を落とすか(Struct: name 保持・type_id→0)、
  lattice-only carrier として lossy か(Function→name 喪失 / Closure・ComposedFunction→nameless Function に潰れる /
  Enum→Struct 化で enum-ness 喪失)を固定。Slice 2 がこの分類を黙って変えると test が落ちる tripwire。
- **`type_id` reader 監査**: `ConcreteType::Struct.type_id` を**読む**本番サイトは 3 箇所のみと確定
  (`bridge.rs:1056` は test)。`expr_tfuncs` の `value_array_element_from_concrete` は既に
  `struct_type_id: FnMut(&str)->Option<usize>` を引数に持ち、`constructor_lattice_to_value_type` は呼び出し元が
  `StructIdLookup` を持つ。`bridge.rs:791` の `convert_concrete_to_array_element` のみ table 未保持だが Complex
  ケースは name で解決済みで、generic `StructOf` arm に resolver を 1 caller hop 通すだけ。**hard blocker なし**と判明。
- 設計書 `docs/vm/CONCRETETYPE_RETIREMENT.md` §3.1(監査結果)・§5(slice 計画の精緻化)を更新。
- full suite 3841/3841、AoT gate OK、clippy 0 warnings(自クレート)。コード変更は test 追加のみ(representation 不変)。

### `ConcreteType` retirement の設計確定(wrapper: CoreType + lattice-only carriers) ✅ (Issue #6720, Phase 6 設計)

- 「LatticeType の `CoreType` payload 化」本体に向けた設計スライス。**「CoreType を直接拡張(type_id/captures/Enum)」は却下**:
  CoreType は 1706 use の共有 semantic core で subtype engine が網羅 match し、`MethodSig.core_signature` として
  シリアライズされる唯一の正。非セマンティックな `type_id` 等を入れると engine に無意味 arm 波及＋serialization 変更
  (cache version)になるため。
- 採用: **wrapper**(Phase 6 end-state)。`ConcreteType = Core(CoreType)` + lattice-only carrier
  (`Function{name}` / `Closure{name,captures}` / `ComposedFunction` / `Enum{name}`)。struct `type_id` は
  name から struct table 経由で resolve(専用 variant にしない)。CoreType は不変。
- variant 分類は `From<&ConcreteType> for CoreType` の実マッピングを ground truth に確定:
  primitives/abstracts/Any/コンテナ/Tuple/Union/`DataType`(→TypeOf, 名前保持)/`Module`(名前保持)等は faithful に
  `Core(_)` へ畳める。Function/Closure/ComposedFunction は CoreType だと nameless `Abstract(Function)` に潰れ、
  `Enum` は `Named` に潰れて #2863 dispatch 区別を失うため carrier 化。
- 設計書 `docs/vm/CONCRETETYPE_RETIREMENT.md` を新規作成(variant インベントリ / type_id 解決戦略 /
  ハザード #5085・#2863・closure・serialization / multi-PR 移行スライス / 検証手順)。`TYPE_REPRESENTATIONS.md`
  Phase 6 から参照。docs のみ・コード変更なし。
### bare alias `Vector`/`Matrix` の `isa`/`<:` ndims 無視を修正 ✅ (Issue #6814)

- `[1,2,3] isa Matrix`・`Matrix{Int64} <: Vector` 等が誤って true(本家 false)。原因=
  `struct_params_are_subtype_with_lookup` の array-family 経路が、supertype 無パラメータ時に
  rank を無視して true を返していた。修正=bare alias の名前が固定する rank(`Vector`=1/`Matrix`=2/
  `AbstractVector`/`AbstractMatrix` 等)を尊重し、subtype rank 一致時のみ true。rank-free 名
  (`Array`/`AbstractArray`/`DenseArray`/`BitArray`)は任意 rank 維持。
- fixture `types/vector_matrix_alias_ndims_6814.jl`(24 assert, julia 1.12 parity)。
  full 3840/3840・AoT green・lib 2888/2888・clippy 0。
- 別根因の comprehension rank 推論バグ(`view`/dispatch)は #6817 に分離。

### `build.sh` の DerivedData 選択を mtime ベースに修正 ✅ (Issue #6821)

- `find_app_path()` が `find | tail -n1` で不定な順序の DerivedData を走査していたため、古いビルド
  （bundle executable 欠落）を選んで `simctl install` が失敗することがあった。
- 最新の更新時刻（mtime）でソートして最も新しい `SubsetJuliaVMApp.app` を選択するように変更。

### `build.sh` の `APP_BUNDLE_ID` を Xcode プロジェクトと一致させる ✅ (Issue #6823)

- `APP_BUNDLE_ID` が `jp.satoshiterasaki.SubsetJuliaVMApp` に固定されていたが、
  Xcode の `PRODUCT_BUNDLE_IDENTIFIER` は `jp.atelier-arith.subsetjuliavm`。
- これにより `simctl uninstall` / `launch` が正しい bundle ID を対象にできるように修正。

## 最新対応 (2026-06-16)

### VM 実行 hot path の直接 `Value::NativeArray` match を共有ヘルパ経由へ ✅ (Issue #6806 slice 1, Milestone #26)

- carrier 撤去の VM 実行エンジン移行(#6806)第 1 スライス(#6806 は未完・継続中)。
- 4 hot-path 実行ファイルの直接 `Value::NativeArray` variant match を撤去:
  `exec/array_index.rs`(generator iter)・`exec/call.rs`/`exec/locals.rs`(`LoadSlotArray`)・
  `exec/call_dynamic.rs`(iterate スコアリング)。共有 `native_array_value_ref` 経由 + 冗長アーム畳み込み。
- 挙動完全保存(`Rc` clone 等価)。監査 allowlist 9 → 5。
  full 3839/3839・AoT 3779/3779・clippy 0・bench 退行なし。
- 副産物: 既存バグ #6814 起票(`Matrix{Int64} <: Vector` が true 等、bare alias の ndims 無視)。

### `Value::NativeArray` carrier 撤去の性能ベースライン記録 + 監査 ratchet 整備 ✅ (Issue #6805, Milestone #26)

- carrier 撤去 epic #6723 を slice 化した Milestone #26 の前提ステップ(撤去本体は #6806 / #6807)。
- `benches/vm_array_benchmark.rs` に多次元 index(`a[i,j]` 32×32)・MemoryRef-backed 構築・`view`/`SubArray`
  親共有の 3 ケースを追加(sjulia↔julia 1.12 出力一致を確認)。撤去前ベースラインを
  `benchmarks/results/vm_array_baseline_6805.md` に記録。
- `scripts/check_value_array_allowlist.sh` に `Value::NativeArray` の allowlist ratchet を追加
  (variant を明示 9 ファイルに固定; list は縮小のみ・新規使用と stale エントリの双方を検知)。
  `docs/vm/CODE_AUDITS.md` / `docs/vm/ARRAY_MEMORY_MIGRATION.md` を更新。
- `scripts/test_aot.sh` baseline を記録(AoT gate green)。観測可能な振る舞いの変更なし。

### `substitute_to_julia_type_lossy` も `CoreType` hub 経由化し #34 の残り round-trip を排除 ✅ (Issue #6720, 3rd slice)

- 2nd slice の follow-up。`TypeExpr::substitute_to_julia_type_lossy` の `Parameterized` 腕を、
  type var 置換後に param 名を render して `from_name_or_struct` で再 parse する経路から、
  構造化 `substitute_to_core` + `core_type_to_julia_type`(= `TypeExpr → CoreType → JuliaType` hub)へ rerouting。
  置換済み params が `CoreType` 構造を保ったまま hub に着地する。
- 新設した private `TypeExpr::substitute_to_core(subst)`: bound type var → `CoreType::from(arg)`、
  unbound var / runtime expr → `CoreType::Any`、`Parameterized` → `Tuple` / 正規化 `Union` /
  `Struct{name, params}`。leaf 腕(top-level の Concrete clone / bound-var clone / unbound→Any)は据え置き。
- 挙動保存: 旧 render+reparse と **byte-identical**(user struct・unbound var・nested・Union・Tuple・Vector を含む
  `substitute_to_julia_type_lossy_matches_string_round_trip_issue_6720` で網羅 pin)。
- これで #34(`TypeExpr → JuliaType`)は両 projection method とも `TypeExpr` の文字列 round-trip を持たない
  (意図的に温存した単一名 leaf parse のみ残る)。
- full suite 3840/3840、AoT gate OK、clippy 0 warnings(自クレート)。

### `TypeExpr → CoreType` 構造化 resolver を新設し §3.3.1/#34 の文字列 round-trip を排除 ✅ (Issue #6720, 2nd slice)

- epic #5916 / `TYPE_REPRESENTATIONS.md` §4 Phase 4 の enabling piece。`impl From<&TypeExpr> for CoreType`
  (`inference_core/type_core/convert.rs`)を新設し、`TypeExpr::to_julia_type_lossy` を
  `TypeExpr → CoreType → JuliaType` 構造化 hub(`core_type_to_julia_type(&CoreType::from(te))`)へ rerouting。
- これまで `to_julia_type_lossy` は `from_name_or_struct(&self.to_string())` で **render + reparse** し、
  parametric application の params を opaque な `JuliaType::Struct(String)` に潰していた(§3.3.1 / 変換 #34 のワート)。
  resolver 経由で `Parameterized` は `CoreType::Struct{name, params}` / `Tuple` / 正規化済み `Union` として
  **構造を保持**したまま hub に着地する。
- 挙動保存: lowering 由来の `TypeExpr` 形(struct field 型)に対し新経路は旧 round-trip と **byte-identical**
  (`type_expr::tests::to_julia_type_lossy_matches_string_round_trip_issue_6720` で網羅 pin)。
  top-level の bare type-var / runtime-expr **leaf** は単一名 parse を温存(未解決 `T` を `Struct("T")`
  placeholder のまま保つ。CoreType 経由だと `TypeVar` に再解釈され divergence するため)。
- **「LatticeType の `CoreType` payload 化」本体(`Concrete(ConcreteType)→Concrete(CoreType)`)は multi-PR に deferral**:
  素朴な swap は `type_id`/`Closure` captures/`Enum` を消失し #5085 / closure / #2863 を破壊するうえ、設計 end-state
  (Phase 6: ConcreteType = CoreType + lattice-only variants)とも異なる。詳細は Issue #6720 スレッドに記録。
- テスト: `type_expr_to_core_tests` 4 本(struct/tuple/union 正規化/nested)＋ 網羅 characterization 1 本を追加。
  full suite 3839/3839、AoT gate 3779/3779、clippy 0 warnings。
### 配列のフィールドアクセス `a.size` / `a.ref`(関数引数経由)を修正 ✅ (Issue #6804)

- `Array{T,N}` は `ref::MemoryRef{T}` / `size::NTuple{N,Int}` を持つ pure-Julia struct。トップレベルの
  `a.size`/`a.ref` は faithful Array 化で動作済みだったが、**関数引数経由**の `f(a)=a.size` が
  `expected numeric value, got Tuple` で失敗していた。原因=遅延 specializer の `compile_field_access` が
  実行時 faithful Array wrapper struct の **パラメトリック** `size`/`ref` フィールド型を誤って解決
  (`size::NTuple{N,Int}` を数値型と誤判定)→ `GetField` の静的型で Tuple 戻り値を数値へ coerce。
- 修正: specializer は array-wrapper struct(`Array`/`Vector`/`Matrix`)のフィールドアクセスを
  インタプリタに委譲(`is_array_wrapper_struct_name` ガードで `Unsupported` を返す)。インタプリタの
  `GetFieldByName` は `a.size`(dims Tuple)/`a.ref`(MemoryRef)を正しく返す。Issue が挙げた fallback
  方針(direction 2)。根本の ArrayOf 推論リーク撤去(direction 1)は carrier 撤去 epic #6723。
- fixture `array/array_field_access_6804.jl`(parity OK 12 assert: トップレベル/関数引数/2D/push!後/Float)。
  full 3834 緑、AoT gate OK。

### `ArrayElementType::UnionOf` を構造化(`Vec<JuliaType>`)し bridge の文字列再パースを除去 ✅ (Issue #6720)

- epic #5916 / `TYPE_REPRESENTATIONS.md` §4 Phase 4 のサブスライス。VM 配列要素型
  `ArrayElementType::UnionOf(String)`(`Union{...}` body を文字列で埋め込み、
  `compile/bridge.rs` が毎回 `from_name_or_struct` で再パースしていた #40 のワート)を
  構造化 `UnionOf(Vec<JuliaType>)` に置換。
- 変換境界の挙動は byte-identical を維持:
  - 表示(`julia_type_name` / introspection / infer)はメンバ順を温存。
  - materialize(`array_element_type_to_julia_type`)と lattice 変換
    (`convert_union_array_element_members`)は `canonicalize_union` でメンバを構造的に
    正規化(flatten/dedup/sort/collapse、#5066)。旧来の `Union{...}` 文字列 round-trip と
    同一結果(parser も同じ `canonicalize_union` に委譲していたため)で、文字列再パースは消滅。
  - 文字列が真に流入する境界(`JuliaType::Struct("Union{...}")` 名のスライス、
    型 AST の `Union` 引数 join 等)のみ `ArrayElementType::union_from_body` で
    メンバへ持ち上げ(brace-aware split・順序保存)。
- メンバ型が `JuliaType` なのは、`Nothing`/`Missing` 等 `ArrayElementType` に storage
  variant を持たない union メンバを運べるようにするため。
- テスト: `array_element` 単体 3 本(`*_issue_6720`)を追加、bridge/builtin_array/array_value の
  既存ピンを構造化形式へ更新。full suite 3834/3834、AoT gate 3774/3774、clippy 0 warnings。
### 分数 BigFloat 指数 `big(2.0)^0.5` のハングを解消(astro-float を vendor + patch)✅ (Issue #6794)

- 分数指数の BigFloat 冪は astro-float 0.3.6 の `exp`/`ln`/`pow` の Ziv 正確丸め改善ループが
  table-maker's dilemma 入力(`big(4.0)^0.5 = 厳密に 2.0` 等、結果が丸め境界上)で**収束せず無限ループ**
  していた。astro は固定精度 API を公開しておらず外部から打ち切れないため、`astro-float-num 0.3.6` を
  `vendor/astro-float-num/` に vendor し `[patch.crates-io]` で差し替え、`exp`(pow.rs)・`ln`(log.rs)・
  内部 `pow`(`e^(n·ln x)`)の各 Ziv ループに**上限**(`ZIV_REFINEMENT_EXTRA_BOUND=512` bit)を追加。
  上限到達時は最良近似を `set_precision(p,rm)` で最近接丸め → 境界ケースは厳密値(`2.0`)を返す。
- `dynamic_ops` 側の整数指数限定ゲート(#6790)を撤廃(`is_bigfloat_pow` は実数オペランドなら分数指数も
  インライン astro `pow` へ)。`big(4.0)^0.5`→`2.0`(従来ハング)、全分数冪が ~0.7s で終了。
- 厳密に表現可能な結果(`4^0.5=2`, `100^0.5=10`, `4^-0.5=0.5` 等)は本家とビット一致。無理数結果は
  astro と MPFR の最終 bit 丸め差で ~1 ULP 異なり得る(astro 採用に内在、本 patch とは無関係)ため
  fixture は厳密値は `==`、無理数は `isapprox`/往復で検証。fixture `bigfloat/bigfloat_fractional_pow_6794.jl`
  (parity OK 17 assert)。full 緑、AoT gate OK。整数指数は #6790 の powi 経路のまま。

### ユーザ `promote_rule` 追加が数値 `promote_type` dispatch を破壊する問題を修正 ✅ (Issue #6782)

- 症状: ユーザが `promote_rule` メソッドを 1 つでも追加すると、無関係な数値ペアの
  `promote_type` が specific な rule ではなく `typejoin` フォールバック(`Integer`/`Number`)を
  返す。`promote_type(Bool,Int64)→Integer`(誤、正は `Int64`)、
  `promote_type(Complex{Float64},Int64)→Number`(誤、正は `Complex{Float64}`)。
- 根本原因: runtime メタデータ dispatcher `Vm::find_best_method_index_from_candidates`
  (`vm/mod.rs`)の先頭にあった **blanket "base/user origin 境界をまたぐ候補集合なら即 `Ok(None)`"
  fence**。ユーザが Base 関数(`promote_rule`)にメソッドを足すと候補集合が base+user 混在になり、
  この fence が `where` 境界付き `Type{T}` parametric メソッド
  (`promote_rule(::Type{Bool}, ::Type{T}) where {T<:Number}`、`Complex{T}`/`Rational{T}` 系)を
  解決できる **唯一のチャネル(メタデータ scorer)を関数まるごと無効化** していた。concrete-typed
  メソッド(`promote_rule(::Type{Int16}, ::Type{Int8})` 等)は typed-core string resolver が解決する
  ため壊れず、parametric メソッドだけが catch-all(`Union{}`)に落ちて `promote_type` が `typejoin`
  へ widening していた。compile-time の `promotion.rs` とは独立、#6735(type_priority 除去)とも無関係に既存。
- 修正: blanket fence を撤去。fence が粗い proxy としていた 2 つの安全不変条件は、関数本体に既存の
  surgical guard が個別に担保している — (1) per-candidate の native-array wrapper 境界除外
  (Issue #6202、`params_cross_native_array_wrapper_boundary`)が Base array-wrapper 候補を legacy
  string resolver に残し、(2) Issue #5926 の origin dominance fence
  (`base_runtime_dominance_crosses_user_candidate`、dominance pre-check 内)が Base-origin メソッドの
  dominance-override だけでの user 候補上書きを防ぐ。よってメタデータ scorer は混在集合でも安全。
- fixture `promotion/user_promote_rule_coexists_6782.jl`(parity OK、ユーザ拡張 2 + 数値 10 assertions:
  Bool/Int64・Complex/Int・Rational 等の parametric ペアと concrete ペアの両方)。`#6735` fixture の
  保留メモも本 fixture を指すよう更新。dispatch 系 unit 349/349、full suite 3831/3831、AoT gate 3771/3771、
  clippy 0 warnings。

### `Value::Dict` carrier 撤去 — public Dict API を pure Dict{K,V} へ一本化 ✅ (Issue #6731)

- ENV が `Value::Dict` carrier の最後の runtime 生成元だった(全 2505 fixture を `new_dict_ref` で
  計測 → `reflection/env_constant.jl` のみ)。`Instr::PushEnv` を `(key,value)` 2-tuple 供給に変更し
  pure `_env_from_pairs(pairs)=Dict{String,String}(pairs)` 経由で struct 化(PR #6787)。以後 carrier
  births は全 fixture 0 → 到達不能を証明後、機械的に撤去(PR #6793): `Value::Dict` variant +
  `DictRef`/`new_dict_ref`/`new_dict_value` + `_dict_*` intrinsic(BuiltinId `_DictGet`..`_DictPairs`)+
  `DictNew`/`DictMerge`/`DictLen`(emit 0)+ `NewDict`/`NewDictTyped`/`NewDictWithPairs` Instr +
  非パラメトリック `::Dict` メソッド + 死んだ arm(~20 ファイル)。
- public `BuiltinId::Dict*` は `try_dispatch_struct_dict` で pure `Dict{K,V}` メソッドへ飛ぶ薄い
  トランポリンとして存続。`Value::Set` 共有 Instr と `DictKey` は維持。`CACHE_VERSION` 58→59。
  fixture `dict/dict_carrier_removed_pure_struct_6731.jl`(parity OK、21 assertions、#6584 トラップ含む)。
  full suite 3833/3833、AoT gate OK。

### `Value::Set` carrier 撤去 — pure Set{T} struct へ一本化 ✅ (Issue #6732)

- #6731 と同じ "prove-dead-then-delete" レシピ。#6721 で `Set{T}` は既に `Dict{T,Nothing}` ベースの
  pure-Julia struct(`base/set.jl`)。`SetValue` allocator を計測 → 全 2508 fixture で `Value::Set`
  births 0(到達不能を証明)。pure Set メソッドは `_set_*` ではなく純粋 Dict 演算を使うため `_set_*` も dead。
- 撤去: `Value::Set` variant + `builtins_sets/` モジュール全体 + `BuiltinId::Set*`/`_Set*`(全 emit 0 /
  caller 無し)+ internals `_set_*` intercept + `frame::set_slot_set/slot_set` + 死んだ arm(~20 ファイル)+
  旧 carrier unit test 2 件。Set Instr は decodable-but-unreachable(error ハンドラ)として保持。
  Dict/Set 共有の `LoadDict`/`StoreDict`/`DictLen`/`DictSet`/`ReturnDict` も完全到達不能化。`CACHE_VERSION`
  59→60(PR #6797)。fixture `sets/set_carrier_removed_pure_struct_6732.jl`(parity OK、20 assertions)。
  full suite 3831/3831、AoT gate OK。

### hash の Rust intercept を停止し pure-Julia method dispatch 化 ✅ (Issue #6728)

- `isequal`/`isless` は既に dispatch-first(ユーザ overload 尊重)。`hash` だけが
  `handlers/mod.rs` `"hash" => misc::compile_hash` で 1-arg を `CallBuiltin(BuiltinId::Hash)` に
  強制 intercept し、ユーザ `hash(::T)` を無視していた(本家との実害ある乖離)。`compile_hash` を
  削除し `hash` を通常の Julia method dispatch(pure `base/hashing.jl`)へ。`BuiltinId::Hash` は
  dispatch fallback として温存。
- `BuiltinId::Hash`≡`_Hash`(確認済み)なのでユーザ型以外の hash 値は不変。唯一差は pure
  `hash(x::Float64)` の -0.0 正規化(contract 適合・`hashing/hash_basic.jl` が assert 済み)。
  注意: sjulia の 1-arg `hash(x)=_hash(x)` は 2-arg `hash(x,h)` に委譲しない(本家と差)ので、
  Dict/Set キーには **1-arg** `hash(::T)` を override する必要。`CACHE_VERSION` 60→61(compiler-only
  だが base bytecode の hash 呼び出しが変わるため base 再生成、PR #6799)。fixture
  `hashing/user_hash_dispatch_6728.jl`(parity OK、17 assertions)。full 3831/3831、AoT gate OK。
- これで Milestone #22 は実質完了(26 close + #6723 は意図的 defer/tracking)。
### BigFloat の `%` / `rem` / `mod` を実装 ✅ (Issue #6796)

- `big(5.0) % big(3.0)` 等が `Unsupported BigFloat operation: Mod` で未対応だった。`RemBigFloat`
  intrinsic(astro_float `rem`、被除数の符号、`x % 0`→NaN)を追加し、**2 経路**に配線:
  - 型が静的に BigFloat と分かる `%`(`compile/expr/binary/mod.rs` の 2 箇所)→ `RemBigFloat`。
  - 無型引数の純 Julia `rem`/`mod`(`base/math.jl` の `x%y`)が通る動的 `%` 経路
    (`CallDynamicBinaryBoth(SremInt)` → `exec/binary_both.rs` の BigFloat fallback に
    `SremInt → RemBigFloat` を追加、BigInt の `SremInt → RemBigInt` と対称)。
- 結果: `%`/`rem` は被除数の符号、`mod` は除数の符号(`math.jl` の `mod` が `%` から導出)、`x%0`→NaN、
  BigFloat×Int/Float 混在も昇格。すべて本家 1.12.6 一致。fixture `bigfloat/bigfloat_rem_mod_6796.jl`
  (parity OK 19 assert)。full 3831 緑、AoT gate OK。
- スコープ外(別 issue #6801 起票): `divrem`/`fldmod`/`floor`/`ceil`/`round`/`trunc`/`div`/`fld`/`cld`
  は BigFloat 未対応(`div=floor(x/y)` の `floor(::BigFloat)` が無い)。broadcast `.%` も別経路。

### レガシー reducer HOF VM 命令の除去 ✅ (Issue #6733)

- dead-but-kept だった reducer HOF VM 命令 13 個(FindAllFunc/FindFirstFunc/FindLastFunc/
  MapReduceFunc(WithInit)/MapFoldrFunc(WithInit)/MapFuncInPlace/FilterFuncInPlace/SumFunc/
  AnyFunc/AllFunc/CountFunc)を Instr enum + exec + effects から削除。連鎖死コード
  (pop_array_or_values/PopArrayResult、hof_exec reducer starter 群)も整理。
- any/all/count/sum/mapreduce/findall は pure-Julia dispatch(#3728/#3731 で移行済み)、range/
  LinRange/first/last 維持、TupleFirst(destructuring)は live のため温存。`CACHE_VERSION` 57→58。
  fixture `hof/reducer_legacy_instr_removal_6733.jl`(parity OK)。

### promote_type 数値優先度テーブル除去 → promote_rule ネットワーク委譲 ✅ (Issue #6735)

- compile-time 数値昇格ハードコード `type_priority`(`compile/promotion.rs`)を除去。promote_type は
  登録済み `promote_rule` registry(`base/promotion.jl`、152 ルール)を正本とし、bootstrap 時のみ
  共有 `inference_core::PrimitiveNumeric` taxonomy + Bool/Complex/Big 明示ルールにフォールバック。
- 挙動不変(全数値ペア promote_type が除去前・本家 1.12 と一致)。fixture
  `promotion/promote_type_pure_julia_6735.jl`(parity OK)。#6727 子スライス 1/4。発見バグ #6781/#6782。

### 空のパラメトリック要素型 Vector の eltype 保持 ✅ (Issue #6768)

- `Vector{UnitRange{Int64}}()` / `UnitRange{Int64}[]` / `Vector{Vector{Int}}()` が `push!` 後も
  `typeof`/`eltype` で要素型を保持(従来は `Vector{Any}` へ widening)。`Int64[]`/`Complex{Float64}`
  は従来どおり保持。具象パラメトリック型(専用タグなし)を `ArrayElementType::Abstract(<名>)` で保持。
- 修正経路: `compile/expr/collection.rs`(`Vector{T}()` の `TypeExpr::Parameterized`)、
  `compile/expr/mod.rs` + `infer/mod.rs`(`T[]` の文字列 catch-all)、
  `lowering/expr/collection.rs`(全 static パラメトリック型 = `TypeOf("...")` を `T[]` 型名抽出に追加)、
  `vm/value/array_element.rs`(`typeinfo_implicit` を `Abstract` 名で再帰評価し表示を本家一致に)。
- fixture `arrays/parametric_eltype_empty_vector_6768.jl`。full 3835 green、AoT gate OK。

### `Union{Type{A}, Type{B}, ...}` 引数に対する型オブジェクト dispatch 修正 ✅ (Issue #6781)

- `promote_type(BigFloat, Float64)` が本家の `BigFloat` でなく `AbstractFloat`(`typejoin` フォール
  バック)を返していた。根本原因は **第二引数が `Union{Type{...}, ...}` のメソッド**
  (BigFloat/BigInt/Rational の `promote_rule`、Issue #5070)が型オブジェクト引数(`Type{Float64}`)に
  対して dispatch でマッチせず、汎用 fallback `promote_rule(::Type{T}, ::Type{S}) = Union{}` に落ちて
  いたこと。`promote_type` は両 promote_rule が `Union{}` のとき typejoin に widening する。
- 2 経路を修正:
  - 実行時 dispatch (`vm/mod.rs` の `value_matches_param`): `Value::DataType` × `JuliaType::Union`
    のアームを追加し、各 Union メンバへ再帰(`Value::DataType` × `Type{T}` アームに到達させる)。
  - コンパイル時 dispatch (`inference_core/dispatch_resolver/core_match.rs`): 型オブジェクト引数を
    type 形パラメータのみに許可していた fast-reject ガードに `CoreType::Union(_)` を追加し、既存の
    `core_is_subtype_full` の Union 判定(`Type{Float64} <: Union{..., Type{Float64}, ...}`)へ通す。
    非 type 形 `Union{Int64, Float64}` は subtype 判定が正しく false を返す。
- これにより BigFloat/BigInt/Rational + 数値の promote_type/promote_rule、およびユーザ定義
  `Union{Type{A}, Type{B}}` メソッド dispatch がすべて本家一致。fixture
  `promotion/bigfloat_promote_type_6781.jl`(parity OK)。

### BigFloat 表示を本家 1.12 の Base.MPFR 形式に一致 ✅ (Issue #6789)

- `big(5.0)` が `5.e+0`、`big(0.25)` が `2.5e-1`、`big(1e6)` が `1.0e+6` のように astro_float の
  生の指数形式で表示され本家(`5.0` / `0.25` / `1.0e+06`)と乖離していた。
- `vm/formatting.rs` に `format_bigfloat_julia`(NaN/Inf/±0 を astro 述語で処理)+ 純粋変換
  `prettify_bigfloat_string` を追加。本家 `_prettify_bigfloat` を移植: 指数 ∈ [-4,5] は位取り
  10 進、それ以外は科学表記。**1.12 固有点**=科学表記の指数を符号付き・2 桁ゼロ詰め `e±NN`
  (`e+06`/`e-08`/`e+100`)にする(1.14 の `eN` ではなく、parity gold standard が 1.12.6 のため)。
- 表示 3 経路を集約: `format_value_slow`、`value_to_string`(`vm/mod.rs` 経由で `ffi/format.rs`
  へ再エクスポート)。astro の桁は MPFR の最短往復桁と一致(0.1/1e-5/1e-6 等で桁一致を確認)。
- fixture `bigfloat/bigfloat_display_6789.jl`(parity OK 19 assert)。残: `-0.0` は astro が
  符号を保持せず `0.0`(既存制約・非回帰)。作業中発見の別バグ #6790(`big(2.0)^n` stack overflow)
  / #6791(BigFloat ÷0 が Inf でなく例外)を起票。

### BigFloat ^ 整数 の無限再帰(stack overflow)を修正 ✅ (Issue #6790)

- `big(2.0)^100` 等が stack overflow。原因=終端する `^(::BigFloat, …)` メソッドが無く、`DynamicPow`
  が runtime `^` dispatch に回して無限再帰(BigInt の `should_use_inline_dynamic_op` 直行と同じ轍)。
- `dynamic_pow` に BigFloat 冪のインラインアームを追加(`RustBigFloat::pow` = astro_float pow、
  両オペランドを exact に BigFloat 化)。`should_use_inline_dynamic_pow` が「一方が BigFloat & 両者
  実数 & **指数が整数値**」の冪をインラインへ回す(`is_bigfloat_pow` / `is_integer_valued_pow_exponent`)。
- **整数値指数限定**: 真に分数の指数(`^0.5`)は astro_float 0.3.6 の `exp(n·ln)` 収束ループが
  ハングするため除外し、従来の挙動(再帰=stack overflow)のまま残す(非回帰)。→ 別 issue #6794 で追跡。
- BigInt **base** の冪(`big(3)^big(2.0)` 等)は `PowBigInt` intrinsic が I64 指数限定で別バグ(別途)。
- fixture `bigfloat/bigfloat_power_6790.jl`(parity OK 15 assert)。

### BigFloat の 0 除算を IEEE (±Inf / NaN) に修正 ✅ (Issue #6791)

- `big(1.0)/big(0.0)` が `DivisionByZero` を raise(本家は `Inf`)。`DivBigFloat` intrinsic の
  `is_zero()` ガードが原因。astro_float の `div` は `result_to_ext` 経由で IEEE 結果(同符号→+Inf、
  異符号→-Inf、0/0→NaN)を直接返すため、ガードを除去するだけで本家一致。表示は #6789 の
  `format_bigfloat_julia`(Inf/-Inf/NaN)が処理済み。整数の 0 除算(`DivBigInt`)は従来どおり throw。
- fixture `bigfloat/bigfloat_div_zero_6791.jl`(parity OK 10 assert)。作業中発見: BigFloat の
  `%`/`rem`/`mod` は未実装(`Unsupported BigFloat operation: Mod`)→ #6796 起票。

## 最新対応 (2026-06-15)

### Pure Julia 化: Printf エンジン @sprintf / sprintf ✅ (Issue #6746)

- `@sprintf`/`sprintf` を pure-Julia C-style Printf エンジン(`base/printf.jl`)化。flags/width/
  precision/conversion をパースし整数・hex/oct・文字列・char を Julia 側でレイアウト。float 変換
  (`%f`/`%e`/`%E`/`%g`/`%G`)のみ Rust 境界 `_printf_fmt_float`(Ryu)へ委譲。
- バグ修正: 旧実装は width/precision/flags 無視・float 既定精度欠落(`%f`→"3.14" vs 上流 "3.140000")。
  `sprintf` route `DirectBuiltin`→`DispatchFirst`、`BuiltinId::PrintfFmtFloat` 追加、`CACHE_VERSION`
  56→57。新 fixture `stdlib/printf_pure_julia_6746.jl`(@sprintf parity OK)。#6730 子スライス 1/4。

### Pure Julia 化: 丸め floor / ceil / round / trunc + RoundingMode + digits/sigdigits/base ✅ (Issue #6742)

- `floor`/`ceil`/`round`/`trunc` を pure-Julia(`base/floatfuncs.jl`)化。CPU intrinsic
  `floor_llvm`/`ceil_llvm`/`trunc_llvm` + 新規 `rint_llvm`(ties-to-even)上に構築、`where
  {T<:AbstractFloat}` で型保持。整数恒等・型付き形(型保持)・digits/sigdigits/base keyword・
  RoundingMode 全 7 モードを網羅。compile kwargs handler 撤去、`base` honor。`CACHE_VERSION` 55→56。
- バグ修正: dynamic 経路の `round` half-away→ties-to-even(`round(2.5)` 3.0→2.0)、`floor(5)`→`5`、
  `round(Int8,3.5)`→`Int8(4)`、RoundingMode tie 変種。fixture
  `floatfuncs/rounding_pure_julia_6742.jl`(parity OK)。#6726 子スライス 1/3。発見バグ #6775。

### Pure Julia 化検証: 配列生成 zeros / ones / similar / reshape dispatch-first ✅ (Issue #6744)

- `zeros`/`ones`/`similar`/`reshape` が pure-Julia(`base/array.jl`)dispatch-first で解決される
  ことを fixture で検証(#6729 スライス 2/3、verify-only)。zeros/ones は #4036 で pure-Julia
  allocation dispatch 化済み、similar/reshape は `where {T,N}` メソッド。
- 旧 Rust 生成 builtin は F64/I64 のみ生成可のため、`Float32`/`Int32`/`Complex{Float64}` 配列が
  正しい要素型で得られること = generic pure-Julia パスが走る証左。fixture
  `array/array_gen_dispatch_first_6744.jl`(parity OK)。Rust 変更なし。発見バグ #6771。

### Pure Julia 化: regex public 検索ラッパー count / findall ✅ (Issue #6749)

- `count(::Regex, s)` / `findall(::Regex, s)` を pure-Julia(`base/strings/search.jl`)で追加。
  regex エンジン(`match`/`eachmatch`)は Rust regex crate 境界として維持し公開ラッパーを Julia 側に。
- `findall` は `Vector{UnitRange{Int64}}`(バイト範囲)、`count` は非重複マッチ数。上流(julia 1.12)
  一致を fixture `regex/regex_count_findall_6749.jl`(parity OK)で検証。BuiltinId 変更なし。
- #6730 子スライス 4/4。発見バグ #6768(空パラメトリック要素型 Vector の eltype 退化)。

### Pure Julia 化: codepoint / bitstring ✅ (Issue #6747)

- `codepoint`(UInt32 を返すよう修正)と `bitstring` を pure-Julia 化(`base/strings/basic.jl` /
  `base/intfuncs.jl`、reinterpret-to-unsigned + `sizeof(typeof(x))` でビット列構築)。`BuiltinId::
  {Codepoint,Bitstring}` + handler + intercept + base_functions 撤去。`CACHE_VERSION` 54→55。
- 維持(byte列/Char primitive、issue 方針通り): `ncodeunits`/`codeunit`/`codeunits`/`Char(n)`/`Int(c)`。
  全型 + first-class value で上流一致。fixture `strings/codepoint_bitstring_pure_julia_6747.jl`。
  full/AoT green、clippy clean。#6730 子 2/4。発見バグ #6766(`sizeof(value)`=8)。

### Pure Julia 化: float 分解 exponent/significand/frexp/issubnormal/nextfloat/prevfloat ✅ (Issue #6740)

- 6 関数を pure-Julia(`base/float.jl`)へ移行。`reinterpret` + per-type IEEE bit-field helper
  (sign_mask/exponent_mask/exponent_one/exponent_half/significand_mask/significand_bits/
  exponent_bits/exponent_bias、Float16/32/64)で上流一致。Rust 境界は `reinterpret` のみ。
- `BuiltinId::{NextFloat,PrevFloat,NextFloatN,PrevFloatN,Exponent,Significand,Frexp,Issubnormal}`
  + handler + intercept + builtin.rs assertion + base_functions + is_pure_math + 未使用 helper を撤去。
  `CACHE_VERSION` 53→54。precursor `reinterpret(Float16↔UInt16/Int16)`(PR #6764)。
- **改善**: 旧 builtin は Float64 専用(Float32/Float16 を collapse)→ pure-Julia 化で型保存。
  `exponent(::Integer)` 追加。subset 対応: `da%U`→`convert(U,da)`、`⊻`→`!=`、Inf bits=`exponent_mask(T)`。
- fixture `floatfuncs/float_decomp_pure_julia_6740.jl`(parity OK)。full/AoT green、clippy clean。#6726 子 2/3。

### Pure Julia 化: リフレクション述語 isbits/ismutable/hasfield ✅ (Issue #6738)

- `isbits`/`ismutable`/`hasfield` を pure-Julia public ラッパー(`base/reflection.jl`)へ。VM メタ
  データ primitive(`isbitstype` 型フラグ・`_ismutabletype`・`_fieldnames`)は Rust 境界に維持。
  `BuiltinId`/`BuiltinOp` の `Isbits`/`Ismutable`/`Hasfield` を全経路(builtin.rs/abstract_interp/
  vm handler/base_functions marker・all_variants・infer ×3/test)から撤去。`CACHE_VERSION` 52→53。
- **バグ修正**: `ismutable("s")` 上流一致(true)に。`hasfield` は `::Symbol` 注釈を外し symbol
  リテラルの QuoteNode 変換エラーを回避(`name in _fieldnames(T)`)。`#6727` 子スライス 4/4 完了。
- fixture `type_inference/reflection_predicates_pure_julia_6738.jl`(parity OK)。full/AoT green、clippy clean。

### Pure Julia 化: parse/tryparse(Float64) を pure-Julia ラッパー化 ✅ (Issue #6748)

- `parse(Float64,s)`/`tryparse(Float64,s)` を pure-Julia(`base/parse.jl`)化、実変換は
  `_tryparse_float64` intrinsic(libc strtod、旧 `TryparseFloat64` を rename)へ委譲。`parse=
  tryparse+ArgumentError` の pure 実装にしたことで `BuiltinId::StringToFloat` を撤去。
  `compile_parse_tryparse` の Float64 分岐を除去しメソッドディスパッチへ統一。`CACHE_VERSION` 51→52。
- **バグ修正**: `parse(Float64,"bad")` が generic error → 上流同様 `ArgumentError`。`parse(Int;base=)`
  /`string(x;base=)` 維持。`parse(Float64,s)` の strtod 実体は境界に残置(issue 方針通り)。
- fixture `numeric/parse_float_pure_julia_6748.jl`(parity OK)。full/AoT green、clippy clean。#6730 子 3/4。

### Pure Julia 化: bit CPU 関数 (count_ones/leading_zeros/trailing_zeros/bitreverse/bswap) ✅ (Issue #6741)

- public 関数を pure-Julia(`base/int.jl`)化し、CPU 命令を underscored 低レベル intrinsic
  `_ctpop_int`/`_ctlz_int`/`_cttz_int`/`_bitreverse_int`/`_bswap_int` へ分離(上流
  `count_ones(x)=ctpop_int(x)%Int` 構造)。BuiltinId variant は維持、名前を underscored へ更新
  (`from_name`/`name()`/intercept/`builtin.rs`/is_base_function/all_builtin_names/EXEMPTED)。
- 挙動不変・全幅上流一致・first-class function value 維持。`_fma` は既に内部 intrinsic のため対象外。
  CACHE_VERSION bump 不要(variant 増減なし、prelude hash で invalidate)。
- fixture `numeric/bitcount_pure_julia_6741.jl`(parity OK)。full/AoT green、clippy clean。#6726 子 3/3。

### Pure Julia 化: length/size/ndims/eltype の dispatch-first 検証 ✅ (Issue #6743)

- 配列クエリ `length`/`size`/`ndims`/`eltype` は既に pure-Julia(`base/array.jl`)へ dispatch-first。
  ユーザー定義メソッドが shadow されず上流一致で動作することを回帰 fixture で pin(Rust builtin は
  内部 carrier 用フォールバックのみ=#6723/#6731/#6732 の carrier 撤去で最終的に解消)。コード変更なし。
- fixture `array/array_query_dispatch_first_6743.jl`(parity OK)。#6729 子スライス 1/3。

### Pure Julia 化: convert / promote の dispatch-first 検証 ✅ (Issue #6736)

- `convert`/`promote` の public API は既に pure-Julia(`base/essentials.jl`/`base/promotion.jl`)へ
  dispatch-first 済み。ユーザー定義 `convert`/`promote_rule` が shadow されず上流一致で動作する
  ことを検証(prior work #3727 等で達成済みの状態を回帰 fixture で pin)。Rust は pure メソッド非
  対応型の実変換フォールバックのみ(=issue 目標通り)。コード変更なし。
- fixture `numeric/convert_promote_dispatch_first_6736.jl`(parity OK)。#6727 子スライス 2/4。

### Pure Julia 化: widemul → pure-Julia + 数値変換 (signed/unsigned/float/reinterpret) ✅ (Issue #6737)

- `widemul` を上流 `widen(x)*widen(y)`(`base/number.jl`)へ移行し、intercept
  (`handlers/misc.rs::compile_widemul` + `handlers/mod.rs`)と `BuiltinId::Widemul` + VM handler を
  撤去。**バグ修正**: 旧 I64 専用 handler は `widemul(Int32,Int32)`/`Int8`/`Int16`/`Int128` で
  `widemul: cannot multiply` エラーだったのが全幅で型保存 widen に。
- `signed`/`unsigned`/`float` は既に pure-Julia dispatch(検証)、`reinterpret` は raw bit primitive
  維持。dead `FloatConv` は handler 規模が大きく本 PR では据置(到達不能)。`CACHE_VERSION` 50→51。
- 既知の残: `widemul(UInt32,UInt32)` 結果型 Int64(上流 UInt64)= `convert(UInt64,::UInt32)`
  mis-tag、**#6755**(bug)で追跡。fixture は値で parity。
- fixture `numeric/numeric_conversions_pure_julia_6737.jl`。full/AoT green、clippy clean。#6727 子 3/4。

### Pure Julia 化: 非破壊畳み込み/探索 (collect/find*/argmin/argmax/prod/min/max) ✅ (Issue #6745)

- `collect`/`findfirst`/`findall`/`argmin`/`argmax`/`prod`/`minimum`/`maximum` と配列 iterate は
  既に pure-Julia(`base/array.jl`)で上流一致。vestigial な dead `BuiltinId`
  `{Prod,Minimum,Maximum,Argmin,Argmax,FindFirst,FindAll}`(CallBuiltin emission 無し・handler 無し)
  を撤去し、`from_name`/`name()`/`all_builtin_names`/EXEMPTED を整理。`CACHE_VERSION` 49→50。
- fixture `array/reducers_finders_pure_julia_6745.jl`(parity OK、narrow-int reduction の Int 昇格を pin)。
  full suite green、AoT gate green、clippy clean。#6729 子スライス 3/3。

### Pure Julia 化: 残存文字列 builtin unescape_string/findall/count を撤去 ✅ (Issue #6724)

- `unescape_string` を char ベース pure-Julia 実装(`base/strings/util.jl`)に修正し dead Rust
  `BuiltinId::UnescapeString` を撤去。旧実装(byte/char index 混在)の multibyte 破損
  (`café`→`cafÃ©`、末尾欠落)を修正、`café`/`αβγ`/emoji 😀/CJK で上流 1.12 一致。
- dead builtin `StringCount` / `StringFindAll` を撤去(from_name 無し、`findall`/`count` の
  String/Char は既に pure-Julia `base/strings/search.jl`)。Rust 撤去 3 BuiltinId +
  3 handler + builtin_string.rs intercept、`CACHE_VERSION` 48→49。
- `occursin`(非regex)は既に pure-Julia。regex needle は `Occursin` builtin 維持(本 issue は
  「regex 除く」)。`isnumeric` は Unicode utf8proc category 依存のため #6752 へ分離。
- fixture `strings/unescape_string_multibyte_6724.jl`、full suite green、AoT gate green、clippy clean。
- Tier C スライス。広域 Issue #6730(文字列 public API)と意図的併存。

### Pure Julia 化: bit 演算の派生関数 (count_zeros/leading_ones/trailing_ones/bitrotate) ✅ (Issue #6722)

- 派生 bit ヘルパを上流 `julia/base/int.jl` 準拠の pure-Julia 定義へ移行(`base/int.jl`)。
  `count_zeros(x)=count_ones(~x)` / `leading_ones(x)=leading_zeros(~x)` /
  `trailing_ones(x)=trailing_zeros(~x)` と、`bitrotate` を `_bitrotate` 共有本体 +
  10 concrete-width dispatch stub(BigInt 除外=上流 MethodError 一致)で実装。
- Rust 撤去: `BuiltinId::{CountZeros,LeadingOnes,TrailingOnes,Bitrotate}` と
  `vm/builtins_math.rs` の 4 handler、`compile/expr/builtin_math.rs` の intercept、
  `builtin.rs`/`base_functions.rs` の登録・テスト・コメント。真 intrinsic
  (`count_ones`/`leading_zeros`/`trailing_zeros`/`bswap`/`bitreverse`)は維持。
- 旧 `Bitrotate` の I64 専用バグ(`bitrotate(UInt8(0b10110001),2)`→`Int64 708`)を修正、
  全 BitInteger 幅で型保存・幅 wrap が上流一致。`CACHE_VERSION` 47→48。
  fixture `numeric/bitrotate_type_preservation_6722.jl`、lib 2878 / full green、clippy clean。
- Tier A スライス。広域 Issue #6726(数値 builtin)と意図的併存。

### 型表現の統合: `CoreType` を `JuliaType↔ConcreteType` 変換のハブ化(Phase 3)✅ (Issue #6599)

- ロードマップ Phase 3(`docs/vm/TYPE_REPRESENTATIONS.md` §4)を着地。`JuliaType↔ConcreteType`
  の直接変換エッジを正準 `CoreType` 経由(`A → CoreType → B`)に寄せ、乖離面を削減。
- **Slice A**: 欠けていた下向きエッジ `impl From<&CoreType> for ConcreteType`
  (`compile/lattice/types.rs`)を新設。`ConcreteType` が表現できないアーム
  (`Bottom→Any`、typevar/`UnionAll`/`Vararg`/value-param の widen、concrete 像のない
  abstract family→`Any`、`Struct{params}`→名前文字列に再埋め込み+`type_id:0`)を
  意図的に lossy とし、各アームを round-trip テストで pin。bare `Array`→`Array{Any}`
  (#5916)を保持。呼び出し元ゼロ(挙動不変)。
- **Slice B**: `julia_type_to_concrete_type_lossy`(`compile/bridge.rs`)を直接 match から
  `ConcreteType::from(&CoreType::from(ty))` へ rerouting。旧 `_ => Any` が捨てていた
  コンテナ型(`Tuple`/`Dict`/`Set`/`Range`/…)が構造を回復する意図的な精度向上(audit で
  正確に 8 件、全て `Any → structured`、回帰なし)。abstract family は concrete 像が無いため
  `Any` のまま。`test_julia_type_to_lattice_agrees_with_concrete_lossy_issue_5916` は green。
- **Slice D(braced struct 精度)は既達のためスキップ**: `concrete_type_to_julia_type` の
  braced parametric struct は既に `from_name_or_struct` 経由で構造回復済み(先行 #6599 PR)。
- 逆方向 `concrete_type_to_julia_type` の CoreType 経由 rerouting は load-bearing な
  reflection 特殊ケース(`DataType` #4843 / `Enum` #2863 / 素の struct 名 / `type_id`)が
  あるため **Phase 4 に明示的に deferred**(follow-up issue で追跡)。
- 検証: full suite(Slice A 3827/3827、Slice B 3828/3828)、clippy `-D warnings` clean。
### Set を pure-Julia `Dict{T,Nothing}` ラッパーへ移行 ✅ (Issue #6721)

- 上流準拠で `struct Set{T} <: AbstractSet{T}; dict::Dict{T,Nothing}; end` を pure-Julia
  化(`base/set.jl`)。core 操作(`push!`/`delete!`/`in`/`empty!`/`length`/`isempty`/
  `iterate`/`pop!`/`copy`/`sizehint!`)は backing `Dict{T,Nothing}`(#6571 で完成)へ委譲。
  `set.jl` の `_set_*` HashSet intrinsic 呼びは全廃。`dict.jl` を `set.jl` より前に
  load するよう `get_base()` の順序を入替。
- **behavioral parity 回復(バグ修正)**: `ft(x::Set{T}) where {T} = T; ft(Set([1,2,3])) == Int64`
  が成立(従来 `MethodError: no method matching ft(::Set)`)。`typeof`/`isa`/`eltype` は
  従来から一致していたが、ユーザ定義パラメトリック `Set{T}` メソッドへ dispatch できなかった。
- 残置(cache 互換 residual、意図的): native `Value::Set`/`SetValue`/`vm/builtins_sets/`/
  10 BuiltinId(`SetNew`/`SetPush`/`SetDelete`/`SetIn`/`SetEmpty` + `_Set*`)+ `NewSet`/
  `SetAdd`/`LoadSet`/`StoreSet` instr。新しい public 経路はこれらを一切生成せず、旧
  bytecode/precompiled cache 互換のためにのみ残す(Dict #6619 と同方針)。`compile_push`/
  `compile_delete`/`compile_empty`/`compile_pop`/`compile_in` 等は Set struct を method
  dispatch へ、native `Value::Set` は legacy fallback へ振り分け。
- テスト: `sets_over_dict_parametric_dispatch_6721.jl`(ft parity + push!/in/length/
  union/intersect/setdiff/eltype/typeof + Set-of-tuples membership + iteration + copy
  独立性)。full suite 3825/3825 green、AoT gate 3765/3765 green、clippy(通常/aot)clean。

### Set のタプル/struct 要素キー(#6693 Set 側)✅ (Issue #6693)

- バグ: `Set([(1,2)])` が「Invalid dictionary key」、`(OneTo(3),) in s` や
  `OneTo(2) in s` が false。native `Value::Set` は要素を `Vec<DictKey>` で持つが
  `DictKey` はスカラ専用で複合キーが無く、struct 要素は heap index で比較されていた。
- 修正(#6624 の Set-wraps-Dict 全面再設計ではなく的を絞った対応):
  - `DictKey::Composite(CompositeDictKey{canonical, original})` を追加(`TypeDictKey`
    と同型: canonical=構造ハッシュ、original=射影用の値)。eq は **canonical +
    `original` の構造 Debug 比較** でハッシュ衝突による誤一致を防止。`original` は
    heap-free 必須。`from_value`/`to_value`/`type_name`/`key_shape`/`KeyShape`/
    `hash_key_shape`/`PartialEq`/`Display`/`format_dict_key` に Composite arm を追加。
  - Set 構築/membership の `DictKey::from_value` 直前で `resolve_value_op_structrefs`
    により heap struct ref を inline 解決(`builtins_sets/shared.rs` の
    `set_key_from_value`、`exec/set.rs` SetAdd はインライン)→ 別構築の等価 struct が
    同一キーに collapse。
  - `In` builtin の Set arm を `matches_value`(raw query を from_value=struct 拒否)
    から、共有 `values_equal` クロージャ(heap 解決+複合 `==`)経由に変更し Array arm
    と統一。
- 構築/membership/dedup/`delete!`/iteration/集合演算(union/intersect/setdiff、pure-Julia)
  が upstream 一致。回帰 `sets/struct_tuple_element_membership_6693.jl` + unit test
  (`composite_dict_keys_compare_and_hash_by_structure_6693` 他)。
- **既知の制限**: 複合キーの厳密なパラメトリック要素型は復元しないため
  `typeof(Set([(1,2)]))` は `Set{Tuple}`(upstream `Set{Tuple{Int64,Int64}}`)と表示。
  fixture では typeof/eltype 文字列は assert していない(別途フォローアップ)。

### `d[k1, k2, ...]` カンマ複数キー Dict 添字 ✅ (Issue #6707)

- バグ: タプルキー Dict の `d[1, 2]`(getindex)/ `d[1, 2] = v`(setindex!)が
  MethodError。upstream は `getindex(t::AbstractDict, k1, k2, ks...) =
  getindex(t, tuple(k1, k2, ks...))`(setindex! も同様)で `d[(1,2)]` と等価。
- 根因: `d[1, 2]` は 2+ の plain index を持つため compile が native 多次元配列
  添字(`IndexLoad(2)` / `IndexStore(2)`)に落ちていた。単一タプルキー `d[(1,2)]`
  は `getindex(::Dict, key)` にディスパッチして動作していた(分岐は index 数依存)。
- 修正: `compile/expr/mod.rs`(getindex)と `compile/stmt.rs`(setindex!)で、
  receiver/target が Dict 系(`ValueType::Dict` / `JuliaType::Dict` / `Dict{...}`
  struct)かつ slice でない index が 2+ の場合に、index を `TupleLiteral` キーへ
  まとめて単一キー `getindex`/`setindex!` をディスパッチ。多次元 **配列** 添字は
  Dict 系判定を通らないため無影響(`A[2,1]` 等は従来どおり)。
- 回帰: `dict/comma_multikey_getindex_setindex_6707.jl`(getindex/setindex!・3要素
  タプルキー・struct 要素キー・配列多次元の無影響、julia 1.12 parity)。

### `===` (egal) を immutable struct で値比較に ✅ (Issue #6709, #6694)

- バグ: `Pt(1,2) === Pt(1,2)` / `Base.OneTo(3) === Base.OneTo(3)` /
  `(Pt(1,2),) === (Pt(1,2),)` が `false`(upstream `true`)。immutable struct も
  heap に `Value::StructRef(idx)` で格納される(#5173 inline 化は一律でない)ため、
  `BuiltinId::Egal` の `(StructRef(a), StructRef(b)) => a == b` arm が heap index
  比較になり、別々に構築した等価 immutable が `===` で不一致になっていた。
- 修正: Egal handler 冒頭で **immutable のみ** を inline snapshot に解決する
  `resolve_immutable_structrefs`(mutable struct は `StructRef` のまま=参照同一性
  を維持)。解決後は immutable は inline `Value::Struct` arm(構造比較)へ、mutable
  は `StructRef` arm(identity)へ落ちるため、tuple 要素も per-element に正しい。
  mutability は `struct_defs[type_id].is_mutable`(parametric struct は `struct_defs`
  に無く `unwrap_or(false)`=immutable で正しく既定化、field-assign の判定と同じ)。
- #6685/#6691/#6693 と同じ StructRef クラス。`(m1,) === (m2,)` 等の mutable
  tuple は参照同一性を保持(別オブジェクトは `false`)。

### StructRef を解決する native value-op を統合(再発防止)✅ (Issue #6694)

- `==`(`TupleEquals`)/`hash`/`_hash`/`in` membership が各々で繰り返していた
  `if contains_structref(..) { resolve_structrefs_deep(..) } else { .. }` idiom を
  単一の正準ヘルパ `resolve_value_op_structrefs(value, heap)`(全 struct 解決、所有
  値で hot-path は move-only=非クローン)+ borrow 版 Cow `resolved_value_op_structrefs`
  に集約。membership は後者経由で hot-path 非クローン化(従来は passthrough でも
  clone)。`===` は前述の mutability-aware `resolve_immutable_structrefs`。
- 再発防止 audit `scripts/check_native_value_ops_resolve_structref.sh`: 4 ヘルパの
  存在を anchor し、`BuiltinId::{Egal,TupleEquals,Hash,_Hash}` の各 arm が resolver
  を呼ぶことを awk で検証(arm から resolver 呼び出しが消えると FAIL)+ `In` が
  `values_equal_for_membership` 経由であることを確認。docs/vm/CODE_AUDITS.md に登録
  (ci.yml stanza は `workflow` スコープ不足のため #4714 同様 maintainer 追記待ち)。
- 回帰: `operators/egal_immutable_struct_6709.jl`(===/==/hash/in を網羅、julia 1.12
  parity)。挙動は behavior-preserving(`===` の immutable 修正を除く)。

### AoT テストを `--features aot` でゲート + AoT clippy 債務一掃 ✅ (Issue #6679)

- 既定ゲート `cargo nextest run --release` は空 feature のため `#[cfg(feature="aot")]`
  コード(`aot` module / `aot_e2e_tests` / `core_ir_aot_tests`)を build/実行しない。
  PR CI が無いため AoT codegen 退行がローカルゲートをすり抜ける(#6629/#5658)。
- `scripts/test_aot.sh`(nextest `--features aot` + clippy `--features aot -D warnings`、
  bash-3.2 安全、`--no-clippy` 可)を追加し CLAUDE.md の Build & Test に手順明記。
- ゲートが検出した AoT 未 lint コードの 43 件の clippy 債務を一掃: `cast_sign_loss`
  はクレート慣例どおり module-level `#![allow]`、scaffolding/helper は `#[allow(dead_code)]`、
  `ptr_arg`→slice、同一 `if` 分岐の merge/allow、`collapsible_match` 等は機械的修正。
  挙動不変、`--features aot` フルスイート 3763/3763 green(PR #6711 で merged)。
### pre-scan 退役: 関数本体スロット型 pre-scan の二重推論を撤去(legacy 削除、capstone) ✅ (Issue #6601)

- `assign_rhs_value_type` seam の catch-all を legacy `infer_value_type_with_structs`
  から共有 `InferenceEngine`(`assign_rhs_value_type_via_engine`)へ flip。corpus
  9 クラス(Var / FunctionRef / Range / UnaryOp / TupleLiteral / FieldAccess /
  Index / BinaryOp / Call)に加え、非 corpus の非リテラル変種(array/dict literal、
  comprehension、ModuleCall、Builtin、ternary、string-concat、`new` 等)も全て
  エンジン経由になり、関数本体スロット型 pre-scan の二重推論(#5922 の主目的)が
  解消。
- legacy `infer_value_type` / `infer_value_type_with_structs` とその専用ヘルパ 4 本
  (`get_struct_name` / `build_type_id_to_name` / `is_range_like_struct_name` /
  `find_complex_type_in_table`)を削除(計 −878 行)。
- リテラル RHS は engine では faithful に表現できず `Top`→`Any` に widen する
  (array-literal local を `Any` にする codegen-specialization hazard)ため、
  driver 側で精密に型付け: 新規 struct-table 対応 `literal_rhs_value_type` が
  deferred literal(array/module/regex/enum/struct)を精密に型付け
  (`ArrayI64`→`ArrayOf(I64)`、`Struct("Complex{Float64}")`→`Struct(id)` 例 `im`)。
  scalar は従来どおり `local_authority` 経由。リテラルは seam に到達しない。
- legacy 比較に依存していた特性化テスト群を直接 engine-value assertion に変換、
  divergence-map スキャフォールド(`prescan_engine_divergence_map_issue_6601` /
  `discovery_divergences_6601` / `is_migrated_assign_rhs_class`)は役目を終えたため削除。
- 検証: full suite 3823/3823(`base_exports_do_not_exceed_upstream` 含む)、AoT gate
  (`scripts/test_aot.sh`)、clippy `-D warnings` clean。事前 probe で非リテラル
  catch-all の engine 化が実 fixture を一切退行させないことを確認済み。

### pre-scan 退役: `Call` Assign-RHS を共有エンジンへ移行(corpus 最終クラス、divergence map 空) ✅ (Issue #6601)

- 関数本体スロット型 pre-scan の `assign_rhs_value_type` seam で、最後に残っていた
  `Expr::Call` クラスを legacy `infer_value_type_with_structs` から共有
  `InferenceEngine` へルーティング。`Call` は *engine-better* クラス
  (`Index` / `BinaryOp` と同じ扱い)。
- 共有 transfer function を 1 箇所修正(ローカル特殊化なし、MAIN コンパイルにも
  反映。`compile/tfuncs/intrinsics.rs`):`tfunc_sqrt`(= `sqrt`/`exp`/`sin`/`cos`/
  `log` が委譲する family)が `Complex{T}` を保存するよう、`is_float` 判定と
  `is_numeric` 判定の間に Complex struct を返す arm を追加。従来 Complex struct は
  `is_numeric` でないため `Top`(→ `ValueType::Any`)に落ちており、`exp(z)` が
  `Any` に divergence していた(`exp(::ComplexF64) === ComplexF64`)。
- `abs(Complex) → F64`(`abs(1.0+2.0im) === Float64`)と `zeros(n) → ArrayOf(F64)`
  (`zeros(3) === Vector{Float64}`)はエンジン側が既に upstream 正確で、legacy が
  imprecise(`ComplexF64` / bare `Array`)だった分をそのまま採用。
- 特性化 pin: `prescan_engine_value_call_issue_6601`(engine 値を直接 assert する
  *value-assertion* pin、RED→GREEN)。`Call` を `is_migrated_assign_rhs_class`
  フィルタへ追加し、divergence map(`prescan_engine_divergence_map_issue_6601`)
  から残る Call 3 行(abs(c): ComplexF64↔F64、exp(z): Any↔Struct、zeros(i):
  ArrayOf↔Array)を削除 → **map は空になった**(corpus の全 Assign-RHS クラスが
  移行完了)。fixture `type_inference/prescan_call_6601.jl`(upstream parity 確認済み。
  raw `exp(Complex)` 値は ULP 差が出るため `typeof` 表示に簡約しつつ `exp`/`abs`/
  `zeros` の 3 Call-RHS スロットは保持)。
- これで #6601 の corpus 移行は完了。残るは catch-all(`_ => legacy`)に届く
  非 corpus `Expr` variant のみで、最終 legacy 削除スライスで catch-all をエンジン
  へ通せば pre-scan 退役が完了する。

### pre-scan 退役: `BinaryOp` Assign-RHS を共有エンジンへ移行 ✅ (Issue #6601)

- 関数本体スロット型 pre-scan の `assign_rhs_value_type` seam で、`Expr::BinaryOp`
  クラスを legacy `infer_value_type_with_structs` から共有 `InferenceEngine`
  へルーティング。`BinaryOp` は *engine-better* クラス(`Index` と同じ扱い)。
- 共有側 transfer function を 2 箇所修正(いずれもローカル特殊化なし、MAIN
  コンパイルにも反映。`compile/tfuncs/arithmetic.rs`):
  - 新規 `tfunc_pow` を `^` に登録(`compile/tfuncs/mod.rs`)。`Int^Int → Int`、
    `String^Int → String`(`"ab"^3` は反復)、それ以外は加算同様の昇格。従来
    `^` は未登録で `i^i` が `Any` に divergence していた(`typeof(2^10) === Int64`)。
  - `tfunc_mul`:`String*String → String`(連結)を加算委譲前に処理。従来は
    String ケースが無く `s*s` が `Any` に divergence(`typeof("ab"*"ab") === String`)。
- Complex 演算(`c+f` / `z+f` / `z*z`)はエンジン側で既に canonical な
  `ComplexF64` を返す(bridge が `Struct{Complex{Float64}}` → `ComplexF64` に
  正準化)。legacy は `F64` / `Struct(100)` を返していた。エンジン変更は不要で
  そのまま採用。
- 特性化 pin: `prescan_engine_value_binaryop_issue_6601`(engine 値を直接 assert
  する *value-assertion* pin、RED→GREEN)。`BinaryOp` を legacy 比較の divergence
  map(`prescan_engine_divergence_map_issue_6601`)から `is_migrated_assign_rhs_class`
  フィルタで除外し、同 map から BinaryOp 5 行(Pow: Any↔I64、Str*Str: Any↔Str、
  ComplexF64+F64 / Struct+F64 / Struct*Struct: ComplexF64↔F64・Struct)を削除。
  fixture `type_inference/prescan_binaryop_6601.jl`(upstream parity 確認済み)。

### pre-scan 退役: `Index` Assign-RHS を共有エンジンへ移行 ✅ (Issue #6601)

- 関数本体スロット型 pre-scan の `assign_rhs_value_type` seam で、`Expr::Index`
  クラスを legacy `infer_value_type_with_structs` から共有 `InferenceEngine`
  (`infer_expr_result` → `bridge::lattice_to_value_type`)へルーティング。
  `Index` は *engine-better* クラス:エンジンは `arr[i]` を正確に要素型
  (`I64`)に推論する(legacy は `Any` を返していた imprecise 経路で、#6601 で
  削除予定)。そのため legacy 比較ではなくエンジンの upstream 正答値を直接
  pin する。
- 前提として共有側の 2 箇所を upstream Julia に一致するよう修正(いずれも
  ローカル特殊化なし、MAIN コンパイルにも反映):
  - `getindex` transfer function(`compile/tfuncs/array_ops.rs`):scalar 整数
    添字の `String[i]` を `Char` に推論(従来は String ケースが無く `Top`/`Any`。
    `typeof("hello"[1]) === Char`)。
  - エンジン `Index` arm の単一 Range/Array 添字スライス(`compile/abstract_interp/engine/mod.rs`):
    `String` スライス `s[1:2]` を `String` に推論(従来は `Array` スライスのみ
    対応で String は `Top`/`Any` に divergence。`typeof("hello"[1:2]) === String`)。
- これにより `arr[i]`→要素型保持、`s[i]`→`Char`、`s[1:2]`→`String` が成立。
- 特性化 pin: `prescan_engine_value_index_issue_6601`(engine 値を直接 assert する
  *value-assertion* pin、RED→GREEN)。`Index` は legacy 比較の divergence map
  (`prescan_engine_divergence_map_issue_6601`)から `is_migrated_assign_rhs_class`
  フィルタで除外(初回導入)し、同 map から Index 2 行(`arr[i]`: I64↔Any、
  `s[i]`: Any↔Char)を削除。corpus に `s[1:2]`(String スライス)を追加。
  fixture `type_inference/prescan_index_6601.jl`(upstream parity 確認済み)。

### Tuple Dict キーの `d[(...)]` 取得と struct 要素キーの一貫ハッシュ ✅ (Issue #6693)

- `d = Dict((1,2)=>10); d[(1,2)]` が `MethodError`、`Dict((OneTo(3),)=>10)` への
  `haskey(d, (OneTo(3),))` が `false`(いずれも upstream は成功)だった。
- 原因 (A) — lowering: 角括弧内の括弧付きタプル `d[(1,2)]` を
  `collect_index_nodes`(`lowering/expr/collection.rs`)が `TupleExpression` も
  spread していたため 2 次元添字 `d[1,2]` と同一に潰され、タプルキーが
  `getindex(::Dict, key)` に届かず native `IndexLoad(2)` に落ちていた。多次元添字
  `A[1,2]` は CST 上 direct children でありタプルを経由しないため、
  `TupleExpression` を spread 対象から外しても配列添字には無影響(upstream でも
  `A[(1,2)]` は単一タプル添字=無効インデックス)。
- 原因 (B) — hash: `Hash` / `_Hash`(`vm/builtins_equality.rs`)が tuple 要素と
  struct を `format!("{:?}")` でハッシュしていたため、heap `StructRef` は heap
  index 文字列になり、別々に構築した等価な struct キーが異なるハッシュになって
  lookup が外れていた(#6685 と同じ StructRef クラス)。ハッシュ前に
  `resolve_structrefs_deep` で構造的に解決(struct ref を含まない場合は no-op)。
- 検証: fixture `dict/tuple_key_getindex_hash_6693.jl`(primitive/1-要素/struct
  要素タプルキーの getindex/setindex!/haskey/get + hash 一貫性, upstream parity
  21 checks)、unit `equal_structs_hash_consistently_after_resolution_6693`。
- 残スコープ(別 follow-up): `Set([(...)])` の複合要素は native `Value::Set` +
  `DictKey`(スカラ専用)に依存し #6624 collections 移行と絡むため対象外。
  `d[1,2]`(カンマ複数キー形式)は upstream の
  `getindex(::AbstractDict, k1, k2, ks...)` 相当 + 抽象 Dict getindex の
  コンパイル経路修正が必要なため別途。

### pre-scan 退役: `FieldAccess` Assign-RHS を共有エンジンへ移行 ✅ (Issue #6601)

- 関数本体スロット型 pre-scan の `assign_rhs_value_type` seam で、`Expr::FieldAccess`
  クラスを legacy `infer_value_type_with_structs` から共有 `InferenceEngine`
  (`infer_expr_result` → `bridge::lattice_to_value_type`)へルーティング。
- 前提として共有エンジンの `FieldAccess` 推論 (`compile/abstract_interp/engine/mod.rs`、
  ローカル特殊化なし)に `Expr` builtin の固定フィールド型の特殊ケースを追加:
  `Expr` は user struct table に載らないため struct-field 経路が見つけられず、従来は
  `head`/`args` が `LatticeType::Top`(→ `ValueType::Any`)へ fall through していた。
  upstream に合わせ `ex.head` → `Symbol`、`ex.args` → `Vector{Any}`
  (`ConcreteType::Array{Any}`)とした。MAIN コンパイルにも反映される意図的な精度向上。
- legacy 側も `Expr.args` を bare `ValueType::Array` から `ArrayOf(Any)`
  (upstream `Vector{Any}`)に揃えて両経路を一致(codegen は `Array | ArrayOf(_)` を
  同一命令で扱い、`ArrayOf(Any)` は boxed numeric 保存も正しく有効化)。
- struct / 未知フィールド / 非 struct / array / Any オブジェクトの各フィールドケースは
  もともと両経路一致。
- 特性化 pin: `prescan_engine_equiv_fieldaccess_issue_6601`(RED→GREEN)、
  `prescan_engine_divergence_map_issue_6601` から FieldAccess 2 行(`ex.head`: Any→Symbol、
  `ex.args`: Any→Array)を削除、`prescan_engine_equiv_migrated_issue_6601` に
  FieldAccess を追加。fixture `type_inference/prescan_fieldaccess_6601.jl`
  (upstream parity 確認済み)。

### pre-scan 退役: `TupleLiteral` Assign-RHS を共有エンジンへ移行 ✅ (Issue #6601)

- 関数本体スロット型 pre-scan の `assign_rhs_value_type` seam で、`Expr::TupleLiteral`
  クラスを legacy `infer_value_type_with_structs` から共有 `InferenceEngine`
  (`infer_expr_result` → `bridge::lattice_to_value_type`)へルーティング。
- 前提として共有エンジンの `TupleLiteral` transfer function を修正
  (`compile/abstract_interp/engine/mod.rs`、ローカル特殊化なし):tuple リテラルは
  upstream Julia では要素型に関わらず常に `Tuple`(`typeof((1, "x", [])) ==
  Tuple{...}`)。従来は非 concrete 要素があると全体を `LatticeType::Top`
  (→ `ValueType::Any`)へ collapse させていたため `(i, a)`(`a::Any`)のスロットが
  `Any` に divergence。非 concrete 要素を `ConcreteType::Any` に widen し、結果を
  常に `ConcreteType::Tuple`(bridge が無条件に `ValueType::Tuple`)とした。MAIN
  コンパイルにも反映される意図的な修正。
- 特性化 pin: `prescan_engine_equiv_tuple_issue_6601`(RED→GREEN)、
  `prescan_engine_divergence_map_issue_6601` から Tuple 1 行を削除、
  `prescan_engine_equiv_migrated_issue_6601` に TupleLiteral を追加。
  fixture `type_inference/prescan_tuple_6601.jl`(upstream parity 確認済み)。

### pre-scan 退役: `UnaryOp` Assign-RHS を共有エンジンへ移行 ✅ (Issue #6601)

- 関数本体スロット型 pre-scan の `assign_rhs_value_type` seam で、`Expr::UnaryOp`
  クラスを legacy `infer_value_type_with_structs` から共有 `InferenceEngine`
  (`infer_expr_result` → `bridge::lattice_to_value_type`)へルーティング。
- 前提として共有エンジンの 2 つの transfer function を upstream Julia / legacy に
  一致するよう修正(`compile/tfuncs/arithmetic.rs`、ローカル特殊化なし):
  - `tfunc_not`(`!`): 論理否定は常に `Bool`(従来は非 Bool concrete を `Top` に
    widen していたため `!i` のスロットが `Any` に divergence。`typeof(!x) === Bool`)。
  - `tfunc_sub`(単項 `-`): concrete 被演算子の型を無条件に保存(従来は
    `is_numeric()` ゲートで、空表エンジンが `Complex{Float64} <: Number` を
    証明できず `-c::ComplexF64` を `Top`/`Any` に widen。`typeof(-(1.0+2.0im)) ===
    ComplexF64`)。MAIN コンパイルにも反映される意図的な精度向上。
- 特性化 pin: `prescan_engine_equiv_unaryop_issue_6601`(RED→GREEN)、
  `prescan_engine_divergence_map_issue_6601` から UnaryOp 2 行を削除、
  `prescan_engine_equiv_migrated_issue_6601` に UnaryOp を追加。
  fixture `type_inference/prescan_unaryop_6601.jl`(upstream parity 確認済み)。

### Tuple `==` が heap struct 要素を値で比較 ✅ (Issue #6685)

- `(OneTo(3),) == (OneTo(3),)` が upstream の `true` に対し sjulia では `false`
  を返していた(`OneTo(3) == OneTo(3)` 単体や `isequal` は正しく `true`)。
- 原因: tuple/named-tuple の `==` は native `TupleEquals` builtin が Rust 側で
  畳み込む(`values_equal_tristate` / `values_isequal`,
  `subset_julia_vm/src/vm/builtins_equality.rs`)。inline `Value::Struct` は
  構造的に比較していたが、`OneTo` のような immutable struct は heap 参照
  `Value::StructRef(idx)` として格納されるため、別々に構築した等価な struct が
  `Debug` 文字列(= heap index)比較に落ち、index 違いで `false` になっていた。
  同じ束縛 `a` を入れた `(a,) == (a,)` は index が一致して `true` になる、という
  identity 依存の症状だった。`isequal` は pure-Julia dispatch で畳み込むため無影響。
- 修正: `TupleEquals` ハンドラ境界で被演算子を `resolve_structrefs_deep` で
  inline struct スナップショットへ解決してから既存の構造比較に渡す。tuple/svec/
  named-tuple 要素と struct フィールドを再帰的に解決するので
  `((OneTo(3),),)` のような入れ子も値で比較される。`contains_structref` の事前
  判定で全要素が primitive な hot path は確保(clone なし)、`visiting` 集合で
  循環 mutable struct も停止する。
- TDD: fixture `tuple/struct_element_equality_6685.jl`(OneTo/UnitRange/Complex/
  named-tuple/入れ子/`axes` 結果/`!=`、upstream Julia 1.12 と一致)+ unit tests
  `vm::builtins_equality::structref_equality_tests`(解決・非解決・入れ子・循環安全)。
- 副産物: `(1,2) in [...]` など tuple の `in`/`∈` は別経路(native `In` builtin の
  `values_equal` クロージャに Tuple arm 無し)で primitive tuple でも `false` を
  返すことを発見 → 別 issue #6691 として起票(本 PR の scope 外)。

### `in` / `∈` over tuple collections が要素を `==` で比較 ✅ (Issue #6691)

- `(1, 2) in [(1, 2), (3, 4)]` が `false`(upstream は `true`)だった。primitive
  tuple でも起きる、#6685 とは別経路のバグ。
- 原因: native `In` builtin(`subset_julia_vm/src/vm/builtins_types.rs`)が独自の
  ローカル `values_equal` クロージャで要素比較しており、scalar/Range/DataType の
  arm しか持たず Tuple/named-tuple/Struct を扱えず `_ => false` に落ちていた。
- 修正: `In` の非スカラー fallback を共有ヘルパー
  `builtins_equality::values_equal_for_membership`(#6685 の `resolve_structrefs_deep`
  + `values_equal_tristate` を再利用し、heap struct ref を解決して `==` を畳む)へ
  ルーティング。これで primitive tuple・named tuple・heap struct 要素(OneTo/
  UnitRange/Complex)が値で比較され、既存の int/str/Range/型メンバシップも維持。
  prevention #6694 の「共有比較ヘルパーへの集約」を一歩前進。
- TDD: fixture `operators/in_tuple_struct_membership_6691.jl`(primitive/named/
  struct/Range/Complex/型、upstream Julia 1.12 と一致)。
- 関連: Dict/Set の struct 入り key は別経路(hash / `DictKey`)で未対応 → #6693。

### `Memory{T}(undef, dims::Tuple)` が 1-tuple 次元を受理 ✅ (Issue #6688)

- `Memory{T}(undef, (n,))` が "Cannot convert Tuple to I64" でコンパイルエラー
  だった(scalar 形 `Memory{T}(undef, n)` は動作)。`Memory` は 1 次元なので
  upstream `base/genericmemory.jl` は `dims` を 1-tuple でも受け取る。
- 原因: `compile_memory_constructor`(`subset_julia_vm/src/compile/expr/collection.rs`)
  が size 引数を直接 `I64` へ coerce していたため、tuple を渡すと失敗していた。
- 修正: `compile_memory_dim_to_i64` ヘルパーを追加。literal 1-tuple `(n,)` は
  コンパイル時に内側要素へ unwrap、`dims::Tuple` 変数(動的 tuple)は実行時に
  `TupleGet`(1-based)で先頭要素を取り出して `DynamicToI64`、それ以外は従来通り
  `I64` へ。multi-element tuple は `Memory` に多次元形が無いため拒否(upstream は
  `MethodError`)。scalar 形は完全に従来の bytecode を維持(回帰なし)。
- TDD: fixture `memory/undef_tuple_dims_6688.jl`(literal/動的 tuple/scalar/
  literal-int-tuple、upstream Julia 1.12 と一致, 値で検証)。
- 副産物: `Memory` の compact `print`/`repr` 表示が upstream と異なる
  (`[1, 2, 3]` ではなく verbose 形、`repr` は空表示)ことを発見 → 別 issue #6697。

### `Memory{T}` の compact print/string/repr 表示 ✅ (Issue #6697)

- `print(m)` が compact `[1, 2, 3]` ではなく verbose 多行形を、`repr(m)` が空の
  `Memory{T}()` を返していた(値レベルの操作は正常)。
- 原因: 表示が複数経路に分散(「multiple formatters」)。`print`/`string`/2 引数
  `show(io, m)` は Rust `format_value`(`vm/formatting.rs`)が verbose
  `format_memory_value` を呼んでおり、`repr`(= `sprint(show, m)`)は pure-Julia の
  generic `show` に落ちて struct 形 `Memory{T}()` になっていた。
- 修正(compact 経路を upstream に一致):
  - `format_value` の Memory arm を、Array wrapper と同じ compact 形を作る
    `format_memory_compact`(typeinfo prefix・空形 `T[]` 込み)に変更。
  - pure-Julia `show(io::IO, m::Memory) = _show_vector_compact(io, m)` を
    `genericmemory.jl` に追加(`show(io, ::Array)` を踏襲)。これで `repr` と
    generic `show(io, x)` over Memory が compact に。
  - 不要になった verbose `format_memory_value` を削除。
- 検証: `print`/`string`/`repr`/2 引数 `show`/空/`Bool`(`Bool[1, 0]`)/`Float64`/
  `Any`(`Any[1, "x"]`)を upstream Julia 1.12 と一致。fixture
  `memory/compact_show_repr_6697.jl`。
- スコープ外: 多行 verbose の `display`(REPL / 3 引数 `show(io, MIME"text/plain", m)`)
  形は別の「display」経路。本 issue は compact 表示が対象。

## 最新対応 (2026-06-14)

### Array literal construction の first increment ✅ (Issue #6649)

- Public untyped array literal と typed empty literal の compiler route を
  `NewMemory` + `MemorySet` + `wrap(Array, mem, dims)` に移行。これにより
  `[1,2,3]` / `[1 2; 3 4]` / `Int64[]` は native array builder ではなく
  Pure Julia `Array{T,N}` wrapper (`ref::MemoryRef{T}`, `size::NTuple{N,Int}`)
  として materialize される。
- Static `ArrayOf(T)` 情報は当面維持し、既存 typed indexing / length / store /
  return fast paths は wrapper-aware VM 境界で動かす。cache 互換の `NewArray*`
  命令は残置。
- TDD: `array/array_literal_struct_routing_6649.jl` と
  `array_construction_routing_6649_tests.rs`。この時点の残スコープだった
  typed non-empty literal / comprehension / empty constructor は後続 increment で対応。

### Array construction routing の typed literal / comprehension increment ✅ (Issue #6649)

- `T[a,b,...]` typed non-empty literal の compiler route を
  `NewArrayTyped` + `PushElemTyped` + `FinalizeArrayTyped` から
  `NewMemory(T,len)` + `MemorySet` + `wrap(Array, mem, dims)` に移行。typed storage
  の narrowing / validation は `MemorySet` 経由で維持する。
- Single comprehension、tuple-destructuring comprehension、multi-iterator
  comprehension の result 初期 materialization を Memory-backed `Array{T,N}`
  wrapper に移行。grow は既存 `ArrayPush` / `ArrayPushTypejoin` を使い、VM 側で
  wrapper `StructRef` の `StoreArray` / `LoadArray` と typejoin widening を補強。
- `Vector{T}()` / `Array{T}()` empty constructor も wrapper route へ移行。
  runtime type parameter (`Vector{T}()` where `T`) は `NewMemoryDynamicTyped` +
  `wrap(Array, mem, (0,))` を使う。
- TDD: `array_construction_remaining_routing_6649.jl` で typed literal、
  comprehensions、empty constructor、`Array{T}(undef, n)`、`zeros` / `fill` /
  `similar` / `trues` / `falses` を確認。bytecode guard は public construction
  body に `NewArray*` / `PushElem*` / `FinalizeArray*` が出ないことを固定。
  direct collect/range materialization は後続 increment、native carrier demotion /
  benchmark は #6653 で完了。

### Array collect/range materialization の wrapper routing increment ✅ (Issue #6649)

- Direct `collect`/range materialization boundary (`collect(1:3)`, integer step
  range, float range, tuple collect, `collect(array)`) の runtime result surface
  を compatibility `Value::NativeArray` から `MemoryRef` backed `Array{T,N}`
  wrapper に移行。
- VM value layer に `ArrayValue -> Array{T,N}` wrapper helper を追加し、
  `collect_iterator` / struct iterator copy path は wrapper を返す。内部で値列が
  必要な `collect_iterator_values` は wrapper を `ArrayValue` snapshot に戻して
  既存 generator/HOF pipeline を維持。
- TDD: `array_collect_wrapper_routing_6649.jl` (upstream Julia parity + sjulia)。
  non-empty generator/HOF-backed `collect(f(x) for x in itr)` の wrapper surface は
  後続 increment、native carrier demotion / benchmark は #6653 で完了。

### Array generator/HOF collect materialization の wrapper routing increment ✅ (Issue #6649)

- `collect(x + 1 for x in 1:3)` の eager generator、`Base.Generator` の
  runtime/named function callable、filtered generator、tuple-splat generator の
  public result surface を `Array{T,N}` wrapper に移行。
- Base の `collect(::Generator)` が選ばれた場合は user-defined method を妨げない範囲で
  VM の `collect_generator` 境界へ戻し、value-mode HOF 完了時は generator collect 用
  フラグで result array を wrapper 化する。通常 broadcast/map 系 HOF は従来の native
  carrier result を維持。
- `get_array_type_id` は bootstrap 中の fallback `0` をキャッシュせず、`Array{Any, Any}`
  struct def を後続 lookup で拾うよう補強。これにより VM 側で生成した wrapper も
  `.ref` / `.size` field access に正しい layout を使う。
- TDD: `array_generator_collect_wrapper_routing_6649.jl` (upstream Julia parity +
  sjulia)。#6649 の public construction/materialization routing scope は完了し、
  final native carrier demotion / benchmark は #6653 で完了。

### Array wrapper indexing/shape の upstream parity 補強 ✅ (Issue #6650)

- `Array{T,N}` wrapper (`ref::MemoryRef{T}`, `size::NTuple{N,Int}`) 上の
  `getindex` / `setindex!` / `length` / `size` / `ndims` / `eltype` は Pure Julia
  method と VM wrapper boundary で既に Memory storage を読む構成。linear /
  cartesian indexing、offset `MemoryRef`、mutation sharing は既存 fixture で固定済み。
- 残っていた `axes` の upstream surface gap を修正。`axes(A)` / `axes(A,d)` は
  `UnitRange` ではなく `OneTo` を返し、0-dimensional `Array{T,0}` では
  `axes(A) == ()`、`axes(A,1) == OneTo(1)` を返す。
- 0-dimensional no-index mutation `setindex!(A, v)` を追加し、
  `getindex(A)` と同じ `MemoryRef` slot を更新する。
- TDD: `array_axes_zero_dim_wrapper_6650.jl` (upstream Julia parity + sjulia)。
  #6650 の indexing/shape scope は完了し、mutation/grow iteration 系は #6651、
  HOF/broadcast/reduce 系は #6652、final native carrier demotion は #6653 で完了。

### Array wrapper mutation/iteration の upstream parity 補強 ✅ (Issue #6651)

- `Array{T,N}` wrapper (`ref::MemoryRef{T}`, `size::NTuple{N,Int}`) の vector
  mutation surface を `MemoryRef` storage 上で upstream-compatible に補強。
  `push!` / `pop!` / `pushfirst!` / `popfirst!` / `insert!` / `deleteat!` /
  `append!` / `resize!` / `empty!` が wrapper `StructRef` の parent storage と
  ref/size を直接更新する。
- offset `MemoryRef` wrapper では parent `Memory` の capacity を使う。
  `push!` は tail capacity、`pushfirst!` は head capacity、`insert!` は tail
  shift、`popfirst!` / `deleteat!(a,1)` は ref offset advance、middle
  `deleteat!` は in-parent left shift で upstream の sharing semantics と一致。
- `iterate(::Array wrapper)` の state を修正し、upstream と同じ
  `(element, next_1based_index)` protocol (`iterate(a) == (a[1], 2)`) に揃えた。
- TDD: `array_memory_mutation_iteration_6651.jl` (upstream Julia parity + sjulia)。
  HOF/broadcast/reduce 系は #6652、final native carrier demotion / benchmark は
  #6653 で完了。

### Array wrapper HOF/broadcast/materialization parity ✅ (Issue #6652)

- `map` / `map!` / `broadcast` / `broadcast!` / `reduce` / `mapreduce` /
  `collect` / `filter` / `filter!` / `sort` / comprehension materialization が、
  `Array{T,N}` wrapper (`ref::MemoryRef{T}` + `size`) source に対して upstream と同じ
  value/shape を返すことを固定。
- public result surface も `MemoryRef` backed `Array` のまま保持する。offset
  `MemoryRef` source、binary `map`、matrix broadcast、filtered comprehension を含めて
  `array_hof_broadcast_wrapper_6652.jl` で upstream Julia parity + sjulia を確認。
- #6652 の HOF/broadcast/materialization scope は完了。final native carrier
  demotion / benchmark は #6653 で完了。

### Array native carrier demotion / benchmark finalization ✅ (Issue #6653)

- 旧 `Value::Array` は既に退役済みのため、残る `Value::NativeArray(ArrayRef)` を
  public Array route から外す final guard として整理。public construction /
  materialization / HOF / broadcast / `similar` / `reshape` は `MemoryRef` backed
  `Array{T,N}` wrapper を返す。`NativeArray` converter / VM instruction handler は
  precompiled cache、VM fallback、formatting/REPL/host boundary の互換境界として残す。
- TDD: `array_native_carrier_demoted_6653.jl` (33 checks, upstream Julia parity +
  sjulia) と `array_construction_routing_6649_tests` の #6653 bytecode guard。
  public materialization body に `NewArray*` / `PushArrayValue` / `AllocUndef*`
  が出ないことを固定。
- `vm_array_benchmark` を追加。短縮 VM-only Criterion では #6649 直前 baseline
  `2404f188e` (同 bench を一時適用)に対し、current は
  `index_mutation_push_pop_128` `7.455 ms` → `25.170 ms` (約 3.4x 遅い)、
  `hof_broadcast_filter_reduce_128` `525.90 ms` → `65.372 ms` (約 8.0x 速い)。
  dict-heavy benchmark は #6622 の `vm_dict_benchmark` と既存 docs を継続参照。
  今後の性能改善は typed Memory storage / intrinsic hot loops で行い、
  native carrier を public default に戻さない。

### AoT が dead な Base helper を `-> Value` で出力する退行(reachability DCE 欠落)✅ (Issue #6629)

- `--features aot` の `aot_e2e_tests::test_aot_e2e_mandelbrot_broadcast_codegen_regression` が
  main で red。`examples/mandelbrot.jl` の AoT 生成 Rust に **29個の `-> Value` 関数**
  (collect/channel/exception 系の Base machinery)が残存し、ガード `!rust_code.contains("-> Value")`
  を破っていた(デフォルトスイートは `--features aot` 無効で検出漏れ)。
- 根本原因: mandelbrot/broadcast/range 関数自体は具象型(`Vec<Vec<i64>>` 等)で正しい。問題は
  **dead な Base helper が無条件出力**されること。Core-IR の `filter_program`(reachability DCE)は
  mandelbrot_grid の Core IR が `broadcast`/`collect` を参照するため collect web を含む 847関数を保持。
  その後 AoT broadcast 特殊化(`__aot_broadcast_*` へ置換)+ inlining で web は dead 化するが、
  **AoT IR レベルの whole-function DCE が無く**、codegen(`generate_program`)が
  `aot_program.functions` を全出力していた(29関数は生成コード中で未呼び出し=dead 定義)。
- 修正: `AotProgram::prune_unreachable_functions`(`aot/ir/aot_types.rs`)を新設し、
  `optimize_aot_program_full` の最終ステップ(inlining/特殊化後)で呼ぶ。entry(main)から
  `CallStatic` + **関数値参照(`Var`)** + `CallDynamic`(全 method)で static 到達閉包を BFS し、
  到達しない関数を prune。**閉包内に `CallDynamic`/`BinOpDynamic` が1つでもあれば prune を中止して
  全保持**(動的ディスパッチプログラムは無退行); 完全 static なプログラム(退行ケース)のみ dead web を除去。
- テスト: 回帰ガード green、`aot::ir::aot_types::prune_tests`(transitive prune / 関数値参照保持 /
  動的ディスパッチ時の全保持)。**full `--features aot` スイート 3728/3728**(prune が他 AoT を壊さない)。
  AoT 専用 feature のため default スイートは無影響。
### Array 移行 B の地ならし: 構造体バック `Array{T,N}`(MemoryRef storage)の表示を修正 ✅ (Issue #6649 / milestone 20)

- milestone 20(Memory{T} 中心コレクション)の Array 移行 B(構築ルーティング)に向けた地ならし。
  構築ルーティングの試作で判明した**構造体バック配列の表示ギャップ**を先行修正した
  (`vm/formatting.rs`)。
- faithful `Array{T,N}` 構造体(#6648)は storage が `ref::MemoryRef{T}` のため、
  `format_array_wrapper_compact` の `Value::Memory` / native 配列分岐のどちらにも当たらず、
  `println(v)` が配列 `[...]` ではなく構造体フィールド
  (`Array{Int64, 1}(MemoryRef{Int64}(index=1), (2,))`)をダンプしていた。`Value::MemoryRef` 分岐を
  追加(`Memory::get` は 1-based、`memref.offset` は 0-based なので `+1`)。
- 併せて要素型名抽出 `array_wrapper_eltype_name` が `Array{Int64, 1}` から `"Int64, 1"`
  (ndims `N` 混入)を返していたのを、ネスト `{}` を考慮した先頭パラメータ抽出へ修正。
  これで構造体バック配列が `[...]` / `Bool[1, 0]` / `Int64[]` と正しく表示される。
- スコープ注記: 公開構築(`T[]` 等)を構造体へ流す**ルーティング本体は本 PR の対象外**。試作で
  `const PROBE = Int[]`(`CallFunctionVariable` 経路が const/global 初期化で未定義化)・base
  exports 数・strings/generated 等に複数のギャップが判明したため、構築ルーティングは後続増分へ。
  本修正はその前提となる表示ギャップを閉じる(#6649 にギャップ一覧を記録)。
- 検証: `vm/formatting` 単体テスト 2 件(MemoryRef storage の `[...]` 表示、eltype 名抽出)。
  `--release` full suite 回帰なし、clippy clean。

### 繰り返し匿名型引数(`::Type{K}, ::Type{V}`)が `where` 型束縛を潰す ✅ (Issue #6661)

- `f(::Type{K}, ::Type{V}, n) where {K,V} = (Memory{K}(undef,n), Memory{V}(undef,n), K, V)`
  を `f(String, Int64, 1)` で呼ぶと、K も V も **Int64**(第2引数の型)に束縛され
  `Memory{Int64}` を返していた(upstream は K=String, V=Int64)。`Dict{K,V}` storage helper
  (#6617)の移植中に発覚。
- 根本原因: `vm/slot.rs` の `build_slot_info` が、匿名 `_` パラメータ(両 `::Type{...}` が
  lowering で `_` になる)を `name_to_slot.entry("_")` で **1スロットに dedup** していた
  (`param_slots = [0, 0, n]`)。その結果 (1) 引数束縛で第2の `_`(Int64)が第1(String)を
  上書きし、(2) `where` 型抽出 `infer_type_binding_from_frame_args` が `param_slots[idx]` で
  スロット値を読むため、K と V を**同じ collapse スロット(=Int64)**から読んで衝突していた。
- 修正: 匿名 `_` は Julia で繰り返し可能・本体から読めない位置パラメータなので、各 `_` に
  **独立スロット**を割り当て(named パラメータは従来通り stable slot に dedup)。`slot_names` が
  真のスロット数になるので `local_slot_count = slot_names.len()` も正しく増え、frame の
  `locals_slots` が正しいサイズになる。`build_slot_types` のサイズ基準も `name_to_slot.len()`
  から総スロット数へ修正(`_` スロットは map に登録されないため)。
- テスト: `dispatch/anonymous_typed_params_where_6661.jl`(2/3個の匿名型引数, mixed, 非Type匿名,
  `Memory{K}/Memory{V}`; julia 1.12 parity 5/5)+ `vm::slot` ユニットテスト2件。full suite 3786/3786。

### `filter(pred, d::Dict)` の結果が `Any` 化し `empty!` が legacy Dict boundary に降格(dispatch順依存) ✅ (Issue #6672)

- `filtered = filter(p -> p.second > 1, d)` の後 `empty!(filtered)` が、native struct-backed
  dispatch ではなく legacy `CallTypedDispatchOrBuiltinStoreDict{builtin: DictEmpty}` を emit し、
  #6621 のガード(`dict_native_demotion_6621_tests`)が **isolation で決定的に失敗 / full-suite では
  実行順により通過**する非決定バグだった(main 由来の既存 RED)。
- 根本原因: `filter` の **call-site 戻り値型推論が Array しか container 型を保持せず**、Dict 受け手だと
  `None` を返して後続の interprocedural 解析(`filter`→`copy(h)`)にフォールバックしていた。この経路は
  depth/fixpoint limit で `Top`(=`Any`)に widen することがあり、`filtered` の型が生成元 `d`
  (`Struct(114)` / `Dict{String,Int64}`)と食い違う。`Any` になると collection-mutation routing
  (`collection_mutation_runtime_candidates`)が runtime 候補 + builtin fallback を選び legacy 化する。
- 修正(upstream 準拠: filter は要素を落とすだけで container 型を保つ): call-site の両推論チャネルで
  dict/set receiver 型を伝播。
  1. `compile/expr/infer/hof.rs` の `infer_filter_call_return_type`(ValueType チャネル): dict/set
     struct receiver の型をそのまま返す。
  2. `compile/expr/infer/julia_type.rs` の `infer_julia_type`: `filter` arm を追加し receiver の
     `Dict{K,V}` / `Set{T}` JuliaType を返す(代入の `julia_type_locals` 追跡ゲートに乗る)。
  3. `compile/abstract_interp/engine/mod.rs` の `infer_filter_return_type`: エンジン層も同じ Dict/Set
     盲点があったため一貫して container 型を返すよう修正。
- 結果 `filtered` は `d` と同一型(`Struct(114)` / `Dict{String,Int64}`)に推論され generic struct
  dispatch へ。override / dict-filter の無い一般経路は不変。
- テスト: `dict/filter_result_native_dict_6672.jl`(filter→empty!/setindex! の挙動, julia 1.12 parity
  2/2)+ 既存 `dict_native_demotion_6621_tests` が isolation(cache クリア)で決定的に green。
  full suite 3784/3784。

### `getindex`(`xs[i]`)(::Any) がユーザー array override に dispatch せず native indexing する ✅ (Issue #6657, getindex 部分)

- `fg(xs::Any) = xs[1]` を、ユーザー override `getindex(::Vector{Int64}, ::Int)=...` を
  持つプログラムで具体 `Vector{Int64}` を渡して呼ぶと、native indexing fast path
  (`IndexLoad`)が override をバイパスして要素値を返していた(upstream julia は override を
  呼ぶ)。3 層が協調しないと解決しない:
  1. **汎用コンパイラ**(`compile/expr/mod.rs` の `Expr::Index` と
     `handlers/arrays.rs` の明示 `getindex` 呼び出し): receiver が真に動的(`Any` 型
     パラメータ)で**ユーザー(非 Base)getindex override が存在**する時のみ、新設
     `BuiltinId::GetIndex` を fallback とする `CallTypedDispatchOrBuiltin(GetIndex, …)`
     を emit(ランタイムで override に dispatch、未一致なら native `IndexLoad` へ)。
  2. **抽象解釈エンジン**(`abstract_interp/engine/mod.rs` の `Expr::Index`): scalar /
     多次元の `xs[i]` 戻り値型を builtin getindex tfunc(要素型)ではなく、method-table
     dispatch の勝者が**ユーザー method の時のみ**その宣言戻り値型で推論(call-site の
     誤った要素型推論を防ぐ)。
  3. **ランタイム関数特殊化器**(`vm/specialize/expr.rs`): ユーザー array getindex
     override が存在する時(`RuntimeCompileContext.disable_array_getindex_specialization`)、
     scalar `xs[i]` の native-indexing fast path を bail し、汎用 body(dispatch 経路)を使う。
- override 識別は origin(`global_index >= base_function_count`)に加え、**自由型変数を持つ
  array シグネチャ**(`Array{T,N}` = Base のシグネチャ形)を候補から除外する堅牢化を実施
  (テストの二重マージ等で base 分類が崩れても Base array getindex を候補に含めず、
  Base getindex body 内の `a[i]` が dispatch 経由で無限再帰するのを防ぐ)。**ユーザー
  override が無い一般プログラムは候補が空 → native fast path 不変**(hot path 退行なし)。
- TDD: フィクスチャ `dispatch/getindex_any_user_method_6657.jl`(julia 1.12 パリティ 7/7)、
  bench `getindex_any_user_override_20000` 追加(Issue #3210)。full suite は
  **3783/3784**(唯一の赤 `dict_native_demotion_6621` は本変更前から main で失敗する既存問題で
  本 PR と無関係)。
- 既知の軽微な差(値は正しい): 多次元 override を `println(m[i,j])` で表示すると seeded-engine の
  Matrix dispatch 精度差で `:sym`(colon 付き)表示になることがある。`==` 等の値レベルは upstream 一致。
### AoT IR 型キャリアを `aot::JuliaType` から `StaticType` へ移行し enum を削除 ✅ (Issue #6598, 残スコープ完遂)

- #6598 の dedup スライス(Array/Matrix 射影の CoreType 経路化、PR #6628)で残っていた
  「`aot::JuliaType` enum 本体の除去」を完遂。AoT の低レベル SSA IR 型キャリア
  (`aot/ir/basic_types.rs` の `VarRef::ty` / `IrFunction::{params,return_type}` /
  `ConstValue::get_type` / `Instruction::TypeAssert::ty`)を `aot::StaticType` へ移行した。
- 消費側を `StaticType` へ更新: `ir_codegen.rs`(`type_to_rust` は `StaticType::to_rust_type`
  に委譲)、`rooting.rs`(`julia_type_requires_rooting_model` → `static_type_requires_rooting_model`、
  保守的に全 heap/aggregate を root: 旧 `Str`/`Array`/`Tuple`/`Struct`/`Any` に加え
  `StaticType` 固有の `Dict`/`Range`/`Function`/`Union` も root。over-root は健全、under-root は
  不健全)、`codegen/cranelift/{helpers,mod}.rs`(`julia_type_to_cranelift` →
  `static_type_to_cranelift`、変種名 `Int64`→`I64` 等)。
- `aot::JuliaType` enum・`impl`(`is_concrete`/`is_numeric`/`to_rust_type`/`primitive_numeric`/
  `Display`)・`From<&aot::JuliaType> for CoreType`(`convert.rs`)・関連単体テストを削除。
- 重要な境界: VM 側 `crate::types::JuliaType` を使う producer/analyzer
  (`analyze/ir_converter` の `julia_type_to_static`、`core_ir_analyzer`、`call_graph`、
  `specialization`、`inference/tests`)は **対象外で温存**。`TypeVar`/`Unknown`/`Symbol`/`Bottom`
  は IR キャリア位置に出現しない(producer が `StaticType::Any` 等へ射影)ことを確認済みで、
  `StaticType` がキャリアの全担当を表現可能。
- 検証: `cargo build --release --features aot` 緑、変更ファイルは clippy clean
  (aot feature 全体の既存 lint debt は標準ゲート外で本変更と無関係)。aot テストスイート
  (`--features aot --no-fail-fast`)で carrier flip / enum 削除とも回帰なし。
### 残 HOF の call-site 推論を tfuncs registry / HofLambdaAnalyzer seam へ移行 ✅ (Issue #6604, 残スコープ)

- `map` 移行(PR #6644)で確立した `TFuncContext::arg_exprs` + `HofLambdaAnalyzer` seam を、
  残る HOF(`broadcast`(binary/n-ary)/`filter`/`reduce`/`foldl`/`foldr`/`mapreduce`/`mapfoldl`/
  `mapfoldr`)の値推論にも適用した。各 HOF の rule を `compile/tfuncs/hof_ops.rs` の free fn
  (`nary_map_call_result` / `filter_call_result` / `reduce_call_result` / `mapreduce_call_result`)
  として切り出し、`compile/expr/infer/hof.rs` のアダプタは element 抽出 → `Array{T}` lattice 構築 →
  rule 呼び出しに整理。lambda/operator の戻り値推論は `CoreCompiler`(`HofLambdaAnalyzer`)へ
  コールバックする。
- `HofLambdaAnalyzer` を拡張: `map_mapped_element_type` を **N 入力要素型**(unary/binary/n-ary)へ
  一般化し、reduce 系のために `reduce_result_type` を追加。reduce-result rule は binary-map の
  数値表が扱わない `^`/`&`/`|`/`xor`/ユーザ定義 `op(acc,elem)` を被覆する(`infer_reduce_operator_return_type`
  を裏方として再利用)。
- 振る舞い保存リファクタ。各 HOF の `Any` 境界の None/Some quirk(binary/n-ary map は
  `Array{Any}` へ widen、unary `map` は入力要素型を保存)を free fn 単位で忠実に再現し、
  `hof_ops` 単体テスト 11 件で固定。
- TDD: 単体テスト先行(RED→GREEN)。新規フィクスチャ `hof/hof_remaining_registry_inference_6604.jl`
  (julia 1.12.6 パリティ 28/28)。`hof::` カテゴリ green、clippy `-D warnings` clean。
- スコープ: engine 側(`abstract_interp/engine/mod.rs`)の並行 HOF 推論は `map` 先例どおり対象外。
  `match function.as_str()` HOF arm 自体の削除は後続。
### `first`/`last`(::Any) がユーザー override に dispatch せず要素型へ coerce する ✅ (Issue #6657, first/last 部分)

- `ff(xs::Any) = first(xs)` を具体 `Vector{Int64}` で呼ぶと、wrapper の戻り値型が
  要素型 `I64` と推論され `StoreSlotI64` が override の非要素値で crash していた
  (`expected I64, got Symbol`)。原因は call-site ごとの単一関数推論エンジン
  (`infer_shared_function_return_type_with_arg_types` → `build_shared_inference_engine(once(func))`)
  が **他関数の method table を持たず**、body 内 `first(xs)` が user override を見えずに
  element-type tfunc(`tfunc_first`)へフォールバックしていたこと。
- 修正(`core_compiler.rs`): エンジン構築後に **user override を含む method table のみ**を
  `seed_initial_method_tables`(安価な `Arc` clone)で seed。body 再推論が override の宣言
  戻り値型を解決できるようになる。Base のみの table は seed せず、non-override 時の tfunc
  fast path(要素型精度)を維持。
- TDD: フィクスチャ `dispatch/first_last_any_user_method_6657.jl`(julia 1.12 パリティ一致、
  4/4)。full suite **3784/3784 green**(新規退行ゼロ)。
- スコープ: #6657 の `getindex`(`xs[1]`)は `IndexLoad` fast path に lower され専用 dispatch
  基盤(getindex builtin)が必要なため #6657 に残置。

### faithful `Array{T,N}` リファクタ(#6648/#6659)由来の 6 fixture 退行を修正 ✅ (Issue #6663)

- #6648 が public Array を faithful `Array{T,N} <: DenseArray`(storage=`ref::MemoryRef`)
  へ移行した際、native-array 消費側が新ストレージ形を扱えず main が 6 fixture RED に
  なっていた(`cargo nextest run --release` の唯一のゲートが崩壊)。全 6 件を修正:
  - `vm/mod.rs::array_wrapper_memory_and_shape` — storage が `Value::MemoryRef`(offset 0、
    `collect` が生成する形)の場合に親 `Memory` を unwrap。`in`/配列等値が collect 結果で
    動くように(collections_set_typed_operations / reflection_type_objectid_hash_dict_key)。
  - `vm/builtins_strings.rs::array_wrapper_chars_to_string` — `MemoryRef` arm を追加。
    `String(collect(...))` 修正(strings_collect_string / types_sizeof_basic)。
  - `base/array.jl` — BitArray family の `size`/`size(_,d)`/`length` メソッド、および
    storage アクセサ `_array_dims`/`_array_offset`/`_array_memory` の `BitArray` メソッドを
    追加。#6648 で rank-parametric `Array{T,N}` メソッドが `BitArray{N}` 名に一致しなくなった
    ため。前者は明示 BitArray 型の call site、後者は broadcast 結果(静的 `Array{Bool}`、実行時
    BitVector)が `Array{T,N}` メソッドへ CallResolve された body 内 `_array_dims` を救う
    (arrays_bitarray_alias_surface / broadcast_bitvector_predicate_broadcast)。
- 検証: full suite **3784/3784 green**(6 RED→0、新規退行ゼロ)、clippy + rustfmt clean。

### `isempty`/`empty!`(::Any) のユーザー override dispatch ✅ (Issue #6610 完了)

- #6610 残りの `isempty`(Bool 返却)/`empty!`(コレクション返却)を完了。haskey と
  同じく、カスタム型で異なる返り値型に override し `Any` 束縛経由で呼ぶと、推論された
  返却型に強制されて `== "..."` が compile 時 `false` へ畳まれていた。
- 根本機構: 関数シグネチャは abstract-interp engine が registry tfunc
  (`infer_return_type`)で算出し、`==` の constant-fold はそのシグネチャを使う。
  両 op とも registry tfunc が受信側型に関係なく返却型を固定していたのが原因。
- 修正(**非対称**、`_mem` 退行回避が肝):
  - `tfuncs/collection_ops.rs::tfunc_isempty` — struct **/ Any / Top(未知)** で `Top`
    defer。`isempty` の結果は Bool 分岐条件で native-array `_mem` に入らないため未知
    受信側を広げても安全。組み込みコレクションは `Bool` 維持。
  - `tfuncs/array_ops.rs::tfunc_empty_bang` — **struct のみ** `Top` defer。`empty!` は
    コレクション自身を返し `_mem` に流れるため、Any/Top を広げると #6648 の faithful
    Array wrapper で `collect("abc")` が `_mem=Any` で壊れる(検証済み)→ struct 限定。
  - `isempty` は expr_tfuncs にも compile handler にも無く registry のみ。`empty!` の
    `compile_empty` は既に user candidate 時 `Any` を返すため追加修正不要。
- 具体コレクション(Array/Dict/String 等)は精度・fast-path とも不変。
- TDD: フィクスチャ `dict/haskey_return_type_defer_6610.jl` を全 3 op + 具体コレクション
  回帰へ拡張、`tfunc_isempty`/`tfunc_empty_bang` の defer unit test 追加。
- upstream(julia 1.12)パリティ一致(4/4)、full suite green。**#6610 クローズ。**

### Array wrapper の faithful `Array{T,N}` foundation ✅ (Issue #6648)

- Pure Julia `Array` wrapper を `Array{T,N} <: DenseArray{T,N}` とし、
  storage field を `ref::MemoryRef{T}`、shape field を `size::NTuple{N,Int}`
  へ移行。`wrap(Array, Memory/MemoryRef, dims)` は rank から `N` を選ぶ
  constructor funnel を通る。
- 既存 native carrier との移行互換として `_mem`/`_size` alias を VM field
  access に閉じ込め、`MemoryRef` storage を `similar`、logical indexing、
  iteration/collect、mutation shrink、linalg wrapper extraction で扱えるようにした。
- Native `reshape(a, dims...)` / `reshape(a, dims::Tuple)` は shared-parent alias を
  保つ VM builtin を使う。runtime dims parser が単一 tuple dims を展開し、logical
  mask reshape の write-through と `prod(...; dims=...)` の runtime keyword dispatch
  を両立。
- public constructor routing は #6649、native carrier demotion / benchmark は
  #6653 で完了。
- 検証: `fixture_tests array::` / `memory::`、`test_array_functions`、
  `array_wrapper_julia_type_uses_native_array_mem_element_type_issue_4340`。

### `Dict{K,V}` Memory-backed storage foundation ✅ (Issue #6617)

- Pure Julia `Dict{K,V}` struct を upstream 形に寄せ、
  `slots::Memory{UInt8}`, `keys::Memory{K}`, `vals::Memory{V}` の typed field
  storage に移行。slot state / shorthash を `UInt8` で扱い、lookup / insert /
  delete / iteration / `empty!` / `rehash!` が typed `Memory` storage を維持する。
- `_new_dict_kv(K, V, n)` helper で `Dict{K,V}` storage を直接構築できるようにし、
  default `_new_dict_kv(n)` は `Dict{Any,Any}` を返す。typed helper の field 型は
  `Memory{UInt8}` / `Memory{K}` / `Memory{V}` として fixture で固定。
- `Dict("x" => 10)` の fixture 期待値を upstream-compatible な
  `Dict{String, Int64}` に更新し、Julia 1.12 で parity 確認。
- 既存バグ #6661(繰り返し匿名 typed parameter が `where` binding を壊す)を起票し、
  helper 引数名の documented workaround を追加。
- 検証: direct `sjulia` smoke、`fixture_tests dict::`、lib `test_dict_functions`、
  `check_workarounds_documented.sh`、`check_workarounds_sync.sh`。

### Generic `Dict` constructors の typed struct narrowing ✅ (Issue #6618)

- Ordinary Julia constructor 経路 `Dict(ps::Pair...)` / `Dict(kv)` を
  `_new_dict_kv(K, V, n)` に接続し、Memory-backed `Dict{K,V}` struct を返すように
  した。Pair splat は `p.first` / `p.second` の runtime 型を `typejoin`、iterable
  constructor は entry の key/value 型を一度走査して `K`/`V` と初期 capacity を
  決める。
- `Dict(pa, pb)` は upstream と同じく narrow integer values を `Signed` へ
  typejoin し、mixed key family は `Any` へ widen。`Dict([("a", 1)])` と
  `Dict(zip(...))` は entry 型から `Dict{String,Int64}` / `Dict{String,Int16}` を
  作り、`keys` / `vals` field は対応する `Memory{K}` / `Memory{V}` を保持する。
- 旧 #6571 parity fixture は literal `Value::Dict` public surface の回帰に絞り、
  struct-backed constructor の型 narrowing は `dict_constructors_6618.jl` で pin。
  full operation parity と public fast path routing は #6620/#6619 に分離。
- 検証: direct `sjulia` / Julia 1.12 parity、`fixture_tests dict::`、lib
  `test_dict_functions`。

### Public `Dict` construction の pure-Julia struct routing ✅ (Issue #6619)

- Public `Dict()` / `Dict(pairs...)` / `Dict(kv)` / `Dict{K,V}(...)` を
  compiler-emitted `NewDict*` fast path ではなく Pure Julia `Dict{K,V}` methods へ
  routing。literal Pair と generator/comprehension construction も Memory-backed
  struct を返す。`NewDict` / `NewDictWithPairs` / `NewDictTyped` は既存 cache/bytecode
  decode 互換のため残す。
- `Dict{K,V}` public typed call は type parameter を `DataType` 値として
  `_dict_from_explicit_types(K, V, ...)` に渡し、runtime の `Dict{String,Int64}` など
  unknown function 化を避ける。
- `Pair{K,V}` JuliaType 推論と Dict constructor tfunc の専用 Pair lattice 変換を追加し、
  `Dict("a"=>1)` の戻り型を `Dict{String,Int64}` struct として slot typing /
  `getindex` / `setindex!` / `pairs` iteration へ伝播。
- struct-backed Dict の必要 public ops を補強: `get!` / `getkey` / `copy` /
  `merge` / `merge!` / `mergewith` / `mergewith!`。`keys` / `values` / `pairs` と
  `empty!` / `delete!` / `pop!` / `merge!` / `get!` の fallback は、Any receiver では
  user override を維持し、compile-time `Dict{K,V}` receiver では Pure Julia Base
  method へ解決する。
- 検証: direct parity fixtures、`fixture_tests dict::`、lib `test_dict_functions`、
  lib `expr_tfuncs`。

### `Dict{K,V}` op/display parity ✅ (Issue #6620)

- struct-backed `Dict{K,V}` の lazy view と public operations を補強。
  `keys` は `KeySet{K}`、`values` は `ValueIterator` を返し、`collect` /
  `length` / `isempty` / membership が upstream-visible に動く。
- `in(::Pair, ::Dict{K,V})`、`filter` / `filter!`、`==` / `isequal` /
  order-insensitive `hash`、compact `repr` / `string` を実装。`hash(d)` と
  local `ks = keys(d); x in ks` は compiler builtin fallback から Pure Julia
  method dispatch へ逃がす。
- fixture `dict_op_display_parity_6620.jl` で views、membership、filter、
  equality/hash/display、mutation reference、mixed Float/Type/Symbol keys、
  rehash lookup を Julia 1.12 と sjulia で固定。
- 残る migration final phase は #6621(native `Value::Dict` / `NewDict*`
  demotion)と #6622(performance benchmark/documentation)。

### native `Value::Dict` / `NewDict*` route demotion ✅ (Issue #6621)

- `Expr::DictLiteral` も `NewDict` + `DictSet` を直接 emit せず、Pair 引数つき
  `Dict(...)` method call として compile。public construction は #6619 と同じ
  Memory-backed `Dict{K,V}` struct route に一本化。
- Public struct-backed Dict 操作の user bytecode に legacy carrier instruction
  (`NewDict*`, `LoadDict`/`StoreDict`/`ReturnDict`) と public `BuiltinId::Dict*`
  fallback が出ないことを `dict_native_demotion_6621_tests` で固定。
- `Value::Dict` / `DictValue` / `_dict_*` intrinsics / public `BuiltinId::Dict*`
  handlers / `NewDict*` decode は旧 bytecode/cache 互換と VM boundary のため残置。
  `BUILTIN_REMOVAL.md` と `DICT_INDEXING.md` もこの分類へ更新。
- 残る final scope は #6622 の performance measurement と docs finalization。

### Pure Julia `Dict{K,V}` VM benchmark と migration finalization ✅ (Issue #6622)

- `vm_dict_benchmark` を追加し、compile 済み bytecode の `Vm::run()` だけで
  typed Int/String key Dict の insert / lookup / iterate / delete /
  post-delete insert(rehash 方向)を測れるようにした。
- 短時間 Criterion 測定では、#6619 直前 `Value::Dict` route が Int keys
  `10.992 ms` / String keys `12.022 ms`、現行 Pure Julia struct route が
  Int keys `73.876 ms` / String keys `48.476 ms`。VM-only regression は
  **6.7x** / **4.0x**。
- この退行は Pure-Julia-First migration の想定内コストとして可視化済み。
  follow-up は native `Value::Dict` default 復帰ではなく、typed `Memory` field
  access、lookup/insert hot helpers、method body specialization に限定する。
- #6571 Dict migration は #6617–#6622 で完了。

### `haskey(::Any)` をユーザー定義の非Bool override に dispatch すると壊れる ✅ (Issue #6610, haskey 部分)

- Bool 返却の Base op `haskey` をカスタム型で非Bool返却に override し、`Any` 型
  束縛経由で呼ぶと、推論された `Bool` 返却に強制されて `ReturnI64` が override の
  値(String)で crash していた。`haskey` は **複数の推論チャネル**で受信側の型に
  関係なく `Bool` に固定されていたのが根本原因:
  1. `tfuncs/collection_ops.rs::tfunc_haskey` — 引数を無視して常に `Bool`
     (registry 経由で abstract-interp engine が関数シグネチャを `-> Bool` に推論)。
  2. `expr/infer/expr_tfuncs.rs` の value/julia general tfunc — `FixedFallback::Bool`
     固定。
  3. `expr/call/handlers/collections.rs::compile_haskey` — 受信側 `Dict|Any` で
     常に `ValueType::Bool` を返す(call-site の式型)。
- 修正: 3チャネルとも「具体 `Dict`/`NamedTuple` 受信側のみ `Bool`、それ以外は
  defer(`Top`/`ConcreteDeferStructAny(Bool)`/`Any`)」へ。`sqrt`/`signbit` の
  `ConcreteDeferStructAny` や `compile_keytype_valtype` の「user candidate があれば
  `Any`」パターンを踏襲。具体 Dict の `haskey` は `Bool` のまま(fast-path 維持)。
- 観測症状は #6539 系の `String vs 非String` 等値 constant-fold: ラッパ
  `call_haskey(x)=haskey(x,"k")` の返り値が `Bool` 推論だと `== "haskey:T"` が
  compile 時に `false` へ畳まれる。`==` site はラッパ本体を再推論するため、本体の
  haskey 推論を直すと畳み込みも解消。
- upstream(julia 1.12)パリティ一致、full suite 3782/3782 green。
- スコープ: #6610 が併記する `isempty`(Bool, Base で多用)/`empty!`(Dict 返却,
  追加チャネル)の override は、より広範な multi-channel 変更のため #6610 に残置。

### `iterate(::Any)` がユーザー定義 `iterate(::Vector{Int64})` に dispatch されない ✅ (Issue #6638)

- `IterateDynamic`(コレクション型が compile 時 `Any`)経路で、native 配列
  (`Value::NativeArray`)が候補スコアリングを完全にバイパスし VM 組み込み
  iterator に直行していたため、ユーザー定義 `iterate(::Vector{Int64})` に
  dispatch できなかった。`collect` の `CallDynamic` 経路は配列もスコアリング
  するため動作しており、この非対称が根本原因。
- 修正: `can_score_iterate_dynamic_candidates` に `Value::NativeArray` を追加。
  ただし #5584(native 配列は VM iterator が既定)を保つため、配列のスコアリングは
  **ユーザー定義候補のみ**(`idx >= base_function_count`)に限定する新ヘルパ
  `scored_iterate_candidates` を resolution パスで使用。Base には Array/Vector を
  受ける `iterate` メソッドが存在しないため、ユーザーが明示的に override した
  場合のみ組み込み iterator を上書きし、それ以外は従来どおり VM iterator が走る。
- upstream(julia 1.12)検証: フィクスチャ `iterate_collect_any_user_methods_4276.jl`
  が 4/4 Pass(`scripts/fixture_julia_parity.sh` でパリティ一致)。
- テスト品質: 当該フィクスチャ末尾の bare `true` が `@testset` 失敗を nextest の
  `dispatch::` 値ゲートからマスクしていたため、実際の dispatch 結果の論理積を
  返す不変条件で締める形へ修正(値回帰が CI に出るようになった)。
- 回帰確認: #5584(`infer_return_type`)、Any 配列 for-loop、override なし配列の
  組み込み iterate(1引数/2引数)すべて従来どおり。full suite 3781/3781 green。

### `LatticeType → JuliaType` 乖離ペアの単一化 ✅ (Issue #6599 部分 / #5916)

- `LatticeType → JuliaType` の構造保存版(`lattice_to_parametric_julia_type`,
  変換表 #14)と文字列不透明版(`lattice_to_julia_type` #15 / 共有コア
  `concrete_type_to_julia_type` #16)の乖離を単一化。braced な
  `ConcreteType::Struct { name }` を共有コアでも `from_name_or_struct` でパースし、
  `Vector{Int64}` → `JuliaType::VectorOf(Int64)` を構造保存(従来は不透明
  `Struct("Vector{Int64}")`)。
- upstream(julia 1.12)検証: `Vector{Int64} === Array{Int64,1}` の具体パラメトリック
  `DataType`、`return_types` も構造化型を返す → 構造保存版が正準。
- bare 名は不透明 `Struct(name)` を維持(保守ゲート)。§3.5 Bottom→Any widening /
  #4679 / Top→Any 禁止は不変。engine `concrete_type_to_julia` は既に委譲済みのため
  1 点修正で全経路一致。
- TDD: pin テスト 2 件(`bridge::…_issue_6599`)を先に red 確認 → 修正で green。
- 残: `ValueType` の `LatticeType` ビュー化(表現削減本体)は #6599 として継続。

### HOF call-site lambda 推論の TransferFn 式参照拡張 + `map` の registry 移行 ✅ (Issue #6604 部分)

- `TransferFn` の入力に式参照チャネルを追加する設計拡張を実装:
  `TFuncContext::arg_exprs`(呼び出しの引数*式*)+ narrow trait
  `HofLambdaAnalyzer`(`compile/tfuncs/registry.rs`)。これにより registry ルールが
  inline lambda の式を解析できるようになり、#6534 が「`TransferFn` では表現不能」と
  文書化していた HOF call-site lambda 推論を registry 経路へ寄せられる。
- proof として `map` を移行: registry ルール `tfuncs/hof_ops.rs::map_call_result`
  が `arg_exprs` / `HofLambdaAnalyzer` 経由で `CoreCompiler` にコールバックし
  lambda の戻り型を推論、`map(x -> x*2.0, Int[])::Vector{Float64}` /
  `map(Float64, Int[])::Vector{Float64}` / `map(abs, Int[])::Vector{Int64}` /
  predicate lambda `::Vector{Bool}` を維持。`infer_map_call_return_type` は薄い
  アダプタ化。analyzer が決められない場合は素の `tfunc_map` にフォールバックし
  挙動は悪化しない。
- 設計は `StructInstantiation`(#5922 wave 5)の `&mut` seam を踏襲: ルール本体は
  式推論アダプタ側に置き(registry-wide dispatch に入れると engine 呼び出しを
  over-match する wave-2 の教訓)、registry はアダプタが明示的に呼ぶ free function。
- 残: `broadcast`/`filter`/`reduce`/`foldl`/`foldr`/`mapreduce` の移行は #6604 として
  継続(式参照チャネルと analyzer seam を再利用予定)。
- 検証: fixture `hof/hof_map_registry_inference_6604.jl`(sjulia/julia 14 passed/0
  failed parity)、unit `compile::tfuncs::hof_ops::tests` 4 件、full fixture 142/142、
  full lib 2811/2811、clippy clean。
### value/name チャネルの wrapper fence を selection core の policy へ吸収 ✅ (Issue #6595)

- native-array wrapper fence(broad-`Any`/`Function` catch-all 判定)を
  `call_dynamic_typed.rs` 私有の free 関数 `typed_dispatch_signature_is_broad_any`
  から、selection core の policy `selection::signature_is_broad_wrapper_fence` へ移管。
  value channel・name channel の両候補集合が同一の policy 権威を共有するようになった。
- value channel の `metadata_best` winner が fence を抜けた broad catch-all のときだけ
  name channel を non-broad 候補で再解決する repair 制御フローを、structured core helper
  `selection::wrapper_fence_name_channel_repair`(metadata winner の broad 判定クロージャ +
  lazy non-broad resolver)へ抽出。`call_dynamic_typed.rs` の `CallTypedDispatch` ハンドラは
  この core helper の薄いアダプタに縮退。
- ハザード #6528(broad `::Function` メソッドが typed 特殊化を上書きして空コレクション
  reduce を壊す)を保存: `signature_is_broad_wrapper_fence` の `Any`/`Function` 判定 +
  repair 経路 + select_typed_dispatch_candidate の non-broad 優先を 9 unit でピン
  (`selection::tests::wrapper_fence_*`)。`hof::` の空 narrow-int/Bool reduce fixture
  (`mapreduce_identity_plus_type_preservation_4619` ほか)を julia 1.12 と完全 parity 確認。
- 検証: `cargo nextest run --release -p subset_julia_vm 'vm::tests::' dispatch_resolver`
  = 157/157、`--lib` = 2749/2749、`fixture_tests hof:: dispatch::` ほか green。
  `cargo clippy --all-targets -- -D warnings` clean。
### pre-scan 退役 2/3: For/ForEach の内部推論を engine 注入へ ✅ (Issue #6602)

- `compile/inference.rs`: `For` 端点 (start/end/step) と `ForEach` iterable のループ
  変数型付けを共有推論エンジン (`InferenceEngine::infer_expr_result`) 経由へ移行。
  エンジンが `Stmt::For`/`Stmt::ForEach` で使うのと同一のラティス補助
  (`range_element_type` / `loop_analysis::element_type`) を再利用し、
  `bridge::lattice_to_value_type` でループ変数型へブリッジ(engine 注入シーム)。
- legacy `infer_value_type_with_structs` + `promote_range_element_value_type`
  並行実装を当該消費者から除去(後者は不要化したため削除)。
- エンジンは struct table + globals シードのみ、関数表なし(置換した pre-scan と同等
  能力)。遅延構築 + 再帰 thread でループ無し関数は追加コスト 0。
- pin テスト 2 本追加: 数値レンジ昇格 (Int64/Float64/混在/局所変数端点)、配列要素型・
  String→Char の ForEach 要素型。
- pre-scan 残消費者(関数本体/inner ctor/main の 2パス化 #6601、globals #6603)は
  別スライス継続。
### CallDynamic family-fallback の文字列 tier を `core_signature` 構造化照合へ ✅ (Issue #6593)

- `CallDynamic` / `CallDynamicOrBuiltin` / `IterateDynamic` の structured
  resolver が使う same-family fallback (`runtime_core_family_fallback_matches`,
  `vm/exec/call_dynamic.rs`) が、各 `CoreType` を `to_julia_name()` で Julia 名
  文字列にレンダリングし直してから `extract_base_type` / `strip_module_prefix`
  で base 名を再パースしていた最後の「文字列 tier」を廃止。
- 新 accessor `CoreType::nominal_family_name(&self) -> Option<&str>` を追加。
  `Struct` / `AbstractUser` / `Named` / `Module` の nominal 変種から、module
  prefix と parametric `{...}` を剥がした bare family 名を **構造化表現から直接**
  読む(共有 `nominal_family_name` ヘルパー経由)。非 nominal 変種は `None`。
  family fallback はこの accessor 同士の比較に置換され、dispatch 毎の String
  確保 + 再パースが消滅。`expected` 側は `core_type_allows_family_fallback` で
  bare `Struct`/`Named` に gate 済みのため挙動は不変。
- 移行前に pin テスト追加(t-wada TDD):
  `nominal_family_name_strips_module_and_params_issue_6593`(module/param
  stripping + 非 nominal 変種が `None` を返すことを pin)。structured family
  fallback テスト `structured_slice_resolver_uses_family_fallback_issue_6502`
  の照合クロージャも `to_julia_name()` round-trip から `nominal_family_name()`
  へ更新。
- 残余の string-encoded resolver(`resolve_runtime_type_pattern_candidates*`,
  `runtime_type_pattern_score*`)は既に `#[cfg(test)]` parity oracle 化済み
  (#6543 slice 2 / `2cbf489ca`)で production 経路からは不参照。本 slice で
  family-fallback hot path から `to_julia_name()` round-trip が完全に消えた。
- `core_signature`/`MethodSig` のシリアライズ形状は不変(CACHE_VERSION bump 不要)。
- 検証: `dispatch_resolver` / `vm::tests::` unit pass、clippy ゼロ警告、
  `rustfmt --check`(変更ファイルのみ)クリーン、`hof::` / `dispatch::` fixture
  pass、dispatch 系 fixture の julia 1.12 パリティ一致。
### User macro expansion-time execution + mutable `Expr.args` ✅ (Issue #6616)

- Replaced user-defined macro static substitution with an upstream-shaped
  expansion-time invocation path using hidden `__source__` / `__module__`
  arguments and unevaluated AST values.
- Added structural conversion from returned runtime AST values back to IR for
  the macro heads exercised by existing macro/metaprogramming fixtures,
  including nested macro calls, `local`, `if`, `for`, `const`, tuple varargs,
  operator calls, and ranges.
- Changed `ExprValue.args` to a shared mutable `Array{Any}` reference so
  `ex.args` aliases the owning `Expr`, matching upstream Core.Expr behavior.
- Added fixtures for Symbolics-style helper-call macros and `Expr.args`
  mutation semantics.

### `comparison.rs` の `Type{<:Bound}` permissive fallback を StructHierarchy で厳密化 ✅ (Issue #6596)

- enum レベルの `JuliaType::is_subtype_of`(`types/julia_type/comparison.rs`)は
  hierarchy を持たないため、`Type{<:Bound}` / bounded-typevar の bound 名が
  `JuliaType::from_name` で解決できない場合(user abstract / bare user struct /
  `Pairs{K,V,I,A}` のような `where`-param 綴り)を **permissive accept** していた。
- 新 API `JuliaType::is_subtype_of_in(&self, other, hierarchy)` を追加し、
  `CoreSubtypeEngine::with_hierarchy` 経由で bound を厳密判定する内部経路
  `is_subtype_of_with_lookup(other, Option<&StructHierarchy>)` を導入。
  hierarchy あり = bound を構造的に判定、hierarchy なし = 従来どおり permissive
  (既存呼び出し元の挙動を完全保存)。
- `Pairs{K,V,I,A}` のように **全パラメータが自由型変数**の bound は upstream
  `Pairs{K,V,I,A} where {K,V,I,A}` と同義なので、bare family 名(`Pairs`)に
  落として hierarchy 判定(`#6251` の `extract_type_bindings` と整合)。
- 知見: 実 dispatch / runtime `<:` / `==` 経路は既に hierarchy-aware
  (`Vm::check_subtype` + method_table の `struct_is_subtype_of_abstract` projection
  fallback + `bind_or_check_julia_type_var_bounded` の `core_is_subtype`)で正しく、
  permissive residue は **enum レベルの低トラフィック compile-time 経路**
  (`LatticeType` ops / union 簡約 / `type_values_subtype`)でのみ latent だった。
  そのため本スライスは厳密化 API の追加と enum メソッドの厳密化(hierarchy 供給時)
  に留め、production caller の再配線は不要(全 dispatch fixture が既に緑のため)。
- 検証: julia 1.12.6 パリティ fixture
  `dispatch/typebound_strict_structhierarchy_6596.jl`(14 passed / 0 failed,
  両インタプリタ一致)+ 新 unit module
  `typebound_hierarchy_strictening_issue_6596`(5 tests)+ フルスイート 3751 緑。
### `expr_tfuncs.rs` pinned adapter divergences の縮小 ✅ (Issue #6600)

- `compile/expr/infer/expr_tfuncs.rs::julia_type_to_lattice`(tfunc 引数 edge)の
  明示 pin を adapter レベルで監査し、**dead だった `TupleOf(_) → Tuple{}` pin を
  削除**(canonical `bridge::julia_type_to_lattice` へ委譲、構造化 `Tuple{…}` 保持)。
- 新監査テスト `pin_audit_load_bearing_arms_diverge_dead_arms_match`: 各 pin arm を
  local pin / canonical 委譲の両方で全 julia-path adapter entry point に通し
  (`#[cfg(test)]` 委譲フック)、final adapter 出力の差分で dead/load-bearing を判定。
  残る全 pin が load-bearing と証明(deferral / `TypeOf → DataType{name}` /
  range / abstract-string・char・array / `Module`・`Function`・`IO`・`IOBuffer`・
  `NamedTuple`・metaprog・`Pairs`・`Generator`・`Enum` は `min`/`max`/`reverse`
  等の出力を変える)。
- `TupleOf` 削除が adapter 不変な理由: julia-path は tuple を
  `julia_type_from_concrete_type` で bare `JuliaType::Tuple` に畳み、唯一 element
  敏感な `length → Const(Int64(n))` も `lattice_to_julia_type` で `Int64` に widen。
- 検証: `type_inference::`(4 chunk green)、`compile::`(1309 green)、
  `expr_tfuncs::tests`(61 green)。#5922 registry 移行と連動。
### `aot::StaticType` の `Array`/`Matrix` 射影を共有 `CoreType` 経由に一本化 ✅ (Issue #6598)

- 変換 #7(`From<&vm::JuliaType> for aot::StaticType`, `aot/types.rs`)の手書き
  fallback から **`Array`(bare)/`MatrixOf` の2アームを削除**。両者は先に走る
  `from_vm_julia_type_lossy`(CoreType 経由)が既に同一の backend 形
  (`Array{element, ndims}`)を生成しており重複していた。`VectorOf` は元々この手書き
  fallback に存在せず CoreType 経路のみで解決されていた(#6579 の方針踏襲)。
- 残った手書きアームは CoreType が意図的に射影しない形のみ(`BigInt`/`BigFloat`→`Any`、
  bare `Tuple`/`Dict`/`Set`、`UnitRange`/`StepRange`→`Range{I64}`、未知 user
  `Struct`、`Enum`→`I32`、抽象族/`Symbol`/`TypeVar`/`Bottom`/`TypeOf`→`Any`、
  `Union` 全 `Any` 畳み込み、`UnionAll` body 展開)。
- `aot::JuliaType` enum 本体は AoT IR の型キャリア(`IrFunction`/`VarRef`/
  `ConstValue::get_type`/cranelift codegen/rooting)として残置。完全削除は IR が
  `CoreType`/`StaticType` を運ぶ構造変更(#6599 領域)を要するため #6598 のスコープ外。
- TDD: 削除前に挙動を pin する
  `aot::types::tests::test_issue_6598_array_projections_route_through_core_type`
  を追加(現行実装で緑であることを確認 → 重複の証明 → アーム削除)。
- 検証: `cargo nextest run --release -p subset_julia_vm --features aot aot::types::tests`
  22/22 緑、`--features aot` フルスイート緑、`aarch64-apple-ios`/`-sim` ビルド成功、
  clippy `--all-targets -- -D warnings` ゼロ警告、`rustfmt --check` クリーン。
### Memory{T}-centric collections 基盤 ✅ (Milestone #20 / Issue #6624)

- `Memory{T}` を唯一の Rust collection 境界とし pure-Julia collections を上に積む
  アーキテクチャ(#6624)の**型システム基盤を実装・マージ**:
  - #6623(PR #6630): `Memory{K}`/`MemoryRef{K}` を parametric struct フィールドに。
  - #6626(PR #6632): `MemoryRef{T}` フィールド + `Memory`/`MemoryRef` 型値(isa/`<:`)。
  - #6625(PR #6633): 整数値型パラメータの値抽出(`N` が `Int` として materialize)。
- PR #6635: pure-Julia `Array{T,N}` over `Memory{T}` のエンドツーエンド動作を fixture で実証。
- 残 #6627(`Value::Array` 完全降格)は大規模 campaign として継続。

### `value_param_base_specificity` の `AbstractUser` 親を構造化照合へ ✅ (Issue #6594)

- value-position scoring 内で `AbstractUser` 親を `JuliaType::from_name(parent)`
  で文字列再パースしていた legacy 経路を廃止し、`CoreType::from(&JuliaType)` が
  既に構造的に保持する `CoreType::AbstractUser { parent }` を直接読む形に統一
  (#6336 / #6543 の binary 経路と同じ authority)。
- 親 boost は新ヘルパー `user_abstract_parent_is_boostable` で構造化判定:
  builtin abstract/concrete 親(`Number`/`Real`/`Integer`/`AbstractVector` …)は
  `parent.specificity()+1`、`Any` / `Bottom` / `Named(_)`(未解決 user 親)/
  `AbstractUser` / `TypeVar` は legacy 同様に flat `AbstractUser` floor (1) を維持。
- 挙動不変を証明するため移行前に pin テスト 2 本を追加:
  `user_abstract_base_specificity_parent_matrix_issue_6594`(親 9 ケースの
  正確な specificity 値)、`user_abstract_and_module_keep_exact_name_tier4_issue_6594`
  (`AbstractUser`/`Module` slot の exact-name tier-4 と子 user struct の
  `CoreSubtypeEngine::with_hierarchy` signature gate)。
- `core_signature`/`MethodSig` のシリアライズ形状は不変(CACHE_VERSION bump 不要)。
- 検証: `dispatch_resolver` 72/72、`vm::tests::` 84/84、`dispatch::` /
  `abstract_tests::` fixture 5/5、clippy ゼロ警告、`rustfmt --check` クリーン。
### `::Function` carve-out (#6512 WORKAROUND) の削除可否を再評価 ✅ (Issue #6597)

- エピック #5915(subtype/型マッチを単一エンジンへ)の残渣。#6512 が runtime
  `::Function` 照合を WORKAROUND 化(`JuliaType::Function => runtime_type ==
  param_type.name()`)していたが、これは既に **PR #6524 で削除済み**で、現状は
  `self.check_subtype(...)` 経由でエンジンに委譲している。後続の f6adade84(#6529)
  ガード(`typed_dispatch_signature_is_broad_any` が `Function` slot を broad 扱い)
  が、空の narrow-int / Bool reduction を型特殊化 Base メソッドに留める。
- #6597 は carve-out が **完全に削除済みかつ安全**であることを再評価・確定した。
  upstream julia 1.12 と全パリティ一致を確認:
  - (a) 直接 callable operator `+` / `*`、(b) `map(+, ...)` / `map(*, ...)`、
    (c) 空 narrow-int / Bool `reduce` / `mapreduce`(#6528/#6529 回帰)。
- 回帰テスト追加: unit `runtime_type_matches_function_param_via_core_subtype_issue_6597`
  (エンジン委譲 + 非 callable 型が `::Function` に誤マッチしない負の保証を pin)、
  fixture `arithmetic/narrow_int_wrapping_5205.jl` に #6597 再評価ブロックを追加。
  WORKAROUNDS.md の #6512 行を実 PR (#6524/#6525/#6529) 付きで Resolved 化。
### Dict pure-Julia 移行エピック完了 ✅ (Issue #6571 / Milestone #18)

- マイルストーン #18 の全子 Issue(#6584–#6589)を解決し、エピック #6571 をクローズ。
  公開 `Dict` サーフェスは dispatch-correct・method-dispatch-first(user/`Struct`)、
  `Value::Dict` は primitive fast-path / cache-compat fallback として残置。
- #6585 修正(PR #6609): bare `ValueType::Dict` の推論ラティス default を
  `Float64`→`Any` に(`compile/bridge.rs`)。#6586/#6587/#6588/#6589 は検証 +
  回帰 fixture + 分類ドキュメント(PR #6611/#6612/#6613)。
- 表現スワップ(`Value::Dict` 撤廃)は perf 退行のため意図的に対象外。一般推論
  エッジケース #6610 は移行スコープ外の standalone bug に分離。

### Dict → pure Julia 移行の基盤整備 + `empty!` ディスパッチ修正 ✅ (Issues #6571, #6584)

- 公開 `Dict` を pure Julia `Dict{K,V}` へ移行するエピック #6571 の基盤 PR。
  全 Dict ハンドラの4分類監査(`BUILTIN_REMOVAL.md`)、セーフティネット
  パリティ fixture、`test_dict_functions()` シグネチャ smoke-test を追加。
- 実バグ #6584 修正: `Any` 型バインディング経由の `empty!(::Dict)` が
  `MethodError` だったのを、bare `empty!(d::Dict) = _dict_empty!(d)` で解消。
- 残作業を #6586–#6589 に分解し、マイルストーン
  *Dict pure-Julia migration (#6571)* で管理(型保存バグ #6585 も起票)。

### テスト保守性: abstract-interp tests.rs 分割 + integration `#[allow]` 集約 ✅ (Issue #6340)

- 7,401 行の単一ファイル
  `compile/abstract_interp/engine/tests.rs` を機能軸で
  `engine/tests/` サブモジュール (mod.rs + 19 トピックファイル) へ分割。
  共有ヘルパー (`dummy_span` / `*_method_sig` / `*_function` など 16 個) を
  `tests/mod.rs` に集約し、各トピックファイルは `use super::*;`(ヘルパー) +
  `use super::super::*;`(engine 内部)の 2 グロブのみで解決。
  分割は `#[test]` 関数の純移動で、テスト名は不変・全 132 件が維持される
  (`cargo nextest list` で確認)。各ファイルは 1,500 行以下。
- 6 つの integration テスト
  (`string_type` / `module_base` / `dict_broadcast` / `array` /
  `compile_sample` / `struct_hof`) で各関数に繰り返されていた
  `#[allow(dead_code)]`(計 640 個)を、ファイル先頭の
  module-level `#![allow(dead_code)]` 1 個へ集約。
- ロジック変更なし。`cargo nextest run --release` 全パス、clippy ゼロ警告。

### Dict non-literal constructors ✅ (Issue #6531)

- Added pure Julia outer constructor methods for non-literal `Dict` inputs and
  routed non-builtin `Dict(...)` call shapes away from default struct field
  construction.
- Verified `Dict(p)`, `Dict(p, q)`, `Dict([p, q])`, and
  `Dict(zip(keys, vals))` against upstream-compatible behavior while preserving
  the existing literal/comprehension `NewDict*` fast path. Full public Dict
  migration is tracked separately as Issue #6571.

### Type/AbstractVector diagonal dispatch via Any ✅ (Issue #6573)

- Fixed the structured runtime typed-dispatch resolver so anonymous covariant
  bounds (`<:Real`) can fall back to subtype matching without weakening named
  diagonal type-variable consistency.
- Added focused CoreType resolver and VM execution tests, and restored the
  existing `dispatch/type_abstract_vector_diagonal_6239.jl` fixture that found
  the regression during Issue #6531 verification.

### Type/AbstractArray rank-TypeVar diagonal dispatch via Any ✅ (Issue #6577)

- Extended the structured runtime typed-dispatch fix to rank-parametric
  abstract arrays, allowing fresh `N` in `AbstractArray{<:Real,N}` to match
  through subtype fallback while preserving rejection for already-bound
  diagonal type variables.
- Restored the existing `dispatch/type_abstract_array_rank_typevar_diagonal_6249.jl`
  fixture and added focused CoreType resolver coverage for the rank-TypeVar
  case.
### Base cache を varint bincode 化して payload を 68% 削減 ✅ (Issue #6453)

- Base cache の `compiled.code` / `compiled.functions` payload の支配要因を profile。
  デフォルトの `bincode::serialize`/`deserialize`(fixint)は **`Instr` enum ごとに
  4byte の u32 discriminant、`usize` オペランドごとに 8byte** を使う。Base は ~78k 命令
  (`LoadSlot` 12460・`PushI64` 6307 など小オペランドが大半)+ ~4.7k functions で、
  この固定幅が payload を支配していた(`code` 1.50MB / `functions` 1.08MB を直接計測)。
- Base cache の全 section を **varint bincode(`cache_codec`)** に切替。`Instr`
  discriminant と小 `usize` を可変長化。デコード結果は bit-identical で **VM セマンティクス
  不変の wire-format 変更のみ**。`allow_trailing_bytes` で version-gate のストリーミング
  読み出し(先頭ヘッダのみ読む `deserialize_from`)を維持。section 枠(`u64` 長さ prefix)は
  raw LE のまま codec 非依存。`CACHE_VERSION` 46→47、persistent namespace v2→v3 で旧
  キャッシュは graceful に miss/recompile。
- 効果(実測): persistent Base cache ファイル **3.12MB → 0.99MB(68.2% 削減)**。
  `code` section -70.3% / `functions` -66.5%。embedded cache(iOS バイナリへ
  `include_bytes!` で埋め込み)では **~2.13MB のバイナリサイズ削減**に直結。デコード CPU は
  varint 化で micro-bench +7.5% だが、persistent/embedded とも読み出しバイト数が 2.13MB
  減るため実 I/O は減少、かつ通常 CLI では warm-prefetch スレッドで critical path 外。
- precompile/cache の round-trip unit 37 件 green。`cargo fmt --check` / clippy ゼロ警告。
  #6440(outer section)/ #6449(sub-section)の follow-up。

### tuple リテラル destructuring swap を tuple 確保なしで lowering ✅ (Issue #6569)

- CPython 3.14 との実測比較で、swap を多用するループが sjulia で CPython の約 9 倍遅い
  ことが判明(原因は毎イテレーションの tuple ヒープ確保)。`lowering/stmt/assignment.rs`
  の `lower_tuple_destructuring_impl` は自己参照型 swap `a, b = b, a % b` を
  `__tmp = (b, a%b); a = __tmp[1]; b = __tmp[2]`(`NewTuple` + `IndexLoad`)に desugar
  していた。
- RHS が **arity 一致の tuple リテラル**で dependent(いずれかの要素が target 参照)の
  場合、各要素を個別の一時変数へ評価してから各 target へ代入する形に変更
  (`__t0 = b; __t1 = a%b; a = __t0; b = __t1`)。全要素を左→右で temp に評価してから
  target を書くため Julia の同時代入セマンティクスを厳密に保ちつつ、`NewTuple`/`IndexLoad`
  を完全に除去。各代入は単純な typed 代入になり specializer でそのまま型安定(#6561 の
  tuple-element 追跡を経由せずに型安定化。mixed 型 swap も各 target が自型を維持:
  `a, s = a+1, s` で `a`→`StoreI64`, `s`→`StoreStr`)。非 tuple リテラル RHS
  (`a, b = f()`)と arity 不一致は従来の temp-tuple + index 経路を維持。independent
  ケースは既存の直接代入 fast-path を維持。
- 効果(VM-only ベンチ `vm_swap_accumulate`, criterion 同一機・静音): `run_only` は
  ~121.5ms(#6561 時点)→ **~14.6ms(約 8.3x 高速化)**。CLI wall-clock の swap ループ
  (2M回)は **0.83s → 0.11s** で **CPython 3.14 と同等**(従来 ~7.5x 遅 → 互角)。
- fixture `tuple/swap_no_tuple_alloc_6569.jl`(2-swap・3-cycle/4要素 rotation・gcd swap・
  mixed 型 swap, julia 1.12 parity)、integration `swap_without_tuple_alloc_6569_tests.rs`
  (構造 3 + lowering 1 + parity 1)で pin。#6561/#6346 の follow-up。

### TypeExpr 表示 / simple-name projection の正準化 ✅ (Issue #5916)

- compile 層に残っていた `type_expr_to_string` helper を削除し、
  `TypeExpr::Display` を parametric instantiation 名、constructor MethodError 表示、
  field type projection の正準表示面として使うようにした。これで `TypeExpr` の
  nested display は compile 層の局所再帰 helper と二重管理されない。
- `Dict{K,V}` / `Set{T}` / `Union{...}` の simple type-name 解決は
  `TypeExpr::as_simple_type_name` へ移動。`Concrete` / `TypeVar` は従来と同じ
  name を返し、`Parameterized` / `RuntimeExpr` は simple-name なしとして
  generic fallback を維持する。
- `TypeExpr` unit に simple-name と nested display の regression を追加し、
  既存 fixture `dict::` / `collections::` / `type_inference::` / `array::` で
  collection / field projection 経路を確認した。
- compile pipeline の struct field table projection も
  `TypeExpr::to_julia_type_lossy` へ委譲。`Union` だけを別 arm にする
  `TypeExpr::to_string` + `JuliaType::from_name_or_struct` の局所 projection を削除した。
- `TypeExpr` パラメータ列表示も `TypeExpr::{render_param_list, format_parameterized}` へ
  集約し、compile context / dynamic call / constructor MethodError /
  collection の局所 `iter().map(TypeExpr::to_string).join(", ")` 実装を削除した。

### AoT struct field の `TypeExpr → StaticType` projection 統合 ✅ (Issue #5916)

- AoT inference engine に残っていた private `type_expr_to_static` match を削除し、
  `StaticType::from_type_expr_lossy` へ移管。`TypeExpr::Display` の Julia type name を
  shared `CoreType` parser / `StaticType::from_julia_name_lossy` に通すことで、
  `Array{Int64,2}` / `Tuple{Int64,String}` / `Union{...}` などの field annotation が
  AoT field table で backend 型に projection されるようになった。
- `Concrete(::Real)` など abstract VM `JuliaType` は従来どおり `Any` へ widen。
  `TypeVar` / runtime expression / projection 不能な user parameterized 型は
  `Struct { name }` fallback を維持し、既存の user-surface 表示は変えない。
- `--features aot` targeted unit で `StaticType::from_type_expr_lossy` と
  `TypeInferenceEngine::analyze_struct` の field projection を pin。

### call-site dispatch cache の IP 直参照 L1 化 ✅ (Issue #6345)

- `Vm` に bytecode と同長の `call_site_caches` を追加し、`CallDynamic` /
  `CallDynamicOrBuiltin` / `IterateDynamic` / `CallDynamicBinary` の cache lookup を
  L1(monomorphic IP-indexed) → L2(既存二段 `dispatch_cache`) → L3(structured resolver)
  の 3 層に整理。L1 hit は exact scalar fingerprint の比較だけで返るため、
  型名文字列生成・`hash_type_name`・`HashMap` lookup を避ける。
- L1 対象は dispatch identity を安全に表せる scalar/singleton 型に限定。
  `Type{T}` / tuple / container / parametric identity は従来どおり L2/L3 に落とし、
  粗い tag 衝突で異なる Julia method を同一視しない。
- L1 positive/negative sentinel/unsupported identity unit を追加し、
  `hot_paths_benchmark` に `CallDynamic` を出す VM-only monomorphic dispatch loop を追加。
  同一 benchmark を当てた `origin/main` 34.17 ms に対して current 30.37 ms
  (criterion mean、約 11% 改善)。

### lazy 特殊化を `FieldAssign` + n-ary 演算子呼び出しへ拡張 ✅ (Issue #6346)

- runtime lazy 特殊化エンジン(`vm/specialize/`)が `obj.field` 読み出し
  (`Expr::FieldAccess`)と可変 struct への `obj.field = value`(`Stmt::FieldAssign`)
  を対象化。`specialize_function` に `&[StructDefInfo]` を渡し、`type_id` から
  フィールド index と型を静的解決して typed `GetField`/`SetField` を発行。代入値は
  インタプリタの `compile_expr_as` と同一命令列でフィールド型へ強制し、parity を保証。
  immutable struct / 未知フィールド / 非 struct は従来どおりフォールバック。
- 連鎖積 `k * b.x * dt`(parser は n-ary `*(k, b.x, dt)` の `Expr::Call` に展開)を
  typed binary-op fold で特殊化対応。`+`/`*` 等の演算子関数呼び出しを左畳み込みし、
  数値以外は generic dispatch へフォールバック。これにより struct フィールド更新を
  含むホットループが丸ごと typed 命令で走るようになった。
- VM-only ベンチ `vm_field_update`(`benchmarks/vm_field_update.jl`)を追加。
  FieldAssign 特殊化 ON で hot loop は GetField=2M/SetField=800k/MulF64=400k・
  by-name field op と CallDynamicBinaryBoth が 0(profiling 計測)。FieldAssign を
  無効化(dynamic フォールバック)した A/B で `run_only` は ~923ms → ~723ms
  (約 22% 高速化)を確認(criterion, 同一機・静音条件)。
- fixture `struct/field_assign_specialization_6346.jl`(7 アサート, julia と parity 一致)、
  specializer unit 10 件、integration `field_assign_specialization_6346_tests.rs`
  (default 4 + profiling 2)で pin。`DestructuringAssign` は lowering が temp+index に
  desugar 済みで現パイプラインでは未生成のため対象外(desugar 後の swap 型安定化は #6561)。

### CoreType array abstract / runtime ReshapedArray dispatch 補強 ✅ (Issue #5915 / #6502)

- `CoreType` 側で `Vector` / `Matrix` / `Array` の `AbstractUser` 親を builtin
  abstract に正規化し、`AbstractVector` が method signature 経由で
  `AbstractUser` として届いても core subtype / dominance が成立するようにした。
- runtime `collect` candidate に `ReshapedArray` を含め、`CallDynamic` 候補選択にも
  signature-wide strict subtype dominance precheck を追加。`collect(::ReshapedArray)`
  と `map(f, ::ReshapedArray)` が generic iterator fallback へ落ちないことを pin。
- `where` 埋め込みで複数文字 type parameter (`MI`) を `Named` から `TypeVar` へ昇格。
  `ReshapedArray{T,1,P,MI}` の `MI` binding が runtime typed dispatch で有効になった。
### desugar 後の destructuring swap を型安定に特殊化 ✅ (Issue #6561)

- 自己参照型の destructuring swap `a, b = b, a % b` は lowering が
  `__tuple_tmp = (b, a % b); a = __tuple_tmp[1]; b = __tuple_tmp[2]` に desugar する。
  従来は `__tuple_tmp[k]`(Tuple への定数 index)が `Any` を返し、swap 先 `a`/`b` が
  `Any` に widen(`StoreAny` + 型不安定な return/downstream)していた。
- runtime lazy 特殊化エンジンに **tuple リテラル一時変数の要素型追跡**を追加。
  `Stmt::Assign` の RHS が `Expr::TupleLiteral` のとき各要素の特殊化型を side table
  (`tuple_element_types`)へ記録し、追跡済み tuple 変数への定数 index `temp[k]` を
  `compile_index`(`try_compile_tracked_tuple_index`)で記録済み要素型に解決して
  `I64`/`F64` のみ typed `Store*` を発行。記録する型は specializer 自身が当該要素式に
  emit した型なので `IndexLoad` が push する `Value` のタグと厳密に一致し、`compile/stmt.rs`
  の `DestructuringAssign` arm のような防御的 `DynamicToI64/F64` coercion は不要(むしろ
  per-iteration オーバーヘッドになり A/B で約 3% 遅くなるため入れない)。非数値要素・
  非定数 index・非追跡 tuple・別値での再代入は generic `Any` 経路を維持。
- 効果: swap 先を **downstream で利用**するパターン(例 swap 後の `s += a`)で真価が出る。
  従来は `a` が `Any` のため `s += a` が `DynamicAdd`(メソッド探索)へ落ち、`s` まで
  `Any` に poison されていたが、型追跡により `AddI64`/`StoreI64` のまま typed 化し return も
  `ReturnI64`。プリミティブが unboxed なこの VM では純粋な swap(gcd 等, swap 先を次の
  swap でしか使わない)は型安定でも実測 neutral だが、downstream 利用では改善する。
- VM-only ベンチ `vm_swap_accumulate`(`benchmarks/vm_swap_accumulate.jl`, untyped
  `swap_sum` を反復し runtime 特殊化を発火・swap 先を `s += a` で利用)を追加。出力は
  upstream Julia と一致(`149905498950`)。A/B(criterion, 同一機・静音)で
  `run_only` は ~123.5ms → ~121.5ms(約 1.6% 高速化, CI 非重複)。tuple 確保が loop を
  支配するため改善幅は穏当。
- fixture `tuple/destructuring_swap_specialization_6561.jl`(GCD/Fibonacci 整数 swap・
  Float64 swap・downstream 利用 swap, julia 1.12 と parity 一致)、specializer unit 4 件、
  integration `destructuring_swap_specialization_6561_tests.rs`(構造 4 + ランタイム
  パリティ 3)で pin。

### `JuliaType::is_subtype_of` built-in family arm 削減 ✅ (Issue #5915)

- enum-level `JuliaType::is_subtype_of` から `AbstractString` / `AbstractChar` /
  `IO` / `Function` / `Type` の局所 family 判定を削除し、先行する
  `CoreSubtypeEngine` の結果をそのまま採用。runtime string subtype と同じ
  core engine にさらに寄せた。
- subtype/type-object unit 119 件と dispatch fixture 2 チャンクで、既存の
  built-in family / `Type{T}` / `UnionAll <: Type` 挙動を pin。

### engine の `LatticeType → JuliaType` 変換 bridge 委譲 ✅ (Issue #5916)

- `compile::abstract_interp::engine` の local `LatticeType` / `ConcreteType` →
  `JuliaType` 変換コピーを正準 `compile::bridge::lattice_to_julia_type` へ委譲。
  engine 側は薄い adapter になり、型表現変換の重複実装をさらに 1 箇所退役した。
- `Pair{K,V}` literal 型名 helper は存置しつつ、型パラメータ名生成も bridge 経由へ統一。
  return cache invalidation と Pair/Dict fixture 周辺は targeted nextest で pin 済み。

### `TypeExpr → JuliaType` projection helper 化 ✅ (Issue #5916)

- `compile/context.rs` と `vm/type_objects.rs` に分散していた `TypeExpr` →
  `JuliaType` projection を `TypeExpr::to_julia_type_lossy` /
  `TypeExpr::substitute_to_julia_type_lossy` に集約。runtime reflection の
  「未束縛 typevar は `Any`」方針は helper 側で保持し、局所 match コピーを削除した。

### import list の演算子名パース ✅ (Issue #6544)

- selective import list の item 開始判定を operator / operator keyword に拡張し、`import Base: *, ==, +` を parser・fixture の両方で pin。比較演算子後のカンマで import list が途切れる既存不具合は解消済み。

### inner constructor `where` 上限境界 enforcement ✅ (Issue #6548)

- 明示型引数つき parametric constructor が inner constructor の `where T<:Real` 上限境界を満たすか compile-time に照合し、不一致時は default constructor へ落とさず catch 可能な `MethodError` を投げるよう修正。`Pos{String}("x")` は upstream Julia と同様に拒否される。

### Base numeric wrapper 推論 snapshot 精度 ✅ (Issue #6547)

- `clamp` / `binomial` / `ndigits` / `widen` / `copysign` に conservative tfunc を追加し、method snapshot が `Any` に広がる wrapper call でも upstream と同じ代表 concrete return type を返すよう修正。fixture と tfunc unit で pin 済み。

### `map(abs, ::Vector{Any})` runtime callable dispatch ✅ (Issue #6550)

- runtime `Generator` specialization が `Vector{Any}` の `Any` element type で `abs(::Any)` fallback を固定しないようにし、各実値ごとの multiple dispatch へ戻した。`map(abs, Any[Holder6550(...)])` は unary user extension を呼ぶ。

### legacy ディスパッチマッチャの CoreType ネイティブ移植 — stages 1–7c-ii-b ✅ (Issue #6495)

- compile-time ディスパッチパイプライン全段(arity 展開 / マッチ / スコア / dominance 事前チェック / tie-breaker / collect・iterate・binary・call・generic-call 候補ヒューリスティクス / 推論キャッシュ無効化)を `core_signature` ネイティブ消費へ移植。新サブモジュール `inference_core/dispatch_resolver/core_match.rs` が legacy マッチャをアーム単位で `CoreType` 上に再実装(#4857/#5383/#5314/#5051/#5050 アーム保存、型名文字列照合なし)
- `MethodSig` 構築を `from_julia_projections`(eager `core_signature`)に一本化(7b)→ structured-unavailable fallback chain を本番退役(7c-i)→ accessor legacy アーム + `legacy_pred` 呼び出し全廃(7c-ii-a)→ `params` を表示用 `param_names` へ置換し `type_params` を削除(7c-ii-b)。Deserialize は投影を再構成せず、production は `core_signature` のみを type source とする
- struct-parents fallback は canonical inverse (`expanded_projected_param_julia_types_for_arity` + `projected_type_params`)を signature source にして、親リンク walk の JuliaType 互換ロジックだけを維持。`sig_param_types` / `*_legacy` オラクルと Bottom-placeholder 投影 fallback テストは撤去
- 各ステージを Base 全コーパスパリティ/恒久ゲートで pin。最終ゲートは accessor-vs-canonical (`base_method_signature_accessors_are_canonical_issue_6495`、serde canonical roundtrip、runtime signature canonical derivation)へ更新。persistent Base cache は wire/CACHE_VERSION を変えず `sjulia_base_cache_v2_<prelude-hash>.bin` へ namespace 分離し、legacy inference snapshot をロード時に破棄。origin/main `3be82144b` 比の `vm_benchmark` / `hot_paths_benchmark` は全項目 >5% 退行なし(最大 +4.4%、改善項目あり)

### callable-value チャネルの `where` 境界 enforcement + `@test` インライン式の等値ミスフォールド修正 ✅ (Issue #6539)

- **callable-value チャネル**: `resolve_callable_value_candidates`(`dispatch_function_variable` 経由の関数変数呼び出し / `CallTypedDispatch` フォールバック)に、明示境界つき `where` パラメータ候補への `core_signature` subtype ゲートを追加(#6543 の CallDynamic* と同じ `Tuple{actuals} <: signature` を共有エンジンで判定)。`f = abs; f(Holder("s"))` が `abs(::Holder{T}) where {T<:Real}` を選ばなくなった。無境界 `where T` はゲート対象外(対角規則 #5050 が既存どおり担当)
- **`@test` インライン形の真因はコンパイル時定数フォールド**(runtime dispatch ではない): `abs(::Any/::Struct)` の ValueType 推論が `Float64` を仮定 → String-vs-非String 等値ショートカットが `abs(hs[2]) == "holder-any"` を `false` に畳んでいた。`abs`/`abs2`/`sign` の legacy フォールバックは Struct/Any/Union 引数で defer するよう修正(JuliaType チャネルは既に defer 済み; Complex の Float64 はレジストリ tfunc が維持)
- **ネスト比較サブバグ**(`@test (a == b) == c`): `==` 結果推論の無条件 `Bool` が同じフォールドを誘発。ユーザープログラムが非 Bool 戻りの 2 引数等値メソッドを定義している場合(`function_ir_by_global_index` で Base/stdlib 起源を除外)、Any オペランドの `==`/`!=` 推論を `Any` に拡張
- fixture `dispatch/callable_value_where_bound_test_inline_6539.jl`(インライン `@test` 形 + 変数束縛コントロール + callable-value チャネル + ネスト比較、`fixture_julia_parity.sh` で julia 1.12 と 8/8 一致)。調査副産物: `map(abs, Any[...])` がユーザー struct で binary `operator` 経路に誤投入される別件を Issue #6550 として起票
### 代入形演算子メソッドの braced `where` 境界脱落を修正 ✅ (Issue #6537)

- `lower_operator_method` の手書き where ループが pure parser の braced 境界ノード形(`BinaryExpression`/`SubtypeConstraint`)を取りこぼし `where {T<:Real}` を `where {T}` 化していた問題を、長形式の WhereClause 処理を共有ヘルパ `parse_where_clause_type_params` に抽出して両経路を一本化することで修正(live `bound` も `upper_bound` と同期、#6518)。param 注釈の typevar 化も非演算子経路と揃えた
- stretch: unbraced `*(a,b) where T<:Real = ...` の parse 失敗(`expected Eq`)も `parse_where_clause` を `parse_type_constraint` ベースに切替えて修正、連鎖 `where T where S` も対応
- inner constructor の WhereClause 処理(`struct_.rs` の第 3 コピー、braced 境界を裸の `<:` テキストとして記録する #5374 同型バグ持ち)も同ヘルパへ一本化。inner ctor 境界の runtime enforcement 欠落は Issue #6548 起票
- lowering unit 9 本 + fixture `dispatch/assignform_operator_where_bounds_6537.jl`(julia parity 15/15)。副産物として `import Base: *, ==, +` の parse バグを Issue #6544 起票

### runtime 候補解決の `core_signature` 構造化照合 (slice 2) ✅ (Issue #6502)

- `inference_core/dispatch_resolver.rs` に構造化リゾルバを追加: `RuntimeCoreCandidate<N>` / `runtime_core_pattern_score`(per-slot、hierarchy-aware `dispatch_pattern_score_in`)/ `resolve_runtime_core_signature_candidates`(`core_signature` ゲート付き max-score first-wins)+ `embed_type_param_bounds` / `runtime_core_signature` / `runtime_candidate_core_type`
- VM 側: `binary_signature_cache` を `RuntimeCandidateCoreSignature`(レンダー名 + per-slot core + 署名ゲート)へ拡張、`check_subtype_core`(`CoreType` 直結の engine subtype)を追加。4 つの動的 dispatch 経路を文字列照合から構造化照合へ移行
- upstream 不一致 3 件を修正 (Issue #6536): パラメトリック struct param の `where` 境界 enforcement / ユーザー抽象境界の構造 tier 維持 / slot 間 typevar 束縛一貫性。fixture `dispatch/runtime_where_bound_enforcement_6536.jl`(sjulia/julia parity 検証済み)+ 構造化 matcher の unit テスト 5 本で pin
- 残余を Issue 化: #6537(代入形演算子の braced `where` 境界が lowering で脱落)、#6539(callable-value / `@test` インライン評価チャネルの境界 enforcement 欠落)

### `CallDynamic` family fallback の構造化照合 ✅ (Issue #6502)

- `CallDynamic` と `IterateDynamic` の残存 fallback tier を、文字列候補 resolver から `RuntimeCoreSliceCandidate` ベースの構造化 resolver へ移行。`usize::MAX` native-iterator sentinel は idx sentinel のまま `CoreType` 候補として採点し、same-family tier は bare `Struct` / `Named` のみに限定
- `runtime_candidate_core_type` の `AbstractUser` / `Module` legacy parse fallback を削除し、`CoreType::from(&JuliaType)` を単一 projection に統一。exact annotation tier は `CoreType` 側の nominal bridge で維持し、子 user struct の structured signature gate は `StructHierarchy` subtype で通す
- 追加 unit: sentinel family fallback が tier 2 で選ばれること、parametric expected が family fallback で通らないこと、`AbstractUser` / `Module` が legacy parse なしで exact/subtype tier を保つことを pin。fixture は `generator::chunk_000` / `iterators::chunk_000` / `hof::*` / `dispatch::*` を検証

### `CallTypedDispatch` 候補キャッシュの構造化 ✅ (Issue #6502)

- `typed_signature_cache` を `RuntimeCandidateCoreSignature` に変更し、`CallTypedDispatch[OrBuiltin*]` の候補側 rendered 名・per-slot `CoreType`・`core_signature` gate を arity ごとに memoize。既存の rendered-name selection は `signature.rendered` を読む互換アダプタになり、次の resolver 置換 slice が候補再投影なしで進められる

### `CallTypedDispatch` production resolver の構造化 ✅ (Issue #6502)

- `CallTypedDispatch[OrBuiltin*]` の候補選択を `RuntimeTypedCoreCandidate` ベースの structured resolver へ移行。candidate matching は cached `CoreType` slots + optional `core_signature` gate で行う。初期 slice で残した rendered specificity tie-break は後続の structured specificity slice で解消済み
- runtime name-search fallback と `metadata_best` 検証も同じ resolver を通すよう統一。`JuliaType::Array` に消えていた rendered `Vector{T}` / `Vector{<:Real}` の parametric array shape は `runtime_candidate_core_type` で structured slot に復元し、Issue #6229 の repeated vector diagonal regression を防ぐ
- unit 2 本(`typed_core_resolver_matches_legacy_string_order_issue_6502`, `typed_core_resolver_keeps_rendered_array_diagonal_issue_6502`)と dispatch/hof/iterators fixture チャンクで pin 済み

### `CallTypedDispatch` specificity tie-break の構造化 ✅ (Issue #6502)

- `resolve_typed_runtime_core_candidates_with_subtype_fallback` の final specificity
  tie-break を rendered type-name helper から `CoreType` slot helper へ移行。
  `core_type_pattern_specificity` が構造 specificity、parametric surface bonus、
  repeated typevar bonus を cached slots から再現し、production typed resolver の
  ordering 互換を保ったまま rendered specificity 依存を退役した。
- unit `typed_core_specificity_matches_rendered_policy_issue_6502` を追加し、
  `Type{T}` / `Type{<:Number}` / repeated `Vector{T}` / `Tuple{}` / `Union{}` /
  `where` surface の legacy score parity を pin。`dispatch_resolver` と VM dispatch
  unit も通過済み。

### `CallTypedDispatch` covariant-bound bridge の CoreType 化 ✅ (Issue #6502)

- `typed_core_candidate_matches_with_subtype_fallback` の covariant-bound fallback から
  `typed_core_covariant_rendered_match` と JuliaType/rendered 名への再投影を削除。
  fallback loop は cached `CoreType` slots を直接 `core_pattern_matches` /
  `subtype_matches` へ渡す。
- `CoreType::TypeVar("_")` は境界 enforcement のみ行い、binding に登録しないようにした。
  これで `Vector{<:Real}` の匿名 covariant slot は複数スロット間の diagonal binding を
  作らず、`Vector{T}` diagonal sibling と同居できる。
- string-only covariant helper は `#[cfg(test)]` parity oracle へ降格。
  unit `typed_core_resolver_uses_covariant_slots_without_rendered_bridge_issue_6502` で
  rendered 名をずらしても structured slot が selection authority になることを pin。

### `CallTypedDispatch` tier split の構造化 ✅ (Issue #6502)

- `resolve_typed_runtime_core_candidates_with_subtype_fallback` の primary/fallback
  tier split を rendered `"<:"` marker scan から `CoreType` slot の explicit bound
  判定へ移行。`core_type_pattern_has_explicit_bound` が `TypeVar` / `TypeOf` /
  parametric container / `UnionAll` 形状を再帰的に見て bounded candidate を
  fallback tier に残す。
- unit `typed_core_resolver_tier_split_uses_bounded_slots_issue_6502` を追加し、
  rendered 名に `<:` marker が無い候補でも bounded slot が tier split の authority
  になることを pin。`dispatch_resolver` と VM dispatch unit も通過済み。

### `CallTypedDispatch` 選択 flow helper 化 ✅ (Issue #6502)

- typed dispatch の final winner ladder を `selection::select_typed_dispatch_candidate` に移管し、非 broad repair / metadata value-channel / compiled name-channel / runtime name-search / fallback index の順序を selection 層で pin。VM 側は runtime search closure を渡す adapter に縮退した
- runtime search は helper 内で lazy に実行され、compiled/metadata が勝つ hot path では scan しない。`typed_dispatch_selection_*` unit 6 本と dispatch/hof/iterators fixture チャンクで検証済み

### legacy string resolver API の production 退役 ✅ (Issue #6502)

- production 参照がなくなった `resolve_type_name_candidates*` / `resolve_runtime_type_pattern_candidates*` / `runtime_type_pattern_score*` を `#[cfg(test)]` 化。旧 string resolver は structured resolver の parity oracle と regression unit 専用になり、production API surface から外れた
- typed-dispatch ordering 互換用の specificity tie-break も structured slots へ移行済み。
  covariant-bound bridge も CoreType 化済み。旧 resolver unit と新しい structured
  specificity/covariant parity unit で test-only 化後も oracle が動くことを確認

### runtime `where` 境界チェックの `StructHierarchy` 化 ✅ (Issue #6502)

- `runtime_value_type_matches_param_with_bindings` に VM の共有 `StructHierarchy` を渡し、
  `where T<:UserAbstract` の上限境界を `CoreSubtypeEngine::with_hierarchy` で enforcement
  するようにした。これで runtime value-channel dispatch もユーザー定義抽象境界を
  compile/core-signature 経路と同じ subtype authority で判定する。
- unit `runtime_where_bound_uses_struct_hierarchy_issue_6502` を追加し、`Dog <: Animal` は
  `where T<:Animal` を満たし、`Int64` は拒否されることを pin。既存 VM dispatch unit と
  dispatch_resolver unit も通過済み。

### 常に throw する関数の戻り型推論を `Union{}` (Bottom) に ✅ (Issue #6532)

- `tfunc_throw`(`compile/tfuncs/intrinsics.rs`、常に `LatticeType::Bottom`)を新設し、`throw`(exact 1)/ `rethrow`(0..=1)/ `error`(0..)を tfunc レジストリへ登録(upstream `add_tfunc(throw, 1, 1, ->Bottom, 0)` 踏襲)。`Base.infer_return_type` が常時 throw 関数で `Union{}`、throw 枝つき分岐で非 throw 枝の型を返すようになり upstream julia 1.12 と一致(fixture `bottom_throw_return_6532` で pin、実行時の throw/catch 挙動は不変)。
- 調査副産物: cached-Base 経路では multi-method Base callee が engine の method/function table 両方から不可視で tfunc レジストリだけが効く(`error` 登録が必須だった理由)。構造的ギャップとして Issue #6538 を起票。
### legacy pre-scan 縮小 wave 6: `use_widening` 死コード削除 + capture 解析の名前専用化 ✅ (Issue #5922)

- 文単位 pre-scan の到達不能な非widening(main/REPL exact types)モードを削除し、widening を
  無条件化(ラッパーも統合)。モジュールレベル lambda capture 解析は束縛名集合しか消費して
  いなかったため、名前専用ウォーカー `collect_local_binding_names_for_capture` に置換し、
  typed pre-scan の消費者を 1 箇所退役(5→4)。式位置の LetBlock 探索は共通ビジター
  `visit_outermost_letblocks` に一本化。等価性は
  `capture_binding_names_match_typed_prescan_keys_issue_5922` で pin。残余の (b) 消費者
  (関数本体 / inner ctor / main のスロット型 pre-scan、For/ForEach ループ変数型、
  globals 収集)は前方参照 widening と `mixed_type_vars` のため存置。
### cached-Base 経路の multi-method Base callee 推論パリティ ✅ (Issue #6538)

- `InferenceEngine::seed_initial_method_tables` を新設し、cached Base `MethodTable` を
  `build_inference_engine` 直後に engine の inference 専用 method table へ丸ごと移植
  (`Arc` 共有で O(#tables)、warm 起動への回帰なし、CACHE_VERSION 据え置き)。
  cached 経路で `mod1` / `factorial` / `flipsign` の `Base.infer_return_type` が
  `Any` → `Int64` となり uncached 経路・upstream julia 1.12 と一致。
  parity pin: `tests/cached_base_inference_parity_6538_tests.rs` + engine unit 2 本。
  残余の両経路共通の snapshot widening 由来不精密は Issue #6547 に切り出し。

## 最新対応 (2026-06-12)

### `julia_type_to_lattice`: `JuliaType::Bottom → LatticeType::Bottom` ✅ (Issue #6523)

- `Union{}` の正準綴り `JuliaType::Bottom` が `_ => Top` に落ちて lattice を反転していた正準コンバータの edge を修正。multi-method callee の `Union{}` return snapshot を join する呼び出し側が `Any` でなく正確な枝型を推論する(upstream 一致、fixture `bottom_return_snapshot_join_6523` で pin)。`LatticeType::Bottom → ValueType::Any` 広化(§3.5)は不変。

### `JuliaType::is_subtype_of` Union/`Type{}` arm の engine 委譲 ✅ (Issue #5915)

- compile-time subtype の Union 分解と `Type{}` invariance を `CoreSubtypeEngine`(`CoreType` solver)へ一本化し local arm を削除。残余は未解決 bound 名の permissive fallback のみ(struct hierarchy 非保持レベルのため)。reverse-parametric quirk による `Type{Vector} <: Type{Vector{Int64}}` 誤 true も upstream 通り false に修正。
### tfuncs 移行 wave 5: パラメトリック struct ctor 解決の adapter 移行 + 変換コピー委譲 ✅ (Issue #5922)

- `infer_expr_type` に残っていた `&mut SharedCompileContext` 依存のコンストラクタ解決
  4 系統(パラメトリック struct ctor / Dict 非builtin-pattern fallback / Rational ctor /
  `{`-instantiated ctor 名)を `expr_tfuncs` アダプタの新設 `StructInstantiation`
  trait(`SharedCtxInstantiation` ラッパ)経由に移行。解決順序は legacy ゲートを
  unit test で pin(exact concrete entry → on-demand instantiation → any instantiation →
  Any / 推論失敗時は base-name id)。wave 2 の教訓どおりルールはレジストリ汎用
  dispatch ではなくアダプタ層にスコープ。
- `expr_tfuncs.rs` の手書き `JuliaType → LatticeType` コピー
  (`concrete_type_from_julia_type`)を削除し、共有 concrete マッピングを正準
  `bridge::julia_type_to_lattice` へ委譲(Issue #5916 cross-credit、§3.6 最終残件)。
  アダプタ固有の divergence(Struct/Signed/Unsigned/Bottom→Top の dispatch-deferral、
  TypeOf→DataType{name}、AbstractString/Range{Any}/TupleOf{} 等の legacy pin)は
  明示 arm + テストで保持。`Bottom→Top` は正準側の Bottom edge(Issue #6523)に
  非依存で pin。Union のみ正準採用(旧コピーの union-loss を修正)。
- HOF call-site 推論(map/filter/reduce/broadcast/mapreduce 系)は「TransferFn は
  引数 lattice 型しか見えず、lambda 本体の式解析が必要」なため tfunc 表現不可と
  コード上に明文化(設計判断、移行対象外)。
- fixture `type_inference/parametric_ctor_resolution_5922.jl`(julia parity 11/11)。
  Dict 非builtin-pattern ctor の end-to-end gap は Issue #6531 で fixture 化して解消。

### lazy specialization `IndexAssign` typed fast path ✅ (Issue #6346)

- `specialize/stmt.rs` が 1D `Vector{Int64}` / `Vector{Float64}` の `a[i] = x`
  を `IndexStoreTyped(1)` として特殊化するようになった。`i::Int64` かつ value
  が要素型一致の場合のみ fast path 化し、型不一致・多次元 index は generic fallback を維持。
- `executable.rs` の typed loop block が array slot と `IndexStoreTyped(1)` を predecode し、
  runtime-specialized 配列書き込み loop を `ExecutableBlock::TypedLoop` で実行できる。
- VM-only Criterion current:
  `timeout 1800 cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark -- --warm-up-time 1 --measurement-time 1 --sample-size 10`
  で `vm_mandelbrot/run_only` median `35.637 ms`,
  `vm_mandelbrot/clone_new_program_run` median `49.616 ms`。これは precomputed bytecode
  の `Vm::run()` 測定で、cold CLI timing ではない。
- テスト: `index_assign_specialization_6346_tests`、`vm::executable::...index_assign..._6346`、
  fixture `arrays/index_assign_specialized_6346.jl` と
  `arrays/index_assign_multidim_fallback_6346.jl`。

### 型表現変換の正準化 wave 4: sibling 所有コピーの委譲統合 ✅ (Issue #5916)

- `type_stability/analyzer.rs`・`abstract_interp/engine/mod.rs`・`vm/builtins_reflection/mod.rs` の手書き `JuliaType → LatticeType` コピー(+ 射影 3 実装)を削除し、正準 `compile/bridge.rs` への thin wrapper 化
- 正準入口を struct-id resolver で一般化(`julia_type_to_lattice_with_struct_resolver`)、要素位置も resolver 対応射影で再帰。resolver 供給時の未解決名(`AbstractDict` 等の抽象族綴り)は engine 従来通り `Top`
- bare `JuliaType::Array` → `Concrete(Any)` の正準側乖離を修正(`Array{Any}` に統一)
- 残: `expr_tfuncs.rs` のコピー(並行 infer 作業の所有領域)

### 残存 ad-hoc 戻り値型ゲートの tfuncs レジストリ移行 ✅ — LinearAlgebra / Dict / collect / rand 系 (Issue #5922)

- `infer_expr_type` の ModuleCall 内ネスト LinearAlgebra match と、`infer_julia_type` の
  ~46行 `match function.as_str()` ブロック + インライン Dict builtin-pattern ゲートを削除し、
  tfuncs レジストリ + `expr_tfuncs` アダプタへ移行(挙動完全保存、legacy 結果は全て unit test で pin)。
- 新設 `tfuncs/linear_algebra_ops.rs`(`LinearAlgebra.` 修飾キーで登録、裸名経路に不干渉)と
  `math_intrinsics::tfunc_rand`(0引数→Float64)。Dict builtin-pattern ゲートは
  `infer_julia_dict_builtin_call` で ValueType 経路と単一共有化し二重ゲートを解消。
- 移行対象外として残るのは `&mut SharedCompileContext` を要するパラメトリック struct
  コンストラクタ、HOF call-site 推論、メソッドテーブルディスパッチ、iterator ラッパ群
  (詳細は STATUS.md 同日セクション)。

### runtime バインディングマッチャーを dispatch_resolver へ移管 ✅ (Issue #5915 / #6502)

- #5915 残件(未委譲マッチャーの移行)のうち runtime 側 2 件を共有モジュールへ移管
  (挙動保存リファクタ、fixture 期待値変更なし):
  - `Vm::value_matches_param_with_bindings`(`vm/mod.rs`)の binding-aware 構造マッチングを
    `inference_core/dispatch_resolver.rs` の
    `runtime_value_type_matches_param_with_bindings` へ移動。VM 側は引数の runtime 型導出と
    値表現 fallback(`value_matches_param`)だけを持つ薄いアダプタに縮退。
    `vm/dispatch_binding.rs` の私有ヘルパー 4 つ(`bind_or_check_runtime_type_var` /
    `runtime_julia_type_contains_type_var` / `runtime_julia_type_mentions_type_params` /
    `runtime_julia_type_needs_array_projection_match`)を削除(contains_type_var は
    resolver 側で公開し `Vm::type_matches` の Tuple ゲートが引き続き利用)。
  - `Vm::check_type_match`(`vm/exec/call_dynamic_typed.rs`)のマッチングポリシーを
    `dispatch_resolver::runtime_type_name_matches_param` へ移動。VM は #5314 の
    leaf-struct 判定(`struct_defs` lookup)とエンジン裏付けの `check_subtype` を
    クロージャで供給するだけの薄いアダプタに。
- `<:` 判定はいずれも `JuliaType::is_subtype_of` / `Vm::check_subtype` 経由で
  `CoreSubtypeEngine` に委譲済み(wave 3)のため、本移管で runtime マッチャーの意味判定が
  すべて inference_core 側に集約された。compile 側 `julia_type_pattern_matches` との
  最終統合と、メソッド選択エントリ枠組み(MethodTable::dispatch / call_dynamic*)の
  一本化は #6502 の後続スライス。

### メソッド選択パイプライン driver を単一コア化 ✅ (Issue #6502)

- `inference_core/selection.rs` に typemap 相当の選択パイプライン driver
  `select_method`(空ゲート → dominance pre-check → conflict ゲート → 最終 scored pick)と
  一般化 winnowing `pick_best` を追加(`pick_max_score` は `pick_best` の thin wrapper に)。
- compile 側 `MethodTable::dispatch_inner` と runtime 側
  `Vm::find_best_method_index_from_candidates` の双方を `select_method` への thin adapter 化
  (挙動保存・fixture 期待値変更なし)。runtime の dominance ladder は
  `runtime_dominance_precheck_index` として compile 側 `dominance_precheck_index` をミラー。
- `vm/mod.rs` の手書き unique-dominant ループ 8 箇所を `selection::unique_dominant_index` へ、
  max-score+vararg-tie fold 2 箇所を `selection::pick_best` へ委譲し削除
  (#6509 の compile 側移行の runtime ミラー)。
- 文字列候補解決の `core_signature` 構造化照合への置換(#6502 step 3)は挙動変更を伴うため
  parity 検証付き別スライスとして deferred(詳細は STATUS.md 同日セクション)。

- #6524(#6512 の `Function` arm エンジン化)直後の main で `reduce(+, Int64[])` /
  `mapreduce(identity, +, Bool[])` などの**リテラル空配列** reduction が
  「reducing over an empty collection is not allowed」で落ちる回帰
  (fixture `hof_mapreduce_identity_plus_type_preservation_4619`)を修正。
- 根本原因: `CallTypedDispatch` の選択は value-channel(`metadata_best`)優先だが、
  Base の typed 特殊化 `reduce(::typeof(+), A::Vector{T})` は native-array wrapper fence
  (#3908/#4189)で value-channel から除外されている。#6524 で `typeof(+) <: Function` が
  エンジン真値になった結果、broad な `reduce(op::Function, itr)` が value-channel の
  唯一のマッチ=unique dominant となり name-channel の正しいランキングを上書き
  (非空でも generic に流れ、空でだけ throw が顕在化)。
- 修正: 同ハンドラ既存の broad-signature ガード `typed_dispatch_signature_is_broad_any` を
  `Function` スロットも broad と数えるよう拡張(`vm/exec/call_dynamic_typed.rs`)。
  非 broad 候補が name-channel で正のスコアでマッチする場合のみ選択が変わるため、
  #6524 の narrow-int fold 修正と `Function` arm のエンジン化はそのまま維持。
- upstream julia 1.12 で fixture の 38 アサーション全 PASS を確認(parity)。

### Callable-value n-ary `+` / `*` fold の narrow integer 対応 ✅ (Issue #6512)

- `vm/narrow_int_arith.rs` に same-type narrow integer arithmetic の共有
  wrap helper を抽出し、binary-both fallback と `dynamic_add` / `dynamic_sub` /
  `dynamic_mul` が同じ modular wrap protocol を使うようにした。
- callable operator value 経由の n-ary `+` / `*` (`f = +; f(Int8(...), ...)`)
  が `Int8` / `Int16` / `Int32` / `UInt8` / `UInt16` / `UInt32` を widening せず
  upstream Julia と同じ narrow 型で wrap。
- `Vm::type_matches` の `::Function` exact-name workaround を撤去し、
  `typeof(+) <: Function` を engine-backed subtype 判定で true にした。
- fixture `arithmetic_narrow_int_wrapping_5205` に direct callable `+` / `*` と
  `map(+, ...)` / `map(*, ...)` 回帰を追加。`convert(Int8, 300)` の
  InexactError(#5192) は維持。

### `TypeParam.bound` の serde 復元ずれを修正 ✅ (`Complex{T} where T<:Real` / isnan・isinf・transpose) (Issue #6518)

- `TypeParam.bound`(`upper_bound` の legacy ミラー)が `#[serde(skip)]` のため、prelude
  `Program` の bincode round-trip で `None` に落ち、`where T<:Real` 制約が消えていた潜在バグを修正。
- `bound` はライブフィールド(`types/julia_type/comparison.rs` のパラメトリック束縛ディスパッチ /
  `compile/context.rs` のパラメトリック構造体 bound 検査が参照)。`TypeParam` に手書き
  `Deserialize` を実装し、デシリアライズ時に `bound` を `upper_bound` から再構築
  (`types/type_param.rs`)。全 production コンストラクタ(`with_upper_bound` 等)と
  `core_signature` 復元(`core_type_var_to_type_param`)の `bound == upper_bound` 不変量に揃え、
  prelude-program / method-table キャッシュが透過に。
- `base_method_tables_serde_roundtrip_reconstructs_projections_issue_6336` がキャッシュ再生成直後の
  fresh-cache 単体実行で安定 PASS。julia 1.12 と挙動 parity(回帰 fixture
  `complex/complex_isnan_isinf_bound_6518.jl`)。

### runtime `<:` を CoreSubtypeEngine 単一 authority 化・legacy fallback 全廃 ✅ (Issue #5915 wave 3)

- `Vm::check_subtype` をエンジン直結に。名義鎖 walk / `type_ancestors` walk /
  Union 分解 / Tuple 共変 walk / where 名義再入 / bare-nominal 文字列一致 /
  `check_parametric_typevar_match` の全 legacy fallback と、authoritative ゲート機構
  (~40 関数)+ 文字列ヒューリスティックを削除(`vm/type_ops/comparison.rs`)。
- エンジン拡張で gap を解消(本家 julia 1.12 parity):`(Named, Abstract)` アーム
  (ユーザ名義型 → built-in abstract 鎖、`struct Money <: Real` 等)と、matcher の
  宣言親 walk(`MyVec{Int64} <: (Wrapper{S} where S)` で `S` 束縛)。
- 残置 carve-out は SubArray arity-gate(VM Base 1-D キャリアタグ依存の
  dispatch 乖離)のみ。`::Function` exact-name(#6512)は n-ary fold 修正後に撤去済み。
  SubArray は subtype エンジンのギャップではない。
- テスト: engine 単体(`inference_core/subtype.rs`)+ `check_subtype` 回帰
  (`test_check_subtype_engine_is_sole_authority_issue_5915`)+ fixture
  (`types/user_type_builtin_abstract_subtype_5915.jl`、julia と出力一致)。
- 監査結果: runtime 文字列パスは単一 authority 達成。コンパイル時
  `JuliaType::is_subtype_of`(sibling scope #5916)は薄い局所アーム残存のため
  **Part of #5915**。
### CallDynamic / CallTypedDispatch 候補ペイロードの構造化 ✅ (Issue #6496)

- #6336 シリーズ第6弾(CACHE_VERSION 45→46)。動的ディスパッチ系 `Instr`
  ペイロードの焼き込み型名文字列を構造化データへ置換:
  - `CallDynamic`: `DynamicCallCandidate::Method(usize)` /
    `NativeIterator(NativeIteratorKind::{Zip..Zip7, Generator})`(旧
    `(usize::MAX, 型名文字列)` sentinel の enum 化)。
  - `CallDynamicOrBuiltin` / `CallDynamicBinary` / `CallDynamicBinaryBoth` /
    `CallDynamicBinaryNoFallback` / `CallTypedDispatch` /
    `CallTypedDispatchOrBuiltin[Result]` / `TypedDispatchStoreDict`:
    候補関数 index のみの `Vec<usize>`。
- ランタイムは `vm::derived_runtime_signature`(`FunctionInfo` ベース)で
  従来文字列を導出して既存の文字列リゾルバ・値-vs-型名フィルタへ渡すため、
  ディスパッチ結果は不変。歴史的焼き込みとの同値性は Base 全コーパス
  パリティゲート `base_method_runtime_signature_derivation_parity_issue_6496`
  で恒久 pin。call-site dispatch cache の無い binary/typed 系は
  `Vm::binary_signature_cache` / `Vm::typed_signature_cache`((func_index,
  arity) キー、Rc 共有)で導出を memoize。
- `MethodSig::runtime_type_names_for_arity` を削除し、emit 側 arity ゲートは
  新設 `MethodSig::accepts_arity`、abstract-array-family ヒューリスティクス
  (dispatch.rs / module_call.rs)は `param_type_at_call_position` へ移行。
  検証: clippy 0 警告、--lib 2653 / fixture_tests 142 全パス + フルスイート。

### 型表現変換の正準化 wave 3: `JuliaType → LatticeType` 正準実装の確立 ✅ (Issue #5916)

- `compile/bridge.rs` に `JuliaType → LatticeType` の正準変換
  `julia_type_to_lattice` / `julia_type_to_lattice_with_struct_table` を新設。
  4 つの並行実装の食い違いを upstream Julia 準拠に解決(空 `Union{}` →
  `Bottom`、union∋`Any` → `Top`、抽象数値の保持、構造体 `type_id` のテーブル
  解決をパラメータ化)。
- `julia_type_to_concrete_type_lossy` を `pub(crate)` 化(sibling 到達可能)。
  要素変換を共有することで lattice 方向と ConcreteType 方向の乖離を防止。
- 新規テスト 6 件(`bridge::test_julia_type_to_lattice_*_issue_5916`)で
  upstream 準拠の挙動と両方向の一致をピン留め。
- sibling 所有の 4 呼び出し箇所(`analyzer.rs:696` / `engine/mod.rs:4354` /
  `expr_tfuncs.rs:695` / `builtins_reflection/mod.rs:2454`)の委譲は scope 外の
  ため次 wave に deferred(`TYPE_REPRESENTATIONS.md` §3.6 に明記)。

### tfunc レジストリ構造体コンテキスト拡張 + complex/Dict/struct ctor ゲート移行 ✅ (Issue #5922)

- `StructIdLookup` トレイト + `TFuncContext::with_struct_ids` で、レジストリの
  contextual tfunc が構造体名 → `type_id` を解決可能に(エンジン側
  `StructTypeInfo` テーブルと式推論側 `SharedCompileContext` の両方が実装)。
- `infer/mod.rs` の `complex` / builtin パターン `Dict` / 非パラメトリック
  struct constructor の 3 ゲートと `julia_type.rs` の `complex` ゲートを
  レジストリ経由(`tfunc_complex_contextual` / `Dict` 規則 /
  `struct_constructor_result`)へ移行。レガシー挙動
  (`ValueType::Struct(type_id)` 維持、Complex{Float64} フォールバック)は
  アダプタでピン留め。パラメトリック instantiation と HOF/LinearAlgebra
  ゲートは未移行(シームをコメントで明示)。
- `struct_constructor_result` の汎用ディスパッチ適用は不採用(エンジンは
  `Base.Generator` のような builtin 表現 struct に `Top` を要する。
  fixture `generator_runtime_callable_constructor` で検出、単体テストでピン留め)。

### wave 3: pre-scan literal 局所型の共有 authority 一本化 ✅ (Issue #5922)

- 新モジュール `compile/abstract_interp/local_authority.rs` に
  `literal_to_lattice`(`Literal -> LatticeType` の単一真実)を追加。
  エンジン `InferenceEngine::infer_literal` がこれに委譲し、pre-scan
  (`collect_local_types_for_inference_with_mixed_tracking`)の literal 代入も
  `literal_assignment_value_type`(= `literal_to_lattice` + bridge)経由に。
  literal 右辺の二重推論を解消(両パスが同一関数を共有)。
- 移行 literal: `Int/Int128/BigInt/BigFloat/Float/Float32/Float16/Bool/
  String/Char/Nothing/Missing/Symbol`(13/24 variant)。round-trip 同値を
  単体テスト 3 本でピン留め。残り 11 variant(配列/struct/module/regex/enum/
  AST 系)は格子非忠実につき `None` を返しレガシー struct-aware 推論へ委譲
  (`Top`→`Any` の暗黙ワイドニング禁止)。
- 非 literal 式クラスは全て旧 pre-scan のまま(将来 wave)。ブラスト半径の
  fixture(type_inference/closures/hof/array(s)/struct_tests/generator/
  complex/dict/tuple/numeric/subarray)で codegen 退行なしを確認。

- `same_type_fast_path` / `promote_numeric_pair` の 2 層を導入し、
  `execute_binary_both` を「同型 fast path → promote → fallback chain」の
  3 層構造へ(upstream `julia/base/promotion.jl` と同型の設計)。畳み込んだ
  グループ: Float64 昇格(Float16×Float64, Float32×Float64,
  Int64×Float64 の arith/div/mod/cmp)、Float32 昇格(Float16×Float32,
  Float32×Int64, Float32×Int128)。promotion 規則は `compile/promotion.rs` /
  PROMOTION.md と一致。未対応演算のエラーラベルは従来文字列を逐語的に保存。
- 到達不能だった dead-float16-duplicate / dead-float32-duplicate arm を
  削除(`left_is_primitive && right_is_primitive` 分岐が同型ペアを常に先取
  するため実行不能。`same_type_fast_path` の単体テスト +
  `tests/fixtures/promotion/` が回帰カバレッジ)。`Value::` パターン数
  509 → 481。
- 挙動例外ペア(Bool、Float16×Int 結果 narrowing、unsigned 幅、Int128 混合、
  BigInt/BigFloat、Char)は Issue #5966 の promote-fallback 再帰トラップを
  避けるため明示 arm のまま。clippy 0 警告、--lib / fixture_tests 全パス、
  `scripts/check_binary_both_fallback_inventory.sh` パス(インベントリは
  削除 arm の 2 行を BINARY_DISPATCH.md から削除して同期)。ベンチ
  (vm_benchmark / hot_paths / calc_pi / mandelbrot)は回帰なし・改善のみ。

### 選択コア第2スライス: ランタイム dispatch パスが selection.rs を採用 ✅ (Issue #6502)

- `inference_core/selection.rs` にランタイム向け汎用プリミティブを2つ追加:
  `pick_max_score`(最大スコア最初勝ち winnow、`u32`/`i32` 両対応)と
  `pick_first_tier`(順序付き tier フォールバック、最初の当選 tier が勝ち、
  エラーは即時伝播)。いずれもクロージャ単相化・追加アロケーションなし。
- `Instr::CallDynamic`(`vm/exec/call_dynamic.rs`)の metadata 候補 3 段
  カスケード(全候補 → ユーザ定義のみ → Base `empty` 許可リスト)を
  `pick_first_tier` 経由に変換(tier の index リストは従来どおり遅延構築、
  3 つの同一 Err arm を 1 つに集約)。`Instr::CallTypedDispatch`
  (`vm/exec/call_dynamic_typed.rs`)のランタイム関数名検索 argmax ループ
  (Issue #3361 経路)を `pick_max_score` 経由に変換。挙動完全保存。
- `call_dynamic_binary.rs` は #3910 以降スコアリングを共有リゾルバへ全委譲
  済み、`vm/dynamic_ops/dispatch.rs` はインライン fast path のゲートのみで
  選択ループなし(変更不要を確認)。残ギャップ(dispatch_resolver 内部の
  argmax ループ= #5915 書き換えと合流、`vm/mod.rs` の tie-breaker ladder、
  直列化ペイロードのスライス (b) = #6496)は BINARY_DISPATCH.md に明記。
- 特性化 fixture 2 本(`dispatch_runtime_typed_dispatch_name_search_6502`,
  `dispatch_call_dynamic_tiered_selection_6502`、upstream Julia parity 検証
  済み)+ selection.rs 単体テスト 9 本を追加。

### compile_call のテーブル駆動ハンドラ分割 ✅ (Issue #6332)

- 約 3,993 行の `compile_call` を 141 行のディスパッチ列へ純粋切り出し
  (挙動・評価順は不変)。upstream `add_tfunc` 方式に倣い、名前キーの
  特殊ケースは `handlers/`(early / arrays / collections / internals /
  math / misc / strings)の統一シグネチャハンドラ +
  3 ディスパッチ地点テーブル(early / pre-match / post-struct)で解決。
  `None` 戻り = 汎用パスへのフォールスルー。
- 名前キーでない順序依存ブロックは同位置呼び出しのヘルパーへ verbatim
  抽出: コンストラクタ解決チェーン → `constructors.rs`、汎用メソッド
  ディスパッチ末尾 ~1,300 行 → `dispatch.rs`、splat / callable 変数 /
  enum 統合 → mod.rs 内ヘルパー。詳細は STATUS.md 同日付セクション参照。

### ランタイムマッチャの subtype 判定を CoreSubtypeEngine へ委譲 ✅ (Issue #5915)

- `type_matches` の `::Array` / `::AbstractArray` / `::Tuple` /
  `::AbstractString` / `::AbstractChar` / `::IO` / `AbstractUser` arm と、
  型変数を含まない `TupleOf`(tuple covariance)を `check_subtype`
  (共有エンジン)経由に統一。upstream Julia 検証済みの parity ユニット
  テスト 3 本を追加
  (`runtime_type_matches_array_tuple_string_io_params_via_core_subtype_issue_5915`,
  `runtime_type_matches_tuple_params_covariantly_issue_5915`,
  `runtime_type_matches_abstract_user_params_via_core_subtype_issue_5915`)。
- 従来の手書き arm が upstream に反していた点を修正: range / SubArray が
  `::AbstractArray` に不一致、`String <: AbstractString` / `Char <:
  AbstractChar` / `IOBuffer <: IO` が不一致、`Tuple{Int64}` が
  `::Tuple{Real}` に不一致。bindings 抽出ロジックは無変更。

### ランタイムマッチャ移行 第2弾: 残り arm を CoreSubtypeEngine へ ✅ (Issue #5915)

- `type_matches` の残り経路を委譲: `Struct(name)` arm の具象パラメトリック
  比較(invariance 維持 + 宣言親 `MyVec{Int64} <: Wrapper{Int64}` を
  upstream どおり一致)、`_` フォールバック(`DataType <: Type`、
  `Set{Int64} <: Set` が一致するように)、TypeVar 付き `TupleOf` の具象
  要素脚(`Tuple{Int64, Int64} <: Tuple{T, Real} where T`)。TypeVar
  ワイルドカードと bindings 抽出、runtime パラメータ不明時の寛容一致は
  ローカル維持。当時 `::Function` のみ legacy 完全一致を Workaround として
  維持していたが、Issue #6512 で n-ary intrinsic fold 側を修正して撤去済み。
- `VectorOf` / `MatrixOf` / Ref/RefValue は Julia の invariance に整合
  済みのため委譲対象外とし、upstream 検証済み回帰テストで固定。
- `dispatch_resolver.rs` の `struct_family_matches` を
  `CoreSubtypeEngine::is_subtype_by_name` 委譲へ(accept-set 不変)。
- parity ユニットテスト 5 本を追加(すべて upstream `julia` で検証):
  `runtime_type_matches_typevar_tuple_concrete_elements_covariantly_issue_5915`,
  `runtime_type_matches_struct_params_via_core_subtype_issue_5915`,
  `runtime_type_matches_vector_matrix_ref_params_stay_invariant_issue_5915`,
  `runtime_type_matches_nominal_fallback_via_core_subtype_issue_5915`,
  `struct_family_matching_uses_subtype_engine_issue_5915`。

### メソッド選択コアの第1スライス: 選択制御フローを inference_core/selection.rs へ ✅ (Issue #6502)

- typemap 相当のメソッド選択制御フロー(候補列挙→マッチ→dominance→選択)を
  `inference_core/selection.rs` に新設。`unique_dominant_index()`(「適格な候補が
  他の全候補を strict に支配するとき唯一の勝者」スケルトン)と
  `pick_scored_match()`(スコア絞り込み→tie-breaker ラダー→曖昧性プロトコル)。
- `MethodTable::dispatch_inner` は薄いアダプタに縮退: 9 個の
  `*dominant_match_index` バリアントと tie-breaker 5(pairwise strict subtype)が
  同一スケルトンへ集約され、`MethodSig` 固有の意味判定はクロージャ注入のみ。
  挙動保存リファクタ(ディスパッチ意味論の変更なし、wire format 不変 →
  CACHE_VERSION 据え置き 45)。
- runtime `call_dynamic*` 経路の同コアへの移行はフォローアップ(Issue #6502 継続)。
### 型表現の変換インベントリ + 死変換削除 ✅ (Issue #5916)

- `docs/vm/TYPE_REPRESENTATIONS.md` を新設: 6 系統の型表現と全 44 変換の
  `file:line`・損失・重複実装マップ、不一致ラウンドトリップ
  (`Bottom→Any→Top` 等)/文字列往復の指摘、`CoreType` を canonical とする
  段階的移行ロードマップ。TYPE_SYSTEM.md からリンク。
- 死変換を削除: vm `JuliaType` → AoT `JuliaType` の `impl From`
  (旧 `aot/types.rs:889`、外部呼び出しゼロ)。aot / cranelift feature
  ビルドで安全性を検証。

### 型表現ラウンドトリップ不一致の修正 wave 2 ✅ (Issue #5916)

- `ValueType::Union(vec![])`(`Union{}` の VM 側キャリア)→ `LatticeType` が
  `Top` に格子反転していたのを `Bottom` に修正(`compile/bridge.rs` の
  `From` impl + table-aware 変種)。`LatticeType::Bottom → ValueType::Any` は
  意図的 widening として維持・テストでピン(空 union キャリア化は
  `Meta.unblock` 型の再帰推定リークで codegen 退行を起こすため revert)。
- `lattice_to_julia_type` が `Bottom → JuliaType::Bottom` を保存
  (`Union{}` は `JuliaType` で表現可能)。#4679 special-case は引き続き必要と
  判定、根拠をコメント化。
- `ConcreteType::Range{element} → CoreType` が要素を保存
  (`Struct{"AbstractRange",[element]}`、`Range{Any}` は bare abstract 維持)。
- `ValueType → JuliaType` の重複 3→2: `vm/type_objects.rs` 側を canonical
  (`builtins_reflection/primitives.rs`) への thin wrapper 化。`Union` の扱いの
  不一致は構造保存側(upstream 準拠)を採用。
- `docs/vm/TYPE_REPRESENTATIONS.md` を更新(§3.5 Resolved、残存 divergence
  記録)。残りの重複統合は sibling 担当ファイルのため deferred。
### 残存 ad-hoc 推論ゲートの tfuncs レジストリ移行 ✅ (Issue #5922)

- gcd/lcm・big・IOBuffer・typeof/promote_type/promote_rule/eltype/keytype/
  valtype・isequal (2 引数, arity ゲート付き)・hash/fld/cld/日付アクセサ・
  trues/falses の戻り値型推論を legacy match ゲートから tfuncs レジストリ +
  `expr_tfuncs` アダプタへ移行 (ValueType / JuliaType 両経路から削除)。
- レジストリ規則は健全な主張のみ行い、レガシー互換の無条件フォールバックは
  アダプタの `FixedFallback` が保持。`builtin_op_inference.jl` フィクスチャ
  (upstream julia で検証済み) とアダプタ単体テスト 5 本を追加。
- 残置 (表現不能): complex / Dict builtin パターン / struct コンストラクタ /
  HOF ハンドラ / collect / rand / LinearAlgebra ModuleCall — 詳細は
  STATUS.md 同日付セクション参照。
### compile_core_program_internal の名前付きパイプラインフェーズ分解 ✅ (Issue #6333)

- 約2,470行の `compile_core_program_internal` を `compile/pipeline_ctx.rs` の
  `CorePipeline` 構造体 + 名前付きフェーズメソッド列(`build_struct_tables` →
  `init_shared_context` → … → `compile_main` → `finalize`)へ純粋に切り出し。
  本体は約70行のフェーズ呼び出し列になった。
- 4要素タプルの戻り値を `CoreCompileOutput` 構造体に置換し、
  `compile_core_program_with_globals` / cache 経路の呼び出し側を追従。
- 挙動・フェーズ実行順・profile ラベルは不変(純リファクタリング)。
### 並列 subtype 階層テーブルの解消 ✅ (Issue #5921)

- `JuliaType::is_subtype_of` の数値/Range 並列テーブルを削除し、built-in
  階層判定を共有 `CoreSubtypeEngine` に一本化(Issue #2494 の手動同期
  duty を解消)。upstream julia 1.12 検証済み 22×22 数値マトリクス +
  range テストと、range ペアへ拡張した parity テストが恒久退行ゲート。
- エンジン修正: `AbstractUnitRange{Int64} <: AbstractRange`(upstream
  true)が parametric abstract スペルでも成立するよう、
  `struct_is_subtype_of_abstract` を `range_family_name_subtype_allowed`
  格子に委譲。

### Issue #6336 完了: MethodSig wire format の core_signature 一本化 ✅ (CACHE_VERSION 45)

- `MethodSig` は `core_signature`(+ 表示用 `param_names`)のみを直列化し、
  `params` / `type_params` はデシリアライズ時に canonical 逆変換
  (`core_type_to_julia_type` / `core_type_var_to_type_param`)で再構成される
  非直列化射影になった。Base 全コーパス round-trip ゲート 2 本 + ユーザ形状
  serde テストで正確性を恒久検証。
- フォローアップ: legacy matcher CoreType 移植 #6495 /
  CallDynamic 系文字列ペイロード #6496。

## 最新対応 (2026-06-11)

### Bool div result type ✅ (Issue #6486)

- Added bundled `base/bool.jl`'s `div(::Bool, ::Bool)` method so Bool division
  preserves the upstream `Bool` result type.
- `div(false, false)` and `div(true, false)` continue to raise `DivideError`.
- Added `bool/div_result_6486.jl`.

### 最終残課題の定量化 + round-trip ゲート ✅ (Issue #6336 第5弾)

- ディスパッチ祖先ウォークの最後の ad-hoc `split('{')` を
  `nominal_family_name` へ統合。
- `base_method_params_roundtrip_core_signature_issue_6336` を追加: Base 全
  メソッド引数(9,051)の `JuliaType → CoreType → JuliaType` round-trip を
  恒久検証し、既知の `Pairs`/`Expr` 二重スペル衝突以外の不一致をブロック。
  `params`/`type_params` 撤去への正確なブロッカー(legacy matcher の
  JuliaType 依存 + 非単射スペル)は BINARY_DISPATCH.md「State of the #6336
  structured-signature migration」を参照。

### signed/unsigned primitive-width fallback conversions ✅ (Issue #6494)

- `BuiltinId::Signed` now preserves signed integer widths and reinterprets each
  UInt8/16/32/64/128 value to the matching Int width.
- `BuiltinId::Unsigned` now preserves unsigned integer widths and reinterprets
  each Int8/16/32/64/128 value to the matching UInt width.
- Added `conversion/signed_unsigned_widths_6494.jl` and VM unit tests covering
  the fallback conversions directly.

### Mixed-width integer div result types ✅ (Issue #6477)

- Added Pure Julia mixed integer `div` methods so mixed-width primitive and
  BigInt integer division no longer reaches the generic `floor(x / y)` fallback
  that widens through `Float64`.
- Signed/unsigned primitive pairs follow upstream's directional result type
  behavior, while same-sign mixed widths promote before calling the same-type
  integer `div` method.
- Added `arithmetic/mixed_width_div_6477.jl` for `div`, `÷`, signed/unsigned
  pairs, and BigInt pairs.

### BigInt narrow integer promote conversion ✅ (Issue #6489)

- Extended the VM `BigInt` numeric constructor to accept Bool, Int8/16/32/128,
  and UInt8/16/32/64/128 in addition to the existing Int64 path.
- `promote(big(10), Int8(3))`, `promote(UInt16(10), big(3))`, and UInt128
  variants now convert both tuple elements to `BigInt`.
- Added `promotion/bigint_narrow_promote_6489.jl`.

### IterateDynamic ペイロード構造化 ✅ (Issue #6336 第4弾, CACHE_VERSION 44)

- `Instr::IterateDynamic(argc, Vec<(usize, String)>)` →
  `IterateDynamic(argc, Vec<usize>)`。コンパイル側 4 emit 箇所は
  `m.global_index` のみを格納し、ランタイムの名前パターンフォールバックは
  候補の `FunctionInfo` + `expanded_param_types_for_call` からアリティ別
  シグネチャを導出(2 引数 `iterate(collection, state)` の state 型
  スコアリング #3910 は維持)。
- bincode レイアウト変更につき `CACHE_VERSION` 43→44(variant の並びは不変、
  ペイロードのみ変更)。stale 候補互換アームを削除。

### struct_is_subtype_of_abstract 親マップ統合 ✅ (Issue #6336 第3弾)

- `MethodTableProjection` から名前キー親マップ(`struct_parents` /
  `abstract_parents`)を削除し、ディスパッチの struct 祖先フォールバック一式
  (`struct_is_subtype_of_abstract` / `needs_struct_parent_fallback` /
  `julia_type_matches_with_struct_parents` /
  `arg_type_is_subtype_of_abstract_with_parents` /
  `julia_signature_match_with_struct_parents`)を共有 `StructHierarchy` 直接
  参照(`declared_parent_link`)へ統合(#5614/#5646 の「3 レジストリ」問題の
  method_table スライス解消)。
- 旧射影との等価性維持: 親なし abstract の除外は `parentless_abstract_names`
  で明示化、`is_empty()` ゲートは precomputed `has_parent_links` に置換。
  ウォーク本体のセマンティクス(保守的 accept 等)は不変。

### Mixed integer promote_type concrete results ✅ (Issue #6487)

- Replaced the integer promotion table's `Union{Type{...}}` declarations with
  explicit concrete `promote_rule(::Type{A}, ::Type{B})` pairs for bundled
  signed/unsigned integers and BigInt.
- This keeps Pure Julia `promote_type` from missing the rule and falling back to
  abstract `typejoin` results such as `Signed`, `Integer`, or `Unsigned`.
- Added `promotion/mixed_integer_promote_type_6487.jl` covering symmetric
  `promote_type` calls and primitive signed/unsigned value-level `promote`
  conversion. BigInt/narrow value conversion remains tracked separately by
  Issue #6489.

### Legacy native-array carrier compatibility isolation ✅ (Issue #6337)

- Centralized the transitional native-array carrier compatibility predicates in
  `vm/native_array_compat.rs` so dispatch wrapper-boundary checks, pointer
  identity, and borrowed native-array access no longer live as local helpers in
  `vm/mod.rs` or `exec/binary_both.rs`.
- Renamed the remaining VM helper and test identifiers away from
  `legacy_array`, including the binary fallback equality/matmul paths and
  array destructure helper names.
- Verified the concrete cleanup target with
  `rg 'legacy_array' subset_julia_vm/src/vm -g '*.rs'` returning zero matches.

### type_objects 名前分解の中央パーサ統合 + legacy-array 例外フラグ化 ✅ (Issue #6336 第2弾)

- `vm/type_objects.rs` のローカル `split_top_level_commas` と各所の
  `find('{')` / `rfind('}')` 分解を削除し、リフレクション表示名処理
  (`base_name_without_params` / `parametric_base_name` /
  `split_parametric_name` / `parametric_arg_tokens` / `canonical_typename`)
  を type_core の中央トークナイザ(新規公開 `parse_parametric_type_name`、
  `CoreType::from_julia_name` と同一実装)上に統合。
- `base_function_accepts_native_array_value`(#3908/#4189 の境界フェンス例外)
  をディスパッチごとの名前文字列照合から、`Vm::new_program` で一度だけ導出する
  `native_array_exempt_functions: Vec<bool>` + `is_native_array_exempt_function(idx)`
  のフラグ参照へ移行。名前照合はプログラム導入時(lowering/link 境界)のみ。

### ウォーム起動オーバーヘッド削減 フェーズ2 ✅ (Issue #6348 完了)

- `println(1+1)` のウォーム起動: ~60ms → **~40ms**(受け入れ条件 ≤50ms 達成)。
- Base キャッシュ deserialize(メインスレッド `warm_base_cache`)と prelude
  `Program` ロード(バックグラウンド `begin_warm_start_prefetch`)を並列化。
- prefetch スレッドが推論エンジン用 Base 関数 clone を事前作成
  (長さ不一致・未開始時は従来パスへフォールバック、wasm は no-op)。
- ワンショット CLI 実行は flush 後にデストラクタをスキップして即終了。

### `for outer i` modifier rejection ✅ (Issue #6465)

- Lowering now rejects `for outer i in itr` when it sees the parser's explicit
  `outer` modifier marker, preventing the previous incorrect execution as
  `for outer in i`.
- `for outer in itr` remains a normal loop binding and is covered by the
  Issue #6414 fixture.
- Added lowering unit coverage for both the accepted identifier form and the
  rejected modifier form.

### specificity ディスパッチ経路の文字列パース撤去 + MethodSig::arg_core_types ✅ (Issue #6336 第1弾)

- `inference_core/specificity.rs` の抽象コンテナパラメータ解析を ad-hoc 文字列
  パース(`find('{')` / カンマ分割 / `<:` 分割)から中央 `CoreType::from`
  ブリッジ経由の構造化検査へ置換し、`parse_diagonal_container_param` /
  `split_diagonal_container_params` / `bound_subtypes(&str,&str)` を削除。
- 対角パターン 5 種の `upper_bound` を `&str` から構造化 `CoreType` へ移行
  (`type_param_upper_bound_core`)。
- `MethodSig::arg_core_types()` アクセサを追加し、
  `empty_trailing_vararg_dominant_match_index` を最初の移行先として
  legacy `params` 直読みから切替。ネスト型(`AbstractVector{Vector{Int64}}`)
  の構造化抽出ユニットテストを追加。
- フィールド削除・キャッシュ版数 bump は未実施(直列化レイアウト不変のため
  不要)。残課題は STATUS.md / Issue #6336 を参照。

### Plots `bar` / `bar!` support ✅ (Issue #6358)

- Added `bar` / `bar!` exports to the bundled `Plots` package.
- Implemented `bar(y)`, `bar(x, y)`, `bar([(x, y), ...])`, and matching
  mutating variants as `:bar` Series constructors.
- Reused the existing Plotly `"type":"bar"` artifact path so iOS and Web render
  bar plots through the same interactive viewer as histogram.
- Added fixture and artifact coverage, and exposed `bar` / `bar!` in app
  completion/sample surfaces.

### dispatch_instr 単一網羅 match 化(28段ハンドラチェーン削除)✅ (Issue #6343)

- `vm/exec/mod.rs` の `dispatch_instr` を、全 422 `Instr` バリアントを明示する
  **ワイルドカードなしの単一 match** に書き換えた。バリアント追加時は dispatch に
  腕を足すまでコンパイルエラーになる。LLVM はこの match をジャンプテーブルに
  コンパイルするため、コールド命令が旧チェーンで払っていた最大 27 回の失敗
  match 通過が消える。
- 28 段の `NotHandled` フォールスルーチェーンと中間 enum 21 種
  (`LocalsResult` / `JumpResult` / `CallDynamicResult` など)を全廃。各ハンドラは
  `Result<DispatchAction, VmError>` を直接返し、旧チェーン末尾の catch-all と
  同じ `NotImplemented` エラーは共有ヘルパー `unhandled()`(#[cold])に集約。
- ip 更新は `jump_to`(後方ジャンプの cancellation チェック維持、#6342)として
  ジャンプハンドラ内へ、call-depth overflow チェックは call/return/print/hof/
  call_dynamic の dispatch 腕に残置。旧チェーン末尾のインライン命令群は
  `execute_misc` へ移動。
- 計測 (criterion, release, 静音環境): `vm_mandelbrot/run_only` 17.36→17.57ms
  (+1.2%、ホット系は #5175 の前方配置で既にチェーン1〜3段目だったため中立)、
  `fib_recursion_25` 306→288ms (−5.8%)。
- 補足: バリアント個別 422 腕 + ハンドラ `#[inline(always)]` の CPython
  generated_cases 方式も試行したが、`rustc` の release コンパイルが 23 分超と
  なり不採用(グループ腕方式はビルド時間が従来同等)。

### Mixed-width integer `DynamicPow` stack overflow ✅ (Issue #6390)

- Added a pow-specific inline VM route for primitive integer operands so
  mixed-width powers like `Int8(2)^Int16(3)` avoid recursive generic
  `^` dispatch.
- `DynamicPow` now preserves the base integer type for nonnegative integer
  exponents, including signed/unsigned width mixes and Bool cases.
- Negative integer exponents now raise catchable `DomainError`, matching
  upstream Julia's integer-power boundary.
- Added Julia-verified arithmetic fixture coverage in
  `arithmetic/mixed_width_pow_6390.jl`.

### Legacy return inference retirement ✅ (Issue #6335)

- Removed the production `infer_function_return_type_v2_with_arg_types` entry
  point and migrated call-site return refinement in HOF, generator, and general
  call inference to `CoreCompiler::infer_shared_function_return_type_with_arg_types`.
- The new adapter builds the shared abstract-interp engine with the compiler
  struct/global context plus the call-site target function, converts known
  `ValueType` arguments to lattice types, and calls
  `InferenceEngine::infer_function_with_arg_types`.
- Updated inference architecture docs so `compile/inference.rs` is documented as
  shared-engine construction rather than a legacy return-inference path.
- Verification: `cargo clippy --all-targets -- -D warnings`;
  `timeout 1800 cargo nextest run --release` (3511 passed, 1 leaky, exit 0).

### メソッド特異性 (diagonal/vararg) ロジックの単一実装化 ✅ (Issue #6331)

- 新設 `inference_core/specificity.rs` にメソッド特異性判定を集約し、
  `compile/method_table.rs` とランタイム dispatcher (`vm/mod.rs`) の双方から
  共有コアを呼ぶ構成に変更(upstream の `jl_type_morespecific` 単一実装に対応)。
- 移行ファミリー: tuple vararg 展開 → tuple diagonal → union-actual →
  type-value / type-vector / type-matrix diagonal → vector diagonal +
  型パラメータ境界・抽象コンテナ解析ヘルパー。
- `vm/mod.rs` の `runtime_*` 特異性ヘルパー群を削除。共有コアに対する直接の
  単体テスト(vararg 展開・dominance、各 diagonal パターン検出、境界正規化)
  を `specificity.rs` に追加。

### Pair expression bare-operator RHS ✅ (Issue #6461)

- Fixed lowering for Pair expressions such as `:f => +`, preserving the RHS
  operator as a first-class function reference instead of filtering it out as the
  Pair operator token.
- Fixed abstract inference for Pair expression returns so functions returning
  `:g => *` report the VM's actual `Pair` struct type id.
- Added parser-shape and operators fixture coverage, including a function body
  returning `:g => *`.

### `IOContext(io, context)` property inheritance ✅ (Issue #6467)

- Fixed direct `IOContext(io, existing_ctx)` construction so inherited
  properties are visible through `get` and `haskey`.
- Normalized the implicit-constructor storage shape where `properties` contains
  an `IOContext`, mirroring the existing direct Pair normalization.
- Added Julia-verified fixture coverage for same-IO and different-IO context
  inheritance.

### Frame typed-slot sidecar removal (contiguous slot storage) ✅ (Issue #6344)

- `Frame` の19本の型別 sidecar `Vec`(`slot_i64` / `slot_f64` / ... / `slot_generator`)を
  削除し、唯一の `locals_slots: Vec<Option<Value>>` に一本化した。sidecar は常に
  `locals_slots` の純粋なミラーだったため、型別読み取りは `locals_slots` を直接
  match する `slot_*` アクセサメソッドに置換。
- 効果: フレームあたりのヒープバッファ 20本 → 1本、スロット store ごとの
  sidecar クリア書き込み 19回 → 0回、`prepare_for_reuse` / `clear_for_pool` の
  Vec 操作 20回 → 1回。
- 計測 (criterion, release): `fib_recursion_25` 320.6ms → 290.9ms (−9.3%)、
  `recursive_calls_depth10` 19.25ms → 17.93ms (−6.9%)。
- フレームプール(Issue #5172)は既存のまま全ホット呼び出し経路で有効。

### Empty `IOContext(io)` constructor ✅ (Issue #6468)

- Added direct `IOContext(io)` construction for empty property contexts.
- Matched upstream idempotence for `IOContext(existing_ctx)` by returning the
  existing context unchanged.
- Added Julia-verified fixture coverage for both plain IO and existing
  IOContext inputs.

### IOContext get/haskey fixture Julia parity ✅ (Issue #6408)

- Updated `iocontext_get_haskey.jl` to construct contexts with standard
  `IOContext(...)` calls rather than the sjulia-only `iocontext(...)` helper.
- The fixture now passes under upstream Julia and sjulia while retaining the
  Issue #3152 `get` / `haskey` coverage.

### Direct `IOContext` pair constructors ✅ (Issue #6409)

- Fixed direct `IOContext(io, :key => value)` property lookup by normalizing the
  single stored `Pair` into property-collection form at IOContext access
  boundaries.
- Added direct multi-pair constructor overloads for upstream-compatible calls
  such as `IOContext(io, :compact => true, :limit => true)`.
- Tests cover Julia-verified `get` and `haskey` behavior for single and multiple
  direct Pair properties.

### Contextual `outer` for-loop variable ✅ (Issue #6414)

- Fixed `for outer in itr` parsing by treating `outer` as a normal binding when
  it is followed by `in`, `=`, or `∈`.
- Preserved parser acceptance for the existing `for outer i in itr` modifier
  syntax; full modifier semantics are tracked separately by Issue #6465.
- Tests cover the parser corpus and a Julia-verified control-flow fixture.
### ウォーム起動オーバーヘッド削減 フェーズ1+ ✅ (Issue #6348 の一部)

- `println(1+1)` のウォーム起動: ~135ms → ~65-70ms(フェーズ1 目標 ≤150ms 達成、
  フェーズ2 目標 ≤50ms は未達で Issue 継続)。
- メソッドテーブルの struct 階層射影を `Arc<MethodTableProjection>` で全テーブル共有
  (毎実行 1100+ 回の階層 clone を 1 回に)。
- `ir_opt` ユーザー限定パスが Base 関数 prefix を deep clone しない
  `UserSegmentOptimized` 返却に変更(compile 側は Base を入力 Program から借用)。
- prelude SHA-256 をプロセスごとにメモ化、`PROGRAM_CACHE` への store を同一プログラム
  2 回目以降のコンパイル時に遅延。
- 計測点を pipeline(parse / prelude load / merge)と compile の未計測区間に追加。

### `-e` semicolon-separated bare operator statements ✅ (Issue #6394)

- Added parser support for bare operator values at statement and delimiter
  boundaries, fixing `f = +; ...` and the equivalent newline-separated form.
- Preserved unary parsing when the operator has an operand, e.g. `x = + 1`.
- Tests cover parser shape and REPL evaluation for the semicolon-separated
  bare-operator assignment path.

### Plots `heatmap` support ✅ (Issue #6360)

- Added bundled `Plots.heatmap` / `Plots.heatmap!` exports.
- `heatmap(z)` maps matrix columns/rows to default x/y indices, and
  `heatmap(x, y, z)` preserves explicit axes while storing matrix z values on a
  `:heatmap` series.
- Plotly JSON generation renders heatmaps as 2D `"type":"heatmap"` traces,
  preserving matrix orientation and `aspect_ratio` layout handling.
- Tests: `packages_plots_heatmap_6360`, `plot_artifact_mime_tests`, and
  `plotting::plotly` cover construction, mutation, and rendered artifacts.

### Plots histogram `weights(...)` wrapper and bar rendering ✅ (Issue #6451)

- Added a bundled `Plots.weights(w)` helper and exported it from `using Plots`.
- `histogram(data; bins=..., weights=weights([...]))` now follows the existing
  weighted histogram path, producing `:bar` series and weighted bin counts.
- Plotly artifact tests cover the user-reported weighted histogram shape and
  assert that it renders as a bar trace.

### Plots `aspect_ratio` keyword ✅ (Issue #6353)

- Added `aspect_ratio` storage to bundled `Plots.Plot` while preserving the old
  `Plot(series, backend)` constructor.
- `plot`, `plot!`, `scatter`, `scatter!`, `histogram`, `histogram!`, and
  `surface` now accept `aspect_ratio` plus the upstream aliases
  `aspectratio`, `axis_ratio`, `axisratio`, and `ratio`.
- Plotly JSON generation reads the plot-level aspect ratio. 2D plots emit
  `scaleanchor:"x"` / `scaleratio` on `yaxis`; 3D plots with fixed aspect emit
  `scene.aspectmode:"data"`.
- Tests: `packages_plots_aspect_ratio_6353`, `plot_artifact_mime_tests`, and
  `plotting::plotly` cover keyword storage, aliases, and rendered Plotly layout.

### VM eval-breaker-style boundary checks ✅ (Issue #6342)

- Removed the per-instruction cancellation atomic load and call-depth comparison
  from both VM dispatch loops.
- Cancellation is now checked at backward jumps and call-frame push boundaries.
  The cancellation flag uses relaxed atomic ordering because it is a standalone
  request flag, not a data-synchronization primitive.
- Call-depth overflow is marked pending when a call frame is pushed, then raised
  after the call/return/print/HOF handler has installed the callee instruction
  pointer. This preserves `try`/`catch` routing without letting call setup
  overwrite the catch handler `ip`.
- Split temporary generated/eval push-pop frames onto
  `try_push_temporary_call_frame` so pending call-depth overflow does not leak
  into the outer VM dispatch loop.
- VM-only Criterion: `vm_mandelbrot/run_only` improved from main `37.467ms` to
  `36.471ms` median. `vm_calc_pi_large/base_gcd_run_only/1000` stayed neutral
  at main `3.9518s` vs after `3.9512s`.

### CompiledProgram Base cache decode profiling and specialization IR omission ✅ (Issue #6449)

- Split the Base-cache `CompiledProgram` payload into sub-sections so
  `SJULIA_COMPILE_PROFILE=1` reports decode time and bytes for `code`,
  `functions`, `specializable_functions`, and the small metadata vectors.
- The first sub-section profile showed `compiled.code` at `~13.9-14.3ms` /
  `3.83MB`, `compiled.specializable_functions` at `~8.9-10.2ms` / `2.91MB`,
  and `compiled.functions` at `~5.2-5.6ms` / `1.11MB`.
- Stopped persisting Base `specializable_functions` in persistent/embedded Base
  caches. Warm compilation rebuilds these registrations from the prelude/user
  Program while keeping cached `CallSpecialize` indices aligned, so the cached
  Base `CompiledProgram` prefix does not need to decode the specialization IR.
- Embedded-cache profiling reduced the Base cache from `8.58MB` to `5.66MB`,
  `compiled.specializable_functions` from `2.91MB` to `8 bytes`,
  `cache.deserialize.compiled` from `~28-30ms` to `~18.7-20.7ms`, and
  `cache.get_or_init_base_cache` from `~39-41ms` to `~29.8-31.8ms`.

### Base cache section decode profiling and method-table payload trim ✅ (Issue #6440)

- Replaced the monolithic outer Base-cache bincode payload with a small section
  envelope. The major payloads (`compiled`, `method_tables`, `closure_captures`,
  `promotion_rules`, `inference_results`) remain bincode-encoded, but
  `SJULIA_COMPILE_PROFILE=1` can now time each section separately.
- Stopped serializing `MethodTable` hierarchy projection maps
  (`struct_parents` / `abstract_parents`) in Base caches. Compile setup rebuilds
  them through `set_struct_hierarchy_projection()`, and the cached warm path was
  already discarding them through `clone_for_reprojection()`.
- Embedded-cache profiling showed `compiled` at `~28-30ms` and `method_tables`
  at `~31-33ms` immediately after sectioning. Skipping the projection maps
  reduced the Base cache from `13.6MB` to `8.58MB`, the method-table section from
  `5.73MB` to `0.70MB`, method-table decode from `~31-33ms` to `~4.2-4.6ms`,
  and `cache.get_or_init_base_cache` from `~65-69ms` to `~39-41ms`.
- Embedded-cache CLI `sjulia -e 'println(1+1)'` improved from `0.40-0.42s` to
  `0.34-0.37s` in the local 3-run profile.

### Warm-start compile overhead profiling and cached-prefix peephole skip ✅ (Issue #6348)

- Added `SJULIA_COMPILE_PROFILE=1` phase timing under the existing `profiling`
  feature for `compile_with_cache` and key `compile_core_program_internal`
  sub-phases.
- Avoided one full merged-Program clone when `ir_inline` has no candidates, and
  kept both `ir_inline` and `ir_opt` from transforming Base function bodies on
  the cached-Base warm path.
- Added a protected-prefix peephole fast path so cached Base bytecode at the
  beginning of `CompiledProgram.code` is copied through instead of rescanned by
  both pre-slotize and post-slotize peephole passes.
- Reused owned cloned `Function`s when building the shared inference engine,
  avoiding a second clone during warm compilation.
- On the cached-Base warm path, skipped top-level Base function parametric
  parameter and struct-literal scans because those instantiations are already in
  the cached instantiation table; user, module, and nested functions are still
  scanned.
- Replaced repeated parent-function lookup during nested closure-capture
  prepopulation with a one-time first-match parent parameter map.
- Shared cached method signatures through `Arc<Vec<MethodSig>>` and used a
  cached-Base `clone_for_reprojection` path so warm compilation no longer
  deep-clones Base method signatures or stale hierarchy projection maps before
  rebuilding the projection for the current program.
- Added Base-cache load sub-phase profile labels for embedded cache body
  deserialization, prelude hash validation, promotion-rule replay, and compile
  context restoration.
- Stopped persisting Base inference return snapshots in persistent/embedded
  Base caches. Same-process source-compiled Base cache hits still keep the
  in-memory snapshot, while warm CLI startup avoids decoding and replaying a
  large seeded return cache that then has to be invalidated when user methods
  are added.
- Added `compile.seed_inference_results` timing so seeded inference replay is
  visible in compile profiles.
- Deferred cached Base bytecode prefix assembly until after user/main suffix
  emit, slotization, and peephole optimization. The final `CompiledProgram.code`
  remains a single contiguous vector, but warm cached compilation no longer runs
  peephole passes over the Base prefix or carries protected ranges through the
  hot path.
- Embedded-cache CLI `sjulia -e 'println(1+1)'` improved from `0.85-0.90s` to
  `0.51-0.55s`. The two protected-Base peephole passes dropped from
  `142.677ms` / `138.460ms` to `12.527ms` / `8.434ms`. The follow-up
  method/inference setup trim reduced `compile_core_program_internal` from
  `284.789ms` to `258.711ms`, with `compile.build_inference_engine` at
  `3.378ms` and `compile.method_table_setup` at `58.053ms`. The method-table
  COW follow-up reduced `compile.cached_method_tables_clone` from `25-30ms` to
  `~0.8ms`, with `compile_core_program_internal` in the `230-241ms` range and
  CLI wall in the `0.46-0.52s` range. Omitting persisted inference snapshots
  reduced the Base cache from `15.3MB` to `13.6MB`, `cache.deserialize_body`
  from `~72-74ms` to `~65-66ms`, `compile.method_table_setup` from `~57-61ms`
  to `~31-32ms`, `compile_core_program_internal` to `192-199ms`, and CLI wall
  to `0.44-0.48s`. Deferred cached-prefix assembly then reduced
  `compile.peephole_pre_slotize` / `compile.peephole_post_slotize` to
  `0.08-0.11ms` / `0.05-0.11ms`, with `cache.compile_core_program_internal` at
  `168-173ms` and CLI wall at `0.40-0.42s`.

### Resolved/direct I64 slot-call fusion ✅ (Issue #6315)

- Fused `LoadSlotI64(arg)...; CallResolved(func, argc)` and
  `LoadSlotI64(arg)...; CallInbounds(func, argc)` into `CallResolvedI64Slots` /
  `CallInboundsI64Slots`, extending slot-direct I64 call argument loading beyond
  lazy specialization sites.
- The VM direct-call executor now reads I64 slot sidecars before stack
  materialization and attempts the existing Euclidean modulo loop / generic
  `I64Function` direct paths. Misses fall back through `LoadSlotI64`-equivalent
  value loading and the normal direct-call frame path.
- Renamed the previous `ExecutableBlock::GcdI64*` profile/internal terminology to
  `ExecutableBlock::EuclideanModuloI64*` so the name describes the recognized
  loop shape rather than the Base API using it.
- VM-only Criterion: `vm_calc_pi_large/base_gcd_run_only/1000` moved from
  `396.96ms` at the start of #6315 to `363.76ms`; current user `mygcd` measured
  `303.98ms`. Embedded-cache CLI `calc_pi(1000)` measured `1.23s` for user
  `mygcd` and `1.27s` for Base `gcd`.
- Tests cover Base `gcd` bytecode using `CallResolvedI64Slots`, a non-gcd
  `score6315(i, step)` resolved helper using the same fusion, and the profiling
  event rename to `ExecutableBlock::EuclideanModuloI64Function`.

### Generalized resolved-call I64 function blocks ✅ (Issue #6314)

- Extended the generic `I64Function` executable block decoder beyond the
  one-off Base `abs(::I64)::I64` unary op. Shape-guarded small resolved/direct
  I64 callees can now be stored as nested `I64Function` blocks and called without
  constructing a callee frame.
- Guards remain conservative: no generated functions, varargs, keywords, type
  parameters, non-I64 positional parameters, non-I64 returns, unsupported opcodes,
  unbounded recursion, or unbounded callee lists. Unsupported shapes keep the
  existing normal frame fallback.
- Added `LoadAddI64Slot`, `LoadSubI64Slot`, and `LoadMulI64Slot` support to the
  I64 block interpreter so peephole-optimized integer helper bodies such as
  `x * x + 1` can be represented.
- Avoided cloning cached `I64FunctionBlock`s on every direct fast call; cached
  blocks are now executed by reference, which offsets the nested callee metadata
  and improves existing hot paths.
- VM-only Criterion: `vm_calc_pi/base_gcd_run_only/500` measured `105.94ms`
  before and `102.78ms` after; `vm_i64_function_calls/run_only/20000` measured
  `26.72ms` before and `25.98ms` after. The new non-gcd
  `nested_resolved_helper_run_only/20000` case measures `10.40ms`.
- Embedded-cache CLI `calc_pi(1000)` stayed `1.21s` for user `mygcd`; Base
  `gcd` moved from `1.29s` to `1.26s`.
- Tests: `i64_resolved_call_6314_tests` covers a non-gcd resolved helper call
  inside a hot I64 loop and verifies `ExecutableBlock::I64FunctionNestedCall`
  under the profiling feature.

### Persisted program file modules split by format ✅ (Issue #6328)

- Removed the old public `bytecode` module and split persisted file handling by
  format: `core_ir_file` owns Core IR `.sjir`, while `vm_bytecode_file` owns VM
  bytecode `.sjvmbc`.
- Renamed Core IR file public types to `CoreIrFileError`, `CoreIrFileFlags`, and
  `CoreIrFileHeader`; VM bytecode file errors are exposed as
  `VmBytecodeFileError`.
- Updated CLI, AoT, and test imports to use explicit module names. iOS-facing C
  ABI / FFI names such as `compile_to_ir` and `run_ir_json_*` are unchanged.

### Core IR AoT test naming cleanup ✅ (Issue #6327)

- Renamed the historical AoT Core IR integration test file to
  `core_ir_aot_tests.rs` so the integration test file describes persisted Core IR
  `.sjir` and AoT conversion instead of generic bytecode.
- Renamed historical bytecode-prefixed test cases to `test_core_ir_*` /
  `test_sjir_*` names.
- Renamed the Makefile narrow test target to `test-core-ir-aot`.
- Left the public file-format module rename to #6328 so this test/docs cleanup
  stayed low risk.

### AoT Core IR file conversion filters unreachable keyword sentinels ✅ (Issue #6324)

- `ir_file_to_aot_ir()` now matches the AoT CLI path by filtering persisted Core IR
  through `CallGraph::filter_program()` before type inference and AoT IR conversion.
- This avoids converting unreachable prelude functions whose bodies contain internal
  body-evaluated keyword default sentinels such as `kw === Undef`.
- The fix intentionally does not map `Literal::Undef` to `nothing` or another runtime
  value; unreachable sentinel-only implementation details are excluded from the
  conversion surface instead.
- Tests: `core_ir_aot_tests` covers an unreachable function with a body-evaluated
  keyword default `Undef` guard, and the full `--features aot` integration test now
  passes.

### AoT Core IR API naming cleanup ✅ (Issue #6323)

- Renamed AoT analyze public Rust APIs from historical bytecode terminology to
  Core IR terminology:
  `load_ir_file`, `load_ir_bytes`, and `ir_file_to_aot_ir`.
- Renamed `BytecodeAnalyzer` to `CoreIrAnalyzer` and moved the module file to
  `core_ir_analyzer.rs`; no compatibility wrappers are kept.
- Renamed the unimplemented AoT stub from `compile_from_bytecode` to
  `compile_from_ir_bytes`.
- The iOS-facing C ABI / FFI surface is unchanged: `compile_and_run_detailed`,
  `run_ir_json_*`, `compile_to_ir`, and related bridge functions keep their names.

### Persisted Core IR `.sjir` rename ✅ (Issue #6322)

- Renamed the user-facing persisted Core IR file extension from `.sjbc` to
  `.sjir`; `sjulia --compile` now defaults to `<stem>.sjir`.
- Updated the Core IR file magic bytes from `"SJBC"` to `"SJIR"` and intentionally
  did not keep `.sjbc` as a compatibility alias.
- Replaced explicit Core IR execution CLI wording with
  `sjulia --run-ir <file.sjir>` and `.sjir` extension dispatch.
- Updated AoT CLI wording to `aot --ir program.sjir` for Rust generation from
  persisted Core IR. The public Rust API naming cleanup is handled by #6323.
- Tests: `sjulia_cli_vm_bytecode_tests` covers `.sjir` compile, `--run-ir`,
  extension-based run, and non-generation of the old `.sjbc` default.

### sjulia VM bytecode CLI execution path ✅ (Issue #6317)

- Added `sjulia --compile-vm <file.jl> -o <file.sjvmbc>` for persisting the final
  VM `CompiledProgram` separately from existing Core IR `.sjir` files.
- Added `sjulia --run-vm-bytecode <file.sjvmbc>` and automatic `.sjvmbc`
  extension dispatch so one-shot CLI runs can skip source parse/lower and VM
  bytecode compile.
- `.sjvmbc` stores the original `Program` next to the `CompiledProgram` only to
  reconstruct skipped runtime specialization context after deserialize; VM
  execution still uses the compiled bytecode payload.
- Embedded-cache CLI `benchmarks/calc_pi_benchmark.jl` measured `1.41s` from
  source, `1.24s` from IR `.sjir`, and `0.47s` from VM `.sjvmbc`.
- Tests: `sjulia_cli_vm_bytecode_tests` covers compile, explicit run, and
  extension-based run for `.sjvmbc`.

## 最新対応 (2026-06-10)

### Base gcd resolved-call I64 function blocks ✅ (Issue #6312)

- Base `gcd(::Int64, ::Int64)` の resolved direct call が、callee frame を作る前に
  I64 stack arguments から gcd / generic `I64Function` executable block を試す
  fast path を追加した。miss 時は従来の direct frame path に戻る。
- generic `I64Function` decoder は Base/prelude 由来の `abs(::I64)::I64` unary call
  だけを `AbsI64` op として扱い、Base `gcd` の `abs(a)` / `abs(b)` prefix を
  decode できるようにした。ユーザー定義 `abs` は対象外。
- direct I64 result 後の fused `PushI64; JumpIfEqI64/JumpIfNeI64` compare branch を
  消費できるようにした。
- profile 付き release CLI `calc_pi(50)` は `123,683` → `21,183` instructions。
  embedded cache 付き CLI `calc_pi(1000)` は Base `gcd` 版で `4.40s` → `1.35s`。
- VM-only Criterion に Base `gcd` calc_pi cases を追加した。current は
  `vm_calc_pi/base_gcd_run_only/500` `104.59ms..105.63ms`。
- Broader resolved-call I64 block coverage was resolved in #6314. The smaller
  residual Base `gcd` vs user `mygcd` delta is tracked in #6315.

### Generic direct I64 specialized-function blocks ✅ (Issue #6308)

- cache hit 済みの simple runtime-specialized function が local I64 slot 操作、
  I64 arithmetic/comparison、branch、`ReturnI64` だけで表現できる場合に、
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
- Tests: `scalar_hot_loop_6167_tests` covers non-gcd `advance(i, step)` direct
  `ExecutableBlock::I64Function` execution and preserves result parity.

### Cached I64 slot specialized-call fast path ✅ (Issue #6301)

- `CallSpecializeI64Slots` が全引数を `slot_i64` sidecar から読める場合、
  specialization cache hit 後の hot path で `Vec<Value>` 引数列、runtime
  `ValueType` 列、巨大な `FunctionInfo` clone を毎回組み直さず、cached
  specialized entry へ直接入る fast path を追加した。
- Guard は cache hit 済み・非 generated・非 vararg・keyword なし・type
  parameter なし・param slot 数一致の simple specialized call に限定し、
  sidecar 欠落や複雑な関数形では従来経路へ戻る。
- 既存の gcd executable fast path は `Value` 引数版と `i64` 引数版で共通の
  `execute_gcd_i64_values` helper を使うよう整理した。gcd 固有 pattern を
  追加せず、I64 slot specialized call 全般の cached call entry を軽くする。
- `calc_pi(500)` の profile 付き release CLI aggregate は `1,308,198`
  instructions のまま。今回の改善は bytecode dispatch 数ではなく、
  `CallSpecializeI64Slots` 1 命令の内部 allocation / metadata rebuild 削減。
- VM-only Criterion 中央値は main 再測定から
  `vm_calc_pi/run_only/500` `484.13ms` → `88.08ms`,
  `clone_new_program_run/500` `503.82ms` → `108.43ms`。
  `run_only/100` は `37.70ms` → `19.91ms`,
  `clone_new_program_run/100` は `64.04ms` → `42.08ms`。
- Tests: `scalar_hot_loop_6167_tests` covers a non-gcd `advance(i, step)`
  `CallSpecializeI64Slots` loop and preserves the generic user function result.

### Positive const-step counted loop backedge fusion ✅ (Issue #6305)

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
- VM-only Criterion は `vm_calc_pi/run_only/500` median `511.19ms` 近辺 →
  `462.31ms`, `clone_new_program_run/500` median `514.35ms` 近辺 →
  `494.74ms`。Criterion は両方を performance improved と判定した。
- Tests: peephole unit tests cover `+1` / `+2` positive const-step fusion,
  the fallthrough-exit guard, and negative-step no-fusion. `const_step_for_loop_5166_tests`
  covers `for i in 1:2:n` using a fused backedge carrying `delta=2`.

### Mandelbrot loop slot branch / Float64 slot conversion fusion ✅ (Issues #6167, #6253)

- `mandel_count` の direct-call loop に残っていた
  `LoadSlotI64(lhs); LoadSlotI64(rhs); JumpIfGtI64` を、lhs slot が loop body 内で
  更新される forward loop exit に限定して `JumpIfGtI64Slots` へ fusion した。
- `Float64(x)` / `Float64(width)` などの
  `LoadSlotI64(slot); CallBuiltin(Float64, 1)` を、新 bytecode
  `LoadSlotI64ToF64(slot)` へ fusion した。実行側は既存 `LoadSlotI64` と同じ
  numeric slot 値だけを読み、`Float64(x)` と同じ `convert_to_f64` で `F64` を
  push する。`convert(Int64, x)` は method lookup semantics を持つため消していない。
- `benchmarks/vm_mandelbrot.jl` は未変更。profile 付き release CLI aggregate は
  main baseline から `337,837` → `298,955` instructions (約 11.5% 減)。
  `LoadSlotI64` は `67,684` → `28,802`、`CallBuiltin` は `29,011` → `9,651`。
  `calc_pi` profile aggregate は `9,108,184` のまま。
- VM-only Criterion 中央値は `vm_mandelbrot/run_only` `38.226ms` 近辺 →
  `35.639ms`, `clone_new_program_run` `58.937ms` 近辺 → `56.636ms`。
- Tests: `mandelbrot_6259_tests` now covers `mandel_count` branch/conversion
  fusion and result parity. Peephole unit tests cover the loop-update guard,
  unfused increment recognition, and `LoadSlotI64ToF64`.

### calc_pi slot-argument specialized-call fusion ✅ (Issue #6167)

- `calc_pi` hot loop に残っていた
  `LoadSlotI64(a); LoadSlotI64(b); CallSpecialize(mygcd, 2)` を、関数名ではなく
  argc と直前の `LoadSlotI64` 列で guard する peephole により
  `CallSpecializeI64Slots(func, slots)` へ fusion した。
- VM 側は slot から既存 `LoadSlotI64` と同じ numeric 値を読み、通常の
  `CallSpecialize` と同じ helper に渡す。特殊化 cache、fallback frame binding、
  generated function handling、`@inbounds` context は既存経路と共有する。
- `benchmarks/calc_pi_benchmark.jl` は未変更。profile 付き release CLI aggregate は
  #6299 baseline から `11,628,184` → `9,108,184` instructions (約 21.7%
  減)。`LoadSlotI64` は `2,520,006` → `6` まで落ち、hot profile top 20 から
  消えた。同 profile run の `@time calc_pi(1000)` は `2.894s` → `2.607s`
  (約 9.9% 短縮)。これは CLI aggregate / VM instruction profile であり、
  VM-only Criterion ではない。
- VM-only Criterion を #6299 baseline/current として同じ worktree window で測定した。
  中央値: `vm_calc_pi/run_only/100` `35.764ms` → `35.929ms`,
  `clone_new_program_run/100` `60.517ms` → `58.826ms`,
  `run_only/500` `503.51ms` → `487.27ms`,
  `clone_new_program_run/500` `534.93ms` → `517.22ms`。
  `vm_mandelbrot/run_only` は `37.826ms` → `38.226ms`,
  `clone_new_program_run` は `59.378ms` → `58.937ms`。Mandelbrot bytecode には
  `CallSpecializeI64Slots` は出ない。
- Tests: `scalar_hot_loop_6167_tests` now covers calc_pi slot-argument
  specialized-call fusion and result parity. Peephole unit tests cover argc
  matching, inbounds fusion, and partial fusion when unrelated stack values
  sit below the call arguments.

### calc_pi scoped slot-to-slot loop branch fusion ✅ (Issue #6167)

- `calc_pi` の outer/inner loop exit に残っていた
  `LoadSlotI64(var); LoadSlotI64(stop); JumpIfGtI64` を、
  scoped peephole で `JumpIfGtI64Slots(var, stop, target)` に fusion するように
  した。VM 側は `frame.slot_i64` の pair fast path を先に読み、通常の typed slot
  case では stack push/pop を避ける。
- Fusion は body に `CallSpecialize` / `CallSpecializeInbounds` を含む forward
  exit branch に限定した。Mandelbrot grid loop などの non-`CallSpecialize` loop には
  `JumpIfGtI64Slots` を出さない。
- `benchmarks/calc_pi_benchmark.jl` は未変更。profile 付き release CLI aggregate は
  #6298 後 baseline から `14,154,590` → `11,628,184` instructions (約 17.8%
  減)。`LoadSlotI64` は `5,046,412` → `2,520,006` (約 50.1% 減)。
  同 profile run の `@time calc_pi(1000)` は `3.122s` → `2.876s` (約 7.9%
  短縮)。これは CLI aggregate / VM instruction profile であり、VM-only Criterion
  ではない。
- VM-only Criterion を #6298 baseline/current として再測定した。中央値:
  `vm_calc_pi/run_only/100` `35.739ms` → `35.601ms`,
  `clone_new_program_run/100` `61.345ms` → `59.502ms`,
  `run_only/500` `530.23ms` → `499.75ms`,
  `clone_new_program_run/500` `543.31ms` → `516.56ms`。
  `vm_mandelbrot/run_only` は `37.329ms` → `37.337ms`,
  `clone_new_program_run` は `57.094ms` → `57.851ms`。
- Tests: `scalar_hot_loop_6167_tests` now covers calc_pi slot-to-slot loop exit
  branch fusion and result parity. Peephole unit tests cover both the scoped
  fusion and the non-`CallSpecialize` no-fusion guard.

### calc_pi const-step slot increment fusion ✅ (Issue #6167)

- `for` loop の定数 step increment が slotized 後に
  `PushI64(k); IncVarI64Slot(slot)` / `PushI64(k); DecVarI64Slot(slot)` として
  残るケースを、既存 `AddConstI64Slot(slot, +/-k)` に peephole fusion するように
  した。`calc_pi` では inner `b += 1` と outer `a += 1` がこの形になり、
  定数 increment のためだけの stack push を hot loop から除去する。
- `benchmarks/calc_pi_benchmark.jl` は未変更。profile 付き release CLI aggregate は
  #6293 後 baseline から `15,416,190` → `14,154,590` instructions (約 8.2% 減)。
  同 profile run の `@time calc_pi(1000)` は `3.365s` → `3.147s` (約 6.5%
  短縮)。これは CLI aggregate / VM instruction profile であり、VM-only Criterion
  ではない。
- VM-only Criterion を同じ環境で #6293 後 baseline/current として再測定した。
  中央値: `vm_calc_pi/run_only/100` `37.523ms` → `35.526ms`,
  `clone_new_program_run/100` `61.785ms` → `59.559ms`,
  `run_only/500` `530.84ms` → `524.14ms`,
  `clone_new_program_run/500` `550.21ms` → `520.55ms`。
  `vm_mandelbrot/run_only` は `36.376ms` → `35.219ms`,
  `clone_new_program_run` は `56.368ms` → `56.127ms`。
- Tests: `scalar_hot_loop_6167_tests` covers calc_pi bytecode shape and result
  parity. `const_step_for_loop_5166_tests` now accepts `AddConstI64Slot` as the
  const-step loop increment fusion.

### calc_pi gcd direct call fast path ✅ (Issue #6293)

- Coprime π estimation の hot path で、specialized `mygcd(::Int64,::Int64)` が
  `GcdI64Loop` bytecode shape かつ `ReturnI64` で終わる場合、`CallSpecialize`
  から frame を作らず Euclid loop を直接実行する fast path を追加した。関数名ではなく
  bytecode shape / parameter slot / `Int64` 実引数で guard し、`typemin(Int64) %
  -1` など通常 VM に任せるべき edge は fallback する。
- direct call の直後が `PushI64(const); Eq/Ne; JumpIfZero` の branch-context
  comparison なら、`Int64` 戻り値で branch まで直接消費する。これにより
  `mygcd(a,b) == 1` の hot loop から dynamic equality dispatch と Bool branch
  materialization を避ける。
- `benchmarks/calc_pi_benchmark.jl` は未変更。profile 付き release CLI aggregate:
  `21,716,190` → `15,416,190` instructions (約 29.0% 減)。同 profile run の
  `@time calc_pi(1000)` は `4.758s` → `3.240s` (約 31.9% 短縮)、process real は
  `16.76s` → `14.72s` (約 12.2% 短縮)。これは CLI aggregate / VM instruction
  profile であり、VM-only Criterion ではない。
- Tests: `calc_pi_6293_tests` covers result parity and profiling-feature guards
  for `ExecutableBlock::GcdI64Function` plus
  `ExecutableBlock::I64FunctionCompareBranch`.

### Mandelbrot Complex runtime specialization ✅ (Issue #6259)

- Runtime values now preserve `ComplexF32` / `ComplexF64` as concrete
  `ValueType` tags even when they are carried through erased `Any` / generic
  `Struct` slots. Array element type conversion, bridge lowering, primitive
  reflection, type-object reflection, slot typing, and return/pop handling all
  understand those tags.
- The runtime specializer lowers the Mandelbrot escape loop's concrete
  `ComplexF64` operations into Float64 field arithmetic: `abs2(z)` inlines to
  `re*re + im*im`, `z^2` inlines to the complex square formula, and
  `ComplexF64` `+`/`-`/`*` rebuild via the existing parametric `Complex`
  constructor path. The specialized escape body contains no `DynamicPow`, no
  `CallDynamicBinaryBoth`, and no resolved `abs2` call.
- Function-variable and broadcast trampoline calls now reuse lazy runtime
  specialization. After dispatch selects a fallback function, `CallFunctionVariable`,
  runtime callable calls, and HOF frame setup derive actual argument
  `ValueType`s, consult the specialization cache, append specialized bytecode on
  a miss, and jump to that entry while retaining the fallback frame layout.
  This covers `Base._broadcast_apply` in `mandelbrot_escape.(C, Ref(maxiter))`.
- `benchmarks/mandelbrot_benchmark.jl` stays unchanged. Aggregate CLI VM profile:
  `3,162,141` → `2,608,939` instructions; normal release `sjulia` `@time`
  3-run average: `3.31s` → `1.48s`. Mandelbrot ASCII output is unchanged.
  These are CLI aggregate numbers, not a VM-only Criterion measurement.
- Tests: `mandelbrot_6259_tests` covers specialized bytecode shape, direct
  result parity, broadcast result parity, and profiling-feature guards that the
  function-variable/broadcast path no longer executes `DynamicPow` in the escape
  loop.

### Mandelbrot escape executable block ✅ (Issue #6253)

- The VM predecoder now recognizes the runtime-specialized
  `mandelbrot_escape(::ComplexF64, ::Int64)` loop shape and installs a
  `ComplexF64MandelbrotEscape` executable block. The block scalarizes the escape
  loop into direct `Float64` arithmetic, avoiding per-iteration `GetField`,
  `NewParametricStruct`, and temporary `Any` slot traffic.
- Executable-block early returns route through the existing return-routing logic,
  preserving normal function and top-level return behavior. VM profile output now
  prints `ExecutableBlock::...` events separately.
- `benchmarks/mandelbrot_benchmark.jl` was left unchanged. Aggregate CLI VM
  profile: `2,608,939` → `1,561,995` instructions (about 40.1% fewer). With
  embedded prelude/base caches, release CLI `@time` 3-run average improved from
  `1.471s` to `1.369s` (about 6.9%). These are CLI aggregate / VM instruction
  profile measurements, not VM-only Criterion numbers.
- Tests: `vm::executable::tests::complex_mandelbrot_escape_runtime_specialization_adds_executable_block_6253`
  and profiling-feature
  `mandelbrot_6259_tests::broadcast_runtime_callable_escape_uses_executable_block_6253`.

### Complex scalar times real array dynamic fallback ✅ (Issue #6294)

- Fixed an existing `origin/main` compile-time bug where `ComplexF64 *
  Vector{Float64}` / `Vector{Float64} * ComplexF64` failed with
  `Cannot convert ComplexF64 to I64`.
- `compile/expr/binary` now treats dedicated `ValueType::ComplexF32` /
  `ValueType::ComplexF64` values as scalar operands for array-scalar dynamic
  dispatch, just like `Struct(_)` Complex values. The expressions now reach the
  existing `CallDynamicBinaryBoth(MulFloat, ...)` runtime fallback.
- Tests: existing `complex::chunk_000` (`complex_scalar_real_array_mul`) and
  `complex::chunk_001` (`complex_binary_both_helpers_3908`).

## 最新対応 (2026-06-09)

### interprocedural exception inference defers to pure-Julia classification ✅ (Issue #6272)

- `Base.infer_exception_type` / `infer_effects` now compose a user wrapper's
  exception type by consulting the pure-Julia reflection classification
  (`Base._classified_exception_type`) for pure-Julia Base callees such as
  `gcd`/`lcm`, instead of Rust-side `gcd`/`lcm` name special-cases and instead of
  walking their self-recursive bodies (which made `g_gcd(a,b)=gcd(a,b)` inference
  hang ~35s; previously masked by a workaround). Mirrors upstream
  `abstractinterpretation.jl` (`state.exctype ⊔ₚ this_exct`): a caller joins each
  callee's exception type, looking it up rather than re-deriving it.
- The exception walk threads a `BaseCalleeExceptionClassifier`; the engine treats
  `function_table` callees marked Base (`base_function_count` provenance) as
  terminal and consults the classification, recursing only into user callees. The
  VM-side `VmBaseExceptionClassifier` re-enters pure Julia synchronously via
  `eval_dispatch_call`. The Rust `gcd`/`lcm` name special-cases in
  `immediate_exception_type` / `exception_type_for_expr` /
  `terminal_exception_classified_call` were removed.
- `_classified_exception_type` was extended to all fixed-width integer widths
  (signed `gcd`→`OverflowError`, unsigned `gcd`→`Union{}`, any fixed-width
  `lcm`→`Union{DivideError,OverflowError}`), so direct and wrapper calls match
  upstream julia 1.12.6.
- Tests: `fixtures/reflection/infer_exception_type_gcd_lcm_6272.jl` (15 asserts,
  sjulia/julia parity; includes the regression that a throwing argument
  `gcd(v[i], b)` does not suppress the callee's own exception);
  `..._interprocedural_5600.jl` unchanged. Full suite 3449 green.

### gcd/lcm over BigInt report Any ✅ (Issue #6284)

- `Base.infer_exception_type` / `infer_effects` for `gcd`/`lcm` over `BigInt` now
  report `Any` (`nothrow == false`) instead of under-reporting `Union{}` (proven
  no-throw). `BigInt` `gcd`/`lcm` delegate to GMP via `ccall`, which the inferrer
  cannot prove `nothrow`, so upstream julia 1.12.6 reports `Any` — for direct
  calls, mixed `BigInt`/fixed-width pairs (which promote to `BigInt`), and user
  wrappers composing the same pure-Julia classification. Closes the only
  remaining upstream divergence left by #6272 (which covered fixed-width widths).
- Four coordinated changes: (1) the pure-Julia `_classified_exception_type` gains
  a `BigInt` `gcd`/`lcm` arm returning `Any`; (2) `classified_value_to_exception_type`
  maps a `JuliaType::Any` classification to `ExceptionType::Any` (composer entry);
  (3) `exception_type_to_julia_type` surfaces `ExceptionType::Any` as
  `Some(JuliaType::Any)` (composer exit); (4) the interprocedural recursion limit
  (`depth > 16`) now returns `ExceptionType::Bottom` (the merge identity) rather
  than `Any`, so clean deep recursion stays `Union{}` now that `Any` is surfaced.
  Fixed-width widths stay precise (regression guard).
- Tests: `fixtures/reflection/infer_exception_type_gcd_lcm_bigint_6284.jl`
  (16 asserts, sjulia/julia parity); `..._gcd_lcm_6272.jl` (15) and
  `..._interprocedural_5600.jl` (12) unchanged.

### closure scalar capture observes reassignment (function-local) ✅ (Issue #6262)

- A closure capturing a scalar **function-local** now observes later
  reassignments of that local instead of a stale value snapshot:
  `function f() counter=0; g=()->counter; counter=5; g() end` returns `5`
  (was `0`). Matches Julia's `Core.Box` cell semantics. Previously only
  reference-like captures (arrays) tracked mutation, because `CreateClosure`
  snapshots the captured value.
- Fix: a new post-lowering pass `lowering/closure_box.rs` boxes a local as `Ref`
  when it is captured by a closure AND reassigned ≥2 times at its scope's top
  level, rewriting the binding to `v = Ref(init)`, reads to `v[]`, and
  reassignments to `v[] = x` in both the defining scope and the capturing
  closure (inline `Stmt::FunctionDef` or a separately-lifted `FunctionRef`
  closure). sjulia's `Ref` is already reference-semantic on capture, so all
  references share one cell. The pass is conservative — single-assignment
  captures, shadowing, compound assignment, and non-read uses leave the local
  unboxed — and its exhaustive `match`es mirror `compile::free_vars`.
- Remaining sub-case (follow-up #6281): a scalar that is local to an `@testset`
  / `@time` block whose body spreads the binding and the closure across separate
  bare `begin` blocks is not yet boxed (bare blocks are not their own scope in
  Julia; the pass currently treats them as scopes). Top-level/module-scope
  captures already work because such names are globals (dynamic lookup).
- Test: `closures::*` (`fixtures/closures/scalar_capture_reassign_6262.jl`)。

### closure scalar capture observes reassignment (@testset / bare block) ✅ (Issue #6281)

- Follow-up to #6262. A closure capturing a scalar local to an `@testset` block
  (or a top-level bare `begin … end`) now observes later top-level reassignments
  of that local instead of a stale snapshot: `@testset "t" begin counter=0;
  get_counter=()->counter; counter=5; @test get_counter()==5 end` passes.
- Root cause (found by dumping the lowered IR): `@testset`/`@test`/bare `begin`
  bodies lower to **nested empty-binding `let` blocks**
  (`Stmt::Expr(LetBlock { bindings: [], … })`), not `Stmt::Block`. The binding,
  its reassignments, and the capturing closure (a lifted `__lambda_N` referenced
  by `FunctionRef`) all live **together** in the innermost `LetBlock` body, but
  the #6262 pass never descended into empty-binding `let` blocks, so it never
  reached that scope. (The issue's hypothesis that the binding and closure land
  in *separate* bare blocks turned out to be incorrect.)
- Fix: `lowering/closure_box.rs` `recurse_scopes_stmt` gains one arm that treats
  an empty-binding `Stmt::Expr(LetBlock { bindings: [], body })` as a defining
  scope and descends into it (like a bare block). A `let` *with* bindings is a
  real binding scope and is left unchanged.
- Test: `fixtures/closures/testset_closure_capture_reassign_6281.jl` — its final
  boolean is the regression guard (a stale snapshot makes it `false`); inner
  `@test`s are diagnostics; sjulia/julia parity. The previously-masked
  `get_counter()==5` failure in `testset_closure_capture.jl` is also resolved.
- Out of scope here, resolved next (#6288): a closure capturing a scalar local to
  a `@time` block — fixed immediately after, see below.

### closure capturing a @time/@elapsed-block-local: compile + boxing ✅ (Issue #6288)

- A closure capturing a variable local to a `@time` (or `@elapsed`) block now
  (1) compiles — it previously failed with `Undefined variable` even for a plain
  read-only capture — and (2) observes later reassignments of that local
  (`Core.Box` semantics), completing the `@testset` coverage of #6281. E.g.
  `@time begin c=7; g=()->c; g() end` works, and
  `r = @time begin counter=0; v=()->counter; counter=5; v() end` yields `r == 5`
  (was a stale `0`). Matches upstream Julia 1.12.6.
- Root cause (found by dumping the lowered IR): `@time`/`@elapsed` lower their body
  to `#result# = let … end` — an **empty-binding `let` block as an assignment
  value**. Neither the lambda-capture pre-analysis nor the boxing pass descended
  into an assignment's *value*, so the block-local (`c`) was never recognized
  (only `@testset`, detected via its `_testset_begin!` marker, was handled).
- Fix (three arms): (1) `collect_testset_local_binding_names_from_stmts` gains a
  `Stmt::Assign { value: empty-binding LetBlock }` arm that collects the body's
  binding names (the `@time` wrapper as a capture scope); (2)
  `collect_testset_scope_assigned_binding_names` now descends into assignment
  *values* (to reach the nested `#result# = let …`); (3) `closure_box.rs`
  `recurse_scopes_stmt` gains the matching `Stmt::Assign { value: empty-binding
  LetBlock }` arm so the `@time` body is a boxing scope, like `@testset`.
- Test: `fixtures/closures/time_block_closure_capture_6288.jl` (read-only /
  capture+reassign / `r = @time begin … end` forms + an `@testset` regression;
  trailing boolean is the regression guard; sjulia/julia parity).

## 最新対応 (2026-06-08)

### value-position `&&` / `||` final-operand value preservation ✅ (Issue #6278)

- `&&` / `||` in value position now return the final operand's value as-is
  instead of coercing it to `Bool`: `true && 1` → `1`, `false || "y"` → `"y"`,
  `true && "x"` → `"x"` (previously `true && "x"` raised a *compile* error
  "Cannot convert Str to Bool"). Matches upstream Julia. Follow-up to #6162
  (left/condition operand) which kept the right operand coerced.
- `compile_and_expr` / `compile_or_expr` compile the right operand with its
  natural type; the expression type widens to `Any` when that type isn't `Bool`
  (one branch yields the operand, the other a constant `Bool`).
- Critical: every binary-op type-inference path (`infer/mod.rs` ×2,
  `inference.rs` ×3) was updated in lockstep with the codegen via the shared
  `short_circuit_result_type`, otherwise an inline `(a && b) == lit` comparison
  mis-compiled against a stale `Bool` left type (dual-inference-gate). Bool-typed
  operands keep result type `Bool`, so existing code is unaffected.
- Test: `bool::*` (`fixtures/bool/short_circuit_value_6278.jl`)。

### value-position `&&` / `||` non-Bool operand accepted ✅ (Issue #6162)

- `&&` / `||` used in value position (`x = a && b`, `println(a || b)`, a function
  body that is a bare `&&` / `||`) now raises
  `TypeError: non-boolean (Int64) used in boolean context` for a non-Bool left
  operand (e.g. `1 && true`), instead of coercing the operand to `Bool` via
  `I64ToBool`. Matches upstream Julia.
- Branch (condition) position — `if`/`while`/ternary and `&&`/`||` as a
  condition — was already strict (PR #6165). This closes the remaining
  value-position gap.
- Fix: `compile_and_expr` / `compile_or_expr` (`compile/expr/unary.rs`) compile
  the left operand with its natural type (no `I64ToBool` coercion) so the
  following `JumpIfZero` enforces the VM's Bool-only boolean-context check
  (`expect_bool` → `TypeError`).
- Out of scope (follow-up): final-operand value preservation, e.g. `true && 1`
  should return `1`, not `true`.
- Test: `bool::*` (`fixtures/bool/boolean_context_6162.jl`)。

### try/catch implicit-return value discarded ✅ (Issue #6223)

- A function whose final expression is a `try/catch[/else/finally]` now returns
  the value of whichever branch executed (try body if no exception, catch body
  if one; `else` replaces the try value; `finally` never contributes the value),
  matching upstream Julia. Previously the value was discarded and the return
  type's default (`0` for `Int64`) was returned.
- Root cause: the compile-layer implicit-return path (`compile_block_with_implicit_return`
  and the function-body tail-statement match in `compile/stmt.rs`) handled a
  tail `Stmt::Try` via the catch-all arm (`compile_stmt` + `emit_default_return`),
  which leaves no value on the stack.
- Fix: the lowering's expression-position try transform (`lower_try_as_expr`,
  Issue #4784) was refactored into a shared `try_stmt_into_value_expr` that
  converts a `Stmt::Try` into the value-producing `Expr::LetBlock`. The
  implicit-return path now reuses it via a new `compile_try_with_implicit_return`,
  so tail-position and expression-position try/catch share one transform.
- Test: `exceptions::chunk_000` (`fixtures/exceptions/try_implicit_return_6223.jl`)。

### Rational bare-parametric binary dispatch cache invalidation ✅ (Issue #6270)

- Base method bodies annotated with bare parametric structs such as
  `x::Rational` / `y::Rational` no longer static-dispatch binary operators to
  an over-specific concrete specialization such as
  `*(Rational{BigInt}, Rational{BigInt})`.
- Function parameter typing now preserves the `JuliaType::Struct("Rational")`
  UnionAll-shaped annotation, and binary operator compilation skips the
  over-specific static dispatch path for bare parametric struct operands.
- The precompiled Base cache version is now 24 so stale direct-call bytecode is
  regenerated.
- Test: `rational::chunk_001` (`fixtures/rational/test_div_fld_cld_rem_mod.jl`)。

### @testset declared global String Any-carrier reads and concat ✅ (Issues #6268/#6269)

- String globals declared with `global s` inside macro-expanded `@testset`
  scopes can now execute `s = s * "-suffix"` without statically coercing an
  `Any` operand into Base's `Union{Char,String}` `*` signature.
- Declared-global reads now use non-slotized `LoadGlobalAny`, and declared
  globals infer as `Any`, so a later `global x; x = 42; @test x == 42` reads
  frame 0 and avoids stale String slot reads or false constant folding.
- Test: `strings::chunk_003` (`fixtures/strings/string_local_any_carrier_5081.jl`)。

### explicit Rational parametric constructor runtime dispatch fallback ✅ (Issue #6267)

- Explicit parametric Rational constructors such as
  `Rational{Int64}(Int8(3)//Int8(4))` no longer fall through to the raw struct
  constructor and fail with `Struct constructor expects 2 arguments, got 1`.
- When a concrete parametric constructor table has same-arity methods, the
  compiler now emits `CallTypedDispatch` even if static dispatch is too broad,
  letting runtime argument types choose between `x::Integer` and `x::Rational`.
- Test: `rational::chunk_001` (`fixtures/rational/parametric_typed_constructor.jl`)。

### heap-backed Rational unary float intrinsic conversion ✅ (Issue #6266)

- Rational inputs that compile directly to `CallBuiltin(Round)` or `SqrtF64`,
  such as `round(5//3)` and `sqrt(1//4)`, no longer fail with
  `expected numeric value, got StructRef(N)`.
- Unary float operations now use heap-aware numeric conversion for Rational and
  Irrational `StructRef` values while keeping primitive `Float16` / `Float32`
  result-width preservation.
- Tests: `rational::chunk_000` (`fixtures/rational/math_round.jl`) and
  `rational::chunk_001` (`fixtures/rational/vm_extraction_generic_5160.jl`)。

### linalg Array/ArrayOf rank-unknown matrix multiplication dispatch ✅ (Issue #6264)

- Compile-time `ValueType::Array` / `ValueType::ArrayOf(_)` no longer filters out
  LinearAlgebra matrix/vector `*` candidates just because rank is unknown.
- Runtime dispatch can now select the actual `AbstractMatrix, AbstractMatrix`
  method for values such as `C * nullspace(C)`, avoiding
  `MethodError operator(Matrix{Float64}, Matrix{Float64})`.
- Test: `linalg::chunk_001` (`fixtures/linalg/nullspace_logdet_adjoint.jl`)。

### Irrational DynamicPow inline Float64 fallback ✅ (Issue #6265)

- `^` with an Irrational singleton operand, such as `ℯ^2`, now stays on the inline
  dynamic operation path and uses the existing Irrational-to-`Float64` fallback.
- This prevents generic `^` method dispatch from recursively dispatching back to
  `^`, fixing the `log(ℯ^2)` stack overflow.
- Test: `math::chunk_000` (`fixtures/math/log_two_arg.jl`)。

### @testset global Dict haskey Any receiver fallback ✅ (Issue #6263)

- `haskey` on a `global` Dict inside a macro-expanded `@testset` local scope no
  longer compiles an `Any` receiver through a statically selected `Dict` method
  and errors with `Cannot convert Any to Dict`.
- `haskey(::Any, key)` / `haskey(::Dict, key)` now routes through
  `CallTypedDispatchOrBuiltin(DictHasKey, ...)`, preserving runtime method
  dispatch where applicable and falling back to the retained Dict probe.
- Test: `dict::chunk_000` (`fixtures/dict/testset_global_haskey_any_6263.jl`,
  `fixtures/dict/dict_local_any_carrier_5081.jl`)。

### ndims array DataType rank before value-method dispatch ✅ (Issue #6260)

- `ndims(Vector{Int})`, `ndims(Matrix{Int})`, and `ndims(Array{T,N})` now return
  the array type rank directly instead of dispatching to the value-array
  `ndims(a::Array)` method and reading `_size` from a DataType object.
- `ndims(::Type{T}) where {T<:Number}` still uses method dispatch for numeric
  type objects.
- Test: `arrays::chunk_000` (`fixtures/arrays/test_ndims_type_5118.jl`)。

### @testset lambda capture pre-analysis keeps scoped names ✅ (Issue #6261)

- Macro-expanded `@testset` LetBlock type pre-scan remains concrete-type isolated
  for #6256, but module-level lambda capture pre-analysis now keeps testset-local
  assignment names as `Any` capture candidates.
- Lambdas such as `x = 10; f = () -> x + 1` inside `@testset` no longer compile
  with `Undefined variable: x`.
- Test: `closures::chunk_000` (`fixtures/closures/testset_closure_capture.jl`)。

### while true dead-tail reflection Bottom preservation ✅ (Issue #6258)

- `Base.return_types` / `Base.infer_return_type` now preserve `Union{}` for
  empty-body `while true` functions even when bytecode still contains a dead
  literal tail after the self-loop jump.
- The reflection bytecode literal scan no longer overrides an inferred Bottom
  snapshot, and skips tiny code windows containing `Jump`.
- Tests: `type_inference::chunk_002` (`fixtures/type_inference/while_true_no_exit_4679.jl`)
  and `compile::abstract_interp::engine::tests::test_issue_6258_empty_while_true_dead_tail_infers_bottom`。

### dump-bytecode broken stdout pipe handling ✅ (Issue #6254)

- `sjulia --dump-bytecode` now writes through `io::Write` and treats stdout
  `BrokenPipe` as a clean exit instead of panicking when a downstream pipeline
  consumer exits early.
- Test: `sjulia_cli_dump_bytecode_tests::dump_bytecode_tolerates_closed_stdout_issue_6254`。

### Macro-expanded @testset local type-scope isolation ✅ (Issue #6256)

- Macro-expanded Pure Julia `@testset` `LetBlock`s that contain `_testset_begin!`
  now get Julia-compatible local type-scope isolation during local pre-scan and
  compile-time local map handling.
- Reusing the same local name across separate `@testset` blocks no longer keeps a
  stale `ComplexF64` slot/type shape when the later block assigns `ComplexF32`.
- Test: `fixtures/macro/testset_reuse_local_slot_type_6256.jl`。

### Mandelbrot Complex integer-power fast path ✅ (Issues #6252/#6255, refs #6253)

- Added concrete `Complex{Float64}` / `Complex{Float32}` Pure Julia methods for
  `abs2`, `+`, `-`, `*`, and `/`, so common Complex dynamic calls can run through
  field access and typed float arithmetic.
- `Complex{Float64/Float32}^Integer` now follows integer-power semantics instead of the
  analytic real-exponent path; the `n == 2` path computes the same result as `z*z`
  directly. This fixes the Mandelbrot `z^2 + c` output drift and removes the
  expensive `log`/`exp` path from the benchmark.
- `ComplexF32` arithmetic, `inv`, integer powers, and `abs2` now preserve
  `ComplexF32` / `Float32` result types.
- `benchmarks/mandelbrot_benchmark.jl` was left unchanged. Its sjulia ASCII output now
  matches Julia after stripping the timing line.
- Test: `fixtures/complex/float_integer_pow_6252_6255.jl`。

### Tuple bounded fallback after diagonal miss ✅ (Issue #6251, refs #5072)

- `Tuple{T,T}` diagonal methods now handle homogeneous real tuples while mixed real
  `Tuple{Int64,Float64}` falls back to independent `Tuple{<:Real,<:Real}`.
- Anonymous bounded TypeVar `_ <: Real` no longer behaves like a repeated binding across tuple slots.
- Non-Real tuple elements still raise `MethodError`, and direct plus `Any`-routed runtime calls match upstream Julia.
- Test: `fixtures/dispatch/tuple_bounded_fallback_after_diagonal_6251.jl`。

### Type/AbstractArray rank-TypeVar diagonal specificity ✅ (Issue #6249, refs #5072)

- Repeated `T` across `Type{T}` and `AbstractArray{T,N}` now outranks fixed
  `Type{Integer}, AbstractArray{<:Real,N}` for concrete `Type{Int64}` plus `Vector{Int64}` or
  `Matrix{Int64}` pairs.
- Abstract `Type{Integer}` bindings and exact `Type{Int64}, AbstractArray{Int64,N}` methods keep Julia-compatible precedence.
- Direct and `Any`-routed runtime calls match upstream Julia.
- Test: `fixtures/dispatch/type_abstract_array_rank_typevar_diagonal_6249.jl`。

### Type/AbstractArray rank-omitted diagonal specificity ✅ (Issue #6247, refs #5072)

- Repeated `T` across `Type{T}` and rank-omitted `AbstractArray{T}` now outranks fixed
  `Type{Integer}, AbstractArray{<:Real}` for concrete `Type{Int64}` plus `Vector{Int64}` or
  `Matrix{Int64}` pairs.
- Abstract `Type{Integer}` bindings and exact `Type{Int64}, AbstractArray{Int64}` methods keep Julia-compatible precedence.
- Direct and `Any`-routed runtime calls match upstream Julia.
- Test: `fixtures/dispatch/type_abstract_array_rank_omitted_diagonal_6247.jl`。

### Type/AbstractArray rank-1 diagonal specificity ✅ (Issue #6245, refs #5072)

- Repeated `T` across `Type{T}` and `AbstractArray{T,1}` now outranks fixed
  `Type{Integer}, AbstractArray{<:Real,1}` for concrete `Type{Int64}, Vector{Int64}` pairs.
- Abstract `Type{Integer}` bindings and exact `Type{Int64}, AbstractArray{Int64,1}` methods keep Julia-compatible precedence.
- Direct and `Any`-routed runtime calls match upstream Julia.
- Test: `fixtures/dispatch/type_abstract_array_rank1_diagonal_6245.jl`。

### Type/AbstractArray rank-2 diagonal specificity ✅ (Issue #6243, refs #5072)

- Repeated `T` across `Type{T}` and `AbstractArray{T,2}` now outranks fixed
  `Type{Integer}, AbstractArray{<:Real,2}` for concrete `Type{Int64}, Matrix{Int64}` pairs.
- Abstract `Type{Integer}` bindings and exact `Type{Int64}, AbstractArray{Int64,2}` methods keep Julia-compatible precedence.
- Direct and `Any`-routed runtime calls match upstream Julia.
- Test: `fixtures/dispatch/type_abstract_array_rank2_diagonal_6243.jl`。

### Type/AbstractMatrix diagonal specificity ✅ (Issue #6240, refs #5072)

- Repeated `T` across `Type{T}` and `AbstractMatrix{T}` now outranks fixed
  `Type{Integer}, AbstractMatrix{<:Real}` for concrete `Type{Int64}, Matrix{Int64}` pairs.
- Abstract `Type{Integer}` bindings and exact `Type{Int64}, AbstractMatrix{Int64}` methods keep Julia-compatible precedence.
- Direct and `Any`-routed runtime calls match upstream Julia.
- Test: `fixtures/dispatch/type_abstract_matrix_diagonal_6240.jl`。

### Type/AbstractVector diagonal specificity ✅ (Issue #6239, refs #5072)

- Repeated `T` across `Type{T}` and `AbstractVector{T}` now outranks fixed
  `Type{Integer}, AbstractVector{<:Real}` for concrete `Type{Int64}, Vector{Int64}` pairs.
- Abstract `Type{Integer}` bindings and exact `Type{Int64}, AbstractVector{Int64}` methods keep Julia-compatible precedence.
- Direct and `Any`-routed runtime calls match upstream Julia.
- Test: `fixtures/dispatch/type_abstract_vector_diagonal_6239.jl`。

### Type/matrix diagonal specificity ✅ (Issue #6237, refs #5072)

- Repeated `T` across `Type{T}` and `Matrix{T}` now outranks fixed
  `Type{Integer}, Matrix{<:Real}` for concrete `Type{Int64}, Matrix{Int64}` pairs.
- Abstract `Type{Integer}` bindings and exact `Type{Int64}, Matrix{Int64}` methods keep Julia-compatible precedence.
- Direct and `Any`-routed runtime calls match upstream Julia.
- Test: `fixtures/dispatch/type_matrix_diagonal_6237.jl`。

### Type/vector diagonal specificity ✅ (Issue #6235, refs #5072)

- Repeated `T` across `Type{T}` and `Vector{T}` now outranks fixed
  `Type{Integer}, Vector{<:Real}` for concrete `Type{Int64}, Vector{Int64}` pairs.
- Abstract `Type{Integer}` bindings and exact `Type{Int64}, Vector{Int64}` methods keep Julia-compatible precedence.
- Direct and `Any`-routed runtime calls match upstream Julia.
- Test: `fixtures/dispatch/type_vector_diagonal_6235.jl`。

### Type/value diagonal specificity ✅ (Issue #6233, refs #5072)

- Repeated `T` across `Type{T}` and a value argument now outranks fixed
  `Type{Integer}, Integer` for concrete `Type{Int64}, Int64` pairs.
- Abstract `Type{Integer}` bindings and exact `Type{Int64}, Int64` methods keep Julia-compatible precedence.
- Direct and `Any`-routed runtime calls match upstream Julia.
- Test: `fixtures/dispatch/type_value_diagonal_6233.jl`。

### Union specificity ✅ (Issue #6231, refs #5072)

- Finite `Union` methods now outrank broader supertypes when the actual argument is covered by
  a stricter Union arm.
- Broader Union arms such as `Union{Real,String}` do not overrank narrower supertypes like `Integer`.
- Direct and `Any`-routed runtime calls match upstream Julia.
- Test: `fixtures/dispatch/union_specificity_6231.jl`。

### Vector diagonal specificity ✅ (Issue #6229, refs #5072)

- Repeated `Vector{T}` methods now outrank independent `Vector{<:Real}` bounds when
  both vector arguments share the same concrete element type.
- Mixed element types still reject the diagonal binding and use the independent-bound method.
- Direct and `Any`-routed runtime calls match upstream Julia.
- Test: `fixtures/dispatch/vector_diagonal_specificity_6229.jl`。

### Nested Matrix literal rank-aware projection ✅ (Issue #6227, refs #6225/#5072)

- Nested array literal element projection now uses the inner literal rank:
  `[[1], [2]]` reports `Vector{Vector{Int64}}`, while `[[1 2], [3 4]]` reports
  `Vector{Matrix{Int64}}`.
- Runtime dispatch from `Any` selects `Vector{Matrix{T}}` over shallow `Vector{T}` for nested matrix values.
- Test: `fixtures/dispatch/nested_matrix_literal_rank_6227.jl`。

### Nested Vector literal runtime dispatch ✅ (Issue #6225, refs #5072)

- Homogeneous nested vector literals such as `[[1], [2]]` now preserve `Vector{Int64}` as
  the outer array's logical element type, so `typeof` reports `Vector{Vector{Int64}}`.
- Runtime dispatch from an imprecise `Any` slot now selects `Vector{Vector{T}}` over
  shallow `Vector{T}` for the preserved nested vector value.
- Test: `fixtures/dispatch/nested_vector_runtime_dispatch_6225.jl`。

### Invariant Vector TypeVar runtime specificity ✅ (Issue #6222, refs #5072)

- `CallTypedDispatch` が FunctionInfo-backed candidate matching を先に使うようになり、
  invariant `Vector{T}` occurrence を含む `where` method が wrapper 経由で誤選択されない。
- `f(::T, ::Vector{T}) where {T<:Real}` と `f(::Integer, ::Vector{<:Real})` は
  direct / wrapper のどちらでも `Vector{Int64}` と `Vector{Real}` に対して Julia と同じ fixed method を選ぶ。
- Test: `fixtures/dispatch/invariant_vector_typevar_runtime_6222.jl`。

### Tuple vararg ambiguity filtering ✅ (Issue #6220, refs #5072)

- competing tuple vararg methods で fixed prefix slot と vararg element の specificity が逆方向に分かれる場合は、
  scalar score の fixed-prefix bias へ落とさず Julia と同じ曖昧 `MethodError` にする。
- `Tuple{Vararg{Integer}}` vs `Tuple{Int64,Vararg{Any}}` は all-Int tuple で曖昧、
  empty tuple と mixed tail は unique method を選ぶ。
- Test: `fixtures/dispatch/tuple_vararg_ambiguity_6220.jl`。

### Tuple vararg specificity by actual shape ✅ (Issue #6218, refs #5072)

- competing tuple vararg method params を actual tuple length に合わせて展開し、
  `Tuple{Vararg{Int64}}` が `Tuple{Int64,Vararg{Any}}` より specific な all-Int tuple で勝つようにした。
- mixed tail は fixed-prefix fallback を使い、same-tail `Tuple{Int64,Vararg{Int64}}` は既存通り fixed-prefix method を選ぶ。
- declaration order に依存しない。
- Test: `fixtures/dispatch/tuple_vararg_specificity_6218.jl`。

### Empty vararg element specificity ✅ (Issue #6216, refs #5072)

- 空の unbounded vararg 呼び出しでも宣言された vararg element type を method specificity に使う。
- `f(xs::Int64...)` は `f(xs::Integer...)` より specific なので、`f()` / `f(1, 2)` の両方で
  Julia と同じ method を選択する。
- declaration order と fixed prefix 付き trailing vararg でも同じ選択を維持する。
- Test: `fixtures/dispatch/empty_vararg_specificity_6216.jl`。

### @generated direct body type-argument execution ✅ (Issue #6214, refs #5074)

- 小さい pure 関数の IR inliner が `@generated` method を通常関数として展開しないようにした。
- `@generated function f(x); x + 1; end; f(2)` は generated body の `x == Int64` で実行されるため、
  Julia と同じ `MethodError` になる。
- `return :(x + 1)` のように returned Expr payload 内の bare `x` を runtime 引数として評価する既存 path は維持する。
- Test: `fixtures/generated/direct_body_type_args_6214.jl`。

### Empty vararg unbound type parameter matching ✅ (Issue #6212, refs #5074)

- `xs::T... where T` の空 vararg 呼び出しでは `T` が制約されないため、
  body が `T` を読む場合は Julia と同じ `UndefVarError` を送出する。
- `xs` 自体を読む value-only path は引き続き `()` を返し、非空 homogeneous vararg では
  `T = Int64` のように束縛される。
- generated body の vararg type tuple slot でも同じく空 tuple から `T = Tuple{}` を推論しない。
- Test: `fixtures/generated/empty_vararg_unbound_type_param_6212.jl`。

### @generated Array static parameter body binding ✅ (Issue #6210, refs #5074)

- generated body 実行時にも通常 method call と同じ `where` static parameter binding を行い、
  `Array{T,N}` signature から `T` / `N` を抽出する。
- `@generated function f(a::Array{T,N}) where {N,T}; "N = $N, T = $T"; end` は
  Julia と同じく `Matrix{Float64}` 引数で `"N = 2, T = Float64"` を返す。
- positional/generated body argument slots の concrete type object 差し替えは維持しつつ、
  `T` は frame type binding、`N` は rank value binding として参照できる。
- Test: `fixtures/generated/array_static_params_6210.jl`。

### @generated vararg `$args` interpolation ✅ (Issue #6208, refs #5074)

- generated syntactic-unquote に成功した body でも `$` interpolation を含む場合は generated metadata を
  保持し、`x...` slot を concrete argument type tuple に差し替える。
- `@generated function f(x...); :($x); end` は Julia と同じく `f(1, 2) == (Int64, Int64)` を返し、
  runtime value tuple `(1, 2)` を返さない。
- mixed interpolation/runtime refs は returned-Expr eval、bare-only unquote は通常 runtime method という
  既存の分岐を維持する。
- Test: `fixtures/generated/vararg_interpolation_6208.jl`。

### @generated syntactic-unquote default arguments ✅ (Issue #6206, refs #5074)

- generated syntactic-unquote に成功した method は generated metadata を付けず、通常 runtime method として
  optional positional default wrapper から呼べるようにした。
- `@generated function f(x, a=5); :(x + a); end` は Julia と同じく `f(7) == 12`、
  `f(7, 6) == 13` を返し、runtime 引数が `DataType` に差し替わらない。
- returned-Expr fallback / cache 経路は generated metadata を維持するため、既存の body type binding と
  staged Expr cache coverage はそのまま残る。
- Test: `fixtures/generated/unquote_default_args_6206.jl`。

## 最新対応 (2026-06-07)

### @generated mixed interpolation/runtime argument refs ✅ (Issue #6204, refs #5074)

- generated returned code の quote 内で `$arg` interpolation と裸の同名 runtime argument 参照が
  同居する場合、Phase 3 syntactic-unquote を避けて returned-Expr eval へ委譲する。
- `:(($a, $b, a, b))` は Julia と同じく `(Int64, (Int64, Int64), 1, (2, 3))` を返し、
  generated-time type interpolation と runtime frame lookup を分離する。
- 既存の `$N` / `$(N + 1)` syntactic-unquote fixtures は維持し、mixed name collision のみ fallback する。
- Test: `fixtures/generated/mixed_interpolation_runtime_args_6204.jl`。

### Runtime bounded dispatch from Any containers ✅ (Issue #6202, refs #5926/#5072)

- `Any[Int64, Float64]` や `Any[[1, 2], Float64[...]]` から取り出した runtime value でも、
  bounded where method の tighter bound (`T<:Integer`) が looser bound (`T<:Real`) より優先される。
- `CallDynamic` は user candidate に限り `FunctionInfo` metadata を使って runtime dispatch を採点し、
  `Type{T}` / `Vector{T}` の candidate 文字列だけでは表現できない `where` bounds を保持する。
- 非 `DataType` value でも `get_value_julia_type(...).extract_type_bindings(...)` から
  `Vector{T}` の `T` を bind し、bound check を通す。
- Base/prelude の Array wrapper candidate は legacy native array 境界で従来どおり除外し、
  `dispatch_array_type` と `similar(::Any)` 系の既存 fallback を保つ。
- Test: `fixtures/dispatch/runtime_bounded_dispatch_from_any_6202.jl` と VM unit
  `test_find_best_method_index_issue_6202_*`。

### Predecoded typed loop executable blocks ✅ (Issue #6169)

- `vm/executable.rs` を追加し、hot typed loop を bytecode から predecode して実行する
  conservative executable layer を VM に組み込んだ。
- `TypedLoopBlock` は Mandelbrot 固有名や式を持たず、typed slot arithmetic、typed compare branch、
  internal forward branch、loopback、`RandF64`、counted-for increment を executable op として実行する。
- runtime slot 型や bytecode 形が合わない場合は通常の stack interpreter に fallback する。
- `GcdI64Loop` は gcd hot path 用 block として残し、runtime `CallSpecialize` append 後の bytecode にも
  executable predecode を適用する。
- VM-only Mandelbrot bench は `run_only` 約 `49.7 ms` → `15.1 ms`、
  `clone_new_program_run` 約 `54.0 ms` → `21.4 ms` に改善した（短縮 Criterion run）。
- `estimate_pi` の typed-friendly counted `for` loop（`rand()`, internal `if`, `x*x`, `1.0`）も
  `TypedLoopBlock` で実行できる。
- Verification: `cargo check -p subset_julia_vm --lib`、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm executable::tests`、
  `timeout 1800 cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark -- --warm-up-time 1 --measurement-time 1 --sample-size 10`、
  `timeout 1800 cargo bench -p subset_julia_vm --bench calc_pi_benchmark -- --warm-up-time 1 --measurement-time 1 --sample-size 10`。

### estimate_pi loop inference/lowering fast path ✅ (Issue #6178)

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

### VM bytecode dump and Mandelbrot branch/increment fusion ✅ (Issue #6159)

- `sjulia --dump-bytecode` を追加し、user functions と main tail の VM bytecode、
  slot table、slot/call/specialization の inline annotation を CLI から確認できるようにした。
- n-ary `+` / `*` call の type inference を binary op と同じ fold 形に寄せ、
  `zi = 2.0 * zr * zi + ci` が `Float64` slot path に落ちるようにした。
- Float64 compare false branch fusion (`JumpIfNotLeF64` 系) を追加し、NaN を含む
  ordered comparison でも `compare; JumpIfZero` と同じ意味論を保つようにした。
- `LoadSlotI64; PushI64(k); Add/SubI64; StoreSlotI64` を
  `AddConstI64Slot(slot, delta)` に融合し、Mandelbrot の `iter += 1` / `x += 1` /
  `y += 1` から load/add/store 列を削った。
- Precomputed bytecode 計測では `benchmarks/vm_mandelbrot.jl` の `Vm::run()`
  median が baseline `0.0509s` から current `0.046546s` へ改善（約 8.6% faster）。
- 2-3x を狙う抜本案（branch-context lowering、loop-local typed registerization、
  superblocks、VM-only benchmark formalization）は Issue #6159 に follow-up として整理した。
- Verification: `cargo check --bin sjulia --features repl`、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm compile::peephole::tests::test_f64_compare_jump_false_branch_fusion compile::peephole::tests::test_slot_const_increment_fusion`、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm --test float_compare_jump_fusion_tests --test slot_const_increment_fusion_tests`。

### VM-only Mandelbrot Criterion benchmark formalization ✅ (Issue #6159)

- `subset_julia_vm/benches/vm_mandelbrot_benchmark.rs` を追加し、
  `benchmarks/vm_mandelbrot.jl` の precomputed bytecode を使った VM-only 測定を正式な Criterion bench にした。
- `run_only` は `Vm::run()` を、`clone_new_program_run` は
  `CompiledProgram::clone + Vm::new_program + run` を測るため、frontend/startup 約 5 秒に埋もれず
  branch-context lowering や registerization の効果を追える。
- 実行方法: `cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark`。
- Verification: `cargo check -p subset_julia_vm --bench vm_mandelbrot_benchmark`、
  `timeout 1800 cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark -- --warm-up-time 1 --measurement-time 1 --sample-size 10`
  pass。短縮 run では `run_only` が約 `49.5 ms`、`clone_new_program_run` が約 `54.8 ms`。

### Branch-context lowering for Bool conditions ✅ (Issue #6159, Bug #6162)

- `if` / `while` / ternary / implicit-return `if` の条件コンテキストで
  `&&` / `||` を stack Bool に materialize せず、false/true branch へ直接 lower するようにした。
- Mandelbrot の `while zr*zr + zi*zi <= 4.0 && iter < maxiter` は
  `JumpIfNotLeF64(exit); JumpIfGeI64(exit)` の連続 branch になり、
  `PushBool(false)` / 条件 materialization 用 `Jump` / 後段 `JumpIfZero` が消えた。
- `SJULIA_VM_PROFILE=1 target/release/sjulia benchmarks/vm_mandelbrot.jl`:
  total instructions は `5,096,052` から `4,040,622` へ減少（約 `20.7%` fewer）。
- Formal Criterion bench の参考値:
  `run_only` は約 `47.2 ms`、`clone_new_program_run` は約 `51.9 ms`
  （直前の formalization smoke はそれぞれ約 `49.5 ms` / `54.8 ms`）。
- Verification: `timeout 1800 cargo nextest run --release -p subset_julia_vm --test branch_context_lowering_tests`、
  `timeout 1800 cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark -- --warm-up-time 2 --measurement-time 3 --sample-size 20`、
  `cargo run --release --bin sjulia --features repl -- --dump-bytecode benchmarks/vm_mandelbrot.jl`。

### calc_pi VM benchmark runner ✅ (Issue #6159)

- `benchmarks/calc_pi_benchmark.jl` を VM benchmark 対象として扱うため、
  `benchmarks/scripts/run_vm_calc_pi.sh` を追加した。
- `calc_pi_benchmark.jl` は `@time` 行が非決定的なので、runner は Julia / sjulia の
  deterministic な `N=...` result lines だけを比較し、全スクリプトの process wall time を記録する。
- 対象 workload は `N=100` / `N=500` / `N=1000` の gcd-heavy nested loop。
- 実行方法: `RUNS=3 ./benchmarks/scripts/run_vm_calc_pi.sh`。
- Verification: `RUNS=1 ./benchmarks/scripts/run_vm_calc_pi.sh` pass。
  Result lines は Julia / sjulia で一致し、参考 wall time は Julia `0.23s`、sjulia VM `5.78s`。

### VM Mandelbrot F64 slot superinstructions ✅ (Issue #4301)

- `Float64` slot square / load-op superinstructions を追加し、Mandelbrot inner loop の
  `zr*zr`、`zi*zi`、`2.0*zr`、`...+cr` などの hot bytecode を短縮した。
- generic `LoadSlot` / typed `LoadSlotF64` の runtime fast path で、slot の実値が `Float64` の場合の
  fall-through `x*x` / `x*y` と residual dynamic add を直接処理する。
- Precomputed bytecode 計測では `benchmarks/vm_mandelbrot.jl` の `Vm::run()` median が `0.049954s`、
  `clone + Vm::new_program + run` median が `0.054315s`。
- Verification: `cargo check -p subset_julia_vm --features repl`、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm compile::peephole::tests::test_slot_f64_square_and_load_op_fusion`、
  `timeout 1800 cargo nextest run --release mandelbrot`、`git diff --check` pass。

### @generated signature Expr cache compatibility ✅ (Issue #5936)

- `@generated` fallback が返した staged `Expr` を、関数 index と concrete argument signature
  (`Tuple{argtypes...}` 相当) ごとに VM 内で cache するようにした。
- cache hit では generated body を再実行せず、cached Expr を現在の call frame 上で `eval` するため、
  `@generated function f(x); counter[] += 1; return :(x); end` は同じ `Int64` 呼び出しで counter を増やさない。
- Full generated staging driver / lower-to-bytecode cache ではなく、#5936 の returned-Expr fallback に対する
  tuple-signature cache compatibility slice。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/signature_cache_5936.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。

### @generated body argument type binding ✅ (Issue #5936)

- generated body の positional / vararg argument slots を runtime value ではなく concrete argument type object
  (`Int64`, `Type{Int64}`, `(Int64, Float64)` など) で実行するようにした。
- cache hit の returned staged `Expr` eval は引き続き実引数 frame 上で実行するため、body の型分岐と
  staged expression の runtime 引数参照を分離する。
- Full generated staging driver ではなく、#5936 の generated-body environment compatibility slice。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/body_arg_types_5936.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。

### @generated splat call compatibility ✅ (Issue #5936)

- `f(args...)` 経由の generated call でも direct call と同じ concrete argument type binding と
  returned staged `Expr` signature cache を使うようにした。
- named generated splat call の expanded args から `where` type params を束縛し、cache hit と
  first miss の returned `Expr` eval は実引数 frame で行い、generated body 実行だけ type-object slots へ差し替える。
- Full generated staging driver ではなく、#5936 の generated call-site coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/splat_call_5936.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。`timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated function alias splat calls ✅ (Issues #6163/#5936)

- function-valued alias 経由の `alias(args...)` が `CallFunctionVariableWithKwargsSplat` に lower される場合も、
  generated body argument slots を concrete type object に差し替えるようにした。
- named splat call と同じく、cache hit と first miss の returned `Expr` eval は実引数 frame で行う。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/alias_splat_6163.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo nextest run --release -p subset_julia_vm --test float_compare_jump_fusion_tests`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated function alias calls ✅ (Issues #6166/#5936)

- function-valued alias 経由の `alias(x)` が `CallFunctionVariable` に lower される場合も、
  generated body argument slots を concrete type object に差し替えるようにした。
- direct call と同じく、cache hit と first miss の returned `Expr` eval は実引数 frame で行う。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/alias_call_6166.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated keyword calls ✅ (Issues #6171/#5936)

- `CallWithKwargs` / `CallWithKwargsSplat` 経由の generated call でも、
  positional と keyword slots を generated body 実行時だけ concrete type object に差し替えるようにした。
- generated `Expr` cache key に keyword argument type を含め、同じ positional 型でも
  `y::Int64` と `y::Float64` の staged `Expr` を混同しないようにした。
- cache hit と first miss の returned `Expr` eval は、keyword runtime values を保持した frame で行う。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/keyword_calls_6171.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated quoted Expr payload eval ✅ (Issues #6172/#5936)

- generated body の `cond ? :(x + 1) : :(0)` のような ternary tail/return も、
  両分岐が staged Expr 候補なら `GeneratedEval` で包むようにした。
- `GeneratedEval` は `QuoteNode(Expr)` payload を一段 unwrap した後、その `Expr` を runtime argument frame 上で
  eval するようにした。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/quoted_expr_6172.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated calls through map ✅ (Issues #6170/#5936)

- value-mode / numeric-mode HOF helper が直接 frame を作る経路でも、generated body argument slots を
  concrete type object に差し替えるようにした。
- HOF state machine の frame-return path は維持し、first miss の returned `Expr` eval は runtime element frame 上で行う。
- Verification: upstream Julia / direct `target/release/sjulia` で
  `generated/map_call_6170.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000`
  pass。`timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated returned Expr(:return) eval ✅ (Issues #6183/#5936)

- generated fallback の returned `Expr` eval が `Expr(:return, value_expr)` を staged result marker として扱い、
  payload を runtime argument frame 上で評価するようにした。
- `@generated function f(x); return Expr(:return, Expr(:call, :+, :x, 3)); end` は Julia と同じく
  `f(4) == 7` になる。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback eval head coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/return_head_6183.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated returned Expr(:let) eval ✅ (Issues #6185/#5936)

- generated fallback の returned `Expr` eval が `Expr(:let, binding..., body)` を一時 eval frame 上で評価し、
  binding を body にだけ見せるようにした。
- `Expr(:let, Expr(:(=), :y, Expr(:call, :+, :x, 2)), Expr(:call, :*, :y, 3))` のように
  binding RHS と body が runtime 引数を読む代表ケースを Julia と同じ結果にした。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback eval head coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/let_head_6185.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated returned Expr(:call, GlobalRef, ...) eval ✅ (Issues #6187/#5936)

- generated fallback の returned `Expr(:call, callee, args...)` eval が `GlobalRef(Base, :+)` などの
  GlobalRef callee を qualified function dispatch へ渡せるようにした。
- `Expr(:call, GlobalRef(Base, :+), :x, 4)` / `GlobalRef(Base, :*)` の代表ケースを Julia と同じ結果にした。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback call-callee coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/globalref_call_6187.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### eval/generated Expr(:copyast) head ✅ (Issues #6190/#5936)

- runtime `eval` が `Expr(:copyast, QuoteNode(ex))` を評価し、quoted AST payload を data として返すようにした。
- generated fallback の returned `Expr(:copyast, QuoteNode(Expr(:call, :+, :x, 6)))` も Julia と同じく
  `Expr(:call, :+, :x, 6)` 値を返す。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback eval head coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/copyast_head_6190.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### eval/generated Expr(:comparison) chains ✅ (Issues #6192/#5936)

- runtime `eval` が `Expr(:comparison, value, op, value, op, value...)` の全ペアを左から評価し、
  最初の false で `false` を返すようにした。
- generated fallback の returned `Expr(:comparison, 1, :<, 2, :>, 3)` が Julia と同じく `false` になる。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback eval head correctness slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/comparison_chain_6192.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### eval/generated Expr(:elseif) head ✅ (Issues #6194/#5936)

- runtime `eval` が `Expr(:elseif, cond, then[, else])` を `Expr(:if, ...)` と同じ conditional head として
  評価するようにした。
- generated fallback の returned `Expr(:elseif, B, 10, 20)` が Julia と同じく `Val(true)` で `10`、
  `Val(false)` で `20` になる。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback eval head coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/elseif_head_6194.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated returned keyword Expr calls ✅ (Issues #6196/#5936)

- generated fallback の returned `Expr(:call, callee, Expr(:parameters, Expr(:kw, ...)), args...)` eval が
  keyword entries を positional args から分離し、既存 runtime kwargs dispatch に渡すようにした。
- `Expr(:call, :f, Expr(:parameters, Expr(:kw, :y, 5), Expr(:kw, :z, 3)), :x)` の代表ケースが
  Julia と同じ keyword binding result になる。
- Full generated staging driver ではなく、#5936 の returned-Expr fallback keyword-call AST coverage slice。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/keyword_expr_6196.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated returned block/logical/quote Expr heads ✅ (Issue #5936)

- `Expr(:block, ...)` と `Expr(:(=), ...)` assignment は generated returned-Expr eval 経路で
  Julia と同じ sequential body として扱えることを fixture 化した。
- `Expr(:&&, ...)` / `Expr(:||, ...)` short-circuit heads と `Expr(:quote, ...)` AST-data return も
  同じ compatibility path の代表ケースとして固定した。
- これは full lower-to-bytecode staging driver ではなく、既存 returned-Expr compatibility path の回帰固定。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/block_logical_quote_5936.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### @generated staged loop Expr reproduction ✅ (Issue #5936)

- #5936 本文の `@generated function sumn(::Val{N}) where N; ex = :(0 + 0); for i in 1:N; ex = :($ex + $i); end; return ex; end`
  代表再現を fixture 化した。
- generated body が loop で `Expr` を組み立て、返却された staged `Expr` を runtime eval して
  `Val(3) == 6` / `Val(5) == 15` を返す。
- これは full lower-to-bytecode staging driver ではなく、既存 returned-Expr compatibility path の回帰固定。
- Verification: upstream Julia / direct `target/release/sjulia` で `generated/staged_loop_expr_5936.jl` pass。
  `timeout 1800 cargo nextest run --release --test fixture_tests generated::chunk_000` pass。
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass。
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### Function-table precise callee backedges ✅ (Issue #5939)

- precise な user-function call は、method-table dispatch と同じ `method_edges` に observed callee argtypes を stamp するようにした。
- `function_table` 経由で `callee(::Int64)` だけを呼んだ caller cache は、後続の `callee(::Float64)` mutation では retire しない。
- `CachedReturn` / Base cache schema は変えず、imprecise args や arity/type が明確でない call は従来どおり bare edge fallback に残す。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939 issue_5603` pass（17/17）。

### PartialStruct precise callee backedges ✅ (Issue #5939)

- PartialStruct return inference の user-function call も、precise な function-table call なら
  `method_edges` に observed callee argtypes を stamp するようにした。
- `outer(::Int64)` の PartialStruct side-cache が `inner(::Int64)` だけに依存する場合、後続の
  `inner(::Float64)` mutation では caller の PartialStruct fact を retire しない。
- arity/type binding が明確でない call は従来どおり bare edge fallback に残し、健全性優先の invalidation を維持する。
- Verification: `timeout 1800 cargo nextest run --release -p subset_julia_vm --lib issue_5939 issue_5603`
  pass（18/18）。

### Method-edge transitive global reads ✅ (Issues #6176/#5939)

- precise `DispatchedMethodEdge` 経由の caller cache が、typed callee method identity
  (`callee(Int64)` など) に記録された `global_reads` も fold するようにした。
- `caller(::Int64) -> callee(::Int64) -> G` のような経路で、`G` の binding change が caller cache を
  targeted に retire することを engine test で固定した。
- Bare callee name と method identity の両方を読むため、nullary/legacy dependency の互換性は維持する。
- Verification: `timeout 1800 cargo test -p subset_julia_vm compile::abstract_interp::engine::tests::test_issue_5939_method_edge_transitive_global_read_invalidates_caller --release`
  pass。`timeout 1800 cargo nextest run --release -p subset_julia_vm --lib issue_5939 issue_5603 issue_4285`
  pass（24/24）。

### Binding invalidation method-edge cleanup ✅ (Issue #5939)

- binding change で cache を retire した関数について、`global_binding_dependencies` と
  `function_dependencies` だけでなく `method_dependencies` も clear するようにした。
- `caller(::Int64) -> callee(::Int64) -> G` のような precise method-edge 経由の global-read cache は、
  `G` の binding change 後に古い method-edge record を残さず、再推論で current world の dependency を作り直す。
- Verification: `timeout 1800 cargo test -p subset_julia_vm compile::abstract_interp::engine::tests::test_issue_5939_method_edge_transitive_global_read_invalidates_caller --release`
  pass。`timeout 1800 cargo nextest run --release -p subset_julia_vm --lib issue_5939 issue_5603 issue_4285`
  pass（26/26）。

### Method-edge transitive dependency propagation ✅ (Issues #6179/#5939)

- `record_call_dependency` / `record_method_call_dependency` が、callee の bare name だけでなく
  function-table method identity key (`callee(Int64)` など) からも transitive dependency edges を fold するようにした。
- cold callee inference では最初の caller edge 記録時点で callee dependencies が未確定なため、
  callee inference 完了後に dependency recording を再実行し、dedupe しつつ transitive method edges を取り込む。
- `caller(::Int64) -> mid(::Int64) -> leaf(::Int64)` の cache が leaf mutation で targeted に retire することを固定した。
- Verification: `timeout 1800 cargo test -p subset_julia_vm compile::abstract_interp::engine::tests::test_issue_5939_method_identity_dependency_edges_propagate_transitively --release`
  pass。`timeout 1800 cargo nextest run --release -p subset_julia_vm --lib issue_5939 issue_5603 issue_4285`
  pass（25/25）。

### PartialStruct transitive method-edge propagation ✅ (Issues #6181/#5939)

- PartialStruct return inference でも cold callee inference 完了後に precise dependency recording を再実行し、
  caller side-cache entry が callee method identity の transitive edges を取り込むようにした。
- `outer(::Int64) -> mid(::Int64) -> inner(::Int64)` の PartialStruct fact が、`inner(::Int64)` mutation で
  targeted に retire することを固定した。
- Verification: `timeout 1800 cargo test -p subset_julia_vm compile::abstract_interp::engine::tests::test_issue_5939_partial_struct_method_edges_propagate_transitively --release`
  pass。`timeout 1800 cargo nextest run --release -p subset_julia_vm --lib issue_5939 issue_5603 issue_4285`
  pass（26/26）。

### Limited/tentative side-cache precise method edges ✅ (Issue #5939)

- `limited_results` と `tentative_results` の side-cache entry も `DispatchedMethodEdge` による
  signature-aware method invalidation を使うことを engine test で固定した。
- `callee(::Int64)` に依存する limited/tentative entry は、後続の `callee(::Float64)` mutation では残り、
  `callee(::Int64)` mutation で targeted に retire する。
- Verification: `timeout 1800 cargo test -p subset_julia_vm compile::abstract_interp::engine::tests::test_issue_5939_side_cache_method_edges_preserve_unmatched_callee_mutation --release`
  pass。`timeout 1800 cargo nextest run --release -p subset_julia_vm --lib issue_5939 issue_5603 issue_4285`
  pass。

### Call-site MethodInstance cache keys ✅ (Issue #5939)

- user-function call-site inference の return cache key を bare callee name ではなく、
  callee の primary method identity (`name(declared_param_types)`) から作るようにした。
- `infer_function_with_arg_types` 直呼びと、関数 body 内からの interprocedural inference が同じ
  MethodInstance-oriented key contract を使うため、legacy bare-name cache entry の再生成を防ぐ。
- Legacy `get_cached_return_type(name, args)` は unique primary-key entry への fallback を維持し、
  既存の name-based lookup 互換性は残す。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939 issue_5603` pass（15/15）。

### Diagonal morespecific dominance coverage ✅ (Issue #5926)

- `Tuple{T,T} where T` が `Tuple{Any,Any}` fallback より specific になる #5926 の diagonal-family contract を、
  compile-time `MethodTable::dispatch` と runtime `Vm::find_best_method_index` の両方で regression test 化した。
- same-typed args は diagonal method を選び、mixed args は diagonal rule を満たさず `Any,Any` fallback に戻ることも固定。
- Full topological morespecific replacement ではなく、既存 dominance pre-check が両選択サイトで同じ
  diagonal behavior を維持するための coverage slice。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926_method_table_dominance_selects_diagonal_over_any_any test_find_best_method_index_issue_5926_dominance_selects_diagonal_over_any_any` pass（2/2）。

### Method-identity dependency stamps ✅ (Issue #5939)

- inference 中の dependency map key を bare `func.name` ではなく primary cache identity に寄せるため、
  lexical `active_function` と invalidation 用 `active_dependency_key` を分離した。
- 同名の別メソッド body が 1 つの dependency bucket を共有し、片方の precise callee edge がもう片方の
  cache entry に stamp される過剰失効を防ぐ。
- Serialized `CachedReturn` / persisted `InferenceCacheKey` の format は変えず、#5939 の
  method-instance backedge 精密化を内部 map key の粒度から前進させる slice。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939 issue_5603` pass（14/14）、
  `timeout 1800 cargo check -p subset_julia_vm --lib` pass、
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### Dispatch-winner based method invalidation ✅ (Issue #5939)

- same-name method mutation invalidation を「mutated signature が call argtypes に単に match するか」ではなく、
  post-mutation dispatch winner かどうかで判定するようにした。
- `f(::Int64)` の cache は `f(::Any)` 追加/置換では retire せず、逆に `f(::Any)` cache は
  `f(::Int64)` 追加で retire するため、#5939 の method-identity 精度に一段近づく。
- precise method-edge invalidation も同じ winner 判定を使い、callee の less-specific method mutation が
  more-specific callee に dispatch した caller を過剰 invalidation しない。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939 issue_5603` pass（13/13）。

### @generated returned Expr eval head coverage ✅ (Issue #5936)

- `@generated` fallback が返した staged `Expr` を `eval(...)` する経路で、
  `:tuple` / `:vect` / `:if` / `:curly` / `:string` / `:ref` head が実際の generated body として動くことを
  fixture で固定した。
- Full generated staging driver ではなく、#5927-#5932 の eval head support と #5936 の returned-Expr
  compatibility を接続する regression coverage。
- Verification: upstream Julia / `target/release/sjulia` direct で
  `generated/expr_head_eval_5936.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::`
  pass。

### Precise method-table dependency edge stamping ✅ (Issue #5939)

- method-table dispatch が precise argtypes で成功した caller cache は、legacy bare `edges` ではなく
  `method_edges` に observed callee argtypes を stamp する contract を regression test で固定した。
- `callee(::Float64)` の mutation が `callee(::Int64)` だけに依存した caller を retire しない #5603 の
  既存挙動を、#5939 の bare-edge 削減前提として明文化する。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939 issue_5603` pass（12/12）。

### Structured MethodInstanceKey groundwork ✅ (Issue #5939)

- `InferenceCacheKey.fn_id` の legacy `name(declared_param_types)` 文字列を直接組み立てる代わりに、
  structured `MethodInstanceKey` から legacy projection を生成する経路を追加した。
- `MethodInstanceKey` は function 名、declared arg types、where type params、vararg metadata を保持し、
  #5939 の method-identity cache/backedge key 置換で string parse 依存を外す足場にする。
- Persisted inference cache key はまだ `InferenceCacheKey` のままなので、Base cache format / version は変更しない。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939` pass（4/4）。

### Origin-fenced morespecific dominance pre-check ✅ (Issue #5926)

- #5926 の dominance pre-check が Base-origin method を winner にするとき、user-origin 候補が同じ
  candidate set に含まれる場合は pre-check で選ばず、既存の score path へ戻す。
- compile-time `MethodTable::dispatch_inner` と runtime `Vm::find_best_method_index_uncached` の
  両選択サイトで同じ origin fence を使う。
- Full morespecific 統合ではなく、Base method が user candidate を dominance override だけで
  cross-origin に上書きする codegen hazard を抑える slice。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926` pass（10/10）。

### VM runtime base-function origin context ✅ (Issue #5926)

- `Vm` に `CompiledProgram::base_function_count` を保持させ、runtime dispatch mirror でも
  Base/prelude prefix と user function の origin を判定できるようにした。
- `MethodTable` 側の origin context と揃え、後続の morespecific dominance fence が compile-time /
  runtime の両選択サイトで同じ Base/user origin 条件を使える足場にする。
- Full morespecific 統合ではなく、#5926 の runtime origin-visibility groundwork。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926` pass（8/8）。

### MethodTable base-function origin context ✅ (Issue #5926)

- `MethodTable` に `base_function_count` を非永続 dispatch context として持たせ、
  compiler の method-table projection 時に `Program::base_function_count` を thread する。
- `base_function_count` は cache format へ serialize せず、cached method tables でも compile 時に再設定する。
  これにより後続の morespecific dominance fence は `is_base_extension` ではなく
  `global_index < base_function_count` で Base/user origin を判定できる。
- Full morespecific 統合ではなく、#5926 の origin-visibility groundwork。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926_method_table_tracks_base_function_count_for_origin_fences`
  pass。`timeout 1800 cargo nextest run --release --lib issue_5926` pass（7/7）。

### @generated implicit returned Expr eval compatibility ✅ (Issue #5936)

- `@generated` の full-body compatibility fallback で、body 最後の expression が quote/`Expr(...)` 由来の
  staged `Expr` なら `eval(...)` に包み、implicit return でも評価結果を返す。
- 明示 `return ex` の wrapping と同じく、syntactic-unquote path はそのまま使い、loop body などの
  非 tail expression は評価しない。
- Full generated staging driver ではなく、#5936 の implicit returned-Expr compatibility slice。
- Verification: upstream Julia / `target/release/sjulia` direct で
  `generated/implicit_return_expr_eval_5936.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::`
  pass。

### Reflection `hasmethod` sees LinearAlgebra Diagonal Base extensions ✅ (Issue #6124)

- VM reflection preserves Base-extension metadata on `FunctionInfo` and uses it
  when matching `methods` / `hasmethod` / `which` names.
- `LinearAlgebra` methods written as `Base.:*` are visible through `*`, so
  `Matrix{Float64} * LinearAlgebra.Diagonal{Float64}` reports a matching method.
- Verification: upstream Julia fixture pass, direct `target/release/sjulia` repro pass,
  `timeout 1800 cargo nextest run --release --test fixture_tests linalg::` pass.

### @generated returned Expr eval compatibility ✅ (Issue #5936)

- `@generated` の full-body compatibility fallback で、明示 `return ex` が返す quote/`Expr(...)` 由来の staged `Expr` を
  そのまま runtime value にせず、`eval(ex)` として評価結果を返す。
- `try_unquote_generated_block` / `try_unquote_generated_short_body` が扱える syntactic-unquote path は
  既存どおり unquoted IR を使い、今回の `eval(...)` wrapping は fallback path の staged-Expr return に限定する。
- Full generated staging driver ではなく、#5936 の returned-Expr compatibility slice。
- Verification: upstream Julia / `target/release/sjulia` direct で
  `generated/return_expr_eval_5936.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::`
  pass。

### @generated parenthesized unquote expression ✅ (Issue #5936)

- `@generated` の syntactic-unquote compatibility path で `$ident` だけでなく `$(expr)` を lower する。
- `@generated f(::Val{N}) where N = :($(N + 1) * 2)` のような parenthesized interpolation は
  generated-unquote 中だけ inner expression として扱い、quote 外の `$` は引き続き unsupported/error のままにする。
- Full generated staging driver ではなく、#5936 本丸前提の expression-splicing slice。
- Verification: upstream Julia / `target/release/sjulia` direct で
  `generated/paren_unquote_expr_5936.jl` pass。`timeout 1800 cargo nextest run --release --test fixture_tests generated::`
  pass。

### Legacy inference bare-key co-write removal ✅ (Issue #5939)

- `infer_function` / `infer_function_with_arg_types` は non-nullary method result を
  primary `inference_cache_function_id(func)` key のみに保存し、legacy bare `func.name` key の
  co-write をやめた。
- `get_cached_return_type(name, argtypes)` は legacy 互換の bare-name lookup として残すが、
  matching primary key が一意な場合だけ fallback する。`f(::Any)` と `f(::Int64)` のように同じ
  call-site argtypes で複数 method identity が見える場合は first-writer を返さず miss にする。
- Verification: `timeout 1800 cargo nextest run --release --lib test_issue_5939_primary_keys_preserve_method_identity_without_bare_co_write issue_5939_cache_key_exposes_base_function_id test_issue_5939_bare_callee_edge_overinvalidates_unmatched_signature test_issue_4271_method_replacement_invalidates_return_cache test_issue_4271_unrelated_method_mutation_preserves_other_cache test_issue_5603_method_mutation_preserves_unmatched_same_name_cache`
  pass（6/6）。

### Inference cache base function id projection ✅ (Issue #5939)

- `InferenceCacheKey::base_fn_id()` を追加し、`name(declared_param_types)` から bare `name` を取り出す
  #5939 の legacy projection を key 型側へ集約した。
- `InferenceEngine` の invalidation / dependency stamping は ad hoc な string helper ではなく
  `InferenceCacheKey` の projection を使うため、将来 `MethodInstanceKey` へ置き換える際の監査範囲が狭くなる。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5939_cache_key_exposes_base_function_id test_issue_5939_primary_keys_preserve_method_identity_without_bare_co_write test_issue_5939_bare_callee_edge_overinvalidates_unmatched_signature`
  pass（3/3）。

### Method identity cache lookup helper ✅ (Issue #5939)

- `InferenceEngine` の test helper として、legacy bare-name key ではなく
  `inference_cache_function_id(func)` で primary method-identity cache key を読む経路を追加した。
- #5939 の `MethodInstanceKey` 移行では lookup contract を bare name から method identity へ寄せる必要があるため、
  primary key では `f(::Any)` と `f(::Int64)` が分離して読めることを
  regression test で固定した。
- Verification: `timeout 1800 cargo nextest run --release --lib test_issue_5939_primary_keys_preserve_method_identity_without_bare_co_write`
  pass（1/1）。

### Web Playground startup warmup before first Run ✅ (Issue #6127)

- `web/app.js` は WASM と Monaco の load 後、Run button を有効化する前に
  `warmupWasm()` を await する。
- startup warmup は `using Plots; plot(sin)` を同じ `run_from_source` path で実行し、
  cache-enabled WASM artifact に残る初回 source execution cold path を user Run 前に通す。
- Verification: `node --check web/app.js` pass。local server + Playwright Chrome channel で
  `WASM plot warmup completed` log と Run button enabled を確認。

### Android Flutter native build embeds precompiled caches ✅ (Issue #6126)

- `mobile/scripts/build_android.sh` は host `sjulia` を build し、
  `--precompile-prelude` / `--precompile-base` で cache files を生成する。
- Android の各 `cargo ndk` build は `SJULIA_PRELUDE_PROGRAM_CACHE` と
  `SJULIA_BASE_CACHE` を渡し、first-run latency を下げるための caches を `.so` に埋め込む。
- Verification: `bash -n mobile/scripts/build_android.sh` pass、
  `./scripts/build_android.sh` pass、`flutter build apk --debug` pass。

### REPL LinRange StructRef display through FFI ✅ (Issue #6123)

- REPL FFI の `CREPLResult.value` は heap-aware formatter を使い、`StructRef` を
  `session.get_struct_heap()` から解決して表示する。
- `LinRange` は `x = y = range(-3, stop = 3, length = 100)` で
  `<struct ref>` ではなく `-3.0:0.06060606060606061:3.0` と表示される。
- Verification: `timeout 1800 cargo nextest run --lib test_repl_eval_linrange_struct_ref_formats_range_6123 test_repl_surface_inline_lambda_returns_plotly_artifact_6122`
  pass。

### REPL inline lambda result suppression for surface snippets ✅ (Issue #6122)

- REPL の新規関数定義 result 判定から lowering-generated `__lambda_*` を除外した。
- `surface(x, y, (x, y) -> sinc(norm([x, y])))` の初回評価で
  `function __lambda_0` を返さず、Plot return value から Plotly surface artifact を生成する。
- Verification: `timeout 1800 cargo nextest run --lib test_repl_surface_inline_lambda_returns_plotly_artifact_6122`
  pass。

### Flutter mobile 3D Plotly surface rendering ✅ (Issue #6121)

- Flutter mobile `PlotlyView` は iOS と同じ WebView + bundled `plotly.min.js`
  renderer になり、2D/3D Plotly traces を `Plotly.newPlot` で表示する。
- `webview_flutter` dependency と `assets/plotly/plotly.min.js` asset を追加した。
- Verification: `flutter test` pass、`flutter build apk --debug` pass、`git diff --check` pass。

### Flutter mobile Editor Plotly artifact display ✅ (Issue #6118)

- Flutter mobile Editor の `VMBridge.execute` は `CExecutionResult.artifactMime` /
  `artifactData` から `application/vnd.plotly+json` を抽出し、`ExecutionResult.plotlyJSON`
  として返す。
- `EditorState` は successful execution の直近 plot artifact を保持し、Editor output pane は
  REPL と同じ `PlotlyView` で common 2D trace を表示する。
- Verification: `flutter test` pass、`flutter build apk --debug` pass、`git diff --check` pass。
  `flutter analyze` は既存 14 件のみで fail。

### Flutter mobile REPL Plotly artifact display ✅ (Issue #6115)

- Flutter mobile の `CREPLResult` binding が native REPL artifact fields を読み、
  `application/vnd.plotly+json` を `REPLWorker`、`REPLState`、`REPLEntry` へ渡すようになった。
- REPL history は iOS の `PlotlyView` flow を参考に、Android/Flutter では common 2D
  Plotly trace (`scatter` line/markers と `bar`) を Canvas 描画する。
- `using Plots` の後に `plot(sin)` を実行すると、値表示だけで終わらず plot artifact が履歴内に表示される。
- Verification: `flutter test` pass、`flutter build apk --debug` pass、`git diff --check` pass。
  `flutter analyze` は既存 14 件のみで fail。

### Android Flutter REPL background worker ✅ (Issue #6113)

- Flutter mobile REPL の native `repl_session_eval` は dedicated Dart isolate 上の
  long-lived `REPLSession` が実行するようになり、初回評価が数秒かかっても UI isolate を塞がない。
- Worker response は primitive/map payload に限定し、raw FFI pointer は isolate 間で渡さない。
  reset 後の古い評価結果は generation guard で破棄する。
- Verification: `flutter test` pass、`flutter build apk --debug` pass、Android emulator
  `sdk gphone16k arm64` で `1 + 1` → `2` を確認し、logcat filter で
  ANR / input dispatch timeout / activity pause timeout が出ないことを確認。

### Method origin helper for dispatch fences ✅ (Issue #5926)

- `MethodSig::is_base_program_method(base_function_count)` を追加し、Base/prelude 由来かどうかを
  `global_index < base_function_count` で判定する入口を明示した。
- `is_base_extension` は `Base.:+` などを構文的に拡張したかの flag であり、origin marker ではない。
  #5926 の morespecific/topological selection fence は両者を混同しない必要があるため、
  helper と unit test で契約を固定した。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926_method_origin_uses_global_index_not_base_extension_flag`
  pass（1/1）。

### MethodSig where-wrap nested typevar characterization ✅ (Issue #5926)

- `MethodSig::core_signature()` が `x::Vector{T} where T` を
  `Tuple{Vector{T}} where T` として再構成し、`Tuple{AbstractVector}` への subtype /
  strict dominance に使えることを unit test で固定した。
- #5926 の morespecific/topological selection 化では、nested typevar を落とした
  lossy signature では dominance 判定が壊れるため、MethodTable の structured signature
  経路を prerequisite coverage として守る。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926_method_sig_where_wrap_preserves_nested_vector_typevar`
  pass（1/1）。

### Bounded typevar dispatch regression coverage ✅ (Issue #5926)

- `h(x::T) where T<:Number` が `h(x)` の untyped `Any` fallback より優先される
  #5375 regression を、`MethodTable::dispatch_inner` と `Vm::find_best_method_index_uncached`
  の両方の実 dispatch selection test として固定した。
- #5926 の morespecific/topological selection 化で `type_reuse_bonus` や dominance pre-check を
  変更しても、bounded typevar が fallback に負ける退行を両選択サイトで検出できる。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926_method_table_preserves_bounded_typevar_over_untyped_any_5375 test_find_best_method_index_issue_5926_preserves_bounded_typevar_over_untyped_any_5375`
  pass（2/2）。

### Dispatch dominance selection-site characterization ✅ (Issue #5926)

- `MethodTable::dispatch_inner` と `Vm::find_best_method_index_uncached` の両方で、
  #5926 の dominance pre-check が `Vector{T} where T` を `AbstractVector` fallback より
  優先することを unit test で固定した。
- morespecific/topological selection 本体へ進む前に、compile-time / runtime の 2 選択サイトを
  同じ representative で守る coverage を追加した。
- Verification: `timeout 1800 cargo nextest run --release --lib issue_5926_method_table_dominance_selects_vector_over_abstractvector test_find_best_method_index_issue_5926_dominance_selects_vector_over_abstractvector`
  pass（2/2）。

### Legacy inference bare-key co-write migration characterization ✅ (Issue #5939)

- `infer_function_with_arg_types` の `name(declared_param_types)` primary key と
  legacy bare `func.name` lookup の乖離を engine unit test で固定した。
- 同じ call-site argtypes に対して `f(::Any)` と `f(::Int64)` の inferred result は
  primary key では分離される。#5939 の MethodInstanceKey 精密化では、bare lookup が method identity を
  持たない時に first-writer を返さないようにする必要がある。
- Verification: `timeout 1800 cargo nextest run --release --lib test_issue_5939_primary_keys_preserve_method_identity_without_bare_co_write`
  pass（1/1）。

### Bare callee backedge over-invalidation characterization ✅ (Issue #5939)

- `CachedReturn.edges` が bare callee name だけを持つ legacy backedge では、
  callee の変更 signature が caller の実際の dispatch signature と一致しなくても
  caller cache が retire されることを engine unit test で固定した。
- `DispatchedMethodEdge` の精密 edge は unmatched signature を温存できるため、今回の
  characterization は #5939 の残作業を bare `edges: BTreeSet<String>` の
  method-instance identity 化に絞る。
- Verification: `timeout 1800 cargo nextest run --release --lib test_issue_5939_bare_callee_edge_overinvalidates_unmatched_signature`
  pass（1/1）。

### Module-qualified `Diagonal` runtime multiplication dispatch ✅ (Issue #6117)

- `CoreType` / `JuliaType` の struct family 比較で module prefix を正規化し、
  `LinearAlgebra.Diagonal{Float64} <: Diagonal` と
  `LinearAlgebra.Diagonal{Float64} <: Diagonal{Float64}` が true になるようにした。
- Android Flutter SVD sample と同じ `U * Diagonal(S) * V'` / `F.U * Diagonal(F.S) * F.Vt`
  経路が direct `sjulia` で `MethodError` に落ちず、upstream Julia と同じく
  SVD reconstruction を通す。
- 検証中に `hasmethod(*, Tuple{typeof(F.U), typeof(Diagonal(F.S))})` の reflection
  parity gap を発見し、bug follow-up として #6124 を作成した。
- Verification: upstream Julia direct で `linalg/diagonal_test.jl` /
  `linalg/matmul_svd_reconstruct.jl` / mobile + iOS SVD samples pass、
  `target/release/sjulia` direct で同 fixture/sample pass、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm module_qualified_parametric_struct_subtypes_bare_family_issue_6117`
  pass（2/2）、`timeout 1800 cargo nextest run --release --test fixture_tests linalg::`
  pass（2/2）、`timeout 1800 cargo check -p subset_julia_vm --lib` pass、
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass、
  `bash scripts/check_fixture_test_names.sh` pass、`git diff --check` pass。

### Tuple user-parametric bare-family covariance ✅ (Issue #6116)

- CoreType subtype の `Struct{name, params} <: Named(name)` を同一 bare family として
  true にし、`Foo{Int64} <: Foo` と、その tuple covariant 版
  `Tuple{Foo{Int64}} <: Tuple{Foo}` が upstream Julia と一致するようにした。
- 既存 fixture `tuple_user_parametric_covariance_5064.jl` の direct sjulia failure
  (`v isa Tuple{Foo}`, `Tuple{Foo{Int}} <: Tuple{Foo}`) を解消した。
- Verification: upstream Julia と `target/release/sjulia` direct で
  `tuple/tuple_user_parametric_covariance_5064.jl` pass、
  `timeout 1800 cargo build --release --bin sjulia --features repl` pass、
  `timeout 1800 cargo nextest run --release --test fixture_tests tuple::` pass（2/2）、
  `timeout 1800 cargo nextest run --lib parametric_structs_are_invariant_but_match_bare_base test_check_subtype_core_gate_handles_authoritative_runtime_pairs`
  pass（2/2）、`timeout 1800 cargo check -p subset_julia_vm --lib` pass、
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass、`git diff --check` pass。

### `isa(::Type{...})` runtime subtype routing ✅ (Issue #5921)

- `BuiltinId::Isa` の `isa(x, Type{...})` 特例を `type_values_subtype` 直呼びから
  `Vm::check_subtype` へ移し、`Type{Int}` / `Type{<:Number}` / `Type{<:AbstractArray}`
  判定を runtime subtype entry point へ寄せた。
- `isa` final fallback の kind 判定 (`UnionAll <: Type` など) は既存の `JuliaType`
  経路に残し、runtime string subtype へ移すには別途 kind hierarchy 側の対応が必要。
- 検証中に `tuple_user_parametric_covariance_5064.jl` の direct sjulia failure を再発見し、
  既存 closed #5064 の再発として #6116 を作成した（今回 PR では workaround しない）。
- Verification: upstream Julia direct で `types/typeof_first_class_5068.jl` /
  `dispatch/subtype_isa_first_class_5115.jl` /
  `tuple/tuple_user_parametric_covariance_5064.jl` /
  `reflection/isa_typeof_kind_consistency_3909.jl` pass、
  `target/release/sjulia` direct で `types/typeof_first_class_5068.jl` /
  `dispatch/subtype_isa_first_class_5115.jl` /
  `reflection/isa_typeof_kind_consistency_3909.jl` pass、
  `timeout 1800 cargo build --release --bin sjulia --features repl` pass、
  `timeout 1800 cargo nextest run --lib builtins_types` pass（2/2）、
  `timeout 1800 cargo nextest run --release --test fixture_tests types:: dispatch:: reflection::`
  pass（6/6）、`timeout 1800 cargo check -p subset_julia_vm --lib` pass、
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass、`git diff --check` pass。

### Type value equality via mutual subtyping ✅ (Issue #5921)

- runtime type-object equality helper `type_objects_equal` を `JuliaType::type_eq`
  直呼びから `type_values_subtype(left, right) && type_values_subtype(right, left)`
  へ変更し、Julia の `jl_types_equal` と同じ「mutual subtype」モデルに寄せた。
- `Tuple === Tuple{Vararg{Any}}` / `Tuple == Tuple{Vararg{Any}}` を fixture で固定し、
  型値 equality が subtype 側の改善を共有できるようにした。
- Verification: upstream Julia と `target/release/sjulia` direct で
  `operators/type_value_equality_mutual_subtype_5921.jl` pass、
  `bash scripts/check_fixture_test_names.sh` pass、
  `timeout 1800 cargo build --release --bin sjulia --features repl` pass、
  `timeout 1800 cargo nextest run --release --test fixture_tests operators::` pass（2/2）、
  `timeout 1800 cargo nextest run --lib test_type_objects_equal_uses_mutual_subtyping test_type_values_subtype_uses_julia_subtype_relation`
  pass（2/2）、`timeout 1800 cargo check -p subset_julia_vm --lib` pass、
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass、`git diff --check` pass。

### Android Flutter REPL Enter evaluation ✅ (Issue #6110)

- Flutter REPL input の IME action を newline から done に変更し、Android soft
  keyboard の Enter/done action が再生ボタンと同じ `_evaluate` path を通るようにした。
- hardware Enter / numpad Enter も modifier 無しの場合は `_evaluate` に接続し、
  Shift/Alt/Ctrl/Meta 付きのキー入力は既存の TextField handling に残した。
- `mobile/test/widget_test.dart` に TextInputAction.done で `REPLState.history`
  が増える回帰テストを追加した。
- Verification: `git diff --check` pass。Flutter/Dart SDK はこの shell の PATH に無く
  (`flutter` / `dart` command not found)、`flutter test` は未実行。

### Runtime `<:` / `>:` builtin subtype path unification ✅ (Issue #5921)

- `BuiltinId::Subtype` / `BuiltinId::SupertypeOp` の DataType-only fast path
  (`core_datatype_subtype_result`) を削除し、core DataType / user hierarchy
  DataType / callable `Ref` operand すべてを `subtype_operand_name` →
  `Vm::check_subtype` の runtime subtype entry point へ通すようにした。
- `<:` / `>:` の subtype 判定が `type_values_subtype` と `check_subtype` に分岐せず、
  #5921 の type value operation 統合に向けた runtime path が一段薄くなった。
- Verification: upstream Julia direct で `operators/subtype_basic.jl` /
  `operators/operators_supertype.jl` / `dispatch/subtype_isa_first_class_5115.jl` pass、
  `timeout 1800 cargo build --release --bin sjulia --features repl` pass、
  `target/release/sjulia` direct で同 3 fixture pass、
  `timeout 1800 cargo nextest run --lib builtins_types` pass（2/2）、
  `timeout 1800 cargo nextest run --release --test fixture_tests operators:: dispatch:: ref_tests:: comparison:: types_tests::`
  pass（11/11）、`timeout 1800 cargo check -p subset_julia_vm --lib` pass、
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### VM `type_matches` numeric/range subtype unification ✅ (Issue #5921)

- `Vm::type_matches` の `Int64` / `Float64` / `Real` / `Number` / `Integer` /
  `Signed` / `Unsigned` / `AbstractFloat` と range family (`UnitRange` /
  `StepRange` / `AbstractRange`) の手書き判定を、runtime subtype entry point
  `Vm::check_subtype` への委譲に寄せた。
- 既存の `CoreType` 直呼び helper と `Rational` / `Complex` の局所 fallback を削除し、
  Pure Julia struct parent は `StructHierarchy` 経由の `check_subtype` に一本化した。
- Verification: `timeout 1800 cargo nextest run --lib runtime_type_matches_abstract_numeric_params_via_core_subtype_issue_5921`
  pass、`timeout 1800 cargo nextest run --lib vm::type_ops::comparison` pass（22/22）、
  `timeout 1800 cargo nextest run --lib vm::tests::runtime_type_matches_abstract_numeric_params_via_core_subtype_issue_5921 vm::tests::test_runtime_diagonal_type_var_rejects_mixed_bigint_rational`
  pass（2/2）、`git diff --check` pass、`timeout 1800 cargo check -p subset_julia_vm --lib`
  pass、`timeout 1800 cargo clippy --all-targets -- -D warnings` pass。

### Termux Docker smoke ✅ (Issue #6021)

- `docker/Dockerfile.termux` を追加し、Termux userland 上で Rust / clang /
  pkg-config / OpenSSL を Termux package として入れたうえで
  `subset_julia_vm` の library check を実行できるようにした。
- `docker/README.md` に Termux smoke の実行手順と期待値を追加した。
- Verification: `timeout 1200 docker run --rm -v "$PWD":/work/ailujsoi -w /work/ailujsoi termux/termux-docker ...`
  で `rustc` host `x86_64-linux-android` と `cargo check -p subset_julia_vm --lib`
  pass を事前確認し、`timeout 1800 docker buildx build -f docker/Dockerfile.termux --target check .`
  pass。Dockerfile smoke では `rustc 1.94.1` host `x86_64-linux-android` /
  `cargo 1.94.1` と `timeout 1800 cargo check -p subset_julia_vm --lib` pass。

### Raspberry Pi 32-bit Docker smoke ✅ (Issue #6017)

- `docker/Dockerfile.raspberrypi32` の smoke build timeout を 1800 秒から 3600 秒へ拡大した。
  QEMU 下の release `sjulia` build は実測 43m31s で、従来の 30 分上限では
  smoke assertion に到達できなかった。
- `docker/README.md` に binfmt 登録手順、armv7 host-side cross-check fallback、
  2026-06-07 の QEMU smoke 実測時間を追記した。
- Verification: `timeout 1800 cargo check -p subset_julia_vm --lib --target armv7-unknown-linux-gnueabihf`
  pass、`docker run --privileged --rm tonistiigi/binfmt --install arm` pass、
  `timeout 4200 docker buildx build --platform linux/arm/v7 -f docker/Dockerfile.raspberrypi32 --target smoke .`
  pass。Docker smoke では `target/release/sjulia` が `ELF 32-bit ... ARM` executable として生成され、
  `Sys.WORD_SIZE == 32` / `Int === Int32` / `UInt === UInt32` assertions と `println(1 + 2)` が pass。

### native word-size `Int` / `UInt` aliases ✅ (Issues #6097, #6105)

- `Int` / `UInt` の canonical target を `usize::BITS` から決める helper を追加し、
  `JuliaType` / `CoreType` / VM convert / compiler inference / AoT projection の
  bare alias 解決を native word size へ揃えた。
- 64-bit では既存通り `Int === Int64` / `UInt === UInt64`、32-bit target では
  `Int === Int32` / `UInt === UInt32` になる。
- `UInt` が bare type object として未定義になる経路を `is_builtin_type_name` と
  `BuiltinId::from_name` 側で解消した。
- Raspberry Pi 32-bit smoke の workaround を削除し、`Sys.WORD_SIZE == 32` に加えて
  `Int === Int32` / `UInt === UInt32` を assertion 対象にした。
- Verification: upstream Julia と `target/release/sjulia` direct で
  `reflection/native_word_aliases_6097_6105.jl` pass、
  `timeout 1800 cargo nextest run --release -p subset_julia_vm --lib native_word_aliases_follow_target_pointer_width`
  pass、fixture_tests `reflection::` pass、`cargo check -p subset_julia_vm --lib` pass、
  `timeout 1800 cargo check -p subset_julia_vm --lib --target armv7-unknown-linux-gnueabihf`
  pass、`timeout 1800 cargo build --release --bin sjulia --features repl` pass、
  `timeout 1800 cargo clippy --all-targets -- -D warnings` pass。Docker armv7 smoke は
  QEMU 下の release build が Dockerfile 内の 1800 秒 timeout に到達し、smoke assertion
  までは未到達。
