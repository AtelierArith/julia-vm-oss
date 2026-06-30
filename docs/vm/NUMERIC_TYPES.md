# Numeric Type Parity & Intrinsic Dispatch

This document covers numeric type implementation details: parity checklists, intrinsic dispatch completeness, and UInt/Int mixed-type dispatch.

## Numeric Type Parity Checklist (Issue #1856)

When extending support for one numeric type (e.g., Float32), **all similar numeric types** (Float16, Float64) must be checked for the same support. This prevents parity gap bugs where one type works but another silently fails.

### Compiler Locations

When adding support for a new numeric type or extending an existing one, verify ALL of these:

- [ ] `compile/expr/infer/` - Binary op type inference chain (F64 > F32 > F16 > I64 priority)
- [ ] `compile/expr/mod.rs` - Type conversion paths in `compile_expr_as` (T->I64, T->F64, T->F32, T->Any, Union->T)
- [ ] `compile/expr/binary/` - `result_ty`, `operand_ty`, and back-conversion (`DynamicToF32`/`DynamicToF16`) in BOTH `compile_binary_op` (`mod.rs`) AND `compile_builtin_binary_op` (`builtin.rs`)
- [ ] `compile/expr/builtin.rs` - Type constructor delegation list
- [ ] `compile/expr/builtin_types.rs` - Constructor compilation
- [ ] `builtins.rs` - `BuiltinId` enum + `from_name` + `name`

### VM Runtime Locations

