# Type Inference

*Last updated: 2026-06-11*

This is the current entry point for SubsetJuliaVM type inference. The older
all-in-one history/planning document was preserved as
`docs/vm/archived/TYPE_INFERENCE_COMPLETE_20260116.md`.

The compiler uses type inference in two related places:

- **Function return inference**: lattice-based abstract interpretation over
  lowered Core IR functions.
- **Expression inference during bytecode emission**: local `ValueType` /
  `JuliaType` inference used for dispatch selection, typed bytecode, array
  element typing, and HOF call-site specialization.

## Current Source Map

| Area | Current path | Notes |
|------|--------------|-------|
| Abstract interpretation engine | `subset_julia_vm/src/compile/abstract_interp/engine/` | `InferenceEngine`, return caches, recursion handling, method/global invalidation state |
| Conditional narrowing | `subset_julia_vm/src/compile/abstract_interp/conditional.rs` | Environment splitting for supported predicates |
| Loop element analysis | `subset_julia_vm/src/compile/abstract_interp/loop_analysis.rs` | `element_type(...)` for arrays, tuples, ranges, dicts, sets, strings, generators, pairs, and related wrappers |
| Type environments | `subset_julia_vm/src/compile/abstract_interp/env.rs` | `TypeEnv` and environment merge/refinement helpers |
| Lattice types | `subset_julia_vm/src/compile/lattice/` | `LatticeType`, `ConcreteType`, join/meet/subtract/widening |
| Value/lattice bridge | `subset_julia_vm/src/compile/bridge.rs` | `ValueType` <-> `LatticeType` conversion helpers |
| Transfer functions | `subset_julia_vm/src/compile/tfuncs/` | Metadata-bearing type-level rules registered by `register_all(...)` |
| Pipeline adapters | `subset_julia_vm/src/compile/inference.rs`, `subset_julia_vm/src/compile/core_compiler.rs` | Shared-engine construction and call-site return inference adapters |
| Expression inference | `subset_julia_vm/src/compile/expr/infer/` | Value and Julia type inference during bytecode emission |
| Inference trace | `subset_julia_vm/src/compile/inference_trace.rs` | Developer trace reports for a single function |
| Shared type core | `subset_julia_vm/src/inference_core/` | Common type operations used by compiler, VM-facing dispatch, and AoT projection |

Do not use old path names such as `compile/abstract_interp/engine.rs` or
`compile/expr/infer.rs` in new documentation; both are now directories.

## Main Flow

The production compiler builds a shared abstract interpretation engine for the
program instead of using the retired legacy forward-analysis return inference.
The helper in `compile/inference.rs` converts compile-time struct/global tables
into lattice state and returns an `InferenceEngine` populated with all program
functions.

At call sites where argument `ValueType`s are known, expression compilation uses
`CoreCompiler::infer_shared_function_return_type_with_arg_types(...)`, which
converts those argument types into lattice types and calls
`InferenceEngine::infer_function_with_arg_types(...)`. Older references to
`infer_function_return_type_v2(...)` and
`infer_function_return_type_v2_with_arg_types(...)` are historical.

Expression compilation uses `compile/expr/infer/` for local decisions:

- `mod.rs`: top-level expression `ValueType` inference.
- `julia_type.rs`: Julia-level type inference for method dispatch.
- `array.rs`: array literal and nested element inference.
- `hof.rs`: call-site specialization for `map`, `filter`, `reduce`,
  `mapreduce`, `broadcast`, and related higher-order calls.
- `expr_tfuncs.rs`: adapter from expression inference to the transfer-function
  registry.

## Lattice Model

`LatticeType` currently models:

- `Bottom`: unreachable code or an empty type.
- `Const`: statically-known literal values.
- `Concrete`: one known `ConcreteType`.
- `Union`: a finite set of concrete alternatives.
- `Conditional`: retained as a lattice variant, but production branch narrowing
  is implemented through environment splitting.
- `Top`: unknown / `Any`.

The lattice implementation lives under `compile/lattice/`. Conversion to VM
bytecode-facing types goes through `compile/bridge.rs`; avoid manually mapping
between `LatticeType` and `ValueType` in new code unless there is no existing
bridge helper.

## Conditional Narrowing

Branch-sensitive inference is implemented by splitting the type environment for
the then/else paths. The current supported narrowing surface includes:

- `isa(x, T)` for simple variable targets.
- `x === nothing` and `x !== nothing`.
- Compatibility handling for `x == nothing` and `x != nothing`.
- Composite conditions through `&&`, `||`, and `!`.
- Predicate wrappers when the callee can be inlined by the conditional helper.

The implementation is deliberately conservative. It can fail open to `Top`
instead of inventing a precise complement type, especially when subtracting from
an already broad type.

