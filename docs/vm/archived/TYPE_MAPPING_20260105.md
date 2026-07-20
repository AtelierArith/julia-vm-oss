# Type Mapping in SubsetJuliaVM

> **Archive note (2026-06-11):** This older type-mapping guide is preserved as
> historical context. It predates the current `Value` carrier layout, the
> `Value::NativeArray(ArrayRef)` compatibility boundary, and the
> `vm/dynamic_ops/` module split. Use `docs/vm/TYPE_SYSTEM.md` and
> `docs/vm/LATTICE_TYPE.md` for current type representations.

This document describes the type mapping system in SubsetJuliaVM, including how Julia types are represented at compile time and runtime.

## Overview

SubsetJuliaVM uses three levels of type representation:

1. **JuliaType** - Julia's type system representation (compile-time)
2. **ValueType** - VM's compile-time type (used for code generation)
3. **Value** - Runtime value representation

```
JuliaType → ValueType → Value
(Source)    (Compile)   (Runtime)
```

## Type Mapping Table

### Numeric Types

| JuliaType | ValueType | Value | Notes |
|-----------|-----------|-------|-------|
| Int8, Int16, Int32, Int64, Int128 | I64 | I64(i64) | All signed integers map to I64 |
| UInt8, UInt16, UInt32, UInt64, UInt128 | I64 | I64(i64) / U8/U16/U32/U64 | Unsigned integers |
| Bool | I64 | Bool(bool) | **Special case**: Bool <: Integer in Julia |
| Float16, Float32, Float64 | F64 | F64(f64) / F32(f32) / F16 | Floating point types |
| BigInt | BigInt | BigInt | Arbitrary precision integer |
| BigFloat | BigFloat | BigFloat | Arbitrary precision float |

### String and Character Types

| JuliaType | ValueType | Value |
|-----------|-----------|-------|
| String | Str | Str(String) |
| Char | Char | Char(char) |

### Collection Types

| JuliaType | ValueType | Value |
|-----------|-----------|-------|
| Array, Vector, Matrix | Array | Array(ArrayRef) |
| Tuple | Tuple | Tuple(TupleValue) |
| NamedTuple | NamedTuple | NamedTuple |
| Dict | Dict | Dict(DictValue) |
| Set | Set | Set(SetValue) |
| UnitRange, StepRange | Range | Range(RangeValue) |

### Special Types

| JuliaType | ValueType | Value |
|-----------|-----------|-------|
| Nothing | Nothing | Nothing |
| Missing | Missing | Missing |
| Any | Any | (varies) |
| Function | Function | Function |
| Struct(name) | Struct(type_id) | StructRef(idx) |

## Bool Type: Special Handling

In Julia's type hierarchy, `Bool` is a subtype of `Integer`:

```
Bool <: Integer <: Real <: Number <: Any
```

This means Bool values can be used in numeric contexts:
- `true + 1 == 2`
- `false * 10 == 0`
- `-true == -1`

### Implementation Details

1. **Compile Time**: `JuliaType::Bool` maps to `ValueType::I64` because Bool is an Integer subtype
2. **Runtime**: Bool struct fields store `Value::Bool` to preserve type information
3. **Stack Operations**: `pop_i64()` accepts both `Value::I64` and `Value::Bool`

This design was refined in Issue #1612 / PR #1620 to ensure Bool struct fields work correctly.

### Code Example

```rust
// In stack_ops.rs
fn pop_i64(&mut self) -> Result<i64, VmError> {
    match self.pop().ok_or(VmError::StackUnderflow)? {
        Value::I64(v) => Ok(v),
        // Bool is a subtype of Integer in Julia, so accept it as i64
        Value::Bool(v) => Ok(if v { 1 } else { 0 }),
        other => Err(VmError::TypeError(...)),
    }
}
```

## Related Files

- `subset_julia_vm_compile/src/compile/type_helpers.rs` - `julia_type_to_value_type()` function
- `subset_julia_vm_vm/src/vm/stack_ops.rs` - Stack pop operations with type handling
- `subset_julia_vm_vm/src/vm/value/` - Runtime Value enum definition
- `subset_julia_vm/src/types/` - JuliaType enum definition

## Float32 Arithmetic Support

When adding new numeric types (like Float32), ensure all arithmetic operations are supported in `dynamic_ops.rs`:

### Required Operations

For each new numeric type `T`, add handlers for:
- `T + T`, `T - T`, `T * T`, `T / T` (same-type operations)
- `T + I64`, `I64 + T` (integer promotion)
- `T + F64`, `F64 + T` (float promotion - result is F64)
- `T + Bool`, `Bool + T` (boolean promotion)

### Type Promotion Rules

Following Julia semantics:
- `Float32 + Float32 → Float32`
- `Float32 + Int64 → Float32`
- `Float32 + Float64 → Float64` (promotes to wider type)
- `Float32 + Bool → Float32`

### Code Example