- [ ] `vm/exec/binary_both.rs` - `left_is_primitive` / `right_is_primitive` check and runtime mixed-type dispatch paths
- [ ] `vm/dynamic_ops/` - Dynamic numeric operation helpers for generic VM fallback paths
- [ ] `vm/builtins_numeric.rs` - Builtin execution handler
- [ ] `vm/exec/conversion.rs` - `DynamicToT` / `ToF64` / `ToI64` conversion instructions (Issue #2218)
- [ ] `vm/convert.rs` - `convert_value()` dispatch + per-type converter functions (Issue #2267)
- [ ] `vm/stack_ops.rs` - `pop_f64_or_i64()` numeric value extraction (Issue #2218)
- [ ] `vm/exec/array_index.rs` - `IndexStore` typed value conversion for `ArrayData` variants and boxed numeric targets (Issue #2218)

### Intrinsic Operation Parity

When adding an intrinsic to one mixed-type dispatch path, check that **related intrinsics** are also present:

| Category | Operations to check together |
|----------|------------------------------|
| Arithmetic | `AddFloat`, `SubFloat`, `MulFloat`, `DivFloat` |
| Integer Division | `SdivInt`, `SremInt` |
| Comparison | `EqFloat`, `NeFloat`, `LtFloat`, `LeFloat`, `GtFloat`, `GeFloat` |

### Quick Audit Commands

```bash
# Check primitive type coverage in dispatch paths
rg -n 'left_is_primitive|right_is_primitive' subset_julia_vm/src/vm/exec/

# Check F16/F32 parity in compiler
rg -c 'F16|Float16' subset_julia_vm/src/compile/expr/binary/ subset_julia_vm/src/compile/expr/mod.rs
rg -c 'F32|Float32' subset_julia_vm/src/compile/expr/binary/ subset_julia_vm/src/compile/expr/mod.rs
```

## Intrinsic Dispatch Completeness (Issue #1778)

When adding new intrinsics or new type combinations to `vm/exec/binary_both.rs`,
ensure ALL runtime dispatch branches are covered:

### Dispatch Branches

1. **`both_int`**: Both operands are I64 - use integer intrinsics (AddInt, SubInt, etc.)
2. **`both_f32`**: Both operands are F32 - use F32 intrinsics
3. **`has_f32 && has_f64`**: F32-F64 mixed - promote to F64
4. **`has_f32 && !both_f32`** (else): F32-I64 mixed - return F32 result
5. **Default**: F64-I64 and other combinations

### Required Intrinsics per Branch

When adding mixed-type support for Float32, include ALL these intrinsics:
- Arithmetic: `AddFloat`, `SubFloat`, `MulFloat`, `DivFloat`
- Comparison: `EqFloat`, `NeFloat`, `LtFloat`, `LeFloat`, `GtFloat`, `GeFloat`
- Integer ops: `SremInt` (mod/rem), `SdivInt` (div) - use fmod/float semantics for mixed types

### Example: SremInt in F32-I64 path (Issue #1776)

The `circshift` function uses `mod(k, n)` which triggers `SremInt`. When missing from F32-I64 path, it causes: `MethodError: unsupported Float32-Int64 operation: SremInt`

Fix: Add `SremInt` handling with fmod semantics:
```rust
Intrinsic::SremInt => {
    let result = a - (a / b).floor() * b;
    Value::F32(result)
}
```

### Checklist for Adding Intrinsics

- [ ] Add handling in `both_int` path (if applicable)
- [ ] Add handling in `both_f32` path
- [ ] Add handling in F32-F64 mixed path
- [ ] Add handling in F32-I64 mixed path
- [ ] Add handling in default path
- [ ] Add fixture test in `subset_julia_vm/tests/fixtures/` for the new operation

## UInt/Int Mixed-Type Dispatch (Issue #1853)

Unsigned integer types (UInt8, UInt16, UInt32, UInt64, UInt128) can be compared and used in arithmetic with signed integer types (Int64, Int32, etc.) via:

1. **Promotion rules** in `promotion.jl`: `promote_rule(::Type{Int64}, ::Type{UInt8}) = Int64` etc.
2. **Primitive fallback dispatch** in `binary_both.rs`: Small integer types are converted to I64 for intrinsic operations.

### Related Support Notes

- **Bitwise operators** (`>>`, `<<`, `>>>`, `&`, `|`, `xor`/`⊻`) are now supported in Julia source code lowering (Issue #2618). They are lowered to function calls and dispatched through the Julia method table.
- **Hex/binary/octal literals** preserve Julia's typed-width rules (Issue #3559):
  `0x01` and `0b1` are `UInt8`, `0x0001` and `0o400` are `UInt16`, and
  wider forms produce the corresponding `UInt32`/`UInt64` widths. Ranges built
  from these literals preserve the same element type at runtime.

## Numeric Coercion Match Site Inventory (Issue #2286)

All of the following locations contain `match` statements on `Value` variants that perform numeric type coercion. When adding a new numeric `Value` variant, **every site** must be updated.

### Stack operations (`vm/stack_ops.rs`)

| Function | Purpose | Must handle all numeric types? |
|----------|---------|-------------------------------|
| `pop_numeric_as_f64()` | Pop and convert to f64 | Yes — all integer + float + Bool |
| `pop_f64_or_i64()` | Pop numeric with Rational/BigInt support | Yes — all integer + float + Bool |
| `pop_usize()` | Pop as array index | Yes — all integer types (non-negative) |

### Static conversion instructions (`vm/exec/conversion.rs`)

| Instruction | Purpose | Must handle all numeric types? |
|-------------|---------|-------------------------------|
| `ToF64` | Static conversion to f64 | Yes — all integer + float + Bool + BigInt + Rational |
| `ToI64` | Static conversion to i64 | Yes — all integer + float + Bool + Char |
| `DynamicToF64` | Dynamic conversion to f64 | Already complete (reference implementation) |
| `DynamicToI64` | Dynamic conversion to i64 | Already complete (reference implementation) |
| `DynamicToF32`/`DynamicToF16` | Dynamic float conversion | Already complete |

### Array operations (`vm/exec/array_index.rs`)

| Location | Purpose | Notes |
|----------|---------|-------|
| `IndexStore` typed_val | Convert f64 to typed array element | Covers dedicated `ArrayData` variants; I128/U128 element tags use boxed `ArrayData::Any` storage |

### Centralized conversion (`vm/convert.rs`)

| Function | Purpose | Notes |
|----------|---------|-------|
| `convert_value()` dispatch | Route `convert(T, x)` to per-type handler | Uses `define_integer_converter!` macro for all 10 integer types |

### Quick audit command

```bash
# Count I128/U128 handling across coercion sites
rg -c 'I128|U128' subset_julia_vm/src/vm/stack_ops.rs subset_julia_vm/src/vm/exec/conversion.rs subset_julia_vm/src/vm/convert.rs
```

## Integer Type Constructor Semantics (Issue #3063)

Julia raises `InexactError` when an explicit integer type constructor receives an out-of-range value:

```julia
UInt8(-1)    # InexactError: Truncation(-1) in convert(UInt8, -1)
UInt8(256)   # InexactError: Truncation(256) in convert(UInt8, 256)
Int8(128)    # InexactError: Truncation(128) in convert(Int8, 128)
```

### Two Distinct Conversion Paths

SubsetJuliaVM has **two separate paths** for integer type conversion:

| Path | Location | Semantics | Example use |
|------|----------|-----------|-------------|
| **Type constructor** (`BuiltinId::UInt8` etc.) | `vm/type_ops/conversion.rs` → `convert_to_u8()` | Range-checked; raises `InexactError` on overflow | `UInt8(x)` in Julia source |
| **Arithmetic back-conversion** (`DynamicToU8` etc.) | `vm/exec/conversion.rs` | Intentional truncation via `as u8`; used after arithmetic | Return type narrowing |

### Implementation Rule

All `convert_to_*` functions in `vm/type_ops/conversion.rs` **MUST** use `try_from()` for integer-to-integer conversions:

```rust
// CORRECT: range-checked, raises InexactError on overflow
Value::I64(n) => {
    u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
}

// WRONG: silently wraps (-1i64 as u8 = 255)
Value::I64(n) => Ok(*n as u8)
```

Only smaller-to-larger conversions (guaranteed to fit) may use `as` casting:
```rust
// OK: i8 always fits in i16 (widening, no truncation)
Value::I8(n) => Ok(*n as i16),
```

### Code Review Checklist

When adding a new `convert_to_*` function or modifying an existing one:
- [ ] Does the function use `try_from()` for all cases where the source type may not fit in the target?
- [ ] Is each widening-only `as` cast truly guaranteed not to truncate?
- [ ] Are there unit tests in `vm/type_ops/conversion.rs` covering boundary values (min-1, min, max, max+1)?

## Mixed-type operators & the promote-fallback trap (Issues #5966, #5969)

When adding or extending a numeric type, always mirror upstream's **mixed-type**
operator methods (`Complex×Real`, `Rational×Integer`, `Irrational×Real`, …), not just
the same-type ones. A mixed pair with no specific method falls into the generic
`op(::Number, ::Number) = op(promote(x, y)...)` fallback, which **infinite-recurses** if
`promote` cannot widen the pair (Issue #5966). The VM call-depth guard
(`Vm::MAX_CALL_DEPTH`, Issue #5969) is a fail-fast backstop, not a substitute. Full rules
and the recursion mechanism: see **PROMOTION.md → "Promote-fallback termination & the
call-depth guard"**.

## Related Documentation

- `PROMOTION.md` - Promotion system, mixed-type promote-fallback termination, call-depth guard
- `TYPE_SYSTEM.md` - Type system architecture and ValueType variant checklist
- `BINARY_DISPATCH.md` - Binary operator dispatch paths and result type coverage
- `CLAUDE.md` - Top-level contributor guidelines
