# Binary Operator Dispatch

*Last updated: 2026-06-28*

This document covers the binary operator compiler dispatch paths, result type coverage, and related checklists.

## Two Binary-Op *Codegen* Paths — Keep Them In Sync (Issue #8192)

> **Footgun.** Binary-op *bytecode generation* (the choice of typed instruction
> and how operands are promoted) happens in **two independent places**. A change
> to numeric instruction selection or promotion in one path is **silently
> ignored** by functions that go through the other — tests can stay green while
> the optimization quietly does not apply.

| Path | Entry point | When it runs |
|------|-------------|--------------|
| **Main compiler** | `compile/expr/binary/` — `compile_builtin_binary_op` (`builtin.rs`), via `typed_instr_for_intrinsic` | Static, ahead-of-time bytecode generation (the normal path). |
| **Runtime arg-type specializer** | `vm/specialize/expr.rs` — `FunctionSpecializer::{compile_binary_op, emit_binary_op}` (Issue #8167) | At runtime, when an *untyped* function is recompiled for the concrete argument types observed at a call site. |

### Single source of truth for the typed instruction (Issue #8192)

Both paths now resolve the typed `Int64`/`Float64` instruction through one shared
table, `compile::typed_scalar_binary_instr(op, result_is_float) -> Option<Instr>`
(defined in `compile/expr/binary/mod.rs`). `typed_instr_for_intrinsic` (main
path) is a thin adapter over it, and the specializer's `emit_binary_op` calls it
directly. **Add or change a typed scalar binary instruction in exactly one
place — that helper.** Ops with no single typed I64/F64 instruction (`÷`, `^`,
float `%`, `&&` / `||`) return `None` and keep each path's bespoke dynamic /
short-circuit lowering.

### The typed-loop recognizer whitelist coupling (Issue #8183)

The native typed-loop fast path (`vm/executable.rs`,
`try_predecode_typed_loop_range`) only recognizes loop bodies built from a fixed
**whitelist** of instructions. Two consequences:

1. **Every instruction the shared table emits must be on that whitelist**,
   otherwise a specialized hot loop using it silently falls back to
   per-instruction interpretation (≈100× slower) even though it compiles and runs
   correctly.
2. **The specializer must promote operands by coercing each one *as it is
   compiled* (`…ToF64; <op>F64`), never with an on-stack `Swap; ToF64; Swap`
   after both operands are pushed.** `Swap` is *not* on the whitelist (it has no
   meaning on the predecoder's split typed stacks), so a stray `Swap` aborts
   recognition. This is the exact regression #8183 / PR #8189 hit: Stage 1 fixed
   the main compiler, but the same loop, once runtime-specialized, regressed
   until Stage 3 fixed the specializer too. `compile_binary_op`'s fast path
   coerces mixed `Int/Float` `+ - * /` and any `Int/Int` `/` without a `Swap`;
   `emit_binary_op`'s remaining on-stack `Swap` is reached only by the rarer
   non-hot fallbacks (e.g. the n-ary `/(a, b, c)` fold).

### Rule when touching numeric binary codegen

1. Change typed instruction selection **only** in `typed_scalar_binary_instr`.
2. If you add a new typed binary instruction, also teach the typed-loop
   recognizer in `vm/executable.rs` (and the oracle in its guard test).
3. Keep the specializer's operand promotion `Swap`-free on the hot path.

### Guard tests

- `vm::executable::tests::untyped_scalar_hot_loops_specialize_to_swap_free_typed_loops_issue_8192`
  — end-to-end: untyped `+ - * /` (mixed Int/Float, Int/Int division, pure
  Float64) hot loops must runtime-specialize to a recognized, `Swap`-free typed
  loop.
- `vm::executable::tests::shared_binary_table_only_emits_typed_loop_recognized_instrs_issue_8192`
  — unit tripwire: every instruction `typed_scalar_binary_instr` can emit is
  accepted by the typed-loop recognizer.
- `compile::expr::binary::tests::{typed_scalar_binary_instr_table_matches_per_op_expectations_8192, typed_instr_for_intrinsic_delegates_to_shared_table_8192}`
  — pin the shared table and the main-path adapter.

## Three Dispatch Code Paths (Issue #1783, #1785)

The binary operator compiler in `subset_julia_vm/src/compile/expr/binary/` (`mod.rs`, `builtin.rs`, `user_defined.rs`) uses multiple dispatch code paths depending on method table state.

### 1. `all_base_extensions()=true` Path

When ALL methods in the operator's table are Base extensions (defined with `Base.:` prefix or loaded from prelude). This path does NOT shadow builtins for primitive types -- it only dispatches to Base extension methods when at least one operand is a struct type. For primitive-only operations, it falls through to builtin intrinsics.

### 2. `all_base_extensions()=false` Path

When ANY method is NOT a Base extension (e.g., Bool arithmetic operators defined without `Base.:` prefix). This path attempts `table.dispatch()` for all types, then applies `skip_static_dispatch` logic and `needs_runtime_dispatch` checks.

### 3. Builtin Fallback

When no user-defined method matches, the compiler emits builtin intrinsics directly.

## Key Concepts

### `ValueType::Any` for Parametric Type Variables

When compiling `+(::Int64, ::Rational{T}) where T`, parameter `x::Rational{T}` is stored as `ValueType::Any` because `T` is unknown at compile time (`julia_type_to_value_type_with_ctx` returns `Any` for types with type variables).

### `all_base_extensions()` State

Bool arithmetic methods (`+(::Bool, ::Bool)`, etc.) use `Base.:` prefix in `bool.jl`, so `all_base_extensions()` returns true for the `+` method table. This was previously broken (Issue #1975) but is now fixed -- the `all_base_extensions=true` path correctly handles all non-struct type combinations including F16+F32 promotion.

### Static Dispatch Specificity Mismatch with `Any`

When one operand is `Any`, `dispatch([Any, Int64])` may incorrectly match `+(::Float32, ::Int64)` (specificity 5+5=10) over `+(::Rational{T}, ::Int64)` (specificity 4+5=9), because concrete types have higher specificity than parametric struct types.

## The `skip_static_dispatch` Guard (PR #1784)

To prevent incorrect static dispatch when `Any` + struct methods exist:

```rust
let any_arg = arg_types.iter().any(|t| matches!(t, JuliaType::Any));
let has_struct_methods = any_arg && table.methods.iter().any(|m| {
    m.any_param_matches(|core| matches!(core, CoreType::Struct { .. }))
});
let skip_static_dispatch = any_arg && has_struct_methods;
```

When `skip_static_dispatch` is true, runtime dispatch (`CallDynamicBinaryBoth`) is used instead.

## Bool Operators and `Base.:` Prefix (Issue #1975)

Bool arithmetic operators in `bool.jl` now correctly use `Base.:` prefix (e.g., `Base.:(+)(x::Bool, y::Bool)`), making `all_base_extensions()=true` for `+`, `-`. The fix added F16 to all numeric type conversion paths (F16->I64, F16->F64, F16->F32, F16->Any) and F16 result back-conversion.

## Primitive Numeric Skip Guard (Issue #2203, #2225)

Both dispatch paths (all_base_extensions and non-base-extensions) include a guard that skips method dispatch when **both** operands are builtin primitive numeric types. This guard uses `JuliaType::is_builtin_numeric()` (defined in `types.rs`) to check for:

- Integer types: `Int8`, `Int16`, `Int32`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`
- Float types: `Float16`, `Float32`, `Float64`
- Bool (subtype of Integer in Julia)

```rust
// In both dispatch paths:
if left_julia_ty.is_builtin_numeric() && right_julia_ty.is_builtin_numeric() {
    // Skip method dispatch → fall through to compile_builtin_binary_op
}
```

**Why this guard exists**: Without it, primitive numeric operations like `Float32(1.0) + true` would dispatch to `+(::Number, ::Number)` from `promotion.jl`, which calls `promote(x, y)` then `px + py`. The promotion step loses type information because the compiled method doesn't emit `DynamicToF32`/`DynamicToF16` back-conversion instructions. The builtin path in `compile_builtin_binary_op` correctly handles type preservation.

**When adding new methods to the dispatch table** (e.g., in `promotion.jl`, `bool.jl`), verify they don't shadow the builtin path for primitive numeric types. The `is_builtin_numeric()` guard ensures that even if a generic method like `+(::Number, ::Number)` matches, the builtin path is used instead.

## Compile-Time vs Runtime Dispatch (Issue #2437, #2439)

Binary operator dispatch has **two parallel paths** that must be kept in sync:

### Compile-Time Path

`compile_binary_op` in `binary.rs` — knows operand types statically (`ValueType`). Uses `JuliaType::is_builtin_numeric()` (from `types.rs`) to guard against user-defined method shadowing.

### Runtime Path

`CallDynamicBinaryBoth` enters through `call_dynamic_binary.rs` and delegates
to `binary_both.rs` — used for **nary operator reduction** where operand types
are unknown at compile time. When `+(a, b, c)` is parsed, it gets reduced to
`+(+(a, b), c)` via `compile_nary_operator_reduction` in `call.rs`, which emits
`CallDynamicBinaryBoth` instructions. At runtime, this handler uses
`is_builtin_numeric_value()` (from `vm/util.rs`) as the equivalent guard.

### Shared Function Pattern (Issue #2282)

| Compile-Time Function | Runtime Function | Location |
|----------------------|-----------------|----------|
| `JuliaType::is_builtin_numeric()` | `is_builtin_numeric_value()` | `types.rs` / `vm/util.rs` |

Both functions must cover the **same set of numeric types**. When adding a new numeric `Value` variant:
1. Add to `JuliaType::is_builtin_numeric()` in `types.rs`
2. Add to `is_builtin_numeric_value()` in `vm/util.rs`
3. Run `test_is_builtin_numeric_value_completeness` to verify

## `CallDynamicBinaryBoth` Fallback Inventory (Issue #4262)

`subset_julia_vm/src/vm/exec/binary_both.rs` uses the shared runtime resolver
before falling back to Rust. The remaining branches are explicit compatibility
owners, not places to add new public arithmetic policy by default.

### Promote-then-same-type structure (Issue #6338)

Mirroring upstream (`julia/base/promotion.jl`: heterogeneous numeric pairs are
promoted to a common type, and only SAME-type pairs have intrinsic
implementations), `execute_binary_both` runs three numeric layers before the
fallback chain below:

1. `fast_primitive_binary_both` — hot same-type pairs (Int64×Int64,
   Float64×Float64) go straight to intrinsics.
2. `promote_numeric_pair` + `same_type_fast_path` — heterogeneous pairs whose
   dynamic-dispatch semantics are exactly "promote, then same-type op"
   (currently the Float64-promoting group: Float16×Float64, Float32×Float64,
   Int64×Float64; and the Float32-promoting group: Float16×Float32,
   Float32×Int64, Float32×Int128). Promotion rules match
   `compile/promotion.rs` / docs/vm/PROMOTION.md. `same_type_fast_path` also
   owns the Float32×Float32 and Float16×Float16 same-type tables, which the
   `float32-intrinsics` / `float16-intrinsics` arms delegate to (those arms
   still exist because the small-int normalization prologue runs after the
   promote interception and can re-create e.g. Float32×Int64 from
   Float32×Bool). Behavior-exception pairs (Bool operands, Float16×Int
   result-narrowing — narrowing the RESULT, not the operand, differs from true
   promotion by double rounding —, unsigned widths, Int128×Int64/Float64,
   BigInt/BigFloat, Char) intentionally stay on their explicit arms — folding
   a pair whose current behavior diverges from promote-then-same-type would be
   an observable change, and a pair that silently loses coverage can reach the
   Pure Julia promote fallback and recurse unboundedly (Issue #5966).
3. The tagged fallback arms below.

The formerly tagged `dead-float16-duplicate` / `dead-float32-duplicate` arms
(unreachable duplicates: every `Float16×Float16` / `Float32×Float32` pair is
consumed by the `left_is_primitive && right_is_primitive` branch earlier in
the same chain) were removed under Issue #6338; their op tables are the
`same_type_fast_path` ones, covered by its unit tests and
`tests/fixtures/promotion/`.

Classification:

- **bootstrap**: must stay as a VM/runtime primitive boundary until primitive
  numeric and BigInt/BigFloat arithmetic can dispatch through Pure Julia without
  promotion recursion or type-preservation regressions.
- **compatibility**: kept for VM-native representations, cache compatibility, or
  explicit `Value::NativeArray` / runtime type-object boundaries.
- **candidate**: safe to shrink toward Pure Julia method ownership in focused
  follow-up slices once fixtures prove user/Pure Julia methods still win.

| Inventory tag | Owner | Upstream reference | Current reason |
|---|---|---|---|
| `BinaryBothFallback: primitive-dispatch-skip` | bootstrap | `julia/src/gf.c`, `julia/base/promotion.jl` | Builtin primitive numeric, BigInt, and BigFloat pairs skip generic `Number` methods to avoid promotion recursion and preserve intrinsic precedence. |
| `BinaryBothFallback: memory-operator-boundary` | compatibility | `julia/base/abstractarray.jl`, `julia/base/genericmemory.jl` | VM `Memory` is not yet a full Pure Julia array wrapper for all binary ops. |
| `BinaryBothFallback: array-wrapper-equality` | compatibility | `julia/base/array.jl` | Transitional native/wrapper array equality still needs representation-aware comparison. |
| `BinaryBothFallback: unsigned-comparison` | bootstrap | `julia/base/int.jl` | `UInt64` / `UInt128` comparisons must avoid lossy `Int64` conversion. |
| `BinaryBothFallback: uint128-arithmetic` | bootstrap | `julia/base/int.jl` | `UInt128` arithmetic preserves full-width behavior before generic fallback. |
| `BinaryBothFallback: uint64-arithmetic` | bootstrap | `julia/base/int.jl` | `UInt64` arithmetic preserves full-width behavior before generic fallback. |
| `BinaryBothFallback: small-int-normalization` | bootstrap | `julia/base/int.jl` | Bool and narrow integer VM values normalize before primitive intrinsic execution. |
| `BinaryBothFallback: primitive-intrinsic-dispatch` | bootstrap | `julia/src/builtins.c`, `julia/base/int.jl`, `julia/base/float.jl` | Primitive arithmetic and comparisons remain runtime intrinsic-owned. |
| `BinaryBothFallback: int128-intrinsics` | bootstrap | `julia/base/int.jl` | `Int128` operations must preserve `Int128` where Julia does. |
| `BinaryBothFallback: float16-intrinsics` | bootstrap | `julia/base/float.jl` | `Float16` same-type arm; the table itself lives in `same_type_fast_path` (Issue #6338). |
| `BinaryBothFallback: mixed-float16-intrinsics` | bootstrap | `julia/base/float.jl`, `julia/base/promotion.jl` | `Float16×Int` result-narrowing exception (true promotion would double-round); `Float16×Float64/Float32` moved to `promote_numeric_pair` (Issue #6338). |
| `BinaryBothFallback: float32-intrinsics` | bootstrap | `julia/base/float.jl` | `Float32` same-type + normalized `Float32×Int64` arm, delegating to `promote_numeric_pair`/`same_type_fast_path` (Issue #6338). |
| `BinaryBothFallback: generic-float-rem` | bootstrap | `julia/base/float.jl` | `%` / `rem` for float operands uses Julia's remainder formula. |
| `BinaryBothFallback: generic-primitive-intrinsic` | bootstrap | `julia/src/builtins.c`, `julia/base/operators.jl` | Remaining primitive I64/F64 intrinsic path. |
| `BinaryBothFallback: string-char-concat` | candidate | `julia/base/strings/basic.jl`, `julia/base/operators.jl` | String/Char `*` can move toward Pure Julia method ownership. |
| `BinaryBothFallback: string-comparison` | candidate | `julia/base/strings/basic.jl` | Lexicographic string comparison can move once public dispatch coverage is complete. |
| `BinaryBothFallback: struct-equality` | compatibility | `julia/base/operators.jl` | Field-wise comparison is a transitional struct representation fallback. |
| `BinaryBothFallback: symbol` | candidate | `julia/base/symbol.jl`, `julia/base/operators.jl` | Symbol equality can be method-owned once symbol identity is fully represented. |
| `BinaryBothFallback: bool-equality` | bootstrap | `julia/base/bool.jl` | Bool equality remains primitive-owned with the numeric guard. |
| `BinaryBothFallback: char-char` | candidate | `julia/base/char.jl` | Char arithmetic/comparison should shrink toward Pure Julia methods. |
| `BinaryBothFallback: char-int` | candidate | `julia/base/char.jl`, `julia/base/int.jl` | Char/Int arithmetic is a targeted Pure Julia candidate. |
| `BinaryBothFallback: bigint-intrinsics` | bootstrap | `julia/base/gmp.jl`, `julia/base/promotion.jl` | BigInt arithmetic preempts generic `Number` promotion recursion. |
| `BinaryBothFallback: bigfloat-intrinsics` | bootstrap | `julia/base/mpfr.jl`, `julia/base/promotion.jl` | BigFloat arithmetic preempts generic `Number` promotion recursion. |
| `BinaryBothFallback: primitive-struct-methoderror` | compatibility | `julia/src/gf.c` | Struct arithmetic should have matched candidates earlier; this preserves MethodError behavior. |
| `BinaryBothFallback: late-float-rem` | compatibility | `julia/base/float.jl` | Late `%`/`rem` path is retained for cache/legacy mixed numeric routes. |
| `BinaryBothFallback: tuple-equality` | candidate | `julia/base/tuple.jl`, `julia/base/operators.jl` | Tuple equality can move toward Pure Julia once tuple recursion coverage is complete. |
| `BinaryBothFallback: datatype-equality` | compatibility | `julia/src/jltypes.c`, `julia/base/operators.jl` | Runtime type-object identity is still projected through VM values. |
| `BinaryBothFallback: typevar-equality` | compatibility | `julia/src/jltypes.c` | Runtime TypeVar identity is still VM-owned. |
| `BinaryBothFallback: complex-array-mul` | compatibility | `julia/stdlib/LinearAlgebra/src/matmul.jl` | Complex scalar/array multiplication still uses the Rust matrix kernel boundary. |
| `BinaryBothFallback: real-array-mul` | compatibility | `julia/stdlib/LinearAlgebra/src/matmul.jl`, `julia/base/arraymath.jl` | Real scalar/array multiplication remains a transitional native array fallback. |
| `BinaryBothFallback: array-array-matmul` | compatibility | `julia/stdlib/LinearAlgebra/src/matmul.jl` | Native-array matrix multiplication remains a Rust kernel boundary. |
| `BinaryBothFallback: array-scalar-ops` | compatibility | `julia/base/arraymath.jl` | Native Array/scalar element-wise ops are shrinking toward Pure Julia arraymath. |
| `BinaryBothFallback: methoderror-fallback` | compatibility | `julia/src/gf.c` | Final MethodError owner after resolver and explicit compatibility fallbacks fail. |

Run `bash scripts/check_binary_both_fallback_inventory.sh` after editing
`binary_both.rs` or this section.

Issue #4276 closed after this inventory, the audit script, and representative
dispatch-first fixtures confirmed that non-primitive operands score user/Pure
Julia methods before retained runtime compatibility fallbacks. Remaining final
native carrier cleanup is tracked by Issue #4568.

## `infer_expr_type` and `needs_runtime_dispatch` Interaction (Issue #2425, #2441)

In the non-`all_base_extensions` path, `needs_runtime_dispatch` decides whether to emit runtime dispatch or fall through to builtin handling. This decision depends on operand types, which for function call results come from `infer_expr_type` in `infer.rs`.

**Critical dependency**: If `infer_expr_type` returns an incorrect type (e.g., `F64` when the actual runtime result could be a struct like `Complex{Float64}`), then `needs_runtime_dispatch` may incorrectly return false, causing the code to fall through to the builtin path which corrupts struct values via `DynamicToF64`.

**Rule**: `infer_expr_type` must return `ValueType::Any` for any function call whose return type cannot be statically determined (e.g., math functions with struct arguments). It is better to over-approximate (return `Any` and use runtime dispatch) than to under-approximate (return `F64` and skip dispatch).

### `needs_runtime_dispatch` Coverage

The condition must cover ALL combinations where runtime dispatch could be needed:

```rust
let needs_runtime_dispatch =
    (left_is_any && right_is_any)       // Both unknown
    || (left_is_struct && right_is_struct) // Both structs
    || (left_is_struct && right_is_any)   // Struct + unknown (Issue #2425)
    || (left_is_any && right_is_struct)   // Unknown + struct (Issue #2425)
    || (left_is_any && right_is_primitive) // Unknown + primitive
    || (left_is_primitive && right_is_any); // Primitive + unknown
```

The (Struct, Any) and (Any, Struct) cases were missing before Issue #2425, causing `cx * log(z)` to return `Float64` instead of `Complex{Float64}`.

## Code Review Checklist for Binary Dispatch Changes

- [ ] When modifying dispatch logic, verify all eight operand type combinations: (Struct,Struct), (Struct,Primitive), (Primitive,Struct), (Struct,Any), (Any,Struct), (Primitive,Any), (Any,Primitive), (Any,Any)
- [ ] When adding new concrete type methods (like Float32), check if `dispatch([Any, X])` could incorrectly match the new method over existing struct methods
- [ ] When modifying specificity scoring, run all mixed-type and dispatch fixture tests
- [ ] When adding non-base-extension operator methods, verify that `all_base_extensions()` returning false doesn't break struct dispatch
- [ ] Test symmetry: `n op r == r op n` for commutative operators with mixed types
- [ ] When modifying guards in `compile_binary_op` (binary.rs), check if the same guard is needed in `CallDynamicBinaryBoth` (`binary_both.rs`)
- [ ] When modifying guards in `CallDynamicBinaryBoth`, check if the same guard exists in `compile_binary_op`
- [ ] When adding new `Value` variants to `is_builtin_numeric_value` in `util.rs`, also update `JuliaType::is_builtin_numeric` in `types.rs`
- [ ] When modifying `infer_expr_type` return types, verify `needs_runtime_dispatch` still makes correct decisions for struct operands
- [ ] When modifying `needs_runtime_dispatch` conditions, test with expressions where one operand is a function call result (inferred as `Any`) and the other is a known struct type

## Result Type Coverage (Issue #1969)

Both `compile_binary_op` and `compile_builtin_binary_op` in `binary.rs` must correctly compute `result_ty` for all operators and type combinations.

### Result Type Table

| Operator    | both_f32/promotable | both_f16/promotable | has_f64 | has_float (only) | same small int | mixed narrow/int | int64 only |
|-------------|---------------------|---------------------|---------|-------------------|----------------|-----------|------------|
| `+`, `-`, `*` | F32              | F16                 | F64     | F64               | same type      | promoted type | I64        |
| `/`         | F32                 | F16                 | F64     | F64               | F64            | F64       | F64        |
| `^` (pow)   | F32                 | F16                 | F64     | F64               | same type      | base type | I64        |
| `%` (mod)   | F32                 | F16                 | F64     | F64               | same type      | promoted type | I64        |
| `div` (div)   | F32                 | F16                 | F64     | F64               | same type      | promoted type | I64        |
| Comparisons | Bool                | Bool                | Bool    | Bool              | Bool           | Bool      | Bool       |
| `&&`, `\|\|` | Bool              | Bool                | Bool    | Bool              | Bool           | Bool      | Bool       |

**Key points:**
- Division (`/`) always returns float, even for integer operands (F64 in that case)
- `compile_binary_op` checks `both_f32 || one_f32_other_promotable` while `compile_builtin_binary_op` checks `has_f32` (simpler because it only handles primitive types)
- All float operations use F64 `operand_ty` for intrinsics, then convert back to F32/F16 via `DynamicToF32`/`DynamicToF16`

### Small Integer Type Preservation (Issue #2278, PR #2279)

For **same-type** small integer arithmetic, the result preserves the input type:

| Operand Types | Result Type | Notes |
|---------------|-------------|-------|
| `Int8 + Int8` | `Int8` | Preserved via `DynamicToI8` |
| `Int16 + Int16` | `Int16` | Preserved via `DynamicToI16` |
| `Int32 + Int32` | `Int32` | Preserved via `DynamicToI32` |
| `UInt8 + UInt8` | `UInt8` | Preserved via `DynamicToU8` |
| `UInt16 + UInt16` | `UInt16` | Preserved via `DynamicToU16` |
| `UInt32 + UInt32` | `UInt32` | Preserved via `DynamicToU32` |
| `UInt64 + UInt64` | `UInt64` | Preserved via `DynamicToU64` |
| `Int64 + Int64` | `Int64` | Native I64 intrinsics |

**Implementation**: The `same_small_int_type()` helper in `binary.rs` detects same-type small integer operands. After computing the result using I64 intrinsics, the compiler emits a back-conversion instruction (e.g., `DynamicToI16`).

**Mixed narrow integer types** (e.g., `Int8 + Int16`) now use
`promote_numeric_value_types()` and emit the matching `DynamicTo*`
back-conversion for `+`, `-`, `*`, `div`, and `%`; see
`arithmetic/mixed_width_promotion.jl` for type assertions. Mixed-width `^`
uses a pow-specific inline VM route and preserves the base integer type; see
`arithmetic/mixed_width_pow_6390.jl`. Function-call `div(...)` and lowered
`÷` also have Pure Julia mixed-integer methods so they do not fall through to
the generic Float64 `floor(x / y)` fallback; see
`arithmetic/mixed_width_div_6477.jl`.

### Intrinsic Routing for Special Operators

| Operator | I64 operand_ty | F64 operand_ty |
|----------|---------------|----------------|
| `+`      | `AddInt`      | `AddFloat`     |
| `-`      | `SubInt`      | `SubFloat`     |
| `*`      | `MulInt`      | `MulFloat`     |
| `/`      | `DivFloat`    | `DivFloat`     |
| `^`      | `DynamicPow`  | `DynamicPow`   |
| `%`      | `SremInt`     | `DynamicMod`   |
| `div`      | `SdivInt`     | `DynamicIntDiv`|

### Code Review Checklist for result_ty Changes

When modifying `result_ty` in `binary.rs`:

- [ ] Check both `compile_binary_op` AND `compile_builtin_binary_op` -- they have parallel `result_ty` blocks
- [ ] Ensure F32 result triggers `DynamicToF32` back-conversion after F64 intrinsic computation
- [ ] Ensure F16 result triggers `DynamicToF16` back-conversion after F64 intrinsic computation
- [ ] Ensure small integer results trigger appropriate `DynamicToI8/I16/I32/U8/U16/U32/U64` back-conversion
- [ ] Verify `operand_ty` includes `has_f16` checks (not just `has_f64 || has_f32`)
- [ ] Test type preservation scenarios: both-same-float, float-int-mixed, float-float-mixed, both-same-small-int
- [ ] When adding new numeric ValueType variants, update `same_small_int_type()` and `small_int_back_conversion()` in `binary.rs`

## Runtime Dispatch Handler Inventory (Issue #2517, PR #2518)

All runtime dispatch handlers select the most specific matching method through the **shared dispatch resolver** in `inference_core/dispatch_resolver.rs` (Issue #3910 migrated the scoring policy out of the per-handler code; the old `score_type_match()` in `vm/util.rs` has been removed). **Never write inline scoring logic** — always go through the shared resolver.

### Shared Resolver (`inference_core/dispatch_resolver.rs`)

| Function | Purpose |
|----------|---------|
| `resolve_runtime_core_signature_candidates(hierarchy, candidates, actual_cores, subtype_matches)` | **Primary (Issue #6502 slice 2)**: structured `core_signature`-based candidate selection over `RuntimeCoreCandidate` (per-slot `CoreType`s with `where` bounds embedded + optional full-signature gate enforcing bounds and cross-slot typevar binding consistency, Issue #6536). Used by `CallDynamicBinaryBoth` / `CallDynamicBinaryNoFallback` / `CallDynamicBinary` / `CallDynamicOrBuiltin` |
| `runtime_core_pattern_score(hierarchy, expected, actual, subtype_matches)` | Score a structured signature: per-argument hierarchy-aware `CoreType::dispatch_pattern_score_in()` summed, with score=1 subtype fallback per argument |
| `embed_type_param_bounds` / `runtime_core_signature` / `runtime_candidate_core_type` | Candidate derivation: re-attach `where` bounds to typevars, build the `core_signature`-shaped gate, and project a declared `JuliaType` onto the matching `CoreType` (legacy parse kept for `AbstractUser`/`Module` divergent shapes) |
| `runtime_type_pattern_score(expected, actual, subtype_matches)` | String-channel score: per-argument `CoreType::dispatch_pattern_score()` summed, with score=1 subtype fallback per argument |
| `resolve_runtime_type_pattern_candidates(candidates, actual, subtype_matches)` | String channel: pick the best-scoring candidate; ties keep the first candidate (residual users: `CallDynamic` family-fallback tiers) |
| `resolve_runtime_type_pattern_candidates_with_family_fallback(…)` | Same, with an extra same-family fallback (score=2) for string-encoded wrapper families CoreType does not fully know |
| `resolve_callable_value_candidates(…)` | Callable function-variable dispatch (arity, vararg, exact-match bonuses, diagonal rule — Issue #5050; `where` bound enforcement gap: Issue #6539) |

### Shared Helpers (`vm/util.rs`)

| Function | Purpose |
|----------|---------|
| `extract_base_type(s)` | Extract base from parametric type: `"Rational{Int64}"` → `"Rational"` |
| `is_type_variable(s)` | Checks if a parameter string is a TypeVar (single uppercase letter like `T`, `K`, `V`) |
| `has_type_variable_param(s)` | Checks if a type string contains TypeVar parameters (e.g., `Rational{T}`) |
| `parse_parametric_params(s)` | Parses parametric type string into parameter list: `"Tuple{Int64, Float64}"` → `["Int64", "Float64"]` |
| `is_rust_dict_parametric_mismatch(value, expected_type)` | Guard: prevents Rust-backed `Value::Dict` from matching parametric `Dict{K,V}` methods (Issue #2748) |

#### Scoring Rules (`CoreType::dispatch_pattern_score()` in `inference_core/type_core/match.rs`)

The per-argument score measures how well a candidate method's expected parameter type matches the actual runtime value type. Higher scores indicate better matches:

| Score | Match Type | Example |
|-------|-----------|---------|
| 4 | Exact match | `"Rational{Int64}"` matches `"Rational{Int64}"` |
| 3 | Type variable parametric | `"Rational{T}"` matches `"Rational{Int64}"` |
| 2 | Base name match | `"Rational"` matches `"Rational{Int64}"` |
| 2 | Array family | `"Array"` matches `"Vector{Int64}"` |
| 1 | Subtype fallback | `"Number"` matches `"Int64"` (via the caller-supplied `subtype_matches` closure, typically `Vm::check_subtype`) |
| 0 | No match (candidate rejected) | `"String"` vs `"Int64"` |

**ALL CallDynamic\* handlers MUST use the shared resolver — never write inline scoring logic.**

#### Tuple Covariance in Dispatch

`dispatch_pattern_score()` handles Tuple covariance structurally: Julia Tuples are **covariant** in their type parameters, so `Tuple{Int64}` matches `Tuple{Any}` (e.g., `Tuple{Any}` vs `Tuple{Int64}` scores 3, the type-variable/parametric tier). Element types are compared position by position; the weakest element determines whether the candidate survives.

### Handler Inventory

| Handler | File | Operands | Notes |
|---------|------|----------|-------|
| `CallDynamic` | `call_dynamic.rs` | Single arg | Generic single-arg dispatch with fallback |
| `CallDynamicOrBuiltin` | `call_dynamic.rs` | Single arg | Unary functions with builtin fallback (floor, ceil, round, trunc) |
| `IterateDynamic` | `call_dynamic.rs` | Single arg (collection) | Collection iterate dispatch |
| `CallDynamicBinary` | `call_dynamic_binary.rs` | Binary (one Any) | One operand type known, one Any |
| `CallDynamicBinaryBoth` | `binary_both.rs` | Binary (both Any) | Both operand types unknown at compile time |
| `CallDynamicBinaryNoFallback` | `binary_no_fallback.rs` | Binary (both known) | User methods shadow builtins completely |
| `CallTypedDispatch` | `call_dynamic_typed.rs` | N-ary (typed params) | Type{T} pattern dispatch with Dict guard |

### Call-Site Cache Layers (Issue #6345)

`CallDynamic`, `CallDynamicOrBuiltin`, `IterateDynamic`, and
`CallDynamicBinary` now probe dispatch caches in three layers:

1. **L1 monomorphic inline cache**: `Vm::call_site_caches[ip]`, allocated as
   runtime VM state with the same length as the bytecode. Exact scalar runtime
   identities (`Int64`, `Float64`, `String`, `Bool`, etc.) produce a compact
   fingerprint before `get_type_name()`. A matching entry returns the cached
   `func_index` or `usize::MAX` negative sentinel without hashing or `HashMap`
   lookup.
2. **L2 polymorphic cache**: the existing
   `dispatch_cache: HashMap<usize, HashMap<u64, usize>>`, keyed by
   `call_site_ip` and `hash_type_name(...)`. This remains the compatibility
   path for polymorphic sites and for identities not admitted to L1.
3. **L3 resolver**: the structured runtime resolver described above. A miss
   fills L2 and, when the argument fingerprint is exact, L1.

L1 intentionally excludes `Type{T}`, tuples, containers, parametric structs, and
function singleton types. Those identities need richer Julia type information
than a scalar tag can represent safely, so they continue through L2/L3.

### Audit Commands

Verify no inline `extract_base` functions exist (should be 0 results):

```bash
rg -n "fn extract_base" subset_julia_vm/src/vm/exec/
```

Verify break statements in dispatch are only exact-match early exits:

```bash
rg -n "break;" \
  subset_julia_vm/src/vm/exec/call_dynamic.rs \
  subset_julia_vm/src/vm/exec/call_dynamic_binary.rs \
  subset_julia_vm/src/vm/exec/call_dynamic_typed.rs \
  subset_julia_vm/src/vm/exec/binary_both.rs \
  subset_julia_vm/src/vm/exec/binary_no_fallback.rs \
  subset_julia_vm/src/vm/exec/call_function_variable.rs
```

### Code Review Checklist

- [ ] New `Instr::CallDynamic*` variants MUST use the shared resolver in `inference_core/dispatch_resolver.rs`
- [ ] Never write inline `fn extract_base()` — use `extract_base_type()` from util
- [ ] `break` in candidate loops only allowed for exact match (score == 4)
- [ ] When adding new score levels, update the `dispatch_pattern_score` unit tests in `inference_core/type_core.rs`
- [ ] All dispatch handlers MUST include the `is_rust_dict_parametric_mismatch()` guard (Issue #2748)

### Dict Parametric Mismatch Guard (Issue #2748)

All `CallDynamic*` handlers include a guard that prevents Rust-backed `Value::Dict` from matching parametric `Dict{K,V}` methods. Without this guard, Pure Julia methods expecting `StructRef` with field access (`.slots`, `.keys`, etc.) would fail with "GetFieldByName: expected struct, got Dict" when called with a Rust-backed `Value::Dict`.

```rust
// In vm/util.rs
pub(crate) fn is_rust_dict_parametric_mismatch(value: &Value, expected_type: &str) -> bool {
    if !matches!(value, Value::Dict(_)) {
        return false;
    }
    let expected_base = extract_base_type(expected_type);
    expected_base == "Dict" && expected_type.contains('{')
}
```

**Handler coverage:**

| Handler | Uses guard? |
|---------|------------|
| `CallDynamic` | Yes |
| `CallDynamicOrBuiltin` | Yes |
| `IterateDynamic` | No (operates on collections, not Dict methods) |
| `CallDynamicBinary` | Yes |
| `CallDynamicBinaryBoth` | Yes |
| `CallDynamicBinaryNoFallback` | Yes |
| `CallTypedDispatch` | Yes |

## Binary Method Dispatch Caching (Issue #2817)

Binary operator method dispatch results are cached by operand types in `binary_method_cache` to avoid repeated dispatch computation in hot loops and nary reduction patterns like `+(+(a, b), c)`.

### Cache Structure (`vm/mod.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BinaryDispatchOp {
    Add, Sub, Mul, Div, Mod, IntDiv, Pow,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct BinaryDispatchKey {
    pub op: BinaryDispatchOp,
    pub left: ValueType,
    pub right: ValueType,
}

// In Vm struct:
binary_method_cache: HashMap<BinaryDispatchKey, usize>,
```

### Lookup via `find_cached_binary_method_index()`

The cache is consulted by `DynamicAdd`, `DynamicSub`, `DynamicMul`, `DynamicDiv`, `DynamicMod`, `DynamicIntDiv`, and `DynamicPow` handlers in `vm/exec/arithmetic.rs`. These handlers first check `should_use_inline_dynamic_op()` for same-type primitives, and when that returns false (e.g., mixed-type or struct operands), they call `find_cached_binary_method_index()`:

```rust
fn find_cached_binary_method_index(
    &mut self,
    op: BinaryDispatchOp,
    names: &[&str],
    left: &Value,
    right: &Value,
) -> Option<usize>
```

**Design decisions:**

- **Positive cache only**: Misses (no matching method found) are NOT cached, so newly added methods remain visible without cache invalidation.
- **Keyed by `ValueType`**: The cache key uses `ValueType` (the VM's internal type representation), not string type names. This provides efficient hashing and avoids string allocation.
- **Per-operator**: Each binary operator (`+`, `-`, `*`, etc.) has its own cache entries via the `BinaryDispatchOp` discriminant.

### Code Review Checklist for Cache Changes

- [ ] When adding new binary operators, add a corresponding `BinaryDispatchOp` variant
- [ ] When modifying method tables at runtime (rare), consider whether cache entries could become stale
- [ ] The cache is initialized empty in `Vm::new()` and `Vm::new_program()` -- no explicit invalidation needed for fresh VMs

## Diagonal Rule in Runtime Dispatch (Issue #2607)

The **Diagonal Rule** in Julia states that when a type variable appears multiple times in covariant position (e.g., `f(x::T, y::T) where T`), it must bind to a **concrete type** at dispatch time. This prevents `f(1, 1.0)` from matching `f(x::T, y::T) where T` because `T` cannot simultaneously be `Int64` and `Float64`.

### Current Implementation

At compile time the diagonal rule is enforced in `compile/method_table.rs` via `JuliaType::is_subtype_of_parametric()` (`types/julia_type/comparison.rs`). At runtime, callable function-variable dispatch enforces it in the shared resolver (`callable_value_candidate_diagonal_ok()` in `inference_core/dispatch_resolver.rs`, Issue #5050); the remaining `CallDynamic*` binary handlers do **not** enforce it themselves.

Method-*specificity* decisions that involve diagonal patterns (tuple/vector/type-value/type-vector/type-matrix diagonal dominance) and tuple-vararg expansion live in a single shared module, `inference_core/specificity.rs` (Issue #6331). Both the compile-time method table (`compile/method_table.rs`) and the runtime dispatcher (`vm/mod.rs`) call this module through thin adapters; there are no longer separate `runtime_*` copies of these predicates.

The method-selection *control flow* — candidate enumeration → match → dominance → pick, the typemap-equivalent core mirroring upstream's `jl_lookup_generic` / `typemap.c` — lives in `inference_core/selection.rs` (Issue #6502, first slice): `unique_dominant_index()` is the shared "exactly one eligible candidate strictly dominates all others" skeleton behind every `*dominant_match_index` dominance pre-check and the pairwise-subtype tie-breaker, and `pick_scored_match()` owns the score-winnowing → tie-breaker ladder → ambiguity protocol. `MethodTable::dispatch_inner` is now a thin adapter that injects only the `MethodSig`-specific semantics (matching/scoring, dominance relations, tie-breaker predicates) as closures.

The runtime `call_dynamic*` entry points adopted the same core in the second slice (Issue #6502, wave 2) via two additional monomorphized primitives: `pick_max_score()` is the runtime winnowing skeleton (first candidate attaining the strictly maximal score — earlier candidates win ties), used by `Instr::CallTypedDispatch`'s runtime function-name search in `vm/exec/call_dynamic_typed.rs`, and `pick_first_tier()` owns ordered tier fallback (first tier producing a winner; errors propagate immediately), used by `Instr::CallDynamic`'s metadata candidate tiers (all candidates → user-defined only → Base `empty` allowlist) in `vm/exec/call_dynamic.rs` with tier index lists still built lazily. Value-dependent VM representation filters (Dict/Range/struct-Dict mismatches) stay at the call sites as candidate pre-filters by design. The remaining gap to full unification: the per-resolver argmax loops inside `inference_core/dispatch_resolver.rs` (same `pick_max_score` skeleton, conversion owned by the #5915 matcher rewrite) and the runtime tie-breaker ladder in `vm/mod.rs::find_best_method_index_from_candidates`; the formerly deferred slice (b) — string-encoded candidate lists in serialized `Instr::CallDynamic*` payloads — shipped as structured index/enum payloads (Issue #6496, see migration item 6 below). `vm/exec/call_dynamic_binary.rs` and `vm/dynamic_ops/dispatch.rs` carry no local selection loops (the binary path fully delegates scoring to the shared resolver since Issue #3910; `dynamic_ops/dispatch.rs` only gates the inline fast path).

Since Issue #6336, the specificity module performs **no ad-hoc type-name string parsing**: abstract container parameters spelled as string-encoded `JuliaType::Struct` names (`AbstractVector{T}`, `AbstractArray{T,N}`, ...) are structured once through the central `CoreType::from` bridge and then inspected as `CoreType` values, and the diagonal patterns carry their `where`-clause upper bounds as structured `CoreType`s (`type_param_upper_bound_core`) instead of raw `&str` bound names. The retired helpers (`parse_diagonal_container_param`, `split_diagonal_container_params`, `bound_subtypes(&str, &str)`) no longer exist; the only name→structure step left in these paths is the shared `CoreType::from_julia_name` parser at the `TypeParam`/`JuliaType` boundary.

### State of the #6336 structured-signature migration

Shipped so far (each phase passed the full release suite):

1. specificity diagonal/container logic is `CoreType`-structured (above), and `MethodSig::arg_core_types()` projects argument core types from `core_signature` (consumed by the empty-trailing-vararg dominance pre-check).
2. `vm/type_objects.rs` reflection name handling delegates to the central type-core tokenizer (`parse_parametric_type_name`); the legacy-array dispatch fence exemption is a flag precomputed at program install, not a per-dispatch name match.
3. `MethodTableProjection`'s name-keyed parent maps are gone; the struct-ancestry fallback walks the shared `StructHierarchy` (`declared_parent_link`), with family-name extraction via the shared `nominal_family_name`.
4. `Instr::IterateDynamic` carries structured candidate indices (`Vec<usize>`, CACHE_VERSION 44); its runtime name-pattern fallback derives signatures from `FunctionInfo`.

5. **`core_signature` is the single serialized and in-memory type source of truth** (CACHE_VERSION 45, closing #6336, completed by #6495): `MethodSig` serializes a dedicated wire format — `core_signature` plus display-only `param_names`. The historical `params: Vec<(String, JuliaType)>` / `type_params: Vec<TypeParam>` in-memory projections are deleted. At build time `MethodSig::from_julia_projections` accepts the lowering-produced JuliaType row only long enough to derive `core_signature`, then stores names only; at deserialization no JuliaType projection is reconstructed into the struct. Cold accessors reconstruct a JuliaType view from `core_signature` through the canonical inverse (`inference_core::core_type_to_julia_type` / `core_type_var_to_type_param`) when diagnostics or legacy-compatible helpers still need one.

**Accessor proof:** `JuliaType → CoreType` is not injective in general, so the inverse picks the canonical spelling for each `CoreType` shape. The final invariant is no longer "stored projections round-trip"; it is "accessors equal the canonical inverse of the stored signature." That is pinned by `compile/cache.rs` gates `base_method_signature_accessors_are_canonical_issue_6495`, `base_method_tables_serde_roundtrip_preserves_canonical_signatures_issue_6495`, and `base_method_runtime_signature_derivation_uses_canonical_projection_issue_6495`, plus user-shape coverage in `method_table.rs` (`where`-clauses, `Vararg{T, N}`, nested parametric containers, `Type{T}`, bounded diagonal spellings).

6. **The `CallDynamic`/`CallTypedDispatch` families carry structured candidates** (CACHE_VERSION 46, closing #6496): `CallDynamic` candidates are a dedicated enum — `DynamicCallCandidate::Method(usize)` for real methods plus `DynamicCallCandidate::NativeIterator(NativeIteratorKind)` replacing the `(usize::MAX, "Zip".."Zip7"/"Base.Generator")` string sentinels — while `CallDynamicOrBuiltin`, `CallDynamicBinary[Both|NoFallback]`, `CallTypedDispatch`, `CallTypedDispatchOrBuiltin[Result]` and `TypedDispatchStoreDict` serialize candidate function indices only (`Vec<usize>`). The runtime derives `RuntimeCandidateCoreSignature` from each candidate's `FunctionInfo` (`vm::derived_runtime_signature` / `expanded_param_types_for_call` in `vm/dispatch_binding.rs`): the `CallDynamic`/binary fallback tiers feed structured `CoreType` slots plus optional `core_signature` gates into the shared resolver, while the typed family memoizes the same structure and still reads the rendered side through the compatibility adapter. Equality with the canonical `MethodSig` projection is pinned by `base_method_runtime_signature_derivation_uses_canonical_projection_issue_6495` (plus the collect-normalization no-op gate) in `compile/cache.rs`. Families without a call-site dispatch cache memoize the derivation (`Vm::binary_signature_cache` for the binary pair, `Vm::typed_signature_cache` keyed by `(func_index, arity)` for the typed family). Compile-side, the candidate builders gate arity through `MethodSig::accepts_arity` and call-position heuristics inspect `MethodSig::param_matches_at_call_position`; `MethodSig::runtime_type_names_for_arity` is deleted.

**Final state (Issue #6495):** the legacy matcher pipeline port is complete through stage 7c-ii-b. Production compile-time dispatch — arity expansion, match, scoring, dominance pre-checks, tie-breakers, struct-parents fallback, and compile-time candidate heuristics — consumes `core_signature` natively or a canonical-inverse view derived from it. `MethodSig` no longer stores JuliaType/type-param projections, deserialization does not rebuild them, and the old `sig_param_types` / `*_legacy` oracle families are gone. The persistent Base cache wire version stays unchanged, but the local file namespace is split to `sjulia_base_cache_v2_<prelude-hash>.bin` so stale pre-deletion Base bytecode is not reused. The stage log below records the migration history; the final paragraph supersedes intermediate "remaining" notes in earlier stage descriptions.

**#6495 progress (stage 1):** the matcher pipeline's per-arity input now has a CoreType-native source — `MethodSig::expanded_core_param_types_for_arity` projects the expanded argument types straight from `core_signature` with the same vararg rules as the legacy expansion, pinned slot-for-slot equal over the Base corpus by the permanent gate `base_method_expanded_core_params_parity_issue_6495` (plus user-shape unit coverage in `method_table.rs`). The first consumer that was already CoreType-shaped (`static_arg_tuple_satisfies_method`, the imprecise-`Any` dominance gate) reads it directly; the match/score stages flip in later #6495 slices behind the same parity-gate discipline.

**#6495 progress (stage 2):** the matcher decision itself now has a CoreType-native port — `dispatch_resolver::core_match::core_signature_match_with_bindings` (new submodule `inference_core/dispatch_resolver/core_match.rs`) reimplements `julia_signature_match_with_bindings` arm-for-arm on `CoreType`/`CoreTypeVar`, with no `JuliaType` values and no type-name string surgery: the Tuple/`Vararg{T}` trailing pattern (#4857), TypeVar upper/lower bounds and parametric bounds (#5383), the struct-leaf TypeVar image rule (#5314), `Type{T}` invariant double-bound binding (#5051), nested-diagonal binding recording (#5050), the type-object-argument rejection, plus CoreType ports of `is_subtype_of` / `is_subtype_of_parametric` / `extract_type_bindings` / `check_diagonal_rule_for_params` and the array/range projections. Its `where` clause comes from `MethodSig::core_signature_type_vars` (the `core_signature` `UnionAll` wrappers). It is NOT yet on the production path — the legacy matcher stays authoritative — but its decision (match/no-match **and** binding count) is pinned equal to the legacy matcher over the whole Base corpus by the differential gate `base_method_core_dispatch_match_parity_issue_6495` (cross-applies every method's own arity-expanded parameter tuple against every method in its table; >20 000 decisions). The non-injectivity of `JuliaType → CoreType` is handled by following the spelling the canonical inverse reconstructs: the bare `Array`/`Dict`/`Set`/`Vector{T}`/… images are kept out of the genuine-`JT::Struct` parametric arms via `core_maps_to_julia_struct`, and an undeclared unbounded `CoreType::TypeVar` image is treated as the #5314 struct leaf (the only shape real `core_signature` projections produce; an undeclared *implicit* typevar bind is unreachable through the projection). Stage 3 flips `dispatch_inner` / `signature_matches_arg_types` onto this entry behind the same gate.

**#6495 progress (stages 3–4):** the production compile-time pipeline is flipped onto the port — `method_match_binding_count` (consumed by `dispatch_inner` / `signature_matches_arg_types`) matches through `expanded_core_param_types_for_arity` + `core_signature_type_vars` + `core_match::core_signature_match_with_bindings`, and matched candidates are scored by `core_match::score_core_signature_with_binding_count` (gate: `base_method_core_dispatch_score_parity_issue_6495`). The argument tuple is bridged once per dispatch. The legacy matcher/scorer remain as (a) the fallback when a method lacks a refreshed structured signature and (b) the struct-parents declared-ancestry fallback (`struct_parents_fallback_match`), which walks parent links rather than signature shapes.

**Module abstract types + `Named`-vs-`Named` matching (Issues #7263 / #7265):** two gaps surfaced for methods defined inside a bundled package (module). First, abstract types declared *inside a module* (`Distributions`' `Distribution`/`VariateForm`/…) were collected for structs (`collect_module_structs`) and primitives (`collect_module_primitive_types`) but **not** for abstract types — there was no `collect_module_abstract_types`. So they never reached the compiler's abstract-type registry (`abstract_type_parents` / `abstract_type_names` / the struct hierarchy), and a method parameter annotated with a module-local abstract type (`f(d::Distribution)`) was left as a concrete `Struct("Distribution")` annotation that no value satisfies — the typed method silently lost dispatch to the untyped generic it extends (`median(d::Distribution)` fell through to `Statistics.median(arr)`). `compile/mod.rs::collect_module_abstract_types` now mirrors the struct/primitive collectors and feeds `pipeline_ctx.rs`'s registry (bare names, matching how module structs also register a bare short name). Second, a *parametric package* struct value images as `CoreType::Named` (its family is not a built-in `is_known_struct_family`), so a within-module call (`ncategories(d)` inside `mean(d::Categorical)`, inferring the bare `Named("Categorical")`) was matched against the method's module-qualified param `Named("Distributions.Categorical")`; `core_match`'s `CoreType::Named(expected)` arm only handled a `Struct` actual, so bare-vs-qualified `Named` user-struct names never matched. The arm now also accepts a `CoreType::Named` actual, comparing `strip_module_prefix`-stripped family names (module qualification is not part of type identity).

**#6495 progress (stage 5):** the #5926 dominance pre-check families and the tuple-vararg ambiguity check consume the `core_signature` projections too — `inference_core/specificity.rs` gained arm-for-arm `core_*` counterparts (the tuple-vararg expansion struct is `CoreType`-native and shared by both spellings; tuple/vector diagonals, type-value/vector/matrix diagonals, union-vs-actual), and `compile/method_table.rs::dominance_precheck_index` routes through them via `CoreDominanceInputs` (`arg_core_types` borrows + projected core `where` vars), falling back to the legacy `params`/`type_params` chain only when a match lacks a structured signature. Decision parity with the legacy chain over every multi-match Base-corpus argument tuple (dominance index AND tuple-vararg ambiguity verdict) is pinned by `base_method_core_dominance_parity_issue_6495`. The runtime dominance copies in `vm/mod.rs` keep the legacy `JuliaType` adapters: their candidate parameter lists derive from `FunctionInfo` (#6496), not from `MethodSig` projections, so they do not block the projection deletion.

**#6495 progress (stage 6a):** the `dispatch_inner` tie-breaker ladder's projection-consuming inputs read the `core_signature` projections — the fixed-prefix `Any` count (`any_param_count_fixed_prefix`), the fewest-`where`-params count (allocation-free `MethodSig::core_signature_type_var_count`), the #3144 struct-ancestry filter (`ancestry_filter_passes`; this port is image-exact because `CoreType::from_julia_name` never produces `CoreType::AbstractUser`, so the arm fires for exactly the legacy `JuliaType::AbstractUser` parameter set), and the #5068 strictly-more-specific comparison (`method_params_strictly_more_specific` now borrows the projection via `method_param_cores` instead of cloning `CoreType::from` per pair). Each falls back to the legacy `params`/`type_params` chain when a method lacks a structured signature (`MethodSig::structured_arg_core_types`). Parity over the Base corpus — plus the per-method elementwise image invariant `arg_core_types()[i] == CoreType::from(&params[i].1)` that proves the strictly-more-specific switch — is pinned by `base_method_core_tiebreaker_parity_issue_6495`. The exact-signature tie-breaker initially stayed on `JuliaType` equality because `CoreType::from` is not injective (`Struct("Vector{Int64}")` and `VectorOf(Int64)` share an image); stage 7a ported it onto the projection after pinning that the colliding spellings are unreachable (see below).

**#6495 progress (stage 6b-i):** the `expr/builtin.rs` compile-time candidate-shape heuristics (runtime-iterate candidates, runtime/range/generator collect candidates, the bare-`Tuple`/`Any` first-param matchers, and the iterate struct-base dispatch) read the `core_signature` projection first via `method_first_param_matches(method, core_pred, legacy_pred)`, falling back to the legacy `params` read only for pre-`core_signature` placeholders. The CoreType predicates follow the canonical-inverse decision rule — `core_pred(core)` equals the legacy predicate applied to `core_type_to_julia_type(core)` — so the known bridge non-injectivities resolve to the canonical spelling (`Struct{"Vector",1}`/`Struct{"Matrix",1}` follow the `VectorOf`/`MatrixOf` verdicts; `Struct{"StepRange",0}` follows the bare `StepRange` verdict while parametric `StepRange{..}` stays out of the collect list; no-dedicated-variant abstract families such as `AbstractVector` keep their legacy `Struct`-name verdicts). Per-parameter parity for all six predicates over the whole Base corpus is pinned by `base_method_core_param_heuristics_parity_issue_6495`; unit tests in `builtin.rs` pin the definitional inverse invariant plus direct parity on round-tripping user spellings.

**#6495 progress (stage 6c):** the abstract-interp engine's only `MethodSig`-projection consumer, `method_signature_equivalent` (inference-cache invalidation after method mutation), compares canonical `core_signature`s when both sides carry one, with the legacy `params`/`type_params` comparison kept for `Bottom` placeholders (the availability guard checks the placeholder itself — a zero-parameter placeholder would masquerade as structured under the length-based guard). Vararg markers stay explicit on both paths since neither projection encodes them. `InferenceEngine::add_method` refreshes the invalidation operand so the core arm actually fires for runtime mutations. Pairwise legacy-vs-core equivalence over the Base corpus (plus `Bottom`-degraded fallback parity) is pinned by `base_method_core_signature_equivalence_parity_issue_6495`. Remaining projection consumers before the stage-7 deletion: the `expr/binary*`/`expr/call` compile-time heuristics (6b-ii), the `AmbiguousMethod` error-candidate display reads, and the reflection/type-stability `MethodSig` constructions.

**#6495 progress (stage 6b-ii):** the `expr/binary` and `expr/call` compile-time dispatch heuristics read the `core_signature` projection first. In `expr/binary/mod.rs`, the declared operand pair is read through `method_binary_params_match(method, core_pred, legacy_pred)` (legacy `params` only for pre-`core_signature` placeholders), with canonical-inverse ports of every predicate: `core_is_binary_runtime_dispatch_candidate_type`, `core_is_user_array_runtime_dispatch_candidate_type`, `core_is_linalg_array_dispatch_type` + `core_linalg_array_dispatch_rank` (the linalg `*` compatibility checks keep their `JuliaType`/`ValueType` *actual*-side inputs — only the method-param side switches), `core_is_string_concat_dispatch_type`, `core_is_dispatch_first_equality_type`, the Complex-method filter (`core_is_complex_struct_param`), the struct-spelling filter (`core_param_is_struct_spelling`), and the (Struct, Any) struct-base position match (reuses `CoreCompiler::core_param_struct_base`). The five identical runtime-candidate filter blocks collapsed into `is_binary_runtime_dispatch_candidate_method`. Cold single-method reads switch to reconstruction instead of per-predicate ports: `MethodSig::projected_param_julia_type` (canonical inverse, `Cow`) feeds `compile_user_defined_binary_op`'s operand-coercion/result-type logic and the linalg candidate-dedupe display pair, and the dispatch-first `==` method finder compares the param projection against the once-bridged operand images (`CoreType::from`, injective on the dispatch-first subdomain). In `expr/call/mod.rs`, the specificity gates read `MethodSig::param_specificity` / `all_params_specificity_zero` (provably identical: `JuliaType::specificity` already evaluates `CoreType::from(self).specificity()`, and the stage-6a gate pins the elementwise image invariant), the `Any`-slot dispatch probe checks `CoreType::Any` per slot, and the user range-collect probe gets its own port `core_is_user_range_collect_signature_type` (narrower family list than the 6b-i builtin one). Per-parameter parity for all predicates plus reconstruction and specificity equality over the whole Base corpus is pinned by `base_method_core_binary_heuristics_parity_issue_6495`; unit tests in `binary/mod.rs`, `call/mod.rs`, and `method_table.rs` pin the definitional inverse invariant, direct parity on round-tripping spellings, and accessor agreement across both projection sources. Remaining projection consumers before the stage-7 deletion: the `AmbiguousMethod` error-candidate display reads and the reflection/type-stability `MethodSig` constructions.

**#6495 progress (stage 6b-iii):** the generic dispatch tail in `expr/call/dispatch.rs` (the #6332 extraction, missed by the 6b-ii survey) reads the `core_signature` projection first. New `MethodSig` pair accessors carry the shared sourcing rule: `any_param_matches` / `all_params_match` / `param_matches_at(idx, ..)` (out-of-range indices are non-matches, preserving the legacy `zip` truncation and `params.get` `Option` gates) plus the whole-row `projected_param_julia_types()` reconstruction for diagnostic payloads. Ported readers: the `promote_type`/`promote_rule` and `has_typeof_methods` `CallTypedDispatch` probes (`core_is_typeof_param` — image-exact up to the unreachable `Struct("Type{..}")` name spelling), the three identical generic-`Type{T}` fallback finders (`core_is_typeof_typevar_param`, with the stage-4 `TypeOf(Struct("Q"))` single-letter caveat), the `Any`-slot runtime-dispatch cases 1–3 and the single-arg `CallDynamic` candidate/fallback gates (`core_is_any_param`, with the stage-6a `Struct("Any")` caveat), the ambiguity `candidate_sigs` display rows, the per-slot argument-coercion gates (`projected_param_julia_type`), and the binary-`map` element-type fallback. Remaining `params` reads in the file are arity-only (`len()`), which survive the stage-7 rename to `param_names`, plus `param_type_at_call_position` (an internal `MethodSig` accessor; its projection-first rewrite happens inside `MethodSig` at stage 7). Per-parameter parity for the three predicates plus whole-row reconstruction over the Base corpus is pinned by `base_method_core_call_dispatch_heuristics_parity_issue_6495`; unit tests in `dispatch.rs` pin user-shape predicate parity and method-probe agreement across both projection sources.

**#6495 progress (stage 7a):** the remaining production *value* reads of the `params`/`type_params` projections are ported, leaving only the `MethodSig` constructions (pipeline_ctx / type-stability / reflection) and the legacy fallback chains ahead of the projection deletion. In `method_table.rs`: the exact-signature tie-breaker compares `structured_arg_core_types()` against the once-bridged argument cores (`exact_signature_match`; the legacy `JuliaType` equality remains as the structured-unavailable fallback and the gate oracle — accepted divergence is exactly the non-injective `CoreType::from` spellings, unreachable from lowering and from the canonical inverse, now pinned corpus-wide by the extended `base_method_core_tiebreaker_parity_issue_6495`); the `AmbiguousMethod` diagnostic payload rows come from `projected_param_julia_types()`; the empty-trailing-vararg dominance pre-check and the inner-constructor filter in `expr/call/constructors.rs` read the new guarded `MethodSig::has_where_params()`; and `add_method`'s replacement detection consults the legacy projection-equality arm only when either side lacks a structured signature (provably redundant otherwise: `compute_core_signature` is deterministic over the projections, and both sides are always refreshed or wire-canonical). A new vararg-aware accessor `param_matches_at_call_position(position, core_pred, legacy_pred)` (the predicate form of the historical `param_type_at_call_position` position mapping — that legacy accessor is now `#[cfg(test)]`, kept as the gate oracle) carries the abstract-array-family probes in `expr/call/dispatch.rs` (case 4) and `expr/call/module_call.rs` onto the new `call::core_is_abstract_array_family_type` port; `module_call.rs`'s per-slot argument-coercion gates read `projected_param_julia_type` (mirroring the stage-6b-iii dispatch.rs port, preserving this site's own Any/narrow-integer/abstract-integer gate), and `builtin_math.rs`'s `CallDynamicOrBuiltin` candidate filter reads `param_matches_at(0, core_param_is_struct_spelling, ..)`. Parity for the new probes (per parameter and per vararg-mapped call position, including one past the end) and for `has_where_params` is pinned by the extended `base_method_core_call_dispatch_heuristics_parity_issue_6495`; unit tests in `method_table.rs` pin exact-match / call-position / `where`-presence agreement across both projection sources.

**#6495 progress (stage 7b):** all production `MethodSig` constructions are centralized through the new eager constructor `MethodSig::from_julia_projections(..)`, which derives the canonical `core_signature` at construction time instead of leaving the `CoreType::Bottom` placeholder for `MethodTable::add_method`'s refresh (the refresh still runs, defensively — `compute_core_signature` is deterministic over the projections, so deriving earlier is a provable no-op). Converted sites: `pipeline_ctx.rs` (function definitions + inner constructors), `type_stability/analyzer.rs` (`method_sig_from_return`, `seed_method_tables`, inner-constructor seeding), `vm/builtins_reflection/mod.rs` (`reflection_method_table`, `seed_reflection_return_snapshots`). After this stage a `Bottom`-placeholder signature is never observable in production (struct-literal construction remains only in tests, several of which exercise the structured-unavailable fallbacks on purpose), and the constructor is the single seam the stage-7 projection deletion has to reshape. The remaining projection consumers are the legacy fallback chains themselves (`method_table.rs` internals, `inference_core/dispatch_resolver.rs`'s legacy matcher, the `method_signature_equivalent` fallback arm, the wire `Deserialize` reconstruction) — all scheduled for the deletion stage.

**#6495 progress (stage 7c-i):** the structured-unavailable fallback chains on the dispatch pipeline are retired — they became unreachable at stage 7b (every production `MethodSig` carries a refreshed `core_signature`). In `method_table.rs`: `method_match_binding_count` no longer falls back to the full legacy pipeline (`expanded_core_param_types_for_arity` returning `None` now means exactly "arity rejected", where the legacy expansion returned `None` too; `julia_signature_match_with_struct_parents` is deleted — the per-arm gates inline its halves), `dispatch_inner`'s legacy scorer arm is gone (a matched method always expands), `dominance_precheck_index` / `tuple_vararg_conflicting_match` skip the pre-checks instead of re-running the legacy `params`/`type_params` chain, the tie-breaker inputs use conservative defaults for the (test-only) placeholder case (`exact_signature_match` → not exact, `any_param_count_fixed_prefix` → 0, `where_param_count` → the wrapper count unconditionally, `ancestry_filter_passes` → vacuous pass, `method_params_strictly_more_specific` → false), `static_arg_tuple_satisfies_method` drops its legacy-expansion bridge, and `add_method`'s replacement detection drops the legacy projection-equality arm (provably redundant for refreshed sides). The abstract-interp `method_signature_equivalent` (engine/mod.rs) compares canonical `core_signature`s unconditionally — its `Bottom`-placeholder fallback arm and the corresponding half of `base_method_core_signature_equivalence_parity_issue_6495` are deleted. The legacy implementations (`*_legacy` tie-breakers, `dominance_precheck_index_legacy`, `tuple_vararg_conflicting_match_legacy`, and the legacy dominance families they call, including `sig_param_types`) are retained under `#[cfg(test)]` as the parity-gate oracles until the projection fields are deleted (stage 7c-ii). Still production after this stage: the user-defined struct-parents fallback (`struct_parents_fallback_match`, fed by `expanded_param_types_for_arity` + `type_params` — it walks declared parent links, not signature shapes) and the `MethodSig` accessors' legacy arms (`param_matches_at` family, `projected_param_julia_type(s)`, `param_specificity`, `has_where_params`), both of which the field deletion reshapes.

**#6495 progress (stage 7c-ii-a):** the `MethodSig` accessors' legacy `params`/`type_params` arms are retired and the cross-crate `legacy_pred` call-site sweep is done. The pair-predicate accessors lost their legacy halves (`any_param_matches` / `all_params_match` / `param_matches_at` / `param_matches_at_call_position` now take only the CoreType predicate; `method_first_param_matches` in `expr/builtin.rs` and `method_binary_params_match` / `is_linalg_mul_candidate_method` in `expr/binary/mod.rs` likewise), and the value accessors are canonical-inverse-only (`projected_param_julia_type` -> `Any` on a test-only `Bottom` placeholder, `projected_param_julia_types` -> empty row, `param_specificity` -> 0, `all_params_specificity_zero` -> conservative `false`, `has_where_params` -> the unconditional `UnionAll`-wrapper count, matching `where_param_count`). The two inline structured-unavailable arms in `expr/call/mod.rs` (Any-slot probe) and `expr/builtin.rs` (iterate struct-base finder) follow the same retirement. Projection-side legacy predicates with no production caller left (`is_binary_runtime_dispatch_candidate_type`, `is_linalg_array_dispatch_type`, `is_string_concat_dispatch_type`, the four collect/iterate candidate predicates, `is_range_collect_signature_type` in `expr/call/mod.rs`, `legacy_is_typeof_param`/`legacy_is_typeof_typevar_param`/`legacy_is_any_param`) moved under `#[cfg(test)]` as the Base-corpus parity-gate oracles; `linalg_array_candidate_compatible{,_for_value_type}` (closure-only helpers with no remaining caller) are deleted. Actual-argument-side checks (`is_dispatch_first_equality_type`, `is_user_array_runtime_dispatch_candidate_type` on inferred operand types, `is_abstract_array_family_type_name` inside the core predicate) remain production -- they read the caller's argument types, not the projections. At this point no production code read a `MethodSig` projection *value* outside `method_table.rs` itself; stage 7c-ii-b then removed the projection fields and renamed the surviving arity-only storage to display-only `param_names`.

**#6495 progress (stage 7c-ii-b):** the in-memory projection fields are deleted. `MethodSig` now stores `param_names` plus `core_signature` only; `type_params` is gone; production construction derives the core signature in `from_julia_projections` and discards JuliaType inputs; serde keeps the existing wire/CACHE_VERSION shape and does not reconstruct projections. The struct-parents fallback receives its signature side from the canonical inverse (`expanded_projected_param_julia_types_for_arity` + `projected_type_params`) and then walks the declared parent links as before. The old `sig_param_types` / `*_legacy` oracle families and Bottom-placeholder projection fallback tests are removed; cache gates now assert accessor-vs-canonical invariants instead of stored-projection parity. Persistent Base cache loads drop legacy serialized inference snapshots, and the local cache filename namespace is split to `v2` without changing the wire schema.

**#6495 final state:** every production dispatch decision flows through `expanded_core_param_types_for_arity` → `core_match::core_signature_match_with_bindings` → `core_match::score_core_signature_with_binding_count` → core dominance/tie-breakers, with the declared-ancestry fallback reading a canonical-inverse JuliaType view only for the parent-walk rules that still operate on JuliaType. Permanent gates in `compile/cache.rs` now cover canonical accessors, serde roundtrip, runtime signature derivation, and collect-candidate normalization. Criterion comparison against the `origin/main` baseline used for the branch showed no dispatch hot-path regression above 5%; the largest rerun delta was `closure_capture_affine_map_1000` at +4.4%, while several recursive/HOF paths improved.

### Why This Is Low Risk

Runtime types are always **concrete** (never abstract or union types at the value level). When the VM executes a `CallDynamicBinaryBoth` instruction:

1. Both operand values have concrete types (e.g., `Int64`, `Float64`)
2. The method candidate's parameter types are checked via the shared resolver scoring (`runtime_type_pattern_score()`)
3. If a method has `(T, T) where T`, the runtime would need to verify both args bind `T` to the same type

In practice, the compile-time diagonal rule catches most violations. Runtime violations would only occur if:
- Type inference failed to determine operand types (both are `Any`)
- A method with diagonal type variables is the best runtime match

### Future Work: Adding Runtime Diagonal Rule

If diagonal rule violations are observed at runtime, add enforcement to all `CallDynamic*` handlers:

```rust
// Pseudocode for runtime diagonal rule check
if method.has_diagonal_typevars() {
    let bindings = extract_type_bindings(method, &arg_types);
    if !bindings.all_consistent() {
        continue; // Skip this candidate
    }
}
```

### Code Review Checkpoint

When modifying `CallDynamic*` handlers:
- [ ] Check if the modification could cause diagonal rule violations at runtime
- [ ] If adding new method matching logic, consider whether type variables need consistency checks
- [ ] Test with `f(x::T, y::T) where T` and mixed-type arguments (e.g., `f(1, 1.0)`)

## Related Documentation

- `NUMERIC_TYPES.md` - Numeric type parity checklist and intrinsic dispatch
- `TYPE_SYSTEM.md` - Type system architecture
- `CLAUDE.md` - Top-level contributor guidelines