```rust
// In dynamic_ops.rs - dynamic_mul
match (a, b) {
    // ... existing cases ...
    // Float32 operations
    (Value::F32(x), Value::F32(y)) => Ok(Value::F32(x * y)),
    (Value::F32(x), Value::I64(y)) => Ok(Value::F32(x * *y as f32)),
    (Value::I64(x), Value::F32(y)) => Ok(Value::F32(*x as f32 * y)),
    // F32 <-> F64 mixed operations promote to F64
    (Value::F32(x), Value::F64(y)) => Ok(Value::F64(*x as f64 * y)),
    (Value::F64(x), Value::F32(y)) => Ok(Value::F64(x * *y as f64)),
    // ...
}
```

This pattern was established in Issue #1625 / PR #1628.

## Indexable Types

When implementing custom indexable types (like SubArray), ensure both read and write operations are supported.

### Requirements Checklist

1. **Julia Side**:
   - Define `Base.getindex(collection, indices...)` for reading
   - Define `Base.setindex!(collection, value, indices...)` for writing

2. **VM Side** (`subset_julia_vm_vm/src/vm/exec/array_index.rs`):
   - Handle type in `IndexLoad` instruction for reading
   - Handle type in `IndexStore` instruction for writing
   - Ensure stack contract: collection must remain on stack after `IndexStore`

3. **Tests**:
   - Add read test: `@test collection[i] == expected`
   - Add write test: `collection[i] = value; @test collection[i] == value`
   - **Never skip write tests** without a linked GitHub Issue

### Stack Contract for IndexStore

Julia's `setindex!` returns the stored value, but the VM's `IndexStore` expects the collection to remain on stack after modification. This semantic difference requires special handling:

```rust
// In array_index.rs - handle_index_store
// After calling setindex!, push the collection back onto the stack
self.stack.push(collection_value);
```

### Code Example

```rust
// In array_index.rs
Instr::IndexStore => {
    // Stack: [..., collection, indices..., value]
    let value = self.stack.pop_value()?;
    let indices = /* pop indices */;
    let collection = self.stack.pop_value()?;

    match &collection {
        Value::Array(arr) => {
            // Handle array setindex!
        }
        Value::StructRef(idx) => {
            // Check if struct supports setindex!
            // Dispatch to Julia's setindex! method
        }
        // ... other indexable types
    }

    // Push collection back for chained operations
    self.stack.push(collection);
    Ok(())
}
```

This pattern was established in Issue #1613 / PR #1616.

## Adding New Numeric Types Checklist (Issue #1649)

When adding a new numeric type `T` to SubsetJuliaVM, follow this comprehensive checklist to prevent missing implementations:

### 1. Runtime Operations (`dynamic_ops.rs`)

Add handlers for all 4 arithmetic operations in all type combinations:

```rust
// Same-type operations
(Value::T(x), Value::T(y)) => Ok(Value::T(x op y))

// Cross-type with I64
(Value::T(x), Value::I64(y)) => Ok(Value::T(x op *y as T_inner))
(Value::I64(x), Value::T(y)) => Ok(Value::T(*x as T_inner op y))

// Cross-type with F64 (promotes to F64)
(Value::T(x), Value::F64(y)) => Ok(Value::F64(*x as f64 op y))
(Value::F64(x), Value::T(y)) => Ok(Value::F64(x op *y as f64))

// Cross-type with Bool
(Value::T(x), Value::Bool(y)) => Ok(Value::T(x op if *y { 1 } else { 0 }))
(Value::Bool(x), Value::T(y)) => Ok(Value::T(if *x { 1 } else { 0 } op y))
```

### 2. Compile-time Type Conversions (`compile/expr/mod.rs`)

Add conversion cases in `compile_expr_as` for:

- `(ValueType::Bool, ValueType::NewType)` - Bool to new type
- `(ValueType::I64, ValueType::NewType)` - Int64 to new type
- `(ValueType::F64, ValueType::NewType)` - Float64 to new type (if applicable)
- `(ValueType::NewType, ValueType::F64)` - New type to Float64

### 3. Test Coverage Template

Add fixture tests that verify all operations:

```julia
# File: tests/fixtures/promotion/<type>_arithmetic_comprehensive.jl

using Test

@testset "T Comprehensive Arithmetic" begin
    @testset "Same-type arithmetic" begin
        @test T(2.5) + T(1.5) == T(4.0)
        @test T(2.5) - T(1.5) == T(1.0)
        @test T(2.5) * T(1.5) == T(3.75)
        @test T(5.0) / T(2.0) == T(2.5)
        @test typeof(T(2.5) + T(1.5)) == T
    end

    @testset "Cross-type with Int64" begin
        @test T(2.5) + 3 == T(5.5)
        @test 3 + T(2.5) == T(5.5)
        @test typeof(T(2.5) + 3) == T
    end

    @testset "Cross-type with Float64 (promotion)" begin
        @test T(2.5) + 1.5 == 4.0
        @test typeof(T(2.5) + 1.5) == Float64
    end

    @testset "Cross-type with Bool" begin
        @test T(2.5) + true == T(3.5)
        @test typeof(T(2.5) + true) == T
    end

    @testset "Type conversions" begin
        @test Float64(T(2.5)) == 2.5
        @test Int64(T(3.0)) == 3
    end
end

true
```

