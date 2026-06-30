# Numeric Type Preservation Guide

This document describes how numeric types must be preserved through arithmetic operations across multiple dispatch code paths in the pipeline.

## Part 1: Float Type Preservation

This section describes how float types (Float16, Float32, Float64) must be preserved.

## Background (Issue #1647 / #1653 / #2221)

Float32 arithmetic operations (+, -, *, /) were returning Float64 instead of Float32. The root cause was inconsistent type preservation across multiple dispatch paths, making it easy for F32 to "leak" to F64 at any point in the pipeline.

## The Four Dispatch Code Paths

Float type preservation must be maintained at each of these paths:

```
Julia source: floor(Float32(1.5))
       │
       ▼
  [1. Compiler]  binary.rs: Detect F32 operands → emit F64 intrinsic + DynamicToF32
       │
       ▼
  [2. Static Intrinsics]  intrinsics_exec.rs: apply_unary_float_op_with_heap preserves F16/F32/F64
       │
       ▼
  [3. Dynamic Binary Dispatch]  binary_both.rs + dynamic_ops/: Runtime binary ops preserve F32
       │
       ▼
  [4. Dynamic Unary Builtins]  call_dynamic.rs CallDynamicOrBuiltin: Delegates to
       │                        apply_unary_float_op_with_heap for type-preserving unary math
       ▼
  Result: Float32(1.0)  ← type must still be Float32
```

### Path 1: Compiler Back-Conversion (`compile/expr/binary/`)

The compiler detects float operand types and emits appropriate instructions:

- **`result_ty` detection**: Must distinguish between F16, F32, and F64 operands. The `both_f32`, `both_f16`, and `has_f64` flags control which result type is used.
- **Back-conversion**: After computing with F64 intrinsics, the compiler emits `DynamicToF32` or `DynamicToF16` to restore the original type.
- **Both paths**: `compile_binary_op` AND `compile_builtin_binary_op` have parallel `result_ty` blocks — both must be updated.
- Fixed in #2279.

### Path 2: Static Intrinsics (`vm/intrinsics_exec.rs`)

The VM executes float intrinsics and must preserve type through the result:

- **`pop_f64_or_i64()` hazard**: This helper converts F32 values to f64, losing the original type. It is acceptable for operations that return a different type (e.g., comparisons return Bool), but must NOT be used when the result should preserve the input float type.
- **Arithmetic intrinsics** (`AddFloat`, `SubFloat`, `MulFloat`, `DivFloat`): Check for F32 operand pairs and push `Value::F32` results.
- **Unary math intrinsics** (`SqrtLlvm`, `FloorLlvm`, `CeilLlvm`, `TruncLlvm`, `AbsFloat`): Use `apply_unary_float_op_with_heap` helper for type-preserving dispatch.
- Fixed in #2219.

### Path 3: Dynamic Binary Dispatch (`vm/exec/binary_both.rs`, `vm/dynamic_ops/`)

Runtime dispatch for binary operations that couldn't be resolved at compile time:

- **`both_f32` path**: When both operands are F32, dispatch to F32-specific arithmetic.
- **F32-I64 mixed path**: Promote I64 to F32, compute, return F32 result.
- **F32-F64 mixed path**: Promote F32 to F64, compute, return F64 result (Julia promotion rules).
- **Parity requirement**: Float32 handlers must mirror Float16 handlers — if Float16 has a dispatch path, Float32 must have the equivalent.
- `CallDynamicBinaryBoth` routes through `binary_both.rs`; generic dynamic helper
  fallback paths live under `vm/dynamic_ops/`.
- Already type-preserving by design.

### Path 4: Dynamic Unary Builtins (`vm/exec/call_dynamic.rs` — `CallDynamicOrBuiltin`)

Handles unary math builtins (floor, ceil, round, trunc) when dispatched dynamically:

- Delegates to `apply_unary_float_op_with_heap` from `intrinsics_exec.rs` — the same helper used by static intrinsics (Issue #2284).
- This unification ensures that both static and dynamic paths use a single source of truth for type preservation logic.
- Fixed in #2283, unified in #2284.

## Shared Type Preservation Helper

The `apply_unary_float_op_with_heap` function in `intrinsics_exec.rs` is the
single source of truth for unary float type preservation:

```rust
pub(crate) fn apply_unary_float_op_with_heap(
    val: Value,
    struct_heap: &[StructInstance],
    op: fn(f64) -> f64,
) -> Result<Value, VmError> {
    match val {
        Value::F16(a) => Ok(Value::F16(half::f16::from_f64(op(a.to_f64())))),
        Value::F32(a) => Ok(Value::F32(op(a as f64) as f32)),
        other => {
            let a = value_to_f64_with_heap(&other, struct_heap)?;
            Ok(Value::F64(op(a)))
        }
    }
}
```

This helper is used by:
- **Static intrinsics** (Path 2): `SqrtLlvm`, `FloorLlvm`, `CeilLlvm`, `TruncLlvm`, `AbsFloat`
- **Dynamic unary builtins** (Path 4): `Exp`, `Log`, `Sin`, `Cos`, `Tan`, `Floor`, `Ceil`

## Julia's Promotion Rules

Type preservation follows Julia's standard promotion rules:

| Left | Right | Result | Rule |
|------|-------|--------|------|
| Float16 | Float16 | Float16 | Same-type preservation |
| Float32 | Float32 | Float32 | Same-type preservation |
| Float64 | Float64 | Float64 | Same-type preservation |
| Float16 | Int64 | Float16 | Float wins over Int |
| Float32 | Int64 | Float32 | Float wins over Int |
| Float64 | Int64 | Float64 | Float wins over Int |
| Float16 | Float32 | Float32 | Wider float wins |
| Float16 | Float64 | Float64 | Wider float wins |
| Float32 | Float64 | Float64 | Wider float wins |

## Method Dispatch vs Builtin Path Interaction (Issue #2203, #2225)

**Critical**: The compiler's method dispatch and builtin code paths must be carefully coordinated to preserve float types. The method dispatch layer can be "too eager" — matching generic methods like `+(::Number, ::Number)` for primitive types that should go through the type-aware builtin path.

The builtin path (`compile_builtin_binary_op`) correctly emits `DynamicToF32`/`DynamicToF16` back-conversion instructions. The method dispatch path does **not** — it compiles to a function call that goes through Julia's `promote()`, which loses type information.

Both dispatch branches in `compile_binary_op` (all_base_extensions and non-base-extensions) use `JuliaType::is_builtin_numeric()` to skip method dispatch when both operands are primitive numeric types, ensuring the builtin path handles them.

**Warning**: When adding new operator methods to the Julia dispatch table, always verify that primitive numeric type combinations still route through the builtin path. Test with `typeof()` assertions, not just value checks.

## Checklist for New Arithmetic Operations

When adding a new arithmetic operation that involves float types:

- [ ] **Path 1 — Compiler** (`binary.rs`): Add F16, F32, F64 cases to `result_ty` in BOTH `compile_binary_op` AND `compile_builtin_binary_op`
- [ ] **Path 1 — Compiler** (`binary.rs`): Emit `DynamicToF32`/`DynamicToF16` back-conversion when `result_ty` is F32/F16
- [ ] **Path 2 — Static intrinsics** (`intrinsics_exec.rs`): For unary ops, use `apply_unary_float_op_with_heap`; for binary ops, handle F32 operand pairs explicitly — do NOT use `pop_f64_or_i64()` for type-preserving operations
- [ ] **Path 3 — Dynamic binary dispatch** (`binary_both.rs`, `vm/dynamic_ops/`): Add F32 and F16 paths matching existing dispatch patterns
- [ ] **Path 4 — Dynamic unary builtins** (`call_dynamic.rs`): Add new builtin to the `CallDynamicOrBuiltin` op match (uses `apply_unary_float_op_with_heap` automatically)
- [ ] **Tests**: Test `typeof()` on results, not just values, to catch type promotion issues
- [ ] **Parity**: Verify F16 and F32 handlers exist in parallel (if one exists, the other must too)

## Testing Type Preservation

Always test with `typeof()` and `===` (identity comparison), not just `==` (value comparison):

```julia
# GOOD: catches type promotion bugs
@test typeof(Float32(1.0) + Float32(2.0)) === Float32

# BAD: passes even if result is Float64(3.0) instead of Float32(3.0)
@test Float32(1.0) + Float32(2.0) == 3.0
```

### Fixture Tests

- `tests/fixtures/types/float_type_preservation.jl` — Arithmetic type preservation for all float types (Issue #1653)
- `tests/fixtures/types/float_field_type_preservation.jl` — Struct field type preservation (Issue #1651/#1655)
- `tests/fixtures/types/typed_f32_f16_instructions.jl` — Typed Return/Store/Load instructions (Issue #1893)
- `tests/fixtures/math/intrinsics_type_preservation.jl` — Unary math type preservation for Floor/Ceil/Sqrt/Trunc/Abs (Issue #2221)

### Running Tests

```bash
# Run all type preservation tests
timeout 1800 cargo nextest run --release --test fixture_tests types_float_type_preservation
timeout 1800 cargo nextest run --release --test fixture_tests types_float_field_type_preservation
timeout 1800 cargo nextest run --release --test fixture_tests types_typed_f32_f16_instructions
```

## Quick Audit Commands

```bash
# Check for uses of pop_f64_or_i64 in arithmetic intrinsics (potential type loss)
rg -n "pop_f64_or_i64" subset_julia_vm/src/vm/intrinsics_exec.rs

# Verify F32/F16 parity in dynamic dispatch
rg -c "F32|Float32" subset_julia_vm/src/vm/dynamic_ops/ subset_julia_vm/src/vm/exec/binary_both.rs
rg -c "F16|Float16" subset_julia_vm/src/vm/dynamic_ops/ subset_julia_vm/src/vm/exec/binary_both.rs

# Check compiler float type detection
rg -n "both_f32|both_f16|has_f32|has_f16" subset_julia_vm/src/compile/expr/binary/

# Verify apply_unary_float_op_with_heap is used in both static and dynamic paths
rg -n "apply_unary_float_op_with_heap" subset_julia_vm/src/vm/intrinsics_exec.rs subset_julia_vm/src/vm/exec/call_dynamic.rs

# Flag any unconditional Value::F64 push after math operations (type erasure risk)
rg -n "push\\(Value::F64\\(result\\)\\)" subset_julia_vm/src/vm/exec/binary_both.rs subset_julia_vm/src/vm/intrinsics_exec.rs
```

## Part 2: Integer Type Preservation (Issue #2278)

For same-type small integer arithmetic, the result must preserve the input type.

### Background

Integer arithmetic operations (+, -, *, %, div, ^) were returning Int64 for all integer types, even small integers like Int8 or UInt16. The root cause was that the compiler only had back-conversion instructions for float types (DynamicToF32, DynamicToF16), not for small integer types.

### Implementation

PR #2279 added back-conversion instructions for all small integer types:

| Type | Back-conversion Instruction | Back-conversion in conversion.rs |
|------|----------------------------|----------------------------------|
| Int8 | `DynamicToI8` | `to_i8()` |
| Int16 | `DynamicToI16` | `to_i16()` |
| Int32 | `DynamicToI32` | `to_i32()` |
| UInt8 | `DynamicToU8` | `to_u8()` |
| UInt16 | `DynamicToU16` | `to_u16()` |
| UInt32 | `DynamicToU32` | `to_u32()` |
| UInt64 | `DynamicToU64` | `to_u64()` |

### Compiler Detection

The `same_small_int_type()` helper in `binary.rs` detects when both operands have the same small integer type:

```rust
fn same_small_int_type(left: &ValueType, right: &ValueType) -> Option<ValueType> {
    match (left, right) {
        (ValueType::I8, ValueType::I8) => Some(ValueType::I8),
        (ValueType::I16, ValueType::I16) => Some(ValueType::I16),
        // ... all small integer pairs
        _ => None,
    }
}
```

After detecting a same-type operation, the compiler:
1. Computes the result using I64 intrinsics (wider type avoids overflow issues)
2. Emits a back-conversion instruction to restore the original type

### Mixed Primitive Numeric Promotion

Mixed concrete primitive pairs now preserve the promoted narrow result type
when that result is narrower than the default `Int64` / `Float64` path. For
example, `Int8 + Int16` returns `Int16`, `Int8 + UInt8` returns `UInt8`,
`Int64 + Float32` returns `Float32`, and `Float16 + Float32` returns `Float32`.

The compiler first asks `promote_numeric_value_types(...)` for the Julia
promotion result, emits the typed `I64` / `F64` arithmetic opcode for the
operation, then emits the matching `DynamicTo*` back-conversion instruction for
the promoted narrow result (Issue #3742, refined by Issue #5080).

### Julia's Integer Promotion Rules

| Left | Right | Result | Rule |
|------|-------|--------|------|
| Int8 | Int8 | Int8 | Same-type preservation |
| Int16 | Int16 | Int16 | Same-type preservation |
| Int32 | Int32 | Int32 | Same-type preservation |
| Int64 | Int64 | Int64 | Same-type preservation |
| UInt8 | UInt8 | UInt8 | Same-type preservation |
| UInt16 | UInt16 | UInt16 | Same-type preservation |
| UInt32 | UInt32 | UInt32 | Same-type preservation |
| UInt64 | UInt64 | UInt64 | Same-type preservation |
| Int8 | Int16 | Int16 | Wider signed wins |
| Int8 | Int32 | Int32 | Wider signed wins |
| UInt8 | UInt16 | UInt16 | Wider unsigned wins |
| Int8 | UInt8 | UInt8 | Same-width signed/unsigned promotes to unsigned |

Mixed-width and mixed-kind narrow pairs are covered by
`arithmetic/mixed_width_promotion.jl`.

### Checklist for Integer Type Preservation

When adding arithmetic support for new integer types:

- [ ] **Compiler** (`binary.rs`): Add the type pair to `same_small_int_type()`
- [ ] **Compiler** (`binary.rs`): Add back-conversion emission in `small_int_back_conversion()`
- [ ] **VM** (`instr.rs`): Add the `DynamicTo*` instruction variant
- [ ] **VM** (`conversion.rs`): Add the conversion handler
- [ ] **Tests**: Add same-type cases to `arithmetic_bit_width.jl` and mixed
      promotion cases to `mixed_width_promotion.jl`

### Fixture Tests

- `subset_julia_vm/tests/fixtures/type_inference/arithmetic_bit_width.jl` — same-type small integer arithmetic preservation (Issue #2278)
- `subset_julia_vm/tests/fixtures/arithmetic/mixed_width_promotion.jl` — mixed-width and mixed-kind narrow primitive promotion (Issue #3742)
- `subset_julia_vm/tests/fixtures/promotion/integer_type_combinations.jl` — mixed integer arithmetic/comparison smoke coverage

## Part 3: Wide-Primitive Type Preservation — the Four-Layer Model (Issue #3699)

Issue cluster #3621 / #3694 / #3696 / #3697 surfaced the same shape of bug for
every wide / narrow primitive that does not fit cleanly into the I64/F64
codegen lattice (Int128, UInt128, Float16, BigInt). Each bug was a type drift
through one specific layer. The mental model below names those layers
explicitly so a new primitive type can be vetted against all four at review
time.

```
Julia source: Int128(1) + Int128(2)
       │
       ▼
  [L1. Compile-time inference]  infer_expr_type / infer_julia_type
       │     "Int128" → ValueType::I128 (else Any → fall through)
       ▼
  [L2. Compile-time early-route]  compile/expr/binary/mod.rs
       │     I128 / U128 / F16 / BigInt early-route emits the right intrinsic
       ▼
  [L3. Pure Julia method-table]  src/julia/base/int.jl, float.jl
       │     div(::Int128, ::Int128), +(::UInt128, ::UInt128), …
       ▼
  [L4. Runtime fallback]  vm/intrinsics_exec.rs, vm/exec/binary_both.rs,
       │                  vm/dynamic_ops/{mod,helpers,dispatch}.rs
       ▼     pop_i64 / try_from must NOT truncate; every primitive needs an arm
  Result: Int128(3)
```

### Layer responsibility per primitive type

Reading order: a missing ✓ is a known type-drift hazard for that primitive on
that layer, generally tracked by an Issue.

| Primitive  | L1 inference | L2 early-route | L3 method-table | L4 runtime fallback |
|------------|--------------|----------------|-----------------|---------------------|
| Int8 / Int16 / Int32 | ✓ | via small-int back-conversion (PR #2279) | ✓ (`div`) — others promote through I64 intrinsic | ✓ |
| Int64      | ✓ | ✓ (default) | ✓ (full set: +, -, *, /, ==, <, <=, >, >=, div, …) | ✓ |
| Int128     | ✓ (Issue #3621) | ✓ (Issue #3621) | ✓ `div` (Issue #3694); `+`/`-`/`*`/cmp via L4 type-preserving intrinsics | ✓ (Issues #3621/#3694) |
| UInt8 / UInt16 / UInt32 | ✓ | via small-int back-conversion | ✓ `div` (Issue #3701); bitwise/shift (Issue #3565); others via L4 | ✓ |
| UInt64     | ✓ | ✓ | ✓ `div` (Issue #3701); bitwise/shift (Issue #3565) | ✓ |
| UInt128    | ✓ (Issue #3697) | ✓ (Issue #3697) | ✓ `div` (Issue #3696); bitwise/shift (Issue #3747) | ✓ (Issues #3696/#3697) |
| Float16    | ✓ (Issue #3621) | ✓ | partial — `+`, `-`, `*`, `/`, `÷` go through L4 | ✓ `dynamic_div` arm added Issue #3699; `dynamic_mod` Issue #1972 |
| Float32    | ✓ | ✓ | ✓ (`+`, `-`, `*`, `/`, `^`, comparisons in `float.jl`) | ✓ |
| Float64    | ✓ | ✓ (default) | ✓ (full set in `float.jl`) | ✓ |
| BigInt     | ✓ | ✓ (`AddBigInt`, …) | partial — most ops go through L4 | ✓ |

### How to read a new bug report against the matrix

Use the four-layer list as a triage rubric:

1. Does the failing case reach `infer_julia_type` with the right type? If
   not, the bug is **L1**.
2. Does the binary early-route in `compile/expr/binary/mod.rs` mention this
   operand type? If the early route routes elsewhere (e.g., BigInt for I128
   in the pre-#3621 world), the bug is **L2**.
3. Is there a `function op(x::T, y::T)` for this op × this width in
   `src/julia/base/int.jl` / `float.jl`? If not, generic fallbacks like
   `div(x, y) = floor(x / y)` widen through Float64 — that is **L3**.
4. Does the runtime helper (`Intrinsic::*Int`, `dynamic_*`, `binary_both`)
   carry an explicit arm for this primitive, or does it `pop_i64` /
   `try_from(... → i64)`? Missing arm → **L4**.

### Preventive checks

- **Fixture matrix** (Issue #3699):
  `subset_julia_vm/tests/fixtures/type_preservation/*_matrix.jl` — covers
  Int128, UInt128, Float16 across {+, -, *, /, ÷, %, ==, <, <=, >, >=} ×
  {same-type, mixed-with-Int64, mixed-with-Float64} ×
  {inline-from-constructor, variable-bound}. The inline / variable split is
  load-bearing because variable-bound type tracking often hid L1/L2 bugs
  (the recurring "variable form passes; inline form fails" pattern).
- **Audit script** (Issue #3699):
  `bash scripts/check_div_specializations.sh` — fails if any Int8/16/32/64/128
  or UInt8/16/32/64/128 width is missing a `function div(x::TN, y::TN)` in
  `subset_julia_vm/src/julia/base/int.jl`. The other operators called out in
  Issue #3699 are not audited at L3 because their type preservation is
  carried at L4 by intrinsic-level type preservation (PR #3565); the
  fixture matrix covers them at runtime.
- **Code-review checklist**: see `docs/vm/CHECKLISTS.md` "New Primitive Type
  / New Operator Method".

### Design note: Option A vs Option B (`WidePrimitiveOp`)

The deeper architectural problem is that runtime intrinsics like
`Intrinsic::AddInt`, `SubInt`, `MulInt`, `SdivInt`, `SremInt` are documented
as I64-specific but get used as if they were polymorphic. Two options for
addressing this systemically:

- **Option A (incremental, current state)**: keep extending each intrinsic
  with type-preserving arms (as #3694 / #3696 did for `SdivInt` and as
  Issue #3699 did for `dynamic_div` / Float16). Simple per-PR, but the
  pattern has to be repeated for every intrinsic × every primitive type and
  it is easy to forget one. The fixture matrix in Issue #3699 is the
  back-stop that surfaces these forgotten arms.

- **Option B (architectural, backlog)**: introduce a `WidePrimitiveOp`
  instruction that takes `(op, operand-types)` and dispatches inside the
  VM. The compiler would only emit it for I128 / U128 / F16 / BigInt;
  I64/F64 keep using the existing intrinsics. This collapses the Issue
  #3621 / #3694 / #3696 / #3697 / #3699 cluster into one place: all wide
  primitives get type-preserving dispatch from the same op-table.

Option A is sufficient as of 2026-05-10 — three #3699 PRs in a week have
closed the most-impactful gaps and the matrix now backs them. Option B is
in the backlog; the matrix tests are the trigger that decides whether the
incremental approach is sustainable or whether `WidePrimitiveOp` is justified.

## Related Documentation

- `TYPE_SYSTEM.md` — Type system architecture and ValueType variant checklist
- `NUMERIC_TYPES.md` — Numeric type parity checklist and intrinsic dispatch
- `BINARY_DISPATCH.md` — Binary operator dispatch paths and result type coverage
- `CHECKLISTS.md` — "New Primitive Type / New Operator Method" review checklist
- `CLAUDE.md` — Top-level contributor guidelines
