# Type Inference in SubsetJuliaVM

> **Archive note (2026-06-11):** This snapshot is kept for historical context.
> Current implementation guidance is in `../TYPE_INFERENCE_COMPLETE.md`; the
> older merged planning document is preserved as
> `TYPE_INFERENCE_COMPLETE_20260116.md`.

## Overview

SubsetJuliaVM uses a lattice-based abstract interpretation engine for type inference, similar to Julia's type inference system. This allows the compiler to infer precise types for variables, function return values, and expressions, enabling better optimizations and more accurate error detection.

## How Type Inference Works

Type inference in SubsetJuliaVM follows these principles:

1. **Abstract Interpretation**: The compiler simulates program execution using abstract types instead of concrete values
2. **Type Lattice**: Types are organized in a lattice hierarchy (Bottom → Const → Concrete → Union → Conditional → Top)
3. **Fixed-Point Iteration**: The inference engine iterates until type information stabilizes
4. **Transfer Functions**: Built-in functions have known return types based on their arguments
5. **Constant Propagation**: Constant values are tracked as `Const` types, which are more specific than `Concrete` types

## What Gets Inferred

### Variable Types

Variables are assigned types based on their usage:

```julia
function example()
    x = 1        # x: Int64
    y = 2.0      # y: Float64
    z = x + y    # z: Float64 (promotion)
    z
end
```

### Loop Variable Types

Loop variables are inferred from the iterator's element type:

```julia
function sum_array(arr)
    total = 0
    for x in arr  # x: Int64 (inferred from arr's element type)
        total += x
    end
    total
end
```

### Conditional Type Narrowing

Type checks like `isa` and `=== nothing` narrow types in conditional branches:

```julia
function process(val)
    if val isa Int
        val + 1  # val: Int64 (narrowed from Any/Union)
    elseif val isa Float64
        val * 2.0  # val: Float64 (narrowed)
    else
        0
    end
end
```

### Union Types

When different branches return different types, a Union type is inferred:

```julia
function mixed_return(flag)
    if flag
        1        # Int64
    else
        2.0      # Float64
    end
    # Return type: Union{Int64, Float64}
end
```

## Supported Type Patterns

### Arrays

```julia
arr = [1, 2, 3]  # Array{Int64}
x = arr[1]       # x: Int64 (element type inferred)
```

### Tuples

```julia
tup = (1, 2.0, "hello")  # Tuple{Int64, Float64, String}
for x in tup
    # x: Union{Int64, Float64, String}
end
```

### Ranges

```julia
for i in 1:10
    # i: Int64 (range element type)
end
```

### Dictionaries

```julia
dict = Dict("a" => 1, "b" => 2)  # Dict{String, Int64}
for (k, v) in dict
    # k: String, v: Int64
end
```

### Sets

```julia
s = Set([1, 2, 3])  # Set{Int64}
for x in s
    # x: Int64
end
```

## When Type Annotations Help

While type inference is powerful, explicit type annotations can help in some cases:

### Function Parameters

```julia
# Without annotation: parameter type is Any
function process(x)
    x + 1  # x: Any (may need runtime type check)
end

# With annotation: parameter type is known
function process(x::Int64)
    x + 1  # x: Int64 (no runtime check needed)
end
```

### Complex Return Types

```julia
# Inference may produce Union{Int64, Float64}
function maybe_number(flag)
    if flag
        1
    else
        2.0
    end
end

# Explicit annotation clarifies intent
function maybe_number(flag)::Union{Int64, Float64}
    if flag
        1
    else
        2.0
    end
end
```

## Common Pitfalls

### Type Widening

When too many different types are combined, the inference engine widens to `Any`:

```julia
# This may widen to Any if too many types are involved
function many_types(flag)
    if flag == 1
        1
    elseif flag == 2
        2.0
    elseif flag == 3
        "three"
    elseif flag == 4
        true
    else
        :symbol
    end
end
```

### Loop Variable Inference Limitations

Some complex iterator types may not have precise element type inference:

```julia
# May infer as Any if iterator type is unknown
for x in some_complex_iterator()
    # x: Any (if iterator type cannot be determined)
end
```

## Reading Type Inference Warnings

The compiler may emit warnings when:

- Type inference produces `Any` (loss of precision)
- Union types exceed complexity limits (widening occurs)
- Conditional narrowing cannot be applied

These warnings help identify code that may benefit from type annotations.

## Performance Impact

Type inference runs during compilation and does not affect runtime performance. In fact, better type inference enables:

- More aggressive optimizations
- Fewer runtime type checks
- Better code generation

## Type Lattice Hierarchy

The type inference system uses a lattice hierarchy:

- **Top** (Any) - Most general type, accepts any value
- **Conditional** - Control-flow sensitive types (defined but uses environment splitting in practice)
- **Union** - Union of multiple concrete types
- **Concrete** - Specific types like Int64, Float64, String, etc.
- **Const** - Constant values known at compile time (e.g., Const(42), Const(true))
- **Bottom** - Most specific type, represents unreachable code

The `Const` type is more specific than `Concrete` and enables constant propagation optimizations.

## Related Documentation

**統合ドキュメント**: [TYPE_INFERENCE_COMPLETE.md](../TYPE_INFERENCE_COMPLETE.md) - 全 TYPE_INFERENCE 系ドキュメントを統合した完全版

**個別ドキュメント**（参照用）:
- [TYPE_INFERENCE_ENHANCEMENT.md](./TYPE_INFERENCE_ENHANCEMENT.md) - Design document
- [TYPE_INFERENCE_IMPLEMENTATION_GUIDE.md](./TYPE_INFERENCE_IMPLEMENTATION_GUIDE.md) - Implementation details
- [TYPE_INFERENCE_STATUS.md](./TYPE_INFERENCE_STATUS.md) - Current implementation status
- [TYPE_INFERENCE_IMPLEMENTATION_STATUS.md](./TYPE_INFERENCE_IMPLEMENTATION_STATUS.md) - Detailed implementation status