### 4. Code Review Checklist

When reviewing PRs that add new numeric types, verify:

- [ ] All 4 arithmetic operations (+, -, *, /) implemented in `dynamic_ops.rs`
- [ ] Same-type operations preserve the type
- [ ] Cross-type with Int64 returns the new type
- [ ] Cross-type with Float64 promotes to Float64
- [ ] Cross-type with Bool works correctly
- [ ] Compile-time type conversions in `compile_expr_as`
- [ ] Comprehensive fixture tests added
- [ ] Tests verify result types with `typeof()`

### 5. Files to Modify

| Location | Purpose |
|----------|---------|
| `subset_julia_vm_vm/src/vm/dynamic_ops/` | Runtime arithmetic operations |
| `subset_julia_vm_compile/src/compile/expr/mod.rs` | Compile-time type conversions |
| `subset_julia_vm_vm/src/vm/value/` | Value enum (if adding new variant) |
| `subset_julia_vm/src/types/` | JuliaType/ValueType (if needed) |
| `tests/fixtures/promotion/*.jl` | Comprehensive test coverage |

## Common Pitfalls

1. **Bool/I64 Mismatch**: When adding new `pop_*` functions, remember that Bool values may appear where integers are expected
2. **Struct Field Types**: Runtime struct fields preserve the actual Value type, not the mapped ValueType
3. **Type Coercion**: Use explicit coercion instructions (e.g., `BoolToI64`) when type conversion is needed at runtime
4. **Incomplete Arithmetic**: When adding new numeric types, ensure ALL arithmetic operations (+, -, *, /) are implemented for all type combinations
5. **Asymmetric Indexing**: When adding indexable types, ensure BOTH `IndexLoad` and `IndexStore` are handled - not just one
6. **Missing Type Combinations**: Don't forget Bool cross-type operations - Bool is a subtype of Integer in Julia

## Float32 Mixed-Type Arithmetic Prevention (Issue #1661)

### Background

Issue #1659 revealed that Float32 mixed-type arithmetic (Float32 + Int64, Float32 + Float64, Float32 + Bool) can fail in complex test files with multiple nested `@testset` blocks, even when the same operations work in isolation.

### Root Cause Analysis

The failure occurs due to:
1. Method dispatch cache state being affected by multiple type combinations in a single file
2. Type inference state propagation issues with complex file structure
3. Dynamic dispatch in `dynamic_ops.rs` not being reached in some scenarios

### Prevention Strategies

#### 1. Separate Test Files by Type Combination

Instead of testing all Float32 combinations in one file, split them:

```bash
tests/fixtures/promotion/
├── float32_same_type.jl        # Float32 + Float32 only
├── float32_int64.jl            # Float32 + Int64 combinations
├── float32_float64.jl          # Float32 + Float64 (promotion)
└── float32_bool.jl             # Float32 + Bool combinations
```

#### 2. Test Template for Type Combinations

When adding tests for a new numeric type, use this template:

```julia
# File: tests/fixtures/promotion/<type>_<other_type>.jl
# Tests <Type> × <OtherType> arithmetic ONLY

using Test

@testset "<Type> + <OtherType> arithmetic" begin
    # Same-type operations
    @test T(2.5) + T(1.5) == T(4.0)
    @test typeof(T(2.5) + T(1.5)) == T

    # Cross-type operations (if applicable)
    @test T(2.5) + other(3) == expected
    @test typeof(T(2.5) + other(3)) == ResultType
end

true
```

#### 3. Debug Logging for Method Dispatch

When debugging Float32 dispatch issues, enable tracing:

```rust
// In dynamic_ops.rs
#[cfg(debug_assertions)]
eprintln!("dynamic_add dispatch: {:?} + {:?}", a.type_name(), b.type_name());
```

#### 4. Code Review Checklist for Float32 Changes

When modifying Float32 arithmetic:

- [ ] Test same-type operations in isolation
- [ ] Test each cross-type combination in separate files
- [ ] Verify `dynamic_ops.rs` handlers are reachable
- [ ] Check Julia method dispatch in `float.jl`
- [ ] Run tests with and without other type combinations in the file

### Known Issues

| Issue | Status | Description |
|-------|--------|-------------|
| #1659 | Open | Float32 mixed-type fails in complex test files |
| #1649 | Fixed | Float32 arithmetic type preservation |

### Related Files

| File | Purpose |
|------|---------|
| `subset_julia_vm_vm/src/vm/dynamic_ops/` | Runtime arithmetic dispatch |
| `subset_julia_vm/src/julia/base/float.jl` | Float32 operator methods |
| `tests/fixtures/promotion/` | Type promotion tests |
