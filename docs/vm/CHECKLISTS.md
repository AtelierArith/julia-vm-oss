# Implementation Checklists

Detailed checklists for adding new types, variants, and features to SubsetJuliaVM. Referenced from `CLAUDE.md`.

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
sjulia writes `target/sjulia_base_cache_v2_<prelude-hash>.bin`; concurrent nextest
fixture processes wait on a lock file and then read the same serialized cache.
Set `SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1` to debug the uncached path;
only the exact value `1` disables the persistent cache.
Serialized Base cache reconstructs runtime specialization context at load time;
if changing Lazy AoT context shape, bump the Base cache format version and
verify a warm-cache fixture that needs parametric specialization.

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
(Issue #6026).

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

## VM Instruction Routing Changes (Issue #3275)

When modifying `emit_return_for_type()`, `emit_store_for_type()`, or `emit_load_for_type()` in `compile/stmt.rs`:

1. [ ] Add fixture tests in the **same PR** covering the changed types (e.g., `typeof(f()) == TargetType`)
2. [ ] If adding a new `ValueType` variant, include tests for return, store, and load paths
3. [ ] Run `timeout 1800 cargo nextest run --release --test fixture_tests` to verify

## Adding AoT Builtin Ops (Issue #3279)

When adding a new `BuiltinOp` variant to the VM IR:

1. [ ] Add a dedicated `AotBuiltinOp` variant in `aot/ir/` — do NOT reuse an existing variant as a proxy
2. [ ] Update `builtin_op_to_aot()` in `aot/analyze/ir_converter/helpers.rs`
3. [ ] Add `return_type()`, `Display`, `from_name()`, and codegen entries for the new `AotBuiltinOp`
4. [ ] If a dedicated variant is not feasible, add `// Workaround: ... (Issue #NNNN)` and create a tracking Issue

## Adding a New Literal/Value Type (Issue #3320, #3304)

When adding a new numeric or value type that should be injectable into REPL, update ALL 12 files in the Literal pipeline:
1. `ir/core.rs` — Add `Literal::NewType` variant
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

When adding or changing an array-like wrapper constructor (`view`, `reshape`, future wrappers) (Issue #8246):
- Keep call-site inference and runtime equality normalization in sync: concrete inputs should infer to an `AbstractArray` subtype instead of widening to `Any`, and equality must normalize the runtime wrapper against itself and native arrays.
- Add or update `subarray_array_like_wrapper_contract_8246` for runtime equality coverage, including the #8240 `view == view` shape and at least one non-`SubArray` wrapper.
- Add or update compile-time unit coverage for constructor inference, such as `array_like_view_constructor_contract_infers_concrete_subarray_8246`, when the constructor has a dedicated inference path.

When broadening method-table `Any` return re-inference (Issue #8246):
- Do not add an unbounded call-site body re-inference fallback. Add recursion/work-budget protection and a focused regression for the motivating call.
- Run `test_repl_value_display_uses_user_show_7168`; REPL result echo must keep user `show` behavior and must not stack-overflow through recursive show paths.

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
- [ ] **Pure Julia method-table**: When adding a Pure Julia method like
      `div(x::Int64, y::Int64)`, did we add the matching specialization for
      **every** signed and unsigned bit width, or just the ones we tested?
      Run `bash scripts/check_div_specializations.sh`.
- [ ] **Runtime fallback (intrinsics_exec.rs)**: When extending an intrinsic
      exec, does it preserve the operand's wide type (`I128` / `U128` /
      `F16`) or does it `pop_i64`-truncate?
- [ ] **Runtime fallback (binary_both.rs / dynamic_ops/)**: When extending
      `CallDynamicBinaryBoth`, does the small-int prologue (~315) accept
      the new type without `try_from` to I64? Above-i64::MAX comparisons
      used to raise OverflowError (Issue #3696). For `dynamic_div` /
      `dynamic_mod` / etc. — does every F16/I128/U128 arm exist?

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
