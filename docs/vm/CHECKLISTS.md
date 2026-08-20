# Implementation Checklists

Detailed checklists for adding new types, variants, and features to SubsetJuliaVM. Referenced from `CLAUDE.md`.

## ABI Change Checklist (Issue #9001)

Any change to `subset_julia_vm_ffi/include/subset_vm.h` that alters the binary
interface visible to iOS / Flutter / web consumers requires the following steps.
`scripts/check_ffi_abi_version.sh` enforces this in CI — it fails if the header
signature hash changes without a corresponding version bump.

**ABI-breaking changes** (require version bump):
- Struct field additions, removals, or reordering (`CSpan`, `CError`, `CExecutionResult`, `CREPLResult`)
- Enum discriminant value changes (`CErrorKind`, `CValueKind`)
- Function parameter or return-type changes
- Ownership/lifetime contract changes (who allocates, who frees)

**Additive-only changes** (no version bump needed):
- New exported functions that do not alter existing ones
- Comment or documentation edits
- Internal Rust changes invisible at the C ABI boundary

### Bump procedure

1. **Header** — increment `SUBSET_VM_ABI_VERSION` in
   `subset_julia_vm_ffi/include/subset_vm.h`.
2. **Rust constant** — increment `SUBSET_VM_C_ABI_VERSION` in
   `subset_julia_vm_ffi/src/abi_version.rs` to the same value.
3. **WASM constant** — update the literal `1` returned from `abi_version()`
   in `subset_julia_vm_web/src/lib.rs` to match.
4. **Swift constant** — update `kSubsetVMABIVersion` in
   `SubsetJuliaVMApp/.../Services/FFI/REPLSessionManager.swift` to match.
5. **Dart constant** — update `_kSubsetVMABIVersion` in
   `mobile/lib/ffi/vm_bridge.dart` to match.
6. **Baseline** — run `bash scripts/check_ffi_abi_version.sh --update` to
   record the new signature hash.
7. **Build validation** —
   ```bash
   cargo build --release -p subset_julia_vm_ffi
   bash scripts/check_ffi_header_compiles.sh
   bash scripts/check_ffi_abi_version.sh
   ```
8. **PR note** — mention in the PR body that the ABI was bumped and that iOS,
   Flutter, and web consumers must rebuild against the new xcframework/library.

### Crate version sync policy

`subset_julia_vm_ffi` and `subset_julia_vm` carry independent crate versions
(currently 0.9.0 vs 0.9.5) because they are separate crates with different
release cadences.  When bumping the **C ABI version**, also bump the crate
version in `subset_julia_vm_ffi/Cargo.toml` to keep the crate version
monotonically ahead of any prior ABI version for traceability.

## Executable docs snippets (Issue #8720)

When `docs/vm/*.md` states Julia behavior through an executable example, prefer
a ```` ```julia-doctest ```` fence with a `# output` marker and expected stdout.
Run `bash scripts/docs_doctest.sh` to compare the snippets against sjulia and,
when available, upstream `julia`.

## Python Helpers Invoked by Source Audits (Issue #11102)

When a `scripts/check_*.sh` or `scripts/audit_*.sh` wrapper launches an external
Python helper:

- [ ] Use literal `python3 scripts/<helper>.py` only when the helper supports the
      repository's ambient Python 3.9 floor. Keep it stdlib-only.
- [ ] Add `from __future__ import annotations` before using PEP 604 unions in
      annotations; Python 3.9 otherwise evaluates `str | None` at import time.
- [ ] Run `bash scripts/check_python_audit_compatibility.sh`; discovery is
      automatic, so do not add a parallel helper allowlist.
- [ ] New imports fail closed until their exact module/member is verified on
      Python 3.9 and added to the checker's reviewed import set.
- [ ] If Python 3.10+ is intentional, invoke the tool through `uv` and declare
      the exact floor in PEP 723 `requires-python` metadata instead of ambient
      `python3`.
- [ ] Run `bash scripts/check_audit_negative_selftest.sh` after changing the
      floor checker or its discovery contract.

## Fixture Helper Coverage (Issue #11041)

When adding a `.jl` helper under `subset_julia_vm/tests/fixtures/`:

- [ ] Prefer a literal `include("helper.jl")` or `evalfile("helper.jl")` when the
      fixture does not specifically need to exercise runtime path computation;
      literal targets are discovered automatically.
- [ ] If the helper is reached only through a computed path, add a row to
      `docs/vm/FIXTURE_COVERAGE_ALLOWLIST.tsv` with a concrete reason naming the
      fixture and why path computation is part of the test.
- [ ] Do not allowlist a literal target. The coverage audit rejects the row as
      stale once it can discover the helper directly.
- [ ] Run `bash scripts/check_unregistered_fixtures.sh` and
      `bash scripts/fixture_coverage_contract_selftest.sh`.

## Identifier Continuation Expansion — Paired Operator-Boundary Lexer Tests (Issue #10848, prevention for #10713)

Any expansion of the identifier-continuation character set in the
`Token::Identifier` regex (`subset_julia_vm_parser/src/token/mod.rs`) MUST be
paired with:

1. New rows in the table-driven lexer test
   `test_identifier_continuation_operator_boundary_table_issue_10848`
   (`subset_julia_vm_parser/src/lexer.rs`) covering the new continuation
   characters adjacent to the operator boundaries `!=`, `!==`, `.!=`, `.!==`,
   and the dotted unary `.!` form — the greedy identifier regex must keep
   rewinding via `Lexer::restart_from` (see `a!=b`, Issue #8194/#10713).
2. Upstream verification of each new row with `julia` (`Meta.parse`).
3. A parse fixture when interpolation or assignment names are affected
   (see `fixtures/parse/mid_identifier_bang_10713.jl`).

## Workspace Crate Dependency / Feature Changes (Issue #9628)

Cargo unifies features across all crates built in one invocation, so a
workspace-level build can mask a missing crate-local dependency feature: the
crate compiles because a *sibling* crate enabled the feature (e.g.
`subset_julia_vm_types` serializing `Arc<core::Function>` compiled only because
`subset_julia_vm` / `subset_julia_vm_bytecode` enabled serde's `rc` feature).
The break then only appears in isolated `-p <crate>` builds.

When adding/changing a dependency in a workspace crate's `Cargo.toml`, or when
adding code that needs a new cargo feature of an existing dependency
(`serde` `rc`/`derive`, `half` `serde`, …):

- [ ] Declare the feature on the crate that *uses* it — never rely on feature
      unification from sibling workspace crates.
- [ ] Verify the crate builds in isolation, including its tests:
      `cargo check -p <crate> --tests` (this builds only the crate + its own
      dependency closure, so unification from siblings does not apply).
- [ ] For the layered rlib crates, spot-check the ones below/above the change:
      `cargo check -p subset_julia_vm_ir --tests`,
      `cargo check -p subset_julia_vm_types --tests`,
      `cargo check -p subset_julia_vm_bytecode --tests`.
- [ ] An isolated targeted test run also exercises this:
      `timeout 1800 cargo nextest run --cargo-profile release-fast -p <crate> <filter>`.

## Collection Eltype Regression Checklist (Issues #9789/#9796)

When changing array literals, array comprehensions, generators, `collect`, or any
compiler/runtime path that chooses a collection element type:

- [ ] Test both empty and non-empty iterators. Runtime typejoin only sees
  observed values, so empty results need an explicit body/default eltype source.
- [ ] Pair direct comprehension (`[body for ...]`) with `collect(body for ...)`
  so `MakeGenerator` defaults and direct array allocation stay in sync.
- [ ] Assert `typeof(result)`, not just values. Empty vectors with the wrong
  eltype often compare equal by contents.
- [ ] Include at least one concrete zero-arg body (`Float32`/`String`), one
  `convert(Any, x)` body that should recover the iterator element type, one
  numeric heterogeneous body that should join to `Real`, and one irreducible
  heterogeneous body that should remain `Any`.
- [ ] For numeric-like singleton structs/constants (`Irrational`, future
  singleton numeric constants), cover both homogeneous singleton preservation
  and mixed `promote_typeof` widening: same singleton (`[pi, pi]`), distinct
  singletons (`[pi, Base.MathConstants.e]`), and Bool/int/F64 mixed forms.
  Assert `typeof(result)` and at least one converted value (Issue #9780).
- [ ] For packed/isbits array representations such as `ComplexF64`/`ComplexF32`,
  cover every mutation boundary that can cross from VM-owned heap values into
  bytecode-owned storage: typed empty `push!`, `Vector{T}(undef, 0)` growth,
  `setindex!`/memory-backed storage, wrapper materialization, and scalar
  readback feeding arithmetic. Bytecode storage should receive logical values,
  not unresolved VM-local `StructRef` values (Issue #9749).
- [ ] Verify with upstream `julia --startup-file=no <fixture>` and
  `bash scripts/fixture_julia_parity.sh <fixture>` before relying on sjulia-only
  output.

## Generator Consumer / Public Indexing Checklist (Issue #9735)

`Base.Generator` is iterate-only in upstream Julia. Generic consumers may not
keep a public `getindex(::Base.Generator)` materialization fallback just because
an internal helper wants `length(x)` + `x[i]`.

When changing generator representation, iterable consumers (`any`, `all`,
`join`, `Tuple`, `first`, `collect`, `sum`), or comprehension lowering:

- [ ] Write generic consumers against `iterate` unless the public API explicitly
      requires an indexable collection.
- [ ] Treat struct-typed iterators (`ValueType::Struct(_)`) like
      `Base.Generator` for single-variable comprehensions: drive `iterate`
      rather than `length(x)` + `IndexLoad` / public `getindex` (Issue #10607).
      Keep `comprehension/partition_iterate_protocol_10442.jl` green because it
      covers `Iterators.partition` and a custom length+iterate/no-getindex
      struct.
- [ ] Keep public `Value::Generator` indexing in `array_index.rs` as a catchable
      `MethodError`, not a stack-pushing materialization branch.
- [ ] Include a direct `@test_throws MethodError g[i]` guard for any new
      generator representation path.
- [ ] For empty filtered generators, only reuse an inferred result eltype when
      the body and predicate provenance are both transparent. A predicate that
      calls user code (or uses an inlined predicate helper) must fall back to
      `Union{}[]`; cover both `collect` eltype and at least one non-`sum`
      consumer such as `first` (Issue #10621).
- [ ] Run:

  ```bash
  julia --startup-file=no subset_julia_vm/tests/fixtures/generator/getindex_methoderror_9457.jl
  julia --startup-file=no subset_julia_vm/tests/fixtures/comprehension/partition_iterate_protocol_10442.jl
  bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/generator/getindex_methoderror_9457.jl
  bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/comprehension/partition_iterate_protocol_10442.jl
  timeout 1800 cargo nextest run --release --test fixture_tests generator::
  timeout 1800 cargo nextest run --release --test fixture_tests comprehension::
  bash scripts/check_array_public_data_access.sh
  ```

## String Indexing Routes (Issue #11621)

When adding or changing a native `String`, byte-backed `StrBytes`, `SubString`,
or other `AbstractString` indexing route:

- [ ] Route every one-based code-unit index through
      `vm/exec/string_index.rs::validate_string_index`; do not duplicate bounds
      and UTF-8 boundary classification in the scalar/vector/range consumer.
- [ ] Keep the caller's inclusive Julia endpoint separate from the exclusive
      Rust byte-slice end returned for the validated final character.
- [ ] Extend the exceptions string-index fixture matrix across ASCII and
      multibyte input, scalar/vector/range access, first/interior/final/OOB
      indices, and `String`/`StrBytes` where constructible.
- [ ] Assert all three structural outcomes: valid character start,
      `StringIndexError` for an in-bounds continuation byte, and `BoundsError`
      for a numeric out-of-bounds index. For caught `StringIndexError`, also
      assert the exact `.string` receiver and `.index` payload.
- [ ] Run upstream Julia first, then the exceptions fixture category and the
      full release suite.

## Typed Exception Payload Carriers (Issue #11647)

When a `VmError` cannot retain an exact runtime `Value` required by the caught
Julia exception struct:

- [ ] Add a `PendingExceptionPayload` variant in
      `vm/exec/exception_payload.rs`; do not add another `Vm` pending field.
- [ ] Construct the payload key and matching `VmError` atomically through
      `exception_error_with_payload`. An adapter for an already-built error must
      use the checked `attach_exception_payload` boundary.
- [ ] Keep `vm_error_to_exception_value`'s unconditional `take_fields_for`
      before `exception_class()` so mismatch and VM-internal errors consume the
      carrier too.
- [ ] Extend `exception_payload_carrier_lifecycle_matrix_11647` with exact,
      mismatch, internal/stale, nested replacement, unhandled clear, and
      same-session recovery coverage for the new kind. Also add a real catch →
      nested catch → `rethrow()` fixture that asserts the outer exact fields.
- [ ] Run `bash scripts/audit_exception_payload_carrier.sh` and its focused
      negative control before the exceptions category and full release suite.

## Generator Trait Fast-Path Checklist (Issue #9727)

Generator trait fast paths (`IteratorSize`, `size`, `length`, `isempty`,
`collect`, and related CallResolved/dynamic dispatch shortcuts) must be
wrapper-aware. A value-level generator whose base is a Pure Julia `Array{T,N}`
wrapper has the same shape as a native array carrier, and must not degrade from
`HasShape{N}()` to `HasLength()`.

When changing generator trait logic:

- [ ] Test both native array carriers and Pure Julia array wrappers. Include
      vector, matrix, and rank-3 array bases so rank erasure is visible.
- [ ] Pair value-level `IteratorSize(generator)` with `size(generator)`.
      `size` can be correct while the value-level trait fast path is wrong.
- [ ] Keep filtered generators on `SizeUnknown()` / `MethodError` paths for
      named, captured, and array-base predicates.
- [ ] Confirm dynamic dispatch and `CallResolved(IteratorSize, 1)` routes agree.
- [ ] Run:

  ```bash
  julia --startup-file=no subset_julia_vm/tests/fixtures/generator/iterator_traits_9379.jl
  bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/generator/iterator_traits_9379.jl
  timeout 1800 cargo nextest run --release --test fixture_tests generator::
  ```

## Convert Target Changes (Issue #9842)

When changing `convert_value`, bundled `convert` methods, or type annotation /
constructor paths that route through `convert(T, x)`:

- [ ] Cover Union targets in both identity and member-conversion forms:
      `convert(Union{Nothing, T}, value::T)`, `convert(Union{Nothing, T}, nothing)`,
      and conversion from a compatible non-member value such as `Int64` to
      `Float64`.
- [ ] Include `Missing` unions as well as `Nothing` unions, because
      `Vector{Union{Missing,T}}` and nullable struct fields route through the
      same target conversion shape.
- [ ] Keep a failing conversion assertion such as
      `convert(Union{Nothing, Int64}, 1.5)` throwing `InexactError`.
- [ ] Include
      `subset_julia_vm/tests/fixtures/conversion/convert_union_targets_9842.jl`
      in the PR test plan and run `conversion::`.

## Typed Comprehension Target Changes (Issue #9754)

When changing typed array/comprehension lowering, `wrap_comprehension_body_with_call`,
typed `Vector{T}` / `Matrix{T}` constructor intercepts, or a target alias that can
flow into `ArrayElementType`, decide whether `T(expr)` and `convert(T, expr)` can
diverge. Upstream typed comprehensions insert each element through storage
conversion, so `T[expr for ...]` must follow `convert(T, expr)` semantics when
constructor calls and conversion have different meanings.

- [ ] Cover a parametric target spelling (`Complex{Float64}`), an alias spelling
      (`ComplexF64`), an already-target-typed body value, and a convertible
      non-target body value.
- [ ] Cover filtered and multi-dimensional typed comprehensions, not only the
      single unfiltered vector case.
- [ ] Assert both values and `typeof(result)`. A value-only test can pass while
      `Vector{ComplexF64}(...)` / `Matrix{ComplexF64}` drifts through a
      constructor intercept to `Vector{Any}` / `Matrix{Any}`.
- [ ] When adding a new `ArrayElementType` or target alias, verify both the
      `JuliaType::Struct("T{...}")` path and the `TypeExpr::TypeVar("Alias")`
      path preserve the forced element type.
- [ ] Keep the regression fixture in the PR test plan:

  ```bash
  julia --startup-file=no subset_julia_vm/tests/fixtures/complex/complex_typed_comprehension_convert_9505.jl
  bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/complex/complex_typed_comprehension_convert_9505.jl
  timeout 1800 cargo nextest run --release --test fixture_tests complex::
  ```

## Typed Array Literal Conversion (Issue #10835)

Upstream inserts every element of `T[a, b, ...]` through `setindex!`, which
applies `convert(T, x)`. The main compiler and runtime specializer must likewise
emit `CallBuiltin(Convert, 2)` for every non-empty typed literal; do not restore
a target-name or storage-tag allowlist. Evaluate the target expression exactly
once before the element expressions, then reload that same value for every
conversion; an earlier element may mutate a binding used by the target.

- [ ] When adding an `ArrayElementType`, a typed-literal target spelling, or a
      `convert(::Type{T}, x)` method, keep the unconditional conversion route in
      both typed-literal code generators.
- [ ] Assert the stored element's `typeof`/`eltype`, not only the outer array's
      type or numeric equality. Boxed values can otherwise retain the wrong
      concrete type while the container tag looks correct.
- [ ] Cover a newly convertible target and an exact-type boxed control in
      `array/typed_literal_container_convert_10750.jl`; keep the specializer
      instruction assertion in
      `compile_typed_array_literal_emits_literal_build_issue_10746`.
- [ ] Run upstream parity and the `array::` fixture category.
- [ ] Cover target-expression mutation so later elements still convert through
      the original evaluated target (logical dynamic-target `eltype` metadata
      remains tracked by Issue #11787).

## Typed Array Literal / Dynamic Array Helper Checklist (Issue #9820)

When changing typed array literal lowering, `FinalizeArray*`, runtime array type
projection, or the native-array wrapper fence:

- [ ] Include a fixture where a typed array literal flows through an `Any` slot
      before public wrapper APIs call internal helpers: `size`, `length`,
      `reshape`, and `similar`. Keep
      `subset_julia_vm/tests/fixtures/array/matrix_literal_tuple_elements_9437.jl`
      in the PR test plan.
- [ ] Keep the native-array helper fence decision table in
      `subset_julia_vm_vm/src/vm/native_array_compat.rs` synchronized with
      `subset_julia_vm/src/julia/base/array.jl`; every new internal
      `Array{T,N}` helper needs an explicit exempt/fenced row.
- [ ] Extend `test_array_receiver_extracts_tuple_vector_struct_type_issue_9437`
      when `array_type_override` starts emitting a new `Vector{T}`/`Matrix{T}`
      spelling, especially module-qualified or nested-parametric element forms.

## Array Equality / Isequal Fallback Checklist (Issue #10356)

When changing compiler array equality fallbacks, readable array wrapper
boundaries, `TupleEquals`, `BuiltinId::Isequal`, or native structural equality
helpers, keep Julia `==` and `isequal` semantics separate. Do not route readable
array `==` through `BuiltinId::Isequal`: signed zero, NaN, and mixed exact/float
numeric comparisons intentionally differ between the two predicates.

- [ ] For readable native/Any/Memory array equality, use the `TupleEquals`
      / value-equality bridge (`==` element semantics), not
      `BuiltinId::Isequal`.
- [ ] Keep `isequal` routed through the `Isequal` bridge and assert that signed
      zero remains distinguished there.
- [ ] Cover both operand orders and `!=` desugaring for mixed carriers. The
      regression shape is not symmetric if only one compiler fallback arm was
      changed.
- [ ] Cover nested readable array-like elements inside another array equality
      fold. Top-level `try_equal_array_like` coverage is not enough:
      `TupleEquals` / `array_elements_equal_tristate_with_shapes` must recurse
      through `array_like_logical_view` so `[view(...)] == [[...]]` and future
      wrapper elements compare by logical contents (Issue #10615).
- [ ] Before changing the unreadable `AbstractArray` fallback, run a
      CodeUnits/SubArray equality fixture or add one; do not blindly convert
      that path away from `Isequal` until a non-recursive, carrier-complete
      pure-Julia `==` dispatch route exists.
- [ ] Run:

  ```bash
  julia --startup-file=no subset_julia_vm/tests/fixtures/comparison/dynamic_array_bigint_equality_9516.jl
  bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/comparison/dynamic_array_bigint_equality_9516.jl
  julia --startup-file=no subset_julia_vm/tests/fixtures/complex/complex_array_eq_carrier_5789.jl
  bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/complex/complex_array_eq_carrier_5789.jl
  timeout 1800 cargo nextest run --release --test fixture_tests comparison::
  timeout 1800 cargo nextest run --release --test fixture_tests complex::
  ```

## New Bundled Package / New Solver — Load-Time Inference Check (Issues #8182/#8185)

`using X` for a bundled package pays the compile-time **return-type inference**
cost of every package function that lacks a declared return type, in
`compile.build_method_tables`. A function that **defines a closure inside a loop
and threads it through a deep (mutually-)recursive call tree** re-specializes that
whole tree per concrete closure under the loop fixpoint, so its body inference can
explode super-linearly even though the output is always correct — `_bfgs` made
`using Optim` ~5.5 s (97 % in `build_method_tables`) until PR #8184 added a
`::MultivariateOptimizationResults` return annotation (5097 ms → 42 ms). This is
**invisible to functional tests** (they stay green); only a load-time/work metric
catches it.

When adding or changing a bundled package (especially a new first-order /
line-search solver — LBFGS/CG/Newton — that uses a HagerZhang-style deep call tree
via a loop-local closure):

- [ ] Measure `using X` once: `SJULIA_COMPILE_PROFILE=1 sjulia -e 'using X' 2>&1 | grep build_method_tables` (needs `--features profiling`), or eyeball wall-clock. A multi-second `build_method_tables` is the red flag.
- [ ] If a function defines an **in-loop closure threaded into a recursive/mutually-recursive helper**, give it an **exact declared return type** to short-circuit body inference (the #7215 / #8182 mechanism). Verify the annotation is exact with upstream `julia`.
- [ ] Add a per-package load-time smoke test asserting bounded inference work, mirroring `engine::tests::work_budget_8185::using_optim_load_inference_stays_bounded_8185` (`work_budget_metrics::peak_work()` after `compile_and_run_str("using X\n…")` must stay under a threshold well below the blow-up). This is the regression guard that stays green-test-proof.
- [ ] Note: the engine's `MAX_INTERPROCEDURAL_ANALYSIS_WORK` backstop only bounds *catastrophic* (host-OOM-class) blow-ups. Historical unannotated package loads (`using Symbolics` ≈ 159k work, `_bfgs` blow-up ≈ 174k) were the same order, so the backstop cannot be the package-load performance mechanism. Declared annotations + per-package smoke tests, NOT the backstop, are the real #8182/#8213 guards (see `compile/abstract_interp/engine/mod.rs`).

## Precompiled Base Cache Build (Issues #2929/#3972/#3973)

Default runtime/test behavior now uses a process-shared persistent Base cache in
the workspace `target/` directory. On the first run without an embedded cache,
sjulia writes `target/sjulia_base_cache_v3_<cache-hash>.bin`; concurrent nextest
fixture processes wait on a lock file and then read the same serialized cache.
Set `SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1` to debug the uncached path;
only the exact value `1` disables the persistent cache.
Serialized Base cache reconstructs runtime specialization context at load time;
if changing Lazy AoT context shape, bump the Base cache format version and
verify a warm-cache fixture that needs parametric specialization.
The cache hash includes Base/prelude source, the schema fingerprint, and the
compiler build fingerprint, so Rust-side compiler/runtime changes invalidate
stale persistent caches automatically. Run `sjulia --cache-status` to inspect
embedded/persistent cache states and fingerprints without deleting stale files.
When a serialized Base cache schema input changes, bump `CACHE_VERSION` in
`subset_julia_vm_compile/src/compile/precompile.rs`, add a `Bumped to <n>`
changelog comment there with the owning Issue and compatibility reason, refresh
`subset_julia_vm_compile/src/compile/base_cache_schema_fingerprint.txt` with
`bash scripts/audit_base_cache_schema_fingerprint.sh --update`, then run the
audit again without `--update` to prove the committed snapshot is read-only
green. For the #9623 update-helper checklist, corrupt a copied/staged snapshot
to confirm default mode fails, run `--update`, confirm `git diff --` shows only
the snapshot rewrite, then run default mode again to confirm the audit passes.

Generated fixture tests are batched within each category as `chunk_NNN` tests
(default size: 32 fixtures) because `nextest` executes each Rust test in a
separate process. Keep category-level targeting (`array::`, `memory::`, etc.)
intact when adding fixtures. If the generated chunk size changes, update
`FIXTURE_BATCH_SIZE` in `subset_julia_vm/build.rs` and keep handwritten
aggregate tests sized consistently enough that the full release suite remains
under `timeout 1800 cargo nextest run --release`. Run
`bash scripts/check_fixture_chunk_size.sh` to report the current manifest count,
generated chunk count, and minimum batch-size guard.

The parse/lower output for the prelude is cached separately as
`target/sjulia_prelude_program_<prelude-hash>.bin`. Set
`SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE=1` to force source parsing and
lowering when debugging prelude loader changes. Release builds can embed an
explicit prelude Program cache with `SJULIA_PRELUDE_PROGRAM_CACHE=<path>` to
avoid filesystem lookup and source parse/lower on first `run_from_source()`
(Issue #6026). The prelude cache hash also includes the compiler build
fingerprint because it persists lowered IR.

For release artifacts that should not read/write `target/`, use the two-pass
build to embed the parsed/lowered prelude Program plus precompiled Base bytecode:

```bash
cargo build --release --bin sjulia --features repl                    # Step 1: Normal build
mkdir -p target
./target/release/sjulia --precompile-prelude target/prelude_program_cache.bin
./target/release/sjulia --precompile-base target/base_cache.bin       # Step 2: Generate caches
SJULIA_PRELUDE_PROGRAM_CACHE=$(pwd)/target/prelude_program_cache.bin \
SJULIA_BASE_CACHE=$(pwd)/target/base_cache.bin \
  cargo build --release --bin sjulia --features repl                  # Step 3: Embed caches
```

These commands assume you are running from the repository root and using the workspace-level `target/` directory. Embedded caches are validated at load time via version number and SHA-256 hash of the prelude source, and take precedence over the persistent runtime cache.

After Issue #7721, Base registry macros expand through the same
`macro_runtime` VM path as user macros. When changing macro runtime
value-to-IR conversion, quote constructor conversion, or the bootstrap macro
kernel, regenerate `target/base_cache.bin` with `--precompile-base` before
measuring or embedding caches. The Rust bootstrap kernel is intentionally
limited to structural macros needed before Base is available (`@inline`,
`@noinline`, `@inbounds`, metadata wrappers, `@view`/`@views`, and
multi-argument `@show`); non-kernel Base macros should exercise the runtime
path in fixture tests.
When changing do-block body lowering, include a fixture where a statement-position
macro appears inside the closure body (both last-position and non-last before a
returned expression) so macro calls keep the caller's active macro/import context
(Issue #9598).
When changing macro quote hygiene, cover both quote-local function definitions
and plain variable bindings introduced by a bare `quote`. Include an escaped
sibling subtree that mutates a caller global with the same name, and assert that
the quote-local binding stays separate. Do not rewrite symbols inside `esc(...)`
or quoted-data subtrees. Explicit `global x` declarations are caller-visible in
upstream Julia, not quote-local bindings; sjulia's unescaped `global` declaration
inside macro quote is still tracked separately (Issue #9692). Base/stdlib macros
are not plain top-level user macros; include a guard such as `@time grid = ...`
so caller expression assignment targets are not gensymmed. Also guard macro-return
named tuples such as `(value=result, time=0.0)`: tuple-contained `=` nodes carry
field labels, not local assignment targets, and the labels must not be gensymmed
(Issue #9619).

**Macro quote hygiene binding boundaries (Issue #9702).** Whenever a NEW
macro-return AST shape is supported, the hygiene pass collectors in
`subset_julia_vm/src/macro_runtime.rs` (`collect_quote_local_names`,
`collect_assignment_target_names`, `rename_quote_local_symbols`) must preserve
the distinction between five binding boundaries — the runtime macro AST reuses
the same `Expr(:=, lhs, rhs)` shape for several of them:

1. **Plain assignment targets** `x = ...` — quote-local, collected and gensymmed.
2. **Tuple destructuring targets** `(a, b) = ...` (tuple on the LHS of `=`) —
   each element is a genuine quote-local target and is collected.
3. **Named tuple field labels** — an `=` node *contained inside*
   `Expr(:tuple, ...)` carries a field label, never a local assignment target;
   the label symbol stays verbatim while non-`esc` references in the field
   *value* are still renamed.
4. **`esc(...)` / hygienic-scope subtrees** — left untouched (caller-owned).
5. **Quoted-data subtrees** (`Expr(:quote, ...)`) — opaque to hygiene; never
   collect from or rewrite them.

Unit guards pinning boundaries 1–3:
`macro_runtime::tests::quote_hygiene_collectors_skip_named_tuple_field_labels_9702`
and `macro_runtime::tests::quote_hygiene_collects_tuple_destructuring_targets_9702`.
Any macro-hygiene change must run BOTH fixture categories (they cover disjoint
fixture sets):

```bash
timeout 1800 cargo nextest run --release --test fixture_tests macros:: --no-fail-fast
timeout 1800 cargo nextest run --release --test fixture_tests macro_tests:: --no-fail-fast
```
When adding a new `Expr` head or macro-return value shape, update
`src/expr_heads.rs` and keep the quote constructor, macro-return lowering, and
runtime `eval` support bits honest. For Base macro changes, run the direct
fixture plus `scripts/check_metaprogramming_roundtrip.sh`, the relevant
`fixture_tests` category, and a full `timeout 1800 cargo nextest run --release`
before embedding/reporting cache results.

### WebAssembly build with embedded cache

The same `SJULIA_BASE_CACHE` and `SJULIA_PRELUDE_PROGRAM_CACHE` mechanisms work
for the `subset_julia_vm_web` WASM build. Embedding both caches eliminates the
first-execution Base compile and prelude source parse/lower in the browser
Playground (the on-disk persistent cache is unavailable in WASM). Bincode v1
uses little-endian fixed-width integers, so caches generated on the host are
consumable by the wasm32 build.

Use the helper script:

```bash
scripts/wasm_build_with_cache.sh                                  # default: --target web
scripts/wasm_build_with_cache.sh --target nodejs
scripts/wasm_build_with_cache.sh --target web --out-dir ./web/pkg # custom output dir
```

Relative `--out-dir` paths are resolved against the *invocation* PWD, so
`./web/pkg` from the repo root means `<repo>/web/pkg` even though the
script `cd`s into the web crate before calling wasm-pack.

Or equivalently, the three-step procedure:

```bash
cargo build --release --bin sjulia --features repl                          # Step 1
./target/release/sjulia --precompile-prelude "$(pwd)/target/prelude_program_cache.bin"
./target/release/sjulia --precompile-base "$(pwd)/target/base_cache.bin"    # Step 2
cd subset_julia_vm_web
SJULIA_PRELUDE_PROGRAM_CACHE="$(pwd)/../target/prelude_program_cache.bin" \
SJULIA_BASE_CACHE="$(pwd)/../target/base_cache.bin" \
  wasm-pack build --target web --profile web-release                        # Step 3
```

**Size tradeoff**: embedding the caches adds the cache files' size to the
resulting `.wasm`. Pick per deployment based on whether download size or
first-run latency matters more.

**Iteration speed (Issue #4438)**: `--precompile-base` output is byte-
deterministic for a given prelude. `scripts/wasm_build_with_cache.sh` skips
the Step 2 cache regeneration when existing `target/base_cache.bin` and
`target/prelude_program_cache.bin` files are newer than both
`target/release/sjulia` and the prelude sources, preserving mtimes so cargo's
incremental tracking does not invalidate `subset_julia_vm` (which
`include_bytes!`s the caches and would otherwise force a full WASM relink under
`lto = true`). Pass `--force-cache` to opt out, or delete the files by hand.

## New Built-in Type Implementation (Issue #3106)

When implementing a new built-in collection or value type (e.g., `Memory{T}`, `Set`, custom struct):

1. [ ] Implement Rust VM handler in `vm/exec/`
2. [ ] Add `BuiltinId` and routing in `builtins_*.rs`; update `docs/vm/BUILTIN_OWNERSHIP.md`
3. [ ] Add Julia-side wrappers in `src/julia/base/`
4. [ ] Add fixture tests covering ALL of these categories:
   - Construction: empty, small (n=3..5), default/typed
   - `getindex`/`setindex!`: basic read/write, boundary indices (1 and end)
   - Mutation: in-place loop update, element swap, `fill!`
   - Type variants: at least 3 element types (Int64, Float64, String or Bool)
   - Small integer types: Int8/Int32/UInt8/Float32 if the type is parameterized
   - Size/length: `length()`, `size()` correctness including zero-length
   - Larger allocation: 50–100 elements + sum/iteration pattern
   - Copy semantics: `copy()` creates independent copy
   - Error handling: out-of-bounds (`BoundsError`) if applicable
5. [ ] Verify each `.jl` file with `julia path/to/test.jl`
6. [ ] Update `docs/vm/DONE.md` and `STATUS.md`

## Name-Keyed Rewrites in the Shared Plan / Register VM (Issue #9803, 2026-07-10)

When adding a plan-build, SSA, register-VM, or AoT rewrite that recognizes a
call by its FUNCTION NAME (e.g. `Float64(x)` → conversion node): Julia lets
users overload even constructor names, so a name+shape match alone silently
bypasses user-method dispatch (PR #10139 regressed
`dispatch/symbol_type_param_dispatch.jl` this way — caught only by the full
suite, not by category tests).

1. [ ] Gate the rewrite on the SAME builtin-vs-user-dispatch evidence the stack
       compiler uses (`compile_generic_dispatch_call` routing: no reachable
       method table for the name, no param/where type-var shadowing). Default
       the gate CLOSED. Precedent: `plan::NumericConvertGate` +
       `lower.rs::numeric_convert_gate`.
2. [ ] Pin the negative case: a test where a user-defined method with that
       name (or a non-numeric operand) is NOT rewritten
       (`shared_plan_numeric_convert_rewrite_requires_open_gate_9803`).
3. [ ] Before merge, run the FULL `timeout 1800 cargo nextest run --release`
       plus `--test ssa_pipeline_tests` — the bypass class shows up in the
       dispatch fixtures and the SSA-parity integration binary, not in the
       rewrite's own module tests.

## Runtime Specializer Name-Keyed Callee Fast Paths (Issue #10418, prevention for #10146)

The VM runtime specializer (`subset_julia_vm_vm/src/vm/specialize/expr.rs::compile_call`)
lowers many calls by FUNCTION NAME (`"Float64"`, `"Int64"`, `"sqrt"`,
`"round"`, …) directly to builtin instructions. Julia resolves the callee
through local scope before global methods or builtins, so a parameter or
local named like a builtin must win over every such fast path. Before PR
#10417 a specialized body compiled a parameter named `Float64` as the
builtin constructor (`f(Float64) = Float64(2)` returned `2.0` instead of
calling the argument); the fix is the front-door local-callee guard
(`self.locals.contains_key(function)`) at the top of `compile_call`.

When adding a name-keyed fast path to the specializer (a new `match function`
arm in `compile_call`, or any early route keyed on a user-visible callee
name):

1. [ ] The new fast path must run AFTER the front-door local-callee guard —
       or explicitly prove the name cannot be locally bound at that point
       (e.g. compiler-internal `_names` that the parser can never bind).
       `bash scripts/check_specializer_callee_guard.sh` enforces the
       ordering inside `compile_call` (CI `code-audits` job); if you split
       or rename `compile_call`, update that script in the same PR.
2. [ ] Extend the shadowing matrix with the new name:
       `vm::specialize::tests::test_issue_10418_local_callee_shadowing_matrix_over_specializer_fast_paths`
       (instruction-level: shadowed → `LoadAny` + `CallFunctionVariable`,
       never the builtin instruction; unshadowed → the builtin instruction)
       and, for user-visible behavior,
       `subset_julia_vm/tests/fixtures/functions/parameter_shadows_numeric_constructor_10146.jl`
       (verify against upstream `julia` first).
3. [ ] Remember the sibling surfaces from the Issue #10418 blast radius:
       `compile_convert_call` target names, `compile_builtin`, and the
       field/array specialization in `stmt.rs` need the same review if they
       start matching user-visible callee names. For plan/SSA/register-VM
       name-keyed rewrites, use the "Name-Keyed Rewrites in the Shared
       Plan / Register VM" checklist above instead.

## VM Instruction Routing Changes (Issue #3275)

When modifying `emit_return_for_type()`, `emit_store_for_type()`, or `emit_load_for_type()` in `compile/stmt.rs`:

1. [ ] Add fixture tests in the **same PR** covering the changed types (e.g., `typeof(f()) == TargetType`)
2. [ ] If adding a new `ValueType` variant, include tests for return, store, and load paths
3. [ ] Run `timeout 1800 cargo nextest run --release --test fixture_tests` to verify

## Fused-op / New `Instr` and `TypedLoopOp` Variant Checklist (Issue #10814, tracked by #10817)

Issue #10817 (LOC/touchpoint debt barometer, `docs/vm/NORTH_STAR.md` NS-7 (c))
found that fused-op variant counts grow by op × type × operand-shape
combinatorics. Before Issue #10814, the typed-loop bail/effect safety net was a
**hand-written denylist** (`matches!` on individual op names) that each new
op's author had to remember to extend; #10504 shows that it missed existing
bail-capable ops. `TypedLoopOp::effects()` now supplies one exhaustive metadata
classification from which the transactionality guard is derived.

When adding a new `Instr` variant (`subset_julia_vm_bytecode/src/instr.rs`) or
`TypedLoopOp` variant (`subset_julia_vm_vm/src/vm/executable.rs`):

1. [ ] Classify every new `TypedLoopOp` in `TypedLoopOp::effects()` for both
   `bail_capable` and `out_of_buffer_effect`, and add it to the existing
   exhaustive stack-effect / jump-target matches. Never add a wildcard `_`
   arm: an unclassified variant must remain a compile error.
2. [ ] If the new variant can bail out of a typed-loop block (arithmetic that
   can trap, a call that can deopt, …) **and** performs a buffer-external
   effect (heap array store, RNG advance, …), verify by hand that the
   recognizer rejects that combination (or add the missing rejection) — see
   Issue #10814 Evidence (#10504/#10536) for the historically missed pairs.
   `IndexStore*` is now transaction-buffered (Issue #10566(c)); `RandF64` is
   the current out-of-buffer effect.
3. [ ] Update `subset_julia_vm_compile/src/compile/cache.rs`'s
   `EXPECTED_INSTR_VARIANT_COUNT` pinned constant when the `Instr` variant
   count changes (test asserts `Instr::VARIANTS.len()`; also doubles as the
   ground truth `scripts/loc_report.sh` cross-checks its own count against).
4. [ ] Run `bash scripts/loc_report.sh` and note the new `Instr`/`TypedLoopOp`
   variant count in the PR if it moved by more than a handful — this is the
   NS-7 (c) touchpoint barometer (`docs/vm/NORTH_STAR.md`), not a gate, but a
   large jump is exactly the growth-quality signal Issue #10817 asks PRs to
   self-report.
5. [ ] Add a differential/parity fixture comparing the typed-loop path against
   the generic interpreter path for the new op (Issue #10814 P1 acceptance
   criterion), when the op is perf-sensitive enough to need a typed-loop form
   at all.

See also `docs/vm/NORTH_STAR.md`'s "Fused-op / `Instr`・`TypedLoopOp` variant
追加の前提条件" section for the design rationale, and Issues #10452 (root-cause
analysis this policy responds to), #9089/#10461 (the equivalent call-path-level
"one semantic resolver" principle this is the instruction-level analogue of).

## Storage-Elision Optimization Checklist (Issue #10820, prevention for #10819/#7556)

Issue #10819 (root cause: `CoreCompiler::store_local` treated
`ValueType::Nothing` as a compile-time singleton and elided its storage —
`Instr::Pop` only — while still marking the local initialized in compiler
metadata). A later assignment that widened that local to an `Any`-backed slot
in only one control-flow branch then emitted a `LoadSlot`/`LoadAny` reachable
via the non-assigning path with no backing value, raising `UndefVarError`
even though the Julia source had executed `x = nothing`. Any optimization
that skips emitting a physical store for a local — because its value is
"known" to be a singleton, a compile-time constant, or otherwise
representation-elidable at the point of assignment — reopens this exact
failure mode if a LATER control-flow path can still widen that local's
runtime representation.

When adding or extending such a storage-elision optimization:

1. [ ] Confirm every later read of the local (via any local Load
   instruction — `LoadSlot`/`LoadAny` and the rest of the paired
   Load/Store family) is dominated by an ACTUAL store on every reachable
   predecessor path, not just on the path the optimization was designed
   around. Compiler metadata that marks a local "initialized"
   (`CoreCompiler::initialized_locals`) is bookkeeping, not proof of a
   runtime-visible value — see `memory/reference/reference_singleton_local_storage_widening.md`.
2. [ ] Materialize the elided value's storage (e.g. `StoreAny`/
   `StoreGlobalAny`) rather than only marking metadata, whenever ANY other
   branch, loop body (including a loop that may execute zero times), or
   `try`/`catch` clause can independently assign that same local to a
   different representation. A constant-push fast path is fine for reads
   PROVEN to stay within the elided representation; it is not fine as the
   only write.
3. [ ] Add fixture coverage for both the assigning and non-assigning paths,
   across `if`, `try`/`catch` (assigning try vs. assigning catch), and a
   loop that may run zero times — see
   `subset_julia_vm/tests/fixtures/control_flow/nothing_initialized_branch_widen_10819.jl`
   and `nothing_initialized_trycatch_loop_widen_10820.jl`.
4. [ ] For a non-trivial change to this area, run the dominance/dataflow
   invariant check: `cargo test -p subset_julia_vm_compile --lib
   slot_backing_verifier` (`subset_julia_vm_compile/src/compile/slot_backing_verifier.rs`,
   a test-only pass — it costs nothing in production compiles). It verifies
   that every local read in a REAL compiled function is dominated by a
   store on every predecessor path, over both hand-built CFG shapes and the
   actual post-slotization bytecode of the #10819/#10820 fixture functions.
   A violation it reports on functions with previously-passing runtime
   behavior indicates a gap in the verifier's own model (missing
   instruction classification, wrong `initially_backed` set) — fix the
   verifier, do not allowlist. A violation on a genuinely new function is
   the real defect this checklist exists to catch.

## RegisterVM Feature Work Checklist (Issue #10060)

When widening the `SJULIA_REGISTER_VM=1` register subset — a new supported
type/op, a new `RegisterInstr`, or relaxing a gate rejection in
`subset_julia_vm_vm/src/vm/register_gate.rs` (prevention for Issues #10047/#10054):

1. [ ] **Classify every new/changed `Instr` variant** in
   `instr_is_register_unsupported_stack_marker()`
   (`subset_julia_vm_vm/src/vm/register_gate.rs`). The match is deliberately
   exhaustive with no `_` arm: a new `Instr` variant (dynamic call, dynamic
   conversion, global marker, …) is a **compile error** until it gets an
   explicit register-gate decision. Never "fix" the compile error by adding a
   wildcard arm.
2. [ ] Update `register_supported_value_type()` / `register_supported_slot_type()`
   together with the `RegisterInstr` expansion — the gate and the interpreter
   must widen in the same PR, never independently.
3. [ ] Add a **positive register execution test** in
   `subset_julia_vm/tests/register_vm_tests.rs`: assert stack/register output
   parity AND that `register_vm_executed_calls() > 0` (the function actually
   took the register path, not a silent fallback).
4. [ ] Add a **negative / currently-stack-only test** for adjacent unsupported
   value kinds (e.g. the Issue #10047 `Float32`/`BigInt` pins): assert output
   parity AND `register_vm_executed_calls() == 0` so partially-supported
   semantics cannot be mis-boxed by partial register execution.
5. [ ] Pin classification changes in
   `register_gate_marker_classification_pinned_issue_10060` (unit test in
   `register_gate.rs`) when a variant moves between the marker and
   gate-neutral arms.

**P6 / default-switch acceptance** (see `docs/vm/REGISTER_VM.md`, Issues
#9904/#9906): any PR that widens the register subset, and any default-engine
switch decision PR, must keep **full fixture parity under the gate** green and
record it:

```bash
SJULIA_REGISTER_VM=1 timeout 1800 cargo nextest run --release --test fixture_tests --jobs 1
```

## Adding AoT Builtin Ops (Issue #3279)

When adding a new `BuiltinOp` variant to the VM IR:

1. [ ] Add a dedicated `AotBuiltinOp` variant in `aot/ir/` — do NOT reuse an existing variant as a proxy
2. [ ] Update `builtin_op_to_aot()` in `aot/analyze/ir_converter/helpers.rs`
3. [ ] Add `return_type()`, `Display`, `from_name()`, and codegen entries for the new `AotBuiltinOp`
4. [ ] If a dedicated variant is not feasible, add `// Workaround: ... (Issue #NNNN)` and create a tracking Issue

## Adding Generated-Wasm Static Data

Generated-Wasm string literals use a backend-only, typed string view. The view
is two little-endian `i32` fields: `{utf8_byte_pointer, utf8_byte_length}`.
Length is the number of UTF-8 bytes, not characters, and the view is distinct
from generated-module array descriptors and the native C ABI.

1. [ ] Intern literals in deterministic first-use order and emit one active data segment.
2. [ ] Keep literal views and payload bytes outside allocator metadata; align the heap base after all static data.
3. [ ] Treat literal data as immutable in Julia lowering. Do not lower mutation, concatenation, or interpolation as a static literal.
4. [ ] Test ASCII, empty, embedded NUL, multibyte UTF-8, duplicate interning, direct calls, memory growth, and byte-identical repeated compilation.
5. [ ] Validate with `wasm-tools`, instantiate with zero imports, and decode exact bytes in Node.
6. [ ] Keep browser compiler artifacts source-stale unless the task explicitly includes a package rebuild.

## Adding an AoT Codegen Template (Issue #11202)

Before adding or changing generated Rust for a call, builtin, assignment, or
aggregate, follow the full contract in
[`AOT_OWNERSHIP_CONVENTIONS.md`](./AOT_OWNERSHIP_CONVENTIONS.md):

1. [ ] Inspect the generated callee signature and record which arguments are
       owned, borrowed, or shared handles.
2. [ ] Classify each argument as a `Copy` scalar, runtime `Value`, immutable
       aggregate, mutable alias-bearing representation, or fresh rvalue.
3. [ ] Search for later uses of every binding passed to an owned parameter. A
       move requires a binding-identity-aware last-use proof; spelling/source
       order is not proof.
4. [ ] For mutable alias-bearing values, use a borrow or shared-handle ABI.
       Do not deep-clone a typed container or mutable struct to satisfy rustc.
5. [ ] Clone a reusable owned `Value` only after confirming that its payload's
       `Clone` preserves Julia identity and mutation visibility.
6. [ ] Add a generated-Rust Cargo/rustc test that calls the template twice with
       the same non-`Copy` binding. A source substring assertion is insufficient.
7. [ ] Run `bash scripts/test_aot.sh`.

## Adding a New Literal/Value Type (Issue #3320, #3304)

When adding a new numeric or value type that should be injectable into REPL, update ALL 12 files in the Literal pipeline:
1. `subset_julia_vm_types/src/ir/core.rs` — Add `Literal::NewType` variant
2. `compile/expr/mod.rs` — Add `Literal::NewType → PushNewType`
3. `compile/utils.rs` — Update BOTH `eval_literal_default` AND `infer_literal_type`
4. `compile/inference.rs` — Update `infer_value_type`
5. `compile/expr/infer/mod.rs` — Update type inference
6. `compile/expr/infer/julia_type.rs` — Update JuliaType inference
7. `compile/abstract_interp/engine/mod.rs` — Update LatticeType
8. `aot/inference/engine/mod.rs` — Update StaticType
9. `aot/analyze/ir_converter/helpers.rs` — Update AotExpr conversion
10. `vm/builtins_macro/ir_conversion.rs` — Update `literal_to_value`
11. `vm/specialize/expr.rs` — Update specialization
12. **`repl/converters.rs`** — Update `value_to_literal()` with type-faithful mapping (**most often forgotten**)

Additionally, when adding a new `Value` variant to `REPLGlobals::set()` (Issue #3287):
- Add to the `other_vars` catch-all arm OR a dedicated typed map
- If injectable: add to `test_all_other_vars_injectable_types_return_some()` in `repl/converters.rs`
- If NOT injectable: add to the non-injectable comment list with tracking Issue
- Run `test_value_to_literal_type_fidelity` and `test_repl_globals_set_handles_all_value_variants_without_panic`

For every new heap-backed / mutable / reference-like `Value` variant (Issue #9582):
- Declare its identity policy explicitly in both direct `===`
  (`builtins_equality.rs::egal_compare_witnessed`) and default immutable struct
  field identity (`equality.rs::compare_struct_field_values_egal`), even when the
  policy is intentionally "not identical".
- Extend `reference_like_values_have_explicit_egal_policies_issue_9582` with a
  same-reference case and a distinct-allocation/same-value case.

When adding or changing an array-like wrapper constructor (`view`, `reshape`, future wrappers) (Issue #8246):
- Keep call-site inference and runtime equality normalization in sync: concrete inputs should infer to an `AbstractArray` subtype instead of widening to `Any`, and equality must normalize the runtime wrapper against itself and native arrays.
- Add or update `subarray_array_like_wrapper_contract_8246` for runtime equality coverage, including the #8240 `view == view` shape and at least one non-`SubArray` wrapper.
- Add or update compile-time unit coverage for constructor inference, such as `array_like_view_constructor_contract_infers_concrete_subarray_8246`, when the constructor has a dedicated inference path.

When broadening method-table `Any` return re-inference (Issue #8246):
- Do not add an unbounded call-site body re-inference fallback. Add recursion/work-budget protection and a focused regression for the motivating call.
- Run `test_repl_value_display_uses_user_show_7168`; REPL result echo must keep user `show` behavior and must not stack-overflow through recursive show paths.

When changing method registration, method replacement, static dispatch, or
direct-call compilation (Issue #9665):
- [ ] Include a source-order redefinition fixture where a call is textually
      between two definitions of the same generic. The earlier call must see
      only the prefix of the method table visible at its source span, while a
      later call sees the redefinition.
- [ ] Cover both same-signature redefinition and a later more-specific method.
      Keep `subset_julia_vm/tests/fixtures/dispatch/source_order_redefinition_world_age_9400.jl`
      in the PR test plan.
- [ ] If the change affects REPL/eval or function-body world visibility, include
      `worldage_intra_eval_9400_is_identical_across_models_9199_s6` and add the
      #9650 function-body variant when that issue is fixed.

## REPL Persistence of Callable Globals (Issues #9436/#9707)

`repl/converters.rs::callable_value_to_expr` is the ONLY hook that rebuilds a
callable REPL global as a name-only source expression (`Expr::FunctionRef` /
`compose(...)`) for the `inject_globals` prelude — both directly for top-level
globals and recursively via `value_to_init_expr_inner` for callables nested in
arrays/structs. Values it declines (returns `None`) are value-carried through
`seed_globals` instead.

When adding a new callable-like `Value` variant (or a new composed callable
form), classify it by **capture safety** in the same PR:

- [ ] Decide and document (in the variant's `callable_value_to_expr` match arm)
      whether the value is fully described by function identity — then a
      name-only `FunctionRef` rebuild is safe — or carries a runtime
      environment (captured locals, curried arguments, callable struct fields)
      that source text cannot faithfully express.
- [ ] If the environment cannot be represented as source, the converter MUST
      return `None` so the value flows into `seed_globals`. NEVER emit a
      partial init expression: a name-only rebuild of `add5 = makeadder(5)`
      lost the captured `n` and the broken prelude statement poisoned every
      later eval in the session, including `1 + 1` (Issue #9436).
- [ ] Classify the variant in `classify_callable_capture_safety` in the
      `repl/converters.rs` test module. It matches exhaustively over ALL
      `Value` variants with no `_` arm, so a new variant breaks compilation
      until classified; keep it in agreement with `callable_value_to_expr`
      (`test_callable_value_to_expr_agrees_with_capture_safety_classification_9707`).
- [ ] Composition rule: one capture-carrying component anywhere in a
      `ComposedFunction` chain must make the WHOLE composition value-carry
      (`callable_value_to_expr` recurses with `?`, so any `None` component
      propagates).
- [ ] Session-level pin: keep
      `repl/tests.rs::test_repl_captured_closure_global_persists_without_poisoning_session_9436`
      green — the captured-closure global persists AND later unrelated evals
      (`1 + 1`, `using Printf`) still succeed.

## Adding a Value Payload for an Existing Julia Type (Issue #9593)

When adding a new `Value` payload that represents an already-existing Julia
semantic type (for example `Value::StrBytes` is still `String`, not a new Julia
type), update and test every semantic boundary in the same PR:

- [ ] Runtime dispatch and reflection: update `get_value_type`,
      `get_value_julia_type`, `typeof` / `get_type_name`, and any
      type-parameter binding path that sees the payload.
- [ ] Typed storage: update `ArrayData`, `MemoryValue`, `StoreSlot*` /
      `LoadSlot*`, array construction, and mutation helpers (`push!`,
      `insert!`, `pop!`, `deleteat!`) so the new payload can round-trip through
      the declared Julia element type.
- [ ] Collections and wrappers: cover `collect`, typed array literals,
      `Vector{T}(undef, n)` + assignment, and wrapper-backed arrays where the
      semantic type can appear.
- [ ] Display and byte/value observers: update `show`, `print` / `repr`,
      `escape_string`-style helpers, equality, hashing, ordering, and any
      raw-payload observer without routing through a lossy carrier.
- [ ] Regression fixture coverage: include both the direct constructor form and
      an `Any`-erased dispatch case (`x::Any; f(x::T)`), plus homogeneous
      `Vector{T}` storage/collection and visible display/repr assertions.
- [ ] Prevention check: decide whether an exhaustive enum coverage test or
      `scripts/check_*.sh` audit can enforce the alias. If not, say why in the
      PR body and file a follow-up Issue before merging.

## Type Representation Syntax Changes (Issue #9627)

When adding or changing type-system syntax, especially bounded `TypeVar`
spellings (`<:Bound`, `>:Bound`, `T<:Bound`, `T>:Bound`,
`Lower<:T<:Upper`):
- [ ] Preserve both lower and upper bounds across `CoreType::from_julia_name`,
      `CoreType` → `JuliaType`, `JuliaType` → `CoreType`, and canonical
      rendering.
- [ ] Update
      `typevar_bounds_survive_core_juliatype_round_trip_issue_9627` with the
      new spelling or representation.
- [ ] Add or update fixture coverage for user-visible subtype/render behavior,
      including at least one non-Array parametric type and any Array/Vector
      projection affected by the syntax.
- [ ] Check subtype/pattern matching paths that consume the representation, not
      just the parser.
- [ ] For TypeVar / UnionAll binder work (Issue #9746), include same-name
      TypeVars in different scopes so name-only matching cannot pass. Keep
      `subset_julia_vm/tests/fixtures/types/typevar_name_collision_scope_9563.jl`
      in the PR test plan.
- [ ] Include runtime type-object alias cases when touching UnionAll/type-object
      equality or subtype paths: generic user UnionAll aliases, tuple Vararg
      aliases, and array aliases. Do not rely only on parser/display tests.
- [ ] Run `bash scripts/check_no_typevar_name_heuristic.sh` after changing
      TypeVar parsing, matching, dispatch resolver, subtype, or runtime
      type-object comparison code. Extend the audit if a new name-only TypeVar
      comparison helper appears outside its current source list.

## Adding New ConcreteType Variants (Issue #3187)

See `docs/vm/LATTICE_TYPE.md` for the full checklist. Key steps:
1. Add variant to `ConcreteType` enum in `compile/lattice/types.rs`
2. Update `test_all_concrete_type_variants_constructible` coverage test
3. Update all exhaustive match sites (`type_depth`, `convert_concrete_to_array_element`, bridge conversions)

## Adding New ArrayData Variants (Issue #3233)

When adding a new `ArrayData(Vec<T>)` variant:
- [ ] `element_type()` — return appropriate `ArrayElementType`
- [ ] `raw_len()` / `is_empty()` — delegate to inner vec
- [ ] `type_name()` — return static str
- [ ] `sum_as_f64()` — numeric types cast to f64; non-numeric → `0.0`
- [ ] `get_value()` — map element to `Value`
- [ ] Add unit tests for each method

## Adding New ArrayElementType Variants (Issue #3230)

When adding a new `ArrayElementType` variant:
- [ ] Update `is_isbits()` — add to true-arm if primitive, false-arm if heap-allocated
- [ ] Update `julia_type_name()` — return correct Julia name
- [ ] Update `to_value_type()` / `from_value_type()` — bidirectional mapping
- [ ] Add tests for `is_isbits`, `julia_type_name`, and round-trip conversion

## Adding New IOKind Variants (Issue #3236)

When adding a new `IOKind` variant:
- [ ] Add factory constructor `IOValue::new_kind() -> Self`
- [ ] Add `is_kind(&self) -> bool` predicate
- [ ] Update `is_open()` handling
- [ ] Update mutual-exclusivity tests (each `is_*` returns false for new kind)

## Changing SerializedBaseCache (Issue #3240)

When modifying `SerializedBaseCache` or `CACHE_VERSION`:
- [ ] Increment `CACHE_VERSION`
- [ ] Verify `test_serialize_deserialize_roundtrip_empty_program` passes
- [ ] Verify `test_version_mismatch_returns_error` passes
- [ ] If adding a new field, add a specific assertion in the round-trip test
- [ ] If a nested persisted wire type changes (for example `MethodSigWire`),
      include that source in the schema fingerprint, bump `CACHE_VERSION`, and
      assert the new identity field survives its own serde round trip (Issue
      #10959).

### Compile-context field restore parity (Issue #10223, escape #10092)

When adding or changing a field in `StructInfo` / `StructDefInfo` or any other
dispatch-relevant compile-context table that the Base/prelude caches carry:
- [ ] The field is either SERIALIZED in the cache format or RECONSTRUCTED from
      IR on **every** cache-restore path — both
      `pipeline_ctx.rs::build_struct_tables` and
      `cache.rs::restore_compile_context_from_program`. (#10092: both restore
      paths silently rebuilt `has_inner_constructor: false` for every cached
      struct because `StructDefInfo` did not serialize the flag, so `WeakRef(x)`
      skipped its outer constructor only when a persistent cache was present.)
- [ ] A fresh compile and a cache-restored compile produce identical tables for
      the affected field (compare, or cover with a fixture whose behavior
      depends on the field).
- [ ] Run the both-cache-mode fixture lane:
      `bash scripts/check_cache_sensitive_fixture_lane.sh` (Issue #10223) —
      and tag any new cache-mode-dependent fixture `cache_sensitive = true` in
      its manifest.

## Core `Stmt` Control Flow / Closure Capture Paths (Issue #11278)

When adding a `Stmt` variant, changing a statement's lexical-scope boundary, or
adding another closure-capture/pre-analysis path:

1. [ ] Classify the variant explicitly in both
       `collect_let_scope_function_captures` and
       `collect_hard_scope_function_captures`; keep their `match` expressions
       exhaustive (no wildcard fallthrough). Document whether the statement
       transfers bindings through a sequence, joins successor paths, or creates
       a hard child whose new bindings die at its boundary.
2. [ ] Extend
       `hard_scope_capture_liveness_tests::module_and_nested_capture_paths_share_control_flow_contract_issue_11278`
       with the same Core IR shape for module hard-scope pre-analysis and a
       nested function. Record body-load and creator-metadata expectations
       separately: `LoadCaptured` positives, `LoadAny` negatives, and the
       matching `CreateClosure.capture_names` inclusion/non-inclusion.
3. [ ] Include a strict negative whose value does not exist at closure creation
       (the later-assignment row is the baseline) so a dynamic `LoadAny` caller
       fallback cannot disguise an accidental capture. If creator-side lexical
       analysis is intentionally more conservative than body pre-analysis,
       preserve that difference as explicit table columns and explain why; do
       not silently make both expectations `true`.
4. [ ] For `if`, cover dispatch-free constant selection and unknown-condition
       definite assignment. For loops/hard children, cover a zero-trip or
       nonescaping child. For `try`, cover independent try/catch/else/finally
       clauses plus catch-binder inside-catch lifetime and nonescape.
5. [ ] Run the targeted release-fast lib matrix and prove effectiveness with a
       temporary semantic mutation that makes the matrix RED, then restore the
       contract and record a GREEN run. Runtime clause-local storage and
       same-frame visibility are a separate concern (Issue #11281).

## Binding Provenance Consumers And Runtime Global Keys (Issue #11317)

Use this checklist when adding a `LocalDeclKind`, consuming `Stmt::LocalDecl`
semantically, or emitting a runtime frame-0 load/store for an explicit global.

1. [ ] Register every semantic `Stmt::LocalDecl` consumer in
       `docs/vm/BINDING_PROVENANCE_CONSUMERS.tsv`. Bind `kind` and match every
       `LocalDeclKind` variant explicitly; do not use `..` or a wildcard where
       declaration provenance affects collection, capture, rendering, or
       execution. Generic recursive visitors that do not consume the declared
       variable are outside this semantic inventory, but every reviewed
       whole-variant `LocalDecl { .. }` ignore remains count-ratcheted by the
       audit; classify any new occurrence before updating that ratchet.
2. [ ] Route declared-global loads and stores through
       `CoreCompiler::emit_load_declared_global` or
       `CoreCompiler::emit_store_declared_global`. Those helpers own the
       module-qualified frame-0 runtime key. Module/import opcodes with distinct
       lookup semantics are not declared-global substitutions.
3. [ ] Extend the existing table-driven contract tests instead of adding a new
       integration-test binary. Across the VM and AoT tables, retain Main,
       module, and function contexts; function, loop, and try scopes; explicit,
       fresh, and compiler-generated bindings; normal and exceptional exits;
       and typed and dynamic value paths.
4. [ ] Run `bash scripts/check_binding_provenance_authority.sh`, the targeted
       `check_audit_negative_selftest.sh` controls for both an ignored
       `LocalDeclKind` and a raw runtime global key inside a declared-global
       branch, the relevant VM regression matrix, and the full default and AoT
       gates.

### REPL frame-0 persistence ownership (Issue #11725)

Use this matrix when adding a frame-0 store producer or changing REPL post-run
projection. A storage name is not its persistence owner: user Main values,
qualified module values, visible import publication, and compiler import
metadata may all execute frame-0 stores.

1. [ ] Classify the producer structurally. Do not infer Main ownership merely
       because a name was visible in the compiled main scope, and never project
       a qualified `M.x` key into the Main value/type mirrors.
2. [ ] Keep visible import publication and compiler provenance/ambiguity slots
       out of value persistence. Package module roots are bindings, not `Any`
       globals to feed back into the next compile.
3. [ ] Carry a qualified value created only by a called function under its
       module-owned runtime key, then prove redefining that module removes the
       old carried member instead of resurrecting it.
4. [ ] Extend
       `repl_binding_ownership_matrix_survives_fresh_rebuild_11725` in the
       existing consolidated REPL test. Cover success and catchable error, and
       assert Main values, Main type metadata, module values, and the observable
       value both before and after a forced fresh rebuild. Treat `ans` separately
       as the explicit host-facing result mirror.
5. [ ] Run the targeted release-fast test, the #9784 regression selection, and
       the full release suite before merge.

## Try-Clause Lexical Ownership / Strict Soft Scope (Issues #11305/#11322/#11335; related #11159)

Use this matrix when changing `Stmt::Try`, top-level binding discovery, or the
strict file-mode soft-scope pass. A try/catch/else/finally clause has its own
lexical owner, but a clause directly under module scope still applies Julia's
strict soft-scope decision to assignments; do not classify the clause itself as
a hard-scope construct.

- [ ] Preserve source-order provenance as separate mutable-global, const-global,
      and retired-clause-local facts. A plain `HashSet<String>` cannot represent
      the warning and shadowing behavior of all three states. A later real
      global/const must remove an older retired-local marker, and an existing
      global must never be retired by a clause shadow.
- [ ] For an ordinary clause assignment, cover all outcomes: an existing mutable
      global is renamed to a fresh local **with** the upstream ambiguity warning;
      an existing const is renamed **without** a warning; a spelling retired by
      an earlier clause is renamed without a warning; and explicit `global`
      continues to target the module binding.
- [ ] Build value-slot candidates from assignment-bearing inventory entries.
      Do not treat `FunctionDef` as an ordinary assignment: generic/method
      identity has separate ownership semantics (Issue #11319).
- [ ] Keep sibling clauses isolated and recurse through nested try/if/block/loop
      shapes when retiring clause-local spellings. Stop executable provenance
      collection at function and quote boundaries. Conversely, propagate an
      enclosing localized name into an ordinary nested-clause assignment;
      only explicit local/global, catch binder, or separate function identity
      shadows it (#11159).
- [ ] Test the contract at three layers: lowering IR (fresh-name ownership),
      runtime value (`@isdefined` and unchanged outer global/const), and CLI
      stderr (warning present or absent). Include a later top-level loop so
      compiler metadata cannot masquerade as a live runtime binding.
- [ ] Do not claim execution-aware top-level effects from a purely syntactic
      clause scan. Untaken-clause explicit globals are tracked by #11338, and
      try expressions nested below general value expressions remain in #11159.
- [ ] Do not assume an untouched bare fresh loop name is already nonescaping.
      Prove post-loop `@isdefined == false` and a post-loop read raises
      `UndefVarError`; the remaining direct-loop lifetime gap is #11339.

## Privileged Lexical State Across Lifted / Eval Boundaries (Issue #11211)

Use this matrix whenever a lowering context carries privileged lexical state
into a `Function` or another side-list payload. Struct-body `new` authority is
the current concrete example; future capabilities must name equivalent seams.

| Boundary | Required behavior |
|---|---|
| Ordinary nested function | Inherit the active lexical authority structurally. |
| Lifted closure | Stamp authority in `LambdaContext::add_lifted_function` at creation time. |
| Macro-lifted task/thunk | Use the same creation-time seam; do not patch a later slice. |
| Runtime `@eval` function | Clear authority for the entire lowering dynamic extent. |
| Eval-lifted closure/thunk | Remain cleared through collection and every descendant. |
| Ownerless ordinary `new` / `new{T}` call | Preserve call shape and use ordinary callee lookup; never synthesize struct ownership. |

- [ ] Establish or clear the authority **before** lowering the body. Do not use
      `lifted_function_count` / `lifted_functions_from_index` as a post-hoc
      stamping watermark.
- [ ] Keep direct root stamping, creation-time lifted stamping, and structural
      collector propagation as the only `Function.new_struct_name` mutation
      sites. Run `bash scripts/check_lambda_context_routing.sh`.
- [ ] Keep this fixture family registered together under `struct/manifest.toml`:
      `global_new_helper_11005.jl`, `ownerless_new_lookup_11204.jl`,
      `ownerless_new_keyword_lookup_11204.jl`, and
      `ownerless_parametric_new_lookup_11204.jl`.
- [ ] When adding a new transparent drain or call shape, extend that family
      across both an authority-bearing lifted descendant and an eval-cleared or
      ownerless descendant.

## Source-Order Comparisons (Issues #11036, #11100)

- [ ] Use raw `Span::start`/`end` only for names, diagnostics, or positions
      proven to belong to one parsed fragment. A definition/use visibility
      comparison must accept an opaque `SourcePosition`, which carries the
      fragment identity, rather than separate `usize` offsets.
- [ ] Keep TLS-restoring source guards non-`Send`, and mint positions only from
      the currently active guard; a retained outer scope must not stamp an
      inner fragment's offsets with the outer identity.
- [ ] For chronology across include, package, prelude/Base cache, or REPL
      fragments, use lowering-assigned `Span::definition_order` and merge via
      `DefinitionOrderCursor`; never infer order from registration-pass order.
- [ ] Run `bash scripts/check_source_position_chronology.sh` after adding a
      lowering pre-scan registry or comparing a definition position with a use
      position. Add same-fragment earlier/later and different-fragment tests.

## Constructor Registration / Dispatch Changes (Issues #10959, #10962, #10974, #11028)

Julia distinguishes bare `Foo(...)` (implicit `Type{Foo}` self) from explicit
`Foo{T}(...)` (implicit `Type{Foo{T}}` self); `MethodSig` projects that self
argument away entirely. `MethodTable`'s serialized
`constructor_self_families: BTreeMap<usize, ConstructorSelfFamily>` is the
sole, cache-surviving source of truth for which of a struct's own methods are
its inner constructors (and which self family) — never reconstruct this from
projected signatures, arity, bounds, or `where` parameter count (a `Foo(x::T)
where T` outer can have an identical projected shape to an inner). When
changing constructor registration (`pipeline_ctx.rs`'s
`add_inner_constructor_method` call site) or any constructor selector
(`compile/expr/call/constructors.rs`, `dispatch.rs`):

- [ ] Run `bash scripts/check_constructor_identity_authority.sh`: no
      `MethodSig::is_inner_constructor`-style side boolean or production field
      read may reappear; selection must query the owning table, type-stability
      reconstruction must use `MethodSig::from_julia_projections`, and cache
      replay must retain `core_signature` plus `constructor_self_families`
      (Issue #11043).
- [ ] Exercise the owner-exact constructor identity matrix: user aliases;
      bounded self binders; qualified-vs-bare lexical owners; same-leaf types
      from different modules; and cold, primed, and cached execution parity.
      For aliases/binders, include a leaf-name collision so a suffix match
      cannot accidentally pass (Issue #11043).
- [ ] Cover BOTH a bare `Foo(...)` call and an explicit `Foo{T}(...)` call for
      any new/changed selector path.
- [ ] Cover the exact-collision case: an outer method whose value signature
      AND `where` clause coincide with an inner constructor's (the #10959
      counterexample — `has_where_params()`/arity/bounds alone cannot
      distinguish them).
- [ ] Cover both last-definition-wins identities: a later explicit-parametric
      inner replaces the earlier explicit row without disturbing a bare outer
      (`explicit_inner_redefinition_replaces_inner_but_preserves_outer_10959`),
      while a bare inner and ordinary constructor with an identical value
      signature share `Type{Foo}` and replace one another in either direction
      (`bare_inner_and_ordinary_constructor_share_last_definition_wins_11028`).
- [ ] Compare constructor chronology with lowering-assigned
      `Span::definition_order`, never pass order or raw `Span::start`/`end`:
      byte offsets are local to each included/loaded file, while structs and
      ordinary methods are registered in separate passes (Issue #11028).
- [ ] Route every independently lowered Program/Module fragment (prelude, Base,
      separate REPL eval, package source/cache) through
      `DefinitionOrderCursor`. Use `append_fragment` for a true append and
      `insert_fragment_after` for a package loaded at a stamped `using`/`import`
      source event; never infer chronology from raw byte spans. Includes and
      batched Base files may transfer vectors raw only because they share one
      `LambdaContext`. Rebase both stored definition vectors and executable
      copies nested in module/main/function/macro/inner-constructor blocks;
      block-local methods retain the same ordinal as their stored copy (Issue
      #11144). Update `DEFINITION_ORDER_MERGE_INVENTORY.tsv` and run
      `check_definition_order_merges.sh`; cover both constructor orders across
      same-file, include, REPL, loaded-module, fresh-cache, and restored-cache
      boundaries, including shared dependencies imported at different package-
      local anchors (Issues #11036/#11128/#11144, related semantic snapshots
      #10462).
- [ ] If a selector queries a table found via `parse_parametric_call` on the
      table's OWN literal key (e.g. `"Rational{T}"`, `"Boxed8103{N,T}"`) —
      distinct from the bare struct-name table where inner constructors
      register — do NOT ALSO filter by
      `is_explicit_parametric_inner_constructor`: table membership there
      already scopes to the explicit self family, and outer constructors
      registered under that literal key never carry the marker. Adding a
      redundant filter there rejects legitimate user-declared outer
      constructors (regressed Issue #8103 during the #10962 migration).
- [ ] If `MethodTable`'s serialized shape changes, follow "Changing
      SerializedBaseCache" above (bump `CACHE_VERSION`, refresh the schema
      fingerprint) AND add/extend a round-trip test proving the carrier
      survives a real `bincode`/`serialize_base_cache` →
      `deserialize_base_cache` boundary (see
      `constructor_self_family_round_trips_and_filtered_clone_drops_stale_rows_10962`
      and `base_constructor_self_family_survives_cache_round_trip_10962`).
- [ ] Extend `subset_julia_vm/tests/fixtures/struct/parametric_inner_ctor_outer_where_10959.jl`
      with any newly-reachable shape rather than duplicating its existing
      exact-collision / redefinition / runtime-forwarding assertions.
- [ ] Runtime-dependent dispatch (a local `DataType` value, or selecting
      among multiple candidate constructors at runtime) is a SEPARATE,
      independently-owned capability gap — see #10968 (local `DataType`) and
      #10971 (per-candidate static binder forwarding). Do not silently claim
      those cases fixed by an identity/cache-persistence change.
- [ ] Run `bash scripts/check_constructor_owner_resolution.sh`. Do not add a
      `short_constructor_name`/leaf-table probe outside the classified owners in
      `CONSTRUCTOR_OWNER_FALLBACK_INVENTORY.tsv`; new paths must resolve the
      canonical owner and consult the owning MethodTable identity instead.
- [ ] Exercise constructor call ownership across qualified, current-module bare,
      and selective-import spellings; concrete, inferred-parametric, and explicit
      `{...}` applications; and positional, direct-kwargs, kwargs-splat, and
      positional-splat forms. Use a callable Base leaf collision (`Dict` or `Set`)
      and assert exact `typeof`, not only value equality (Issue #11172).
- [ ] When lexical owner lookup moves earlier, keep an explicit Base-origin
      negative control: a visible same-leaf user constructor with a matching value
      signature must not enter `Base.<name>` candidates. For kwargs/splats, trace
      callee → positional expressions → keyword expressions → splat iteration →
      dispatch, and require a throwing splat iterator to win over the eventual
      constructor `MethodError` (Issue #11177).
- [ ] Audit the runtime `Value::DataType`/apply-type path separately: qualified
      candidate lookup must remain exact, parametric inner lookup must confirm the
      canonical owner, and any default-constructor bare fallback must remain
      unique-or-fail. Compile-time owner evidence does not prove this runtime path.
- [ ] If a field on `StructDef`/`InnerConstructor` gains new load-bearing
      meaning (as `is_explicit_parametric` did once its `has_where_params()`
      fallback was removed), remember there are **two** persistent caches
      that can serve a stale value: the Base cache (`precompile.rs`
      `CACHE_VERSION` / `audit_base_cache_schema_fingerprint.sh`) AND the
      third-party package loader's `.ji.json` cache
      (`subset_julia_vm/src/loader.rs` `CACHE_VERSION` /
      `module_schema_fingerprint()`). The latter's fingerprint probe does
      NOT automatically cover every `Module`-nested type — it only reflects
      fields the probe explicitly populates (`structs`/`inner_constructors`
      included since Issue #11004; verify newly-relevant fields are covered
      by the probe, not just `#[serde(default)]`-defaulted). A real stale
      on-disk `.ji.json` regressed
      `packages_data_structures_binary_max_heap_8509` this way during the
      #10962 migration — bump both `CACHE_VERSION` constants when in doubt,
      and clear `$TMPDIR/subset_julia_vm_cache` (or `SUBSETJULIA_CACHE_DIR`)
      locally before trusting a "still broken" result from a bundled-package
      fixture.
- [ ] When a parser/lowering path mutates ambient state that is read
      after lowering, inventory the cache-hit lane explicitly. Prefer deriving
      that state from the validated `Program`/`Module` at one post-load commit
      boundary; do not serialize a process-global snapshot. Package nominal
      declarations must cover structs, abstract types, primitive types, and
      nested-module owner paths, with a payload-only cache-hit negative test
      (Issue #11280).

## BuiltinId / cache-schema ratchet: baseline と snapshot は同じ PR で更新する (Issue #10256)

`scripts/check_no_new_domain_builtins.sh` と
`scripts/audit_base_cache_schema_fingerprint.sh` は snapshot/ratchet 型の audit
なので、対象を変更した PR 自身が baseline/snapshot を更新しない限り、その PR の
merge 後に main で赤くなり **無関係な後続 PR 全部をブロック**する(実例:
PR #10067 → Issue #10247、PR #10229 → Issue #10241、PR #10224 の再発)。
両 audit は `scripts/premerge_gate.sh` の default gate に登録済み(#9696/#10256)
だが、gate を待たず PR 内で先に済ませること:

`BuiltinId` variant を追加/削除したとき:
- [ ] `docs/vm/RUST_BOUNDARY_JUSTIFICATION.md` 条件 1–4 のどれに該当するか判断し、
      variant の直前に `// Boundary: condition N (<why>), Issue #NNNN` コメントを付ける
- [ ] `scripts/check_no_new_domain_builtins.sh` の `BASELINE_BUILTIN_COUNT` を
      **同じ PR で** bump し、スクリプト内コメントに PR/Issue 付きで attribution を記録する
      (recorded-bump 手順、Issue #9696)
- [ ] `bash scripts/check_no_new_domain_builtins.sh` が green

`subset_julia_vm_compile/src/compile/base_cache_schema_files.txt` 記載のファイル
(またはマニフェスト自体)を変更したとき:
- [ ] fingerprint はファイル内容を丸ごと hash するため、audit が新 fingerprint を
      報告したら `CACHE_VERSION` を bump する
- [ ] `precompile.rs` の version 履歴へ `Bumped to <n>` コメントを追加し、Issue と
      cache compatibility 上の理由を記録する
- [ ] `bash scripts/audit_base_cache_schema_fingerprint.sh --update` で snapshot を
      **同じ PR で** 更新し、read-only モードで green を確認する
- [ ] default premerge 登録を固定する sync control と、manifest 記載ファイルを実際に
      変更する negative control が green: `bash scripts/check_source_only_audit_sync.sh`
      および該当 `check_audit_negative_selftest.sh` control (Issue #10688)

## Draft PR certification and emergency correction (Issue #11056)

Agent-created implementation PRs remain draft until the lead finishes review
and every required local gate. The author does not mark ready or merge. The
lead runs `bash scripts/premerge_gate.sh --pr <N>` from the exact PR head: the
script verifies the current `origin/main`, clean committed HEAD, draft/base/head
identity, requested gates, and final freshness; only then does it mark ready
and perform a regular merge pinned to the certified SHA. It publishes the
`sjulia/guarded-certification` commit status; the active GitHub `protect main`
ruleset requires that context with strict up-to-date-branch semantics, so a
manual ready/merge cannot bypass local certification (Issue #11087). Run both
`bash scripts/premerge_readiness_selftest.sh` and
`bash scripts/github_merge_ruleset.sh --selftest` after changing this workflow,
and verify live configuration with `bash scripts/github_merge_ruleset.sh --check`.
`--apply` performs GitHub's full-ruleset PUT, which has no compare-and-swap
parameter; repository administrators must serialize ruleset edits until it
finishes. The status is intentionally unbound because this repository has no
dedicated certification GitHub App: it blocks accidental/manual merge bypass,
but trusted writers with status-write permission must not forge the context.

Emergency override is repository-owner-only and must be time-bounded: record
the incident Issue first, disable only the required certification rule, perform
the corrective regular merge, immediately restore it with
`bash scripts/github_merge_ruleset.sh --apply`, and attach a fresh
`--full-suite --pr` equivalent verification result to the incident. Never leave
the ruleset disabled for unrelated merges.

If an uncertified head nevertheless lands on `main`, use the corrective
sequence exercised by Issue #11044 / PR #11045:

- [ ] File the regression `bug` Issue before changing code; record the exposed
      behavior and the prematurely merged PR/head.
- [ ] Fetch current `origin/main` and branch from that exact commit. Do not
      repair from the stale pre-merge review branch.
- [ ] Apply the reviewed root-cause correction and regression coverage, then
      complete the mandatory independent/adversarial review.
- [ ] Run the relevant narrow checks and the full guarded suite on the exact
      current-main head.
- [ ] Open/keep the corrective PR draft and land it only through
      `premerge_gate.sh --pr <N>`; confirm the pinned regular merge and Issue
      closure before reporting recovery complete.

## New Primitive Type / New Operator Method (Issue #3699)

When adding a new primitive numeric type (Int128, UInt128, Float16, …) or a
new operator method (`+`, `-`, `*`, `/`, `÷`, `%`, `==`, `<`, `<=`, `>`, `>=`)
that touches one of those types, verify each of the four type-preservation
layers — see `docs/vm/TYPE_PRESERVATION.md` for the full four-layer model.

Code-review checklist (paraphrased from Issue #3699):

- [ ] **Compile-time inference**: When adding a constructor type-inference
      case (`"Int64" => ValueType::I64`), is the matching entry in
      `infer_julia_type.rs` also present? They have to agree, otherwise
      inline `Type(x)` is inferred as `Any` and the type-preserving early
      routes never fire.
- [ ] **Compile-time early-routes**: When adding/modifying a binary-op early
      route in `compile/expr/binary/mod.rs`, does it list **all** primitive
      operand types it should cover, or does it only mention I64/F64? The
      BigInt early-route used to swallow Int128 (Issue #3621); UInt128 had
      no early route at all (Issue #3697).
- [ ] **Pure Julia method-table**: Keep bundled Base numeric signatures in the
      upstream shape: use parametric / Union signatures such as
      `div(x::T, y::T) where {T<:BitSigned}`, `zero(x::T) where {T<:Number}`,
      `convert(::Type{Complex{T}}, x::Real) where {T<:Real}`, and
      platform-width `::Int` index/dimension arguments instead of
      enumerating primitive widths (`div(x::Int64, y::Int64)`,
      `zero(x::UInt8)`, `signed(x::UInt64)`, `dims::Int64...`) per method.
      Run `bash scripts/check_div_specializations.sh` to ensure the old
      concrete same-type `div` matrix has not been reintroduced.
- [ ] **Runtime fallback (intrinsics_exec.rs)**: When extending an intrinsic
      exec, does it preserve the operand's wide type (`I128` / `U128` /
      `F16`) or does it `pop_i64`-truncate?
- [ ] **Runtime fallback (binary_both.rs / dynamic_ops/)**: When extending
      `CallDynamicBinaryBoth`, does the small-int prologue (~315) accept
      the new type without `try_from` to I64? Above-i64::MAX comparisons
      used to raise OverflowError (Issue #3696). For `dynamic_div` /
      `dynamic_mod` / etc. — does every F16/I128/U128 arm exist?
- [ ] **Numeric matrix coverage (Issue #8698)**: Add one row/value/operator in
      `scripts/gen_numeric_matrix_fixture.jl` for the new type or operator by
      updating `REDUCED_VALUE_SPECS`, `FULL_VALUE_SPECS`, or `OP_SPECS`.
      Regenerate and commit the reduced oracle:
      `julia --startup-file=no scripts/gen_numeric_matrix_fixture.jl`, then run
      `bash scripts/check_numeric_matrix_reduced.sh`.
- [ ] **AoT numeric matrix slice (Issue #9565)**: if the new numeric type,
      operator, or promotion rule is intended to work in AoT, extend
      `scripts/aot_numeric_matrix_reduced.sh` beyond its current Int64/Float64
      supported slice, update its `EXPECTED_ROWS` ratchet, and shrink
      `docs/vm/NUMERIC_MATRIX_AOT_REDUCED_SKIPLIST.tsv`. Then run
      `bash scripts/test_aot.sh` so the same upstream TSV oracle is checked
      against the generated AoT binary, not only the VM.
- [ ] **Full matrix nightly ratchet (Issue #8698)**: Run the full profile before
      merging numeric tower changes:
      `mkdir -p target/numeric-matrix-full && julia --startup-file=no scripts/gen_numeric_matrix_fixture.jl --profile full --out-tsv target/numeric-matrix-full/oracle.tsv --out-fixture target/numeric-matrix-full/unused.jl`
      followed by
      `ORACLE=target/numeric-matrix-full/oracle.tsv ALLOWLIST=docs/vm/NUMERIC_MATRIX_FULL_ALLOWLIST.tsv SKIPLIST=docs/vm/NUMERIC_MATRIX_FULL_SKIPLIST.tsv OUT_DIR=target/numeric-matrix-full TIMEOUT_SECONDS=900 bash scripts/check_numeric_matrix_reduced.sh`.
      Count changes are regressions or improvements. Since Issue #9849,
      `docs/vm/NUMERIC_MATRIX_FULL_ALLOWLIST.tsv` is a zero-residual ratchet:
      run `bash scripts/check_numeric_matrix_full_allowlist.sh` and do not add
      non-header allowlist rows without fixing the regression or filing/linking
      a new Issue for the residual.
- [ ] **Power shortcut parity (Issue #9656)**: Every compiler shortcut that
      bypasses `DynamicPow` must document why its result type matches upstream
      for the full accepted base/exponent family. For fixed-width integer and
      Bool bases, keep `DynamicPow` as the default unless the shortcut preserves
      the base type for signed/unsigned widths, Bool, BigInt, BigFloat, and mixed
      exponent widths. Include direct, first-class (`pow = ^`), broadcast, and
      downstream arithmetic guards; keep
      `subset_julia_vm/tests/fixtures/arithmetic/int128_power_preserves_type_9608.jl`
      in the PR test plan when touching `^` routes.
- [ ] **Splatted promote fallback parity (Issue #9620)**: When porting or
      rewriting an upstream numeric fallback shaped like
      `f(x::Integer, y::Integer) = f(promote(x, y)...)`, verify both the
      canonical splatted form and the equivalent two-variable form. The
      splatted callable-value path must dispatch on the promoted tuple's
      runtime element types and select diagonal / bounded-`where` candidates
      instead of re-selecting the broad fallback. Keep
      `subset_julia_vm/tests/fixtures/dispatch/splatted_promote_self_recursion_9513.jl`
      in the PR test plan and run the `dispatch::` fixture category when
      touching `CallFunctionVariableWithSplat`, callable-value dispatch
      scoring, or promote-fallback operator methods.
- [ ] **Promote-fallback catch-all safety (Issue #9677)**: Before adding
      upstream broad same-type numeric catch-all methods or changing
      promote-fallback operator guards, run custom same-type wrapper coverage
      plus built-in primitive route guards. The minimum smoke set is
      `f = ^; f(2, 2)`, `[2, 3] .^ [2, 2]`, narrow integer broadcast `.+`,
      `Float16`/`Float32` `mod`/`rem`, and `pi < pi` / `pi <= pi`. Keep
      `subset_julia_vm/tests/fixtures/promotion/promote_not_sametype_guard_9334.jl`
      and
      `subset_julia_vm/tests/fixtures/promotion/promote_fallback_primitive_route_guards_9677.jl`
      in the PR test plan for these changes.
- [ ] **N-ary promotion arity coverage (Issue #9896)**: When changing a
      value-level or type-level n-ary Base API such as `promote` /
      `promote_type`, test singleton, fixed-arity, vararg, and splatted tuple
      forms. Keep
      `subset_julia_vm/tests/fixtures/promotion/promote_varargs_9830_9831.jl`
      and `subset_julia_vm/tests/fixtures/promotion/promote_not_sametype_guard_9334.jl`
      in the PR test plan, and run
      `bash scripts/check_promote_builtin_no_tuple_fallback.sh` so a failed
      `BuiltinId::Promote` method lookup cannot silently return unchanged
      arguments as `Value::Tuple`.
- [ ] **BigFloat mixed integer operands under `setprecision` (Issue #9605)**:
      When touching BigFloat arithmetic, comparison, or power paths, add value
      assertions and `precision(result)` assertions for mixed integer operands
      and integer exponents at a boundary wider than the active precision
      (for example `setprecision(BigFloat, 64)` with `big(2)^64 + 1`). Keep
      `BigFloat(integer)` constructor semantics separate from mixed-operation
      operand semantics: the constructor rounds to the active precision, while
      the mixed operation keeps the integer exact until the final result
      rounding.
- [ ] **BigFloat allocation precision (Issue #9651)**: Every new BigFloat
      constructor or result-producing helper must state which allocation
      precision it carries. Zero-valued, `Inf`, and `NaN` results must use
      explicit side metadata (`RustBigFloat::new_with_precision` or an
      equivalent helper), not mantissa-bit inspection. Add zero/non-finite
      precision assertions in fixtures or Rust unit tests; keep
      `subset_julia_vm/tests/fixtures/bigfloat/bigfloat_zero_precision_9599.jl`
      and `subset_julia_vm_bytecode::value` BigFloat precision tests in the PR
      test plan.
- [ ] **BigFloat conversion and signed-zero exactness (Issue #9682)**: New
      BigFloat conversion paths must test `signbit(BigFloat(-0.0))`,
      Float16/Float32 negative-zero promotion, `Bool(false)` strong-zero
      multiplication, and `promote(BigFloat, BigInt)` exactness. Printed output
      alone is not enough; assert `signbit`, `typeof`, value equality, and
      precision where relevant. Keep
      `subset_julia_vm/tests/fixtures/bigfloat/bigfloat_mixed_residuals_9515.jl`
      and the `RustBigFloat::from_f64(-0.0, p)` unit test in the PR test plan.
- [ ] **Rational zero-denominator invariant (Issue #9616)**: When touching
      `//`, `Rational{T}` constructors, Rational normalization, or Rational
      `rem`/`mod`, preserve both sides of the upstream boundary:
      `0//0` must throw `ArgumentError`, while nonzero `1//0` and `-1//0`
      remain valid Inf/-Inf sentinels. Verify direct constructor entry points
      and arithmetic paths that synthesize results through constructors. Keep
      `subset_julia_vm/tests/fixtures/rational/rem_mod_inf_divisor_zerozero_9514.jl`
      in the PR test plan and run the `rational::` fixture category for any
      Rational implementation rewrite.

## Comparison-Chain Lowering Changes (Issue #9632)

When changing `lower_binary_expr`, `lower_binary_expr_with_ctx`, or the shared
comparison-chain builder in `lowering/expr/binary.rs`:

- [ ] Test scalar chained comparisons for truth value, interior call count, and
      short-circuit call count.
- [ ] Test dotted comparison chains for fused broadcast truth values and
      single evaluation of non-atomic interior operands.
- [ ] Cover the operator matrix in
      `comparison_chain_single_eval_matrix_9632.jl` (`<`, `<=`, `==`, `!=`,
      `<:`, `>:` where applicable) or intentionally update that matrix in the
      same PR.
- [ ] Keep `comparison_chain_non_atomic_interiors_lower_to_letblock_9632`
      passing so scalar and dotted non-atomic interiors lower through a
      `LetBlock` instead of duplicated operands.

Both inline-from-constructor AND variable-bound forms must be tested with
`typeof()` assertions (the variable-bound form often passes when the inline
form fails because variable type tracking surfaces the right ValueType,
hiding the gap):

```julia
# Inline (constructor calls in the expression itself)
@test typeof(Int128(1) + Int128(2)) == Int128

# Variable-bound (operands flow through a let)
x = Int128(1); y = Int128(2)
@test typeof(x + y) == Int128
```

Companion fixtures: `subset_julia_vm/tests/fixtures/type_preservation/*_matrix.jl`.

## Broadcast Lowering / Tuple Materialization Changes (Issue #9805)

When changing `lower_broadcast_call_expr`, `lower_argument_list`, or
`subset_julia_vm/src/julia/base/broadcast.jl` materialization:

- [ ] Keep the direct lowering guard
      `broadcast_call_lowering_distinguishes_argument_list_from_tuple_operand_9805`
      passing: `f.(x, y)` lowers to two broadcast operands, while `f.((x, y))`
      lowers to exactly one tuple operand.
- [ ] Add or update tuple-only broadcast assertions that check both value and
      `typeof(...)`: promoted tuple broadcast, Bool tuple broadcast, empty
      tuple, nested tuple, and the mixed array+tuple boundary.
- [ ] Include `subset_julia_vm/tests/fixtures/broadcast/tuple_materialize_shape_9533.jl`
      in the PR test plan for any change to tuple broadcast materialization.
- [ ] If a broken-but-green allowlist row improves, remove it in the same PR
      and keep the allowlist ratchet in the verification plan.

For zero-field value-parameter constructors such as `Val{N}()` and
`Base.HasShape{N}()` (Issue #9731), every regression fixture must include both:

- [ ] a bound RHS form (`shape = Base.HasShape{1}(); ...`) so inference-driven
      direct `NewStruct(type_id, 0)` materialization is covered.
- [ ] an inline argument/comparison form (`sprint(show, Base.HasShape{1}())`,
      `IteratorSize(xs) == Base.HasShape{1}()`) so the explicit parametric
      constructor helper path cannot regress while the bound form still passes.

When touching parametric constructor resolution, keep
`subset_julia_vm/tests/fixtures/types/value_param_constructors.jl` in the PR
test plan and run at least one direct `sjulia -e` inline constructor MWE.

## Owner-Scoped Struct Resolution (Issue #11046)

When changing `StructRegistry`, struct registration, type annotation conversion,
constructor routing, or compile-time field-layout lookup:

- [ ] Carry the declaring module separately from display spelling with
      `insert_owned` when a module-owned parametric instantiation keeps a bare
      display name.
- [ ] Resolve semantic layouts through `resolve_scoped`: exact qualification,
      current-module owner, Main/Base owner when requested, then lexical alias.
- [ ] Do not reintroduce `base_struct_table`/`base_origin_bare_names`; shadowed
      declarations remain reachable through the owner-name index and
      `canonical_entries`.
- [ ] Register bare lexical aliases with `insert_alias`; never infer alias
      identity from equal `type_id` and field layout across owners.
- [ ] Resolve a legacy `ValueType::Struct(type_id)` layout with
      `resolve_type_id`, whose first-declaration index is deterministic while
      Issue #11167 remains open; do not scan `canonical_entries` and take the
      first `HashMap` match.
- [ ] Keep inference-only `StructTypeInfo` name lookup behind
      `lookup_struct_type_info`; identity remains the carried `type_id`/
      `ConcreteType::Struct`, not the projection map key.
- [ ] Run `bash scripts/check_name_based_lookup.sh`, its target-selected
      negative self-tests, the module identity fixtures, and fresh/cache parity.

## Reflection / Type-Object Field Metadata Changes (Issue #9540)

When changing `RuntimeTypeObject`, `_fieldnames`, `_fieldtypes`, `fieldcount`,
`nfields`, `typeof` recovery, or type spelling/canonicalization for a container
or struct family:

- [ ] Verify field-name value kind, not just field count: tuple type field names
      are integer positions (`(1, 2)`), while struct and named-tuple type field
      names are symbols.
- [ ] Keep `fieldnames`, `fieldtypes`, `fieldcount`, and `nfields` covered
      together for empty tuple, concrete tuple, user struct, and named tuple
      shapes.
- [ ] Include
      `subset_julia_vm/tests/fixtures/types/test_nameof_nfields.jl` in the PR
      test plan and run `types_tests::` when reflection/type-object metadata changes.
- [ ] If a new type family has non-symbol or derived field names, add the
      same-shape case to the matrix before relying on
      `RuntimeTypeObject::field_names`.

### Runtime TypeVar projection identity domains (Issues #10412/#10420)

When changing `unionall_var()`, `unionall_body()`, `parameters_with_values()`,
`reflection_parameter_to_value`, `reflection_parameter_to_value_for_owner`, or
any `.var` / `.parameters` reflection projection path:

- [ ] Treat `UnionAll.var` and body `.parameters` as ONE owner-scoped identity
      domain: within a wrapper chain (`Vector`, `Vector.body`,
      `Dict.body.body`, …) the same binder position must project to the same
      `RuntimeTypeVarValue` identity
      (`runtime_typevar_projection_identities`, keyed by the structural
      `TypeVarProjectionKey`: normalized final-body owner + binder depth +
      parsed as-declared bounds; the display name is value-side metadata,
      Issues #10261/#10987). `Vector.var === Vector.body.parameters[1]` and
      the Dict two-binder chain must hold.
- [ ] Constructed parametric type arguments are a SEPARATE identity domain: a
      user `T = TypeVar(:T)` recorded at construction time
      (`runtime_typevar_identities`, keyed by `(name, upper)`, Issue #4698)
      keeps `Vector{T}.parameters[1] === T`, and must never leak into
      wrapper-chain projections (upstream:
      `Vector.body.parameters[1] !== T`).
- [ ] Cache precedence: when both caches could answer an owner-scoped
      projection, the owner-local projection cache wins over the global
      constructed-TypeVar cache — guarded by the unit test
      `reflection_parameter_to_value_for_owner_prefers_owner_scoped_projection_issue_10420`
      (`vm/builtins_reflection`).
- [ ] Run the dedicated fixture
      `subset_julia_vm/tests/fixtures/reflection/typevar_projection_owner_identity_10420.jl`
      — its order is load-bearing (it populates the constructed-TypeVar cache
      FIRST, then asserts owner-scoped identity; #10412 only reproduced in
      that order) — plus the `reflection_agg_misc_9671.jl` identity testsets.
- [ ] Known residual (Issue #10603): the two caches share the rendered
      body-name key (`"Array{T, 1}"`, `"Dict{K, V}"`), so identity is still
      read-order dependent in the reversed orders (body `.parameters` before
      `.var`, or `.var` before constructing `Dict{K,V}`). Do not "fix" a
      projection bug by widening a cache fallback — that re-opens the #10412
      leak; the durable fix is structured UnionAll bodies (#10460).

## Struct-Like AST Reflection Field Access Changes (Issue #9546)

When changing `getfield`, `_getfield`, `getproperty`, `isdefined`, AST value
types, or builtin field metadata for `Expr`, `QuoteNode`, `LineNumberNode`, or
`GlobalRef`:

- [ ] Keep field metadata and value access in sync for both Symbol and integer
      field selectors: `fieldnames(typeof(x))`, `nfields(x)`,
      `isdefined(x, name)`, `isdefined(x, index)`, `getfield(x, name)`, and
      `getfield(x, index)` must agree for every represented field.
- [ ] Verify out-of-bounds integer selectors return `false` from `isdefined`
      and throw `BoundsError` from `getfield`, matching upstream.
- [ ] Include
      `subset_julia_vm/tests/fixtures/metaprogramming/struct_like_reflection_matrix_9546.jl`
      in the PR test plan and run `metaprogramming::` when AST reflection field
      access changes.
- [ ] Do not fabricate a runtime value for metadata-only fields. Either keep the
      field explicitly uncovered with a linked Issue, or add a real runtime value
      model and cover `fieldnames` / `isdefined` / `getfield` together. For
      example, `GlobalRef.binding` is covered by the `Core.Binding` value model
      from Issue #10014.

## IO / Display Routing Changes (Issue #9777)

When changing `IOPrint`, `emit_print_text_to_sink`, `render_value_via_io_method`,
`sprint` capture, or Pure Julia `string`/`showerror` routes that create a
temporary `IOBuffer`:

- [ ] Keep explicit `IOBuffer` sinks higher precedence than active
      `sprint_state`: `print(inner_io, x)` inside `sprint(...)` must mutate
      `inner_io`, not the outer sprint buffer.
- [ ] Verify both the direct sprint sink form
      `sprint(io -> print(io, x))` and the nested buffer form
      `sprint(io -> begin inner = IOBuffer(); print(inner, x);
      print(io, String(take!(inner))) end)`.
- [ ] Include these fixtures in the PR test plan when display/string routing is
      touched:
      `subset_julia_vm/tests/fixtures/io/test_sprint_lambda.jl`,
      `subset_julia_vm/tests/fixtures/error/error_typeassert_typeerror_5146.jl`,
      and
      `subset_julia_vm/tests/fixtures/strings/pure_julia_migration_8780_function_values.jl`.
- [ ] If `take!(IOBuffer)` semantics change, re-check the nested-buffer
      assertions and the raw-byte `write` fixture together; binary `write`
      payloads and textual `print`/`sprint` routing must stay separate.
- [ ] Do not use `write(io, x)` / `write(io, arg)` as a display-text fallback
      for arbitrary values. Use `print`/`show` for text paths, or convert raw
      IOBuffer bytes with `String(take!(io))` after explicitly writing text.
      Run `bash scripts/check_julia_display_write_text_paths.sh` when changing
      Pure Julia display helpers.

## Builtin Global Shortcut Checklist (Issues #10056/#10044)

The compiler resolves some bare globals (`stdout`, `stderr`, `stdin`,
`devnull`, `ARGS`, `PROGRAM_FILE`, …) straight to dedicated instructions in
the `Expr::Var(name, _)` arm of `subset_julia_vm_compile/src/compile/expr/mod.rs`.
When adding or changing such a shortcut (new `Push*` instruction, new special
name, or a new load site for an existing builtin global):

**Identity (Issue #10056, prevention for #10035/#10053):**

- [ ] If the global is identity-observable with `===` (mutable object /
      singleton semantics — every IO stream qualifies), the VM handler must
      load a **VM/session-owned singleton ref** (initialized once in
      `subset_julia_vm_vm/src/vm/state.rs`, cloned in
      `subset_julia_vm_vm/src/vm/exec/stack.rs`) — never call an
      `IOValue::*_ref()`-style constructor per instruction. A fresh
      construction per load makes `stdin === stdin` false while all
      print/read behavior still looks correct.
- [ ] Add a `@test <global> === <global>` identity assertion to
      `subset_julia_vm/tests/fixtures/io/redirect_stdio_pipe_9577.jl`.
      The coverage gate
      `push_io_global_instructions_have_identity_fixture_coverage_10056`
      (`subset_julia_vm/tests/fixture_tests.rs`) enumerates every
      `Instr::Push*` handler that pushes `Value::IO` and fails if the
      fixture lacks the matching identity check or the handler constructs
      instead of cloning VM state.

**Local/keyword shadowing (Issue #10044, prevention for bug #10034):**

- [ ] Every compile-time bare-name fast path must prove local/keyword
      bindings shadow it FIRST. Place the special-case under
      `!self.locals.contains_key(name)`; if the name can be introduced by a
      keyword binding (all lowercase user-callable names can), it must also
      be under `!self.initialized_locals.contains(name)` — `locals` alone
      missed keyword parameters, which is exactly how
      `redirect_stdio(; stderr=...)` compiled its keyword parameter as
      `PushStderr` (#10034).
- [ ] If a name intentionally can never be shadowed by a local, annotate the
      special-case with `// no-local-shadow: <reason>` instead.
- [ ] Run `bash scripts/check_compile_expr_local_shadow_guard.sh` — it flags
      any `name == "..."` special-case in the `Expr::Var` arm that has
      neither a shadow guard nor the annotation. The audit first runs an
      isolated grammar matrix for direct `name`, `name.as_str()`, whitespace
      variants, and an unguarded comparison (#11604).
- [ ] When a guarded name carrier gains a projection or witness accessor,
      update the audit's accepted grammar and conformance matrix in the same
      PR. Run `bash scripts/check_audit_negative_selftest.sh --target-path
      scripts/check_compile_expr_local_shadow_guard.sh` to prove both the
      unguarded-source and removed-projection controls still fail.
- [ ] Verify upstream parity of the shadowing behavior with a keyword-shadow
      fixture check (see `io_kw_stderr_shadow_10034` /
      `io_kw_devnull_shadow_10044` in `redirect_stdio_pipe_9577.jl`) and
      extend the fixture when a new stdio-like global gains local-shadow
      semantics or a new shortcut instruction.

## Call-Compile Handler Keyword-Rewrite Ordering Checklist (Issue #10277)

A `compile_*` handler in `subset_julia_vm_compile/src/compile/expr/call/handlers/`
can mix two independent kinds of arms in one function: a **keyword rewrite**
(inspects `ctx.kwargs` and reroutes the call to a different, keyword-aware
implementation) and a **function-specific exclusion** (returns `None` for a
particular callee/arg shape so it intentionally falls through to generic
method dispatch). Issue #10065 / PR #10259 found that `compile_sprint`
(`handlers/strings.rs`) ran its `print` / `Base.print` exclusion arm BEFORE
its `context` keyword-rewrite arm, so `sprint(print, "abc"; context=:compact
=> true)` hit the exclusion, skipped `sprint_context`, and reached unsupported
generic keyword dispatch — even though the very same call with `show` instead
of `print` worked, because `show` wasn't excluded. The bug looked deliberate
(the exclusion IS deliberate for the context-free case) until a keyword
argument was added, which is exactly why targeted tests missed it.

When adding or editing a handler that combines both arm kinds:

- [ ] Order the keyword-rewrite check FIRST, and make any function-specific
      exclusion arm run only in the `else` branch (no keyword match) — an
      exclusion is only safe to skip the keyword-aware route when it cannot
      be reached by a call that also carries the keyword being rewritten. See
      `compile_sprint` in `subset_julia_vm_compile/src/compile/expr/call/handlers/strings.rs`
      for the corrected shape: `context` kwarg lookup happens before the
      `print`/`Base.print` name check.
- [ ] Add a fixture test for every function-specific exclusion arm, exercised
      BOTH without and WITH the keyword form the exclusion could otherwise
      bypass — a positive-only test (exclusion arm with no keyword) cannot
      catch this class of bug. See
      `subset_julia_vm/tests/fixtures/iocontext/test_sprint_context.jl`
      (`sprint(print, "abc"; context=:compact => true)` and a vararg `print`
      case, added in PR #10259).
- [ ] Before adding a NEW function-specific exclusion arm to a handler that
      already has a keyword rewrite (or vice versa), grep the target
      function for existing `ctx.kwargs` checks and existing name/arg-shape
      `return None` exclusions, and reason through which keyword forms would
      hit the exclusion.

**Why not a generic audit script**: as of Issue #10277, `compile_sprint` is
the only handler across `handlers/*.rs` that combines a keyword-rewrite arm
and a name-based exclusion arm inside one function (verified by grepping
every `ctx.kwargs` use under `compile/expr/call/handlers/`); the other
keyword-rewrite handlers (`compile_mapreduce_init_kwarg`,
`compile_reduce_init_kwarg` in `handlers/misc.rs`, and the `base=` rewrite in
`compile_parse_tryparse` in `handlers/strings.rs`) are single-purpose
functions with no separate exclusion arm to mis-order against. A text-order
grep audit today would have zero real targets and would need to guess at a
control-flow shape (`if`/`else`, early `return None`, `match`) that varies
across handlers, risking false negatives on the very next instance it should
catch. Re-evaluate a `scripts/check_*.sh` audit if a second instance of this
pattern appears.

## Keyword Default Binding / Forwarding Checklist (Issue #11140, prevention for #11135)

Keyword defaults cross three distinct runtime boundaries: direct default
binding, body-evaluated default prologues, and reduced-arity positional-default
forwarding stubs. A change that preserves only the value can still lose whether
the caller supplied it, which changes upstream's exception boundary (`TypeError`
for a supplied mismatch, `MethodError` for a mismatched default).

When changing keyword lowering, `KwParamInfo`, default binding, or a keyworded
call path:

- [ ] Preserve omitted-vs-supplied provenance through every forwarding stub.
      A stub must forward the NOT-SUPPLIED sentinel; the full method that owns
      the default must materialize and validate it.
- [ ] Validate the final bound value for literal and body-evaluated defaults.
      Do not validate the `Value::Undef` sentinel itself; it is also the required
      keyword marker and the body-default materialization trigger.
- [ ] Materialize every omitted default left-to-right before validating any
      annotated keyword. Upstream evaluates all defaults before dispatching to
      the typed inner method, so an earlier bad annotation must not suppress a
      later default's side effect or exception.
- [ ] For body-evaluated defaults, distinguish the guard's materialization
      store from the later validation self-store. Seed per-frame skip state only
      when the initial slot is the omitted `Undef` sentinel; a supplied value
      skips no store because its guard does not write.
- [ ] Resolve a `where`-dependent declared type through the selected frame's
      type bindings for both supplied-value and omitted-default assertions;
      these boundaries must share one structural substitution authority.
- [ ] Keep every optional keyword slot `Any` in the compiled body. A literal
      default is only one runtime source and must not select typed loads,
      stores, or returns that reject a different caller-supplied value accepted
      by the annotation.
- [ ] Route the entry assertion through every `StoreSlot*` opcode, not only the
      generic `StoreSlot`; slotization may specialize numeric, string, symbol,
      collection, and struct stores. Validate the raw stack value before any
      typed pop/conversion.
- [ ] Pin upstream exception types separately: supplied wrong-typed values are
      `TypeError`, wrong-typed defaults are `MethodError`, and omitted required
      keywords are `UndefKeywordError`.
- [ ] Exercise the cross-product represented by
      `kwargs/annotated_kwarg_default_type_11135.jl`: literal/call defaults,
      direct/positional-stub/arrow/full-form anonymous entry,
      concrete/abstract/`where`/user-struct annotations, valid supplied values,
      and supplied mismatches. Heap-backed supplied structs must resolve their
      Julia type name rather than compare as `StructRef` (Issue #11024). Also
      run the #11124 sentinel and #11024 supplied-type fixtures.
- [ ] If lowering changes the serialized `Function.body` semantics without
      changing its serde shape, bump `loader.rs::CACHE_VERSION`. Package source
      hash and `module_schema_fingerprint()` cannot invalidate same-shape stale
      lowered bodies (Issue #11154).
- [ ] Re-audit every `generate_default_arg_stubs` caller and every
      `bind_kwargs_defaults` / `bind_kwargs_with_map` call site; these span
      named, dynamic, function-value, HOF, typed-dynamic, and return paths.

Run:

```bash
julia --startup-file=no subset_julia_vm/tests/fixtures/kwargs/annotated_kwarg_default_type_11135.jl
bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/kwargs/annotated_kwarg_default_type_11135.jl
timeout 1800 cargo nextest run --release --test fixture_tests kwargs::
```

## julia/ サブモジュール更新 (Issue #8668)

`julia/` サブモジュールは設計原則 1 の参照先(「実装で迷ったら upstream を読む」)。
パリティ対象 (docs/vm/PARITY_TARGET.md / ルート `PARITY_TARGET` ファイル) の
対象系列の**最新 patch リリースタグ**を指し続けること。

### いつ更新するか

| 種別 | タイミング |
|---|---|
| patch 追従 (例: 1.12.5 → 1.12.6) | 随時(気づいた人が手順を実行) |
| minor 乗り換え (例: 1.12 → 1.13) | Milestone を切り、全 fixture 再検証を計画してから |

### 更新手順

1. **パリティ対象を確認する**  
   `cat PARITY_TARGET` — 対象系列 (例: `1.12`)を確認。

2. **最新 patch の commit SHA を取得する**  
   ```bash
   gh api "repos/JuliaLang/julia/git/refs/tags/v$(cat PARITY_TARGET | tr -d '\n').$(gh api 'repos/JuliaLang/julia/git/matching-refs/tags/v'"$(cat PARITY_TARGET)"'.' --jq '.[].ref' | grep -v 'rc\|alpha\|beta' | tail -1 | sed 's|.*/v||')" --jq '.object.sha'
   # あるいは手動で最新 patch を確認:
   gh api "repos/JuliaLang/julia/git/matching-refs/tags/v$(cat PARITY_TARGET)." --jq '.[].ref'
   ```  
   例: `v1.12.6` → `15346901f0039751c5488744f1f62de7d87510a8`

3. **サブモジュールポインタを更新する**  
   ```bash
   git -C julia fetch origin v<PATCH_VERSION>
   git -C julia checkout <COMMIT_SHA>
   ```  
   または  
   ```bash
   git -C julia fetch --tags origin
   git -C julia checkout v<PATCH_VERSION>
   ```

4. **変更を確認する**  
   ```bash
   git submodule status julia
   # → 先頭に '-' や '+' でなく空白があれば OK、'+' ならサブモジュールが変わっている
   git diff --submodule=log julia
   ```

5. **`julia/VERSION` が対象 patch に一致することを確認する**  
   ```bash
   cat julia/VERSION   # 例: 1.12.6
   ```

6. **ミラーファイルのドリフトを確認する** (Issue #9005)  
   `subset_julia_vm/src/julia/` 以下で `# upstream:` ヘッダを持つファイルが
   今回の bump で upstream から diverge していないか確認する:
   ```bash
   bash scripts/check_upstream_mirror_drift.sh
   ```
   出力中の `[drift]` 行があれば、該当 upstream ファイルの差分をレビューして
   `src/julia/` 側を更新し、`# upstream:` ヘッダの `<sha>` と `(swept YYYY-MM-DD)`
   を新しいサブモジュール HEAD + 今日の日付に更新する。  
   ドリフトなし(全ファイル `[ok]`)なら次のステップへ。

7. **全 fixture パリティ再実行 + 差分収集**  
   ```bash
   # sjulia build (release binary)
   cargo build --release -p subset_julia_vm --bin sjulia --features repl
   # 各 category を対象に fixture_julia_parity.sh を実行(差分が多い場合は --strict を外して)
   bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/<category>/<fixture>.jl
   ```  
   corpus 掃引ができる場合 (Issue #8614): `bash scripts/parser_corpus_sweep.sh`

8. **差分を分類する**  
   - **upstream の意図的変更** (NEWS.md 記載・対応 PR/commit 追跡できる):
     → expected 更新 + 追従 Issue (`bug` / `unsupported-feature`)
   - **その他** (sjulia 側のバグ候補):
     → `bug` Issue を起票してから修正

10. **同一 PR でサブモジュールと `PARITY_TARGET` を更新する**  
   patch 追従の場合: `PARITY_TARGET` の系列は変わらないが、`julia/VERSION` が
   変わるため両者は同時コミットで追跡可能にすること。

   ```bash
   git add julia PARITY_TARGET
   git commit -m "chore: bump julia/ submodule to v<VERSION> (parity target <SERIES>.x)"
   ```

11. **PR を作成し、マージ後に `docs/vm/PARITY_TARGET.md` の既知ドリフト表を更新する**

### 注意事項

- `julia/` サブモジュールに加えた変更は **コードを変更しない**(reference only)。
  コードを変更したくなった場合は upstream の実装を `subset_julia_vm/src/julia/` に
  移植するか Issue を起票すること。
- minor 乗り換えは sjulia の `VERSION` 定数(SubsetJuliaVM 自身のバージョン)の更新とは
  関係ない。`PARITY_TARGET` の系列のみ変更する。
- ローカル環境の `julia` バイナリが既に対象系列の最新 patch でない場合、
  `bash scripts/parity_julia_version.sh` が警告する。juliaup を使っている場合は
  `julia +<SERIES>` チャネルが自動選択される。

## Adding an Internal `_foo` Builtin (Issue #5676)

When a pure-Julia base method needs to reach a Rust primitive via an internal
`_foo`-style name (mirroring `_regex_replace`, `_endswith_regex`, etc.), **five**
edit sites are required — missing any one causes a silent compile success followed
by a runtime `"Unknown function: _foo"`:

1. **`builtins.rs` enum** — add `BuiltinId` variant anywhere in the enum (declaration order
   no longer matters — see below). Then add a new entry in `compile/instr_wire_ids.rs`:
   one arm in `builtinid_to_wire_id` (`BuiltinId::Foo => N`) and one in
   `builtinid_from_wire_id` (`N => BuiltinId::Foo`), where `N` is the next unused wire ID
   (check the existing highest ID + 1). Run `bash scripts/check_instr_wire_ids.sh` to
   verify coverage, no-dup, and no-reuse (Issue #8628).
   See `memory/feedback/feedback_builtinop_enum_append_only.md`.
2. **`builtins.rs` `from_name()`** — `"_foo" => Some(Self::Foo)`.
3. **`builtins.rs` `name()`** — `Self::Foo => "_foo"` (exhaustive match, compile error
   reminds you if omitted).
4. **`compile/base_functions.rs`** — add `"_foo"` to the routing list (~line 673 near
   `"_regex_replace"`) AND to the `all_builtin_names` test array.
5. **`compile/expr/call/mod.rs`** — add an explicit compile arm (near the `"_regex_replace"`
   arm) that calls `self.compile_expr` on each argument then emits
   `Instr::CallBuiltin(BuiltinId::Foo, N)`. **This is the easy-to-miss step**: without
   it, the call lowering falls through to a dynamic by-name call that fails at runtime.

Handler: add a `BuiltinId::Foo => {...}` arm in the owning `execute_builtin_*` module
(`dispatch_builtin!` in `builtins_exec.rs` tries each in order). Pop args in REVERSE
push order (last arg is on top of the stack). Remember to rebuild with `--features repl`
to re-embed updated base `.jl` files.

## 新しい VmError variant を追加するとき (Issue #8664, parent #8643)

`VmError` (`subset_julia_vm_vm/src/vm/error.rs`) に新しい variant を追加すると、
`vm_error_to_exception_value` が**コンパイルエラー**になる(網羅 match、Issue #8664 で保証)。
コンパイルエラーを解消するために**必ず**以下を実施する:

1. **`vm_error_to_exception_value` に arm を追加** (`vm/exec/error_handling.rs`):
   - **B 分類 (ユーザー可視エラー)**: Julia 例外オブジェクトを構築する arm を実装する。
     対応する Julia 例外型を `julia/base/boot.jl` または `julia/base/` で確認し、
     フィールド構成を一致させる(#8212 / InexactError の前例参照)。
   - **C 分類 (VM 内部エラー)**: `return None` と「内部エラー: Julia 例外に変換しない (Issue #NNNN)」コメントを追加する。
   - **D 分類 (dead code)**: 防御的に適切な arm + dead コメントを追加する。

2. **sjulia Base に例外型を追加** (必要な場合):
   - `subset_julia_vm/src/julia/base/error.jl` に `struct NewError <: Exception ... end` を追加。
   - `subset_julia_vm/src/julia/base/errorshow.jl` に `_showerror_str(ex::NewError)` と
     `showerror(io::IO, ex::NewError)` を追加。
   - `subset_julia_vm/src/julia/base/exports.jl` に `NewError,` を追加。

3. **parity fixture を更新** (`subset_julia_vm/tests/fixtures/exceptions/typeof_showerror_parity_matrix_8665.jl`):
   - 新しいエラーを発生させるコード片を追加し、`typeof(e) == NewError` / `e isa NewError`
     / `sprint(showerror, e)` を `@test` で検証する。
   - `julia --startup-file=no` で先に確認してから sjulia で実行し、
     `bash scripts/fixture_julia_parity.sh` でパリティを確認する。
   - `manifest.toml` の既存エントリの説明文も更新する。

4. **`error_code()` に arm を追加** (`vm/state.rs::error_code`):
   - 使われていない variant でも `_ =>` への落ち込みを防ぐため明示 arm を追加する。

5. **分類表を更新** (`memory/reference_vmerror_exception_conversion_matrix.md`):
   - 新 variant の行を追加し集計を更新する。

### 確認コマンド

```bash
cargo check -p subset_julia_vm --features repl           # 網羅 match コンパイルエラー解消を確認
julia --startup-file=no <fixture>                         # upstream parity 先行確認
SJULIA_BIN=target/dev-fast/sjulia bash scripts/fixture_julia_parity.sh \
  subset_julia_vm/tests/fixtures/exceptions/typeof_showerror_parity_matrix_8665.jl
timeout 900 cargo nextest run --test fixture_tests exceptions::  # 全 exceptions 通過
bash scripts/check_workarounds_documented.sh
bash scripts/check_workarounds_sync.sh
bash scripts/check_fixture_test_names.sh
```

## クレート分割・モジュール移動時の影響チェック (Issues #8640/#8654)

設計文書: `docs/vm/CRATE_SPLIT.md`。モジュールを別クレートへ移す PR
(#8655/#8656、および #8653 の AoT 分離) では、コード移動そのものに加えて
以下を必ず確認する:

1. **bincode キャッシュ (#8611/#8626)**:
   - `Instr` / `BuiltinOp` / intrinsic 系 enum は variant 宣言順でシリアライズ
     される。**移動は宣言順を 1 つも変えずに**行う(diff で enum 本体が
     verbatim move であることを確認)。
   - `compile/precompile.rs` のキャッシュヘッダ fingerprint
     (`hash_enum(…, Instr::VARIANTS)` 等) が移動後のパスから同じ VARIANTS を
     参照し続けることを確認する。
   - 移動後に `--precompile-base` / `--precompile-prelude` でキャッシュを再生成し、
     旧キャッシュが**黙って読まれず**検知・再生成されることを確認する。
2. **build.sh / iOS / WASM**:
   - `subset_julia_vm_ffi` は `[lib] name = "subset_julia_vm"` を維持する。
     新クレートは別名 rlib とし、lib 名衝突がないことを確認する。
   - `cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios-sim`
     と `./build.sh`(xcframework、Base cache 埋め込み)、
     `wasm-pack build --target web --profile web-release` が通ること。
   - `SJULIA_BASE_CACHE` 設定時のリビルドは依存クレート全部の relink を誘発する
     (クレート数が増えるほど高コスト)。キャッシュ埋め込みは 2 段ビルド手順
     (本ファイル Issue #2929 節) のまま行う。
3. **workspace 配線 (root `Cargo.toml`)**:
   - 新クレートを `members` と `default-members` に追加する(`_ffi`、および
     #8653 以降の `_aot` は default-members から除外のまま)。
   - `[lints]` 継承(clippy 設定)と feature 転送(`repl` は統合クレートのみ)
     を新クレートにも設定する。
   - CLAUDE.md/AGENTS.md「Rust compile ergonomics」のクレート表と
     CODE_AUDITS.md の clippy スコープを更新する。
4. **検証**: 各移動 PR はロジック変更ゼロ(純粋な mod 移動 + Cargo.toml)とし、
   full nextest + iOS sim FFI ビルドで無影響を確認、`cargo check -p` の
   before/after を計測して #8640 に記録する(計測プロトコル:
   CRATE_SPLIT.md §6)。
5. **ブラスト半径 grep(Issue #9129、失敗様式 F2)**: コード移動は監査スクリプト・
   ベンチ・ドキュメント・CI に**ハードコードされた旧パス**を静かに切る。移動前後で
   参照を洗い、追従を**同一 PR** に含める:
   ```bash
   grep -rl 'subset_julia_vm/src/<旧パス>' scripts/ benches/ docs/ .github/
   ```
   実績(#8655/#8656 のクレート分割):`check_instr_wire_ids.sh`(silent exit 1)、
   `check_dispatch_determinism.sh`(全 11 エントリ missing)、
   `inventory_rust_semantics.sh` の 3 監査 + ベンチ 1 本がパス切れになり、
   **どのゲートも赤くならないまま**防御装置が無効化された。移動後は必ず
   `bash scripts/check_audit_negative_selftest.sh`(監査を監査する; #9129)と
   関連 `check_*.sh` を実行し、監査が依然として**壊れた入力で FAIL する**ことを
   確認する。監査スクリプトが参照するソースパスを変えたら
   `check_audit_negative_selftest.sh` の `SANDBOX_PATHS` も追従する。

## Performance Decision Protocol(Issue #9129 不変量 4)

性能・表現に関わる**大きな判断**(boxing 方式、キャッシュ戦略、命令融合 #9126、
Value 表現変更など)は、直感や単発計測で決めない。#8650(I128 boxing 却下)/
#9097(キャッシュ eviction 却下)で 2 回実証された次の定型手順に従う。**却下でも
知見を残す**ことが手順の一部である(次に同じ提案が出たときの再計測コストを消す)。

1. **判断式を事前に固定する。** 計測を始める**前に**、採否の閾値を数式で書き下し、
   Issue にコメントとして残す。例:「NS-4 代表ベンチの sjulia/upstream 比が
   baseline 比 **+5% 以内**なら採用、超えたら却下」。事後に閾値を動かさない
   (動かしたくなったら別 Issue)。
2. **A/B は交互(interleaved)に計測する。** baseline と candidate を交互に多数回
   走らせ、マシン負荷の drift を相殺する。**両腕の構成が実際に異なることを
   ベンチ内で assert する**(環境変数の実効値・feature フラグをベンチ本体で検証)。
   F4 の再発防止:#9065 で SSA デフォルト化がベンチの legacy 側も SSA-on にし、
   両腕が同一物を計測して「常に green」に壊れた。
3. **計測は正しい層で、正しいビルドで。** cold CLI 時間を VM 単体の結果として
   報告しない。
   - **CLI 数値**:CLAUDE.md「Precompiled cache build」の**2 段ビルド**で
     prelude/Base キャッシュを埋め込んだ `target/release/sjulia` を使う
     (1 回目のビルドはキャッシュ生成用ヘルパ。`SJULIA_PRELUDE_PROGRAM_CACHE` /
     `SJULIA_BASE_CACHE` を設定した 2 回目のビルド産物のみがキャッシュ埋め込み)。
   - **VM 数値**:precompiled `CompiledProgram` を再利用する `Vm::run()`-only の
     Criterion ハーネスで測る。CLI と VM-only を**別々に**報告する。
4. **マシン静音(machine-quiet)を守る。** Criterion 計測中は同一ホストで
   build/nextest/他ベンチを走らせない(並走は +67% の幻影退行を生む前例あり)。
   時間系指標は **CI 計測を正**とし、ローカルは provisional として ambient load を
   注記する(NORTH_STAR.md NS-4/NS-5 の規約)。
5. **機械的に採否し、知見を固定する。** 手順 1 の式に閾値を当てて採否を決める。
   **却下でも** 計測表・判断・理由を `memory/reference/` と該当 `docs/vm/` に残し、
   関連 Issue にリンクする(#8650/#9097 が先例)。性能改善が実測できたら
   `benches/` にベンチを追加する(Issue #3210)。

### Dispatch-cache / dispatch-resolution changes (Issue #9427)

Any change to the L1/L2 dispatch caches, the `call_site_arg_type_id` id
derivation, or which argument kinds are cache-eligible **must compare
`packages::chunk_*` wall time before/after** (not just pass/fail):

```bash
# baseline (origin/main) and branch, same machine, binary already built:
cargo nextest run --release --test fixture_tests -E 'test(packages::chunk_003)'
```

Rationale: #9427 was a **6× slowdown** (`packages::chunk_003` 50 s → ~294 s) from
PR #9404 making closure/`Type{T}` arguments skip L2 and re-resolve every call. It
passed **two** "green" gates — the dispatch bench (monomorphic Int/Float args, all
tracked kinds) and the branch full suite (correctness-only, per-chunk wall time
never compared). A per-chunk timing guard is not cleanly assertable in nextest, so
this before/after comparison is the checklist gate. See TYPE_INTERNING.md
"Untracked-kind re-caching".

### Float range length/materialization oracle changes (Issue #9757)

Any change that adds or modifies a floating range path (`Float32`, `Float64`,
future `Float16`, `StepRangeLen`, colon ranges, or `range(; length=...)`) must
prove that length/bounds and materialization share the same upstream-shaped
numeric oracle. Do not let `length(r)` use widened endpoint arithmetic while
`getindex` / `collect` use TwicePrecision materialization.

Run the drift-prone fixture and the direct oracle unit checks:

```bash
julia --startup-file=no subset_julia_vm/tests/fixtures/range/float32_colon_length_9510.jl
bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/range/float32_colon_length_9510.jl
cargo test -p subset_julia_vm_bytecode colon_hp_length_float32_uses_float32_rational_len_issue_9510
cargo test -p subset_julia_vm_bytecode test_len_of_float32_step_range_uses_float32_semantics_issue_9510
```

When adding a new float element width or range constructor, extend the fixture
with both ascending and descending endpoint-drift grids, asserting `length(r)`,
`last(r)`, `collect(r)[end]`, and `typeof(collect(r))` together. These checks
catch the #9510 shape where `collect` has the right materializer but the public
range length stops one element early.

### Native range type/accessor changes (Issue #9815)

Any change to `RangeValue`, native range construction, `typeof`/`runtime_type`,
`call_site_arg_type_id`, or `first` / `last` / `step` / `length` / `collect` /
`iterate` / `eltype` / range `adjoint` / broadcast use for `UnitRange` /
`StepRange` / `StepRangeLen` must run:

```bash
julia --startup-file=no subset_julia_vm/tests/fixtures/range/native_range_identity_accessors_9815.jl
bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/range/native_range_identity_accessors_9815.jl
timeout 1800 cargo test -p subset_julia_vm call_site_fingerprint_range_uses_visible_type_params_issue_9815
timeout 1800 cargo test -p subset_julia_vm call_site_fingerprint_float_range_carries_float_type_params_issue_9815
```

This matrix keeps native range visible type parameters, call-site identities,
direct calls, function-value calls (including Issue #10018), `Any`-slot runtime
dispatch, native iteration / collection, and narrow integer collect `Vector{T}`
materialization synchronized (Issue #10024), while also catching native float
StepRangeLen dispatch into struct-field `adjoint` methods.

### Native range exact storage / internal counter changes (Issue #10112)

Any change that adds a range payload, changes `RangeElementType`, adjusts
`RangeValue` boxing, or changes public `first` / `last` / `step` / `length` /
`iterate` / `collect` / `getindex` / `in` / equality / display / splat behavior
must prove that public Julia-compatible values stay exact while VM-internal
loop/allocation counters stay Int-sized only at explicit private boundaries.

- [ ] For BigInt-backed `UnitRange` / `StepRange`, assert endpoints beyond
      Float64-exact precision and include scalar accessors plus materialization:
      `first`, `last`, `step`, `length`, `iterate`, `collect`, `getindex`,
      membership, equality/display if touched, and splat if touched.
- [ ] Keep public `length(::UnitRange{BigInt})` / `length(::StepRange{BigInt})`
      BigInt-compatible. Counted loops, comprehensions, and allocation helpers
      may use `Int(length(...))` only at the internal boundary, or must fall
      back to the generic `iterate` protocol.
- [ ] Box any large exact payload so `Value` remains compact; keep
      `test_value_enum_size_is_compact` in the narrow gate for payload changes.
- [ ] Run:

  ```bash
  julia --startup-file=no subset_julia_vm/tests/fixtures/range/bigint_endpoint_9420.jl
  bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/range/bigint_endpoint_9420.jl
  timeout 1800 cargo test -p subset_julia_vm derive_range_element_type_tests::bigint_stop_unit_range_is_bigint
  timeout 1800 cargo test -p subset_julia_vm derive_range_element_type_tests::bigint_step_range_promotes_all_three_operands
  timeout 1800 cargo test -p subset_julia_vm_bytecode test_value_enum_size_is_compact
  timeout 1800 cargo nextest run --release --test fixture_tests range::
  ```

### Optimized ForEach / counted-loop and runtime-specializer paths (Issue #10457)

Optimized loop and runtime-specialization paths must never reconstruct an
operation from erased types or a narrower value set than the generic bytecode
path handles (PR #10456 fixed three such erasures: #10423 / #9420 / #8869).
Any change to `compile_foreach` (`subset_julia_vm_compile/src/compile/stmt.rs`), the
runtime specializer's `ForEach` handling
(`subset_julia_vm_vm/src/vm/specialize/stmt.rs`), or the specializer entry guards
(`runtime_specialization_supported_for_function` in
`subset_julia_vm_vm/src/vm/exec/call.rs`) must check:

- [ ] **Public iterator `length` is not always an I64 loop bound.** The
      counted-loop rewrite (`length(itr)` + `itr[i]` with the length in an I64
      slot) is only valid when `length` is Int-sized. BigInt-backed ranges
      (`UnitRange{BigInt}` / `StepRange{BigInt}`) intentionally report BigInt
      lengths, so they must stay on the generic `iterate` protocol
      (`iterable_has_bigint_range_length` guard, Issue #9420), or convert with
      `Int(length(...))` only at an explicit VM-internal allocation boundary —
      never on the public value surface.
- [ ] **A `ValueType::Function` argument erases candidate metadata.** A callee
      body that materializes a resolved function value
      (`Instr::PushResolvedFunction`) must not be recompiled by the runtime
      specializer while an argument is only known as `Function`; fall back to
      generic bytecode (Issue #10423). Both entry points share the guard:
      `CallSpecialize` sites and `try_specialized_entry_for_runtime_call`.
- [ ] **Value-parameter kinds are a closed set — keep it complete.** The
      `LoadTypeBinding` raw-value fast path must cover every kind
      `bind_type_params` can store (I64, F64, Bool, Char, Symbol, Tuple); a
      missing kind silently degrades to a `DataType` wrapper (Issue #8869).
- [ ] Run:

  ```bash
  bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/dispatch/callspecialize_resolved_function_10457.jl
  bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/types/types_agg_value_params_10238.jl
  bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/range/bigint_endpoint_9420.jl
  timeout 1800 cargo nextest run --release --test fixture_tests range:: dispatch:: types::
  ```

### Narrow-float range visible-surface changes (Issue #10070)

Any change that adds or modifies `Float16` / `Float32` / `Float64`
`RangeElementType` handling, `StepRangeLen` parameters, float accumulator
selection, `float_hp`, range `eltype`, iterator element typing, `collect`, or
static range type inference must keep every visible surface synchronized. A
range can be numerically correct while its `typeof`, dispatch key, or collected
element type is wrong.

- [ ] Assert the whole visible surface together: `typeof(r)`, `eltype(r)`,
      `typeof(step(r))`, `typeof(first(r))`, `typeof(last(r))`,
      `typeof(r[i])`, `typeof(collect(r))`, `eltype(collect(r))`, and a
      `StepRangeLen{T,R,S,L}` dispatch key.
- [ ] Cover pure `Float16`, explicit-step `Float16`, pure `Float32`, and mixed
      `Float16`/`Float32` operands. Upstream uses Float64 accumulator fields for
      narrow floats, so do not infer `TwicePrecision{Float32}` merely because
      the element type is Float32.
- [ ] Keep runtime construction (`derive_range_element_type` /
      `derive_range_step_type`) and static inference/display paths in sync.
- [ ] Run:

  ```bash
  julia --startup-file=no subset_julia_vm/tests/fixtures/range/float16_float32_colon_type_params_10019.jl
  bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/range/float16_float32_colon_type_params_10019.jl
  timeout 1800 cargo test -p subset_julia_vm derive_range_element_type_tests::pure_float16_stays_float16
  timeout 1800 cargo test -p subset_julia_vm derive_range_element_type_tests::float16_plus_float32_promotes_to_float32
  timeout 1800 cargo test -p subset_julia_vm derive_range_element_type_tests::explicit_step_type_preserves_original_step_width_issue_9519
  timeout 1800 cargo nextest run --release --test fixture_tests range::
  ```

### Source-order / world-age method activation changes (Issues #9650/#9787/#9990/#9998)

Any change to source-order method activation — `FunctionInfo.min_world`,
`visible_from_source_start`, `repl_current_function_count`, or the
`is_current_input_top_level_user_function` / `is_source_ordered_inline_function`
/ `is_root_source_function` boundary logic in
`subset_julia_vm_compile/src/compile/pipeline_ctx.rs::build_method_tables` — must
validate all THREE compile paths TOGETHER, not just the one the change targets.
They have different definitions of "current source" and a fix for one path has
twice regressed another (#9990, #9787):

1. **Ordinary script redefinition** (Issue #9650): a top-level call through an
   earlier-defined function body executes in the call site's source world —
   `subset_julia_vm/tests/fixtures/dispatch/source_world_function_body_redefinition_9650.jl`.
2. **Already-visible overload inference** (Issue #9990): same-arity overloads
   that are ALL already visible before a function-body call must not force
   source-world `CallDynamic` / widen inference to `Any` —
   `subset_julia_vm/tests/fixtures/type_stability/diagnostic_parity_4291.jl`.
3. **REPL full accumulated prior-method visibility** (Issue #9787): a REPL
   full recompile whose `program.functions` is `[current-input functions ...,
   prior-eval functions merged AFTER them]` must keep every merged-after prior
   method immediately visible (`min_world == 1`), never delayed —
   `subset_julia_vm/tests/regression_scope_session_tests.rs::session_boundedness_8625_tests::repl_aliased_array_shared_rc_remaps_once_issue_9787`
   and `...::repl_persistent_struct_heap_stays_bounded_over_1000_iterations_issue_9787`.
   The compiler-boundary invariant this depends on — only the LEADING
   `repl_current_function_count` top-level user functions may receive delayed
   activation — is pinned directly at the `compile_core_program_internal`
   layer by
   `subset_julia_vm_compile/src/compile/pipeline_ctx.rs::repl_source_order_boundary_tests`
   (Issue #9998).

```bash
julia --startup-file=no subset_julia_vm/tests/fixtures/dispatch/source_world_function_body_redefinition_9650.jl
bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/dispatch/source_world_function_body_redefinition_9650.jl
julia --startup-file=no subset_julia_vm/tests/fixtures/type_stability/diagnostic_parity_4291.jl
bash scripts/fixture_julia_parity.sh subset_julia_vm/tests/fixtures/type_stability/diagnostic_parity_4291.jl
timeout 1800 cargo test -p subset_julia_vm --lib repl_source_order_boundary_tests
timeout 1800 cargo nextest run --release --test regression_scope_session_tests -E 'test(issue_9787)'
```

Then keep full `timeout 1800 cargo nextest run --release` mandatory before
merging any source-order/world-age change — the REPL and package fixture
interactions in this area are order/path dependent and have escaped narrower
gates twice already (#9990, #9787).

### Callable-value / source-world dispatch fixes (Issues #9979/#9980/#9992)

Do **not** "fix" a callable-value higher-order-function (HOF) ordering
symptom (a `Function` value selecting the wrong overload, e.g. `f = Base.map`
or a kwargs-splat `plot(cos, xs)` call) by widening or reordering the VM-side
**runtime candidate sort**. That perturbs every call site sharing the same
runtime candidate vector — including unrelated package/metaprogramming paths
(MacroTools-style) — because the sort is shared infrastructure, not specific
to the symptom's call site (#9979's actual root cause).

Instead:

1. Add/extend a **shared-resolver** unit test in
   `subset_julia_vm_types/src/inference_core/dispatch_resolver.rs`
   (`resolve_callable_value_candidates`, see the
   `callable_value_candidates_prefer_*_issue_9979` tests) that pins the
   EXACT specificity ordering your fix depends on, BEFORE touching VM
   runtime-candidate code.
2. Prefer routing the call site through the value-based dispatcher
   (`find_best_method_index_from_candidates`) — see the "New dynamic callable
   opcodes" rule below — over adding VM-side sorting.
3. If the fix is package/module-context-sensitive, add or extend a compact
   fixture that combines a Function-singleton value + module/package context +
   a broad vararg fallback in one call site, e.g.
   `subset_julia_vm/tests/fixtures/dispatch/callable_value_module_context_vararg_fallback_9992.jl` —
   check first whether an existing fixture already covers the combination
   before adding a new one.

**REPL delta changes**: a live-append COMPILE REJECTION must force a
conservative FULL refresh before any fresh-delta compile path resumes (Issue
#9980). A delta path that continues after a live append was rejected can emit
call sites without candidate payloads for methods that exist only in the live
VM / a refreshed full program — do not let the delta path "paper over" a
rejected append by silently falling through to a stale prefix.

### New dynamic callable opcodes (Issue #9987)

Any new VM opcode that calls a runtime `Function`/`Closure`/callable value
(the `Instr::CallFunctionVariable*` family and any future sibling) must
route through the shared **value-based** resolver
(`dispatch_function_variable_for_values`). Direct dynamic-call opcodes must
retain the compiler-resolved callee identity in their bytecode payload, build a
`CallRequest`, and use `resolve_runtime_call_request`. Do not call
`find_best_method_index_from_candidates` or `dispatch_function_variable`
directly in an opcode arm. Function values carry only coarse call-site
type names (`typeof(name)`), so the string scorer alone lets several
`(::Any, ::Any)`-shaped candidates tie and can select a lazy iterator shim or
a generic fallback instead of the Julia-specific method (Issues #9974,
#9981).

This is enforced by a structural audit —
`scripts/check_call_function_variable_value_dispatch_order.sh` — that fails
if any `Instr::CallFunctionVariable*` execution path calls either private
scorer directly or skips the shared resolver. It also pins request-before-
legacy ordering, requires `Instr::CallDynamic` to consume its carried identity,
and rejects direct compiler expression emission that bypasses
`emit_dynamic_call`. Run it after touching those files, and extend its target-
arm detection when adding a new callable opcode:

`invoke` is the deliberate semantic-mode exception: its declared tuple is
complete dispatch input, so literal `Any` must remain `Any` instead of being
refined from the runtime value (Issues #11609/#11619). Keep all four
`Instr::InvokeFunctionVariable*` forms (static/dynamic signature crossed with
positional/keyword arguments) routed through
`invoke_runtime_callable_value_with_signature*` and
`dispatch_function_variable_for_declared_signature`; never route them through
`dispatch_function_variable_for_values` or a runtime `CallRequest` until that
request explicitly represents authoritative declared-signature mode. Extend
`invoke_explicit_signature_5123.jl` across direct/stored callables and both
`Any` and an abstract non-`Any` declaration whenever this path changes.

```bash
bash scripts/check_call_function_variable_value_dispatch_order.sh
```

Also extend the resolver regression matrix for runtime `Type{Parametric{T...}}`
patterns in `subset_julia_vm_types/src/inference_core/dispatch_resolver.rs`
(`runtime_type_object_pattern_requires_bound_params_issue_9981`,
`runtime_type_object_pattern_rejects_non_matching_concrete_type_issue_9987`)
when adding a new type-object binding path: `Union{}` must be rejected unless
type-parameter bindings are actually extracted, a concrete matching struct
must be accepted with its bindings, and a non-matching concrete type object
(wrong family or wrong arity) must be rejected.

### Parametric DataType callable constructor dispatch (Issues #10405/#10502)

First-class parametric constructor values (`ctor = Vector{Float64};
map(ctor, xs)`) dispatch through runtime callable `Value::DataType` handling
in `subset_julia_vm_vm/src/vm/exec/call_function_variable.rs`
(`collect_parametric_datatype_callable_candidates_into`). When touching that
candidate collection/augmentation, the callable type-arg binding, or the
constructor fallback ordering in `compile/expr/call/constructors.rs`:

- [ ] **Runtime `DataType` candidate augmentation must only add generic
      TypeVar methods, never concrete instantiation siblings.** A candidate
      `Name{...}` method row qualifies only if its type arguments reference a
      `TypeVar` (`type_expr_references_typevar`) — `Rational{Bool}(...)` is
      NOT a generic fallback for a `Rational{BigInt}` callable, and a method
      defined only on `Box{Int64}` must stay invisible to a `Box{Float64}`
      callable (MethodError, not sibling reuse).
- [ ] **Generic candidates must not suppress the default `DataType`
      constructor fallback.** After a dispatch miss among the augmented
      candidates, the default constructor path must still fire — the
      `Complex{Int64}(re, im)` class must keep constructing even though
      generic `Complex{T}` conversion methods exist.
- [ ] Pair every direct parametric constructor form with its first-class
      callable forms (bound variable + `map`/`broadcast`) in the regression
      fixtures:
      `subset_julia_vm/tests/fixtures/array/ctor_direct_vs_callable_parity_10213.jl`
      (Vector/Array/Matrix) and
      `subset_julia_vm/tests/fixtures/dispatch/parametric_ctor_callable_parity_10502.jl`
      (Dict/Rational/Complex plus user-struct negative guards). Keep both in
      the PR test plan.
- [ ] Run `bash scripts/check_call_function_variable_value_dispatch_order.sh`
      after touching `call_function_variable.rs` (see the rule above).

### REPL LV5 module collector coverage changes (Issues #9729/#9989/#9996)

Any change to the LV5 module-body collectors or their gate —
`collect_module_body_binding_names` (RESOLUTION, `compile/collect.rs`),
`collect_assign_vars_in_stmts` / `restore_assign_vars_in_stmts` (STATE MIRROR,
`repl/session.rs`), or `module_bindings_fully_mirrorable` — must land together
with an explicit expectation update:

1. **Update the coverage table** `repl::session::lv5_mirror_coverage_tests_9996`
   (per-shape classification: `Assign`, `Block`, module-top-level `If`,
   `AssignExpr`, empty `LetBlock`, local-scope control row) and its
   difference-set expectation
   (`resolution_minus_mirror_difference_set_is_exact_9996`). These fail on any
   coverage change that is not accompanied by an expectation edit.
2. **Update the ADR wording** — `docs/vm/ADR_REPL_EVAL_MODEL.md` LV5
   "Eligibility gate" and LV5b "Modules with a binding the state-mirror can't
   track" — in the same change.
3. **Assert the durable contract, not the path, in new tests** (ADR §"Testing
   guidance"): Legacy ≡ Persistent value equality, state preservation across a
   live→full fallback, and `mirror ⊇ resolution`. Assert live-vs-full path
   policy (`last_vm_build_nanos() == Some(0)` or not) ONLY when the coverage
   table's classification requires it — a stale path-policy assertion was the
   #9989 failure mode after #9729 intentionally widened the mirror.

```bash
timeout 1800 cargo nextest run --cargo-profile release-fast -p subset_julia_vm --lib lv5_mirror_coverage
timeout 1800 cargo nextest run --cargo-profile release-fast -p subset_julia_vm --lib repl::
```

## Optimizer Passes Rewriting `Expr::Call` By Name — Lexical Module Scope Checklist (Issue #10840, prevention for #10771)

`subset_julia_vm_compile/src/compile/ir_inline.rs` (the small pure-function IR
inliner) substitutes a call by looking up a NAME-KEYED candidate table
(`HashMap<String, InlineCandidate>` built by `collect_inline_candidates`, keyed
`"M.SubM.f"` for module functions and `"f"` for top-level ones). Before PR
#10837 the `Expr::Call` (bare/unqualified call) arm looked up that table by
the *unqualified* name only, so a bare call from inside a module method could
be substituted with a same-named TOP-LEVEL function's body before bytecode
dispatch ever got a chance to resolve the module-local method (Issue #10771).
The fix made `inline_unqualified_call` the one policy point: it tries the
current lexical module path (`module_stack`) qualified key FIRST, falling back
to the bare key only when no module-qualified candidate exists.

This shape — an optimizer/lowering pass that recognizes a call by consulting a
name-keyed global table instead of the lexical scope the call actually
appears in — generalizes beyond this one pass. When adding or changing ANY
pass that rewrites `Expr::Call` (or an equivalent bare-call IR node) by
consulting name-keyed candidates:

1. [ ] The rewrite must either (a) resolve the callee through the CURRENT
       lexical module path before falling back to an unqualified/global key
       (precedent: `Inliner::inline_unqualified_call` +
       `Inliner::lexically_visible_module_call_key`, both in
       `ir_inline.rs`), or (b) prove the call was already fully qualified
       (e.g. `Expr::ModuleCall`, whose `module` field names the target
       lexically) before it reaches the low-level lookup helper
       (`Inliner::inline_call`). `inline_call` itself must stay a low-level,
       scope-blind lookup — scope choice belongs in the caller, never inside
       it.
2. [ ] Do not special-case a single nesting depth. Module scope nests
       arbitrarily (`A.B.C...`); a fix that only strips one dotted segment or
       only checks `module_stack.last()` once will silently regress on a
       deeper nest. `Inliner::inline_module` already threads the full dotted
       `module_path` through `module_stack` for this reason — reuse it rather
       than re-deriving a shallower approximation.
3. [ ] Add a same-name-at-every-level regression case: a same-named candidate
       at top level, at the parent module, AND at the nested module the call
       site lives in, then assert the INNERMOST one wins. A fixture with only
       two candidates (module vs. top-level, Issue #10771's original
       regression test) can still pass by accident if a fix collapses `A.B`
       and `A` into the same key. Precedent:
       `ir_inline::tests::inline_small_pure_nested_module_bare_call_prefers_innermost_function_issue_10840`
       (Rust-level inliner coverage) and
       `subset_julia_vm/tests/fixtures/modules/nested_module_bare_call_prefers_innermost_function_10840.jl`
       (end-to-end fixture, verified against upstream `julia` first).
4. [ ] The Rust-level coverage test must fail if the `Expr::Call` arm reverts
       to calling the low-level lookup helper directly on the unqualified
       name (i.e. it must exercise the actual name-collision-across-scopes
       shape, not just "does inlining happen at all" — a same-named top-level
       candidate that the unqualified key would incorrectly match is
       required, not optional).

```bash
cargo test -p subset_julia_vm_compile --lib ir_inline
timeout 1800 cargo nextest run --cargo-profile release-fast --test fixture_tests modules::
```

## Worktree Setup Checklist (Issue #10946)

Fresh `git worktree`s do not initialize submodules, so the `julia/` upstream
corpus is absent and corpus-dependent tests (`parser_corpus_base_ratchet`,
`base_exports_do_not_exceed_upstream`) would skip. `premerge_gate.sh` exports
`SJULIA_REQUIRE_CORPUS=1`, which turns those skips into FAILURES inside a gate
run — a certification must either compare the corpus or refuse.

Before running gates from a new worktree, provide the corpus by either:

```bash
# Option A: initialize the submodule in the worktree
git submodule update --init julia

# Option B: symlink the main checkout's corpus (fast; worktrees only)
ln -s <main-checkout>/julia/base   julia/base
ln -s <main-checkout>/julia/stdlib julia/stdlib
ln -s <main-checkout>/julia/test   julia/test
```

(Contents of an uninitialized submodule directory do not show in
`git status`, so the symlinks do not dirty the gate's clean-tree check.)