## Loop Element Inference

Loop variables are inferred from the iterable lattice type through
`compile/abstract_interp/loop_analysis.rs`. Common cases include:

| Iterable lattice shape | Loop variable type |
|------------------------|--------------------|
| `Array{T}` / `Memory{T}` / typed vector wrappers | `T` |
| `Tuple{T1,T2,...}` | single element type or a union of element types |
| `Range{T}` | `T` |
| `Dict{K,V}` / `Pairs` | pair or tuple-like key/value shape |
| `Set{T}` | `T` |
| `String` | `Char` |
| Unknown or unsupported iterator | `Top` |

If a new iterable type is added, update this helper first, then verify the
fixture category that exercises that iterable. Do not add one-off variable-name
checks to expression inference.

## Transfer Functions

Transfer functions encode type-level semantics for known calls. They are
registered through `compile/tfuncs::register_all(...)` and split by domain:

- `arithmetic.rs`: numeric, comparison, boolean, and bit operations.
- `array_ops.rs`: array constructors and common array operations.
- `string_ops.rs`: string operations.
- `intrinsics.rs`: conversions and intrinsic-like calls.
- `field_ops.rs`: field access and field metadata.
- `iterator_ops.rs`: `iterate`, `length`, `eachindex`, and iterator helpers.
- `collection_ops.rs`: `keys`, `values`, `pairs`, and collection helpers.
- `math_intrinsics.rs`: math functions.
- `complex_ops.rs`: complex-number helpers.

Rules carry arity/cost metadata. When adding a transfer rule, preserve `Bottom`
propagation for unreachable operands and add a focused unit test in the same
module or a fixture when user-visible dispatch changes.

## Caches And Invalidation

`InferenceEngine` caches return facts by function/method identity and argument
lattice types. Current cache state also records:

- method-table dependencies and method-world validity;
- global binding reads and binding-world validity;
- recursion/tentative results for active inference cycles;
- limited-accuracy facts when recursion or depth caps prevent full precision;
- statement and CFG environment side tables used by diagnostics and tracing.

This is a conservative model inspired by Julia's world-age and backedge
invalidation, but it is not a full `MethodInstance` / `CodeInstance` model.

## Diagnostics And Tracing

`compile/diagnostics.rs` provides opt-in diagnostics through
`DiagnosticsCollector`. It can report broadening to `Any`, unknown calls,
unknown fields/elements, recursion/depth limits, and related loss-of-precision
events.

`compile/inference_trace.rs` provides `infer_with_trace(...)` for a single
function. The trace records bound argument types, branch environments,
statement-level environments, recursion events, diagnostics, and the final
return lattice type. It restores diagnostic collector state after the run.

## Function-Body Slot-Typing Pre-Scan (Issue #6601)

Before emitting bytecode for a function body / inner constructor / `main`, the
compiler runs a statement-level **pre-scan**
(`compile/inference.rs::collect_local_types_with_mixed_tracking`) that pre-populates
two pieces of state read during codegen:

1. `compiler.locals` — the **whole-body widened slot type** for every assigned
   local. Codegen reads `self.locals.get(var)` as `target_ty` *before* compiling
   each first `Store` (`compile/stmt.rs`, the `Assign` arm). This is what makes a
   forward reference type correctly: the slot decision at the first store already
   reflects later assignments.
2. `compiler.mixed_type_vars` — the set of locals that must use dynamic
   (`StoreAny`/`LoadAny`) slots.

This is the SOLE remaining pre-scan consumer after Issues #6602 (For/ForEach
loop-var typing → engine injection) and #6603 (globals → engine injection). The
shared engine's forward refinement refines *expression results at call sites*, not
the pre-store slot decision, so this consumer cannot be deleted outright; retiring
it needs a lazy-slot-allocation or 2-pass design (below).

### Load-bearing behavior to preserve

Pinned by the `prescan_*_issue_6601` characterization tests in
`compile/inference.rs`:

| Sequence | Slot type | `mixed_type_vars`? | Why |
|----------|-----------|--------------------|-----|
| `s = 0; s = s + 1.5` | `Any` | no | `I64 ⊔ F64` widens to `Any` (`widen_type`); compound RHS is not a *direct literal*, so not flagged mixed. Note: the slot is `Any` (dynamic), **not** a narrow `F64` — a common misconception. |
| `s = 0.0; s = s + 1.5` | `F64` | no | stable numeric slot, no I64 in the mix |
| `die = 7.0; die = 6` | `Any` | **yes** | direct F64/I64 literal reassignment → dynamic slot |
| `v = 1; v = "s"` | `Any` | **yes** | incompatible non-numeric reassignment (#4285/#3535) |
| `acc = 0; acc = acc / 2` | `Any` | no | compound numeric I64→F64 widening, not the direct-literal rule |

The `mixed_type_vars` semantics live in `collect_local_types_with_mixed_tracking_impl`
(the direct-literal F64/I64 rule and the incompatible-non-numeric rule) and in
`widen_type` (the join itself). `mixed_type_vars` is consumed by `compile/stmt.rs`
to pick `Any` (dynamic) slots.

### Migration seam and hazards

The proven-equivalent migration pattern (Issues #5922 / #6519 / #6602) routes one
RHS class at a time off the legacy `infer_value_type(_with_structs)` onto the
shared authority, with a pin test asserting the resulting `ValueType` is identical.
Literal RHSs are already migrated through
`compile/abstract_interp/local_authority.rs`. The engine is already threaded into
`collect_local_types_with_mixed_tracking_impl` (lazily, for the For/ForEach
consumer), so an RHS migration can reuse `engine.infer_expr_result(...)`.

Hazards that make the remaining non-literal RHS classes **not** drop-in
engine-equivalent (verified while scoping #6601):

- **`Expr::FunctionRef`**: legacy → `ValueType::Function`; the engine produces
  `ConcreteType::Function`, which `bridge::lattice_to_value_type` maps to
  `ValueType::Any`. A direct swap would change the slot type.
- **`Expr::Var` of an unresolved name**: the pre-scan's loop engine is built with
  no function table (`build_shared_inference_engine_empty`), so `resolve_callable_name`
  returns `None` and the engine agrees with legacy (`Any`) for plain unknowns —
  but legacy additionally returns `F64` for a bare `pi`/`π` via `is_pi_name` when
  it is not in `global_types`; the engine returns `Any`.
- **`Expr::BinaryOp` / `Expr::Call` / `Expr::Index` / `Expr::FieldAccess`**: the
  legacy `infer_value_type_with_structs` carries struct-aware Complex promotion,
  a struct-preserving-function list, and array/field element typing that the
  engine reaches through a different (tfunc-registry) path; equivalence must be
  proven class-by-class and validated by the full suite, not assumed.

### Retirement design (the 2-pass plan)

Two viable shapes, both behavior-preserving:

1. **2-pass slot typing.** Keep the pre-scan as the *first* pass that computes the
   widened `locals` + `mixed_type_vars`, but reduce its body to the shared engine
   incrementally (one RHS class at a time, each gated by a `prescan_*` pin test +
   full suite) until `infer_value_type(_with_structs)` has no remaining arms for
   this consumer. The pre-scan then becomes a thin engine driver, and the two
   inference paths fully collapse.
2. **Lazy slot allocation.** Defer the slot-type decision until the first `Store`
   is actually emitted, refining it from the engine's whole-body fixpoint
   (`InferenceEngine::infer_function` records per-statement / per-block
   environments via `statement_type` / `cfg_block_output`). This removes the
   separate pre-scan pass entirely, but requires mapping the engine's join lattice
   onto the exact `mixed_type_vars` contract above — the harder part.

Either way, the `mixed_type_vars` mapping is the crux: the engine's `join` does not
distinguish "direct literal F64/I64" from "compound numeric widening", which the
current rules do. That distinction must be reconstructed (e.g. by inspecting the
assignment RHS shape during the driving pass) before the legacy path can be removed.

## Maintenance Rules

- Keep active docs tied to stable paths and exported helper names, not line
  numbers.
- Preserve historical planning material under `docs/vm/archived/` instead of
  mixing old plans into this current guide.
- Prefer shared helpers: `compile/bridge.rs`, `compile/tfuncs/`, and
  `inference_core/` should absorb common type logic before adding local special
  cases.
- For new struct/type constants, use the global type registry. Do not add
  hardcoded variable-name checks under `compile/expr/infer/`.
- For HOF result typing, update `compile/expr/infer/hof.rs` and the relevant
  Pure Julia method tests together.

## Validation

Useful focused checks for inference work:

```bash
timeout 1800 cargo nextest run --release --lib compile::lattice
timeout 1800 cargo nextest run --release --lib compile::abstract_interp
timeout 1800 cargo nextest run --release --lib compile::tfuncs
timeout 1800 cargo nextest run --release --test fixture_tests type_inference::
```

For doc-only edits, at minimum verify referenced source paths exist and check the
Markdown diff for stale line-numbered references.

## Related Documents

- `docs/vm/TYPE_SYSTEM.md`
- `docs/vm/LATTICE_TYPE.md`
- `docs/vm/HOF_GUIDE.md`
- `docs/vm/CACHE_ARCHITECTURE.md`
- `docs/vm/TESTING_GUIDE.md`
- `docs/vm/archived/TYPE_INFERENCE_COMPLETE_20260116.md`
