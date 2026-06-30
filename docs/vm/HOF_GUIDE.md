# Higher-Order Function (HOF) Implementation Guide

*Last updated: 2026-06-11*

This document covers HOF implementation patterns, the split between Pure Julia and Rust builtin HOFs, operator-as-argument support, and testing checklists.

## Operators as Arguments (Issue #1985 -- RESOLVED)

Bare operators (`+`, `-`, `*`, `/`, etc.) can be passed as function arguments to higher-order functions. The lowering stage converts operator tokens in argument position to `FunctionRef` nodes.

```julia
# All of these work:
reduce(+, [1, 2, 3, 4])        # 10
reduce(*, [1, 2, 3, 4])        # 24
accumulate(+, [1, 2, 3, 4])    # [1, 3, 6, 10]
foldl(-, [1, 2, 3])            # -4

# Lambda wrappers and named functions also work:
reduce((a, b) -> a + b, [1, 2, 3, 4])
myadd(a, b) = a + b
reduce(myadd, [1, 2, 3, 4])
```

**Known remaining limitation:** Assigning an unparenthesized bare operator to a variable (`f = +`) requires parser support for operators as standalone right-hand-side expressions, which is not yet implemented. This does not apply to call-argument position: HOF and collection calls such as `reduce(+, xs)` and `mergewith(+, a, b)` are supported through the `FunctionRef` lowering path.

## HOF Implementation Split: Pure Julia vs Rust Builtins

HOFs in SubsetJuliaVM are split between Pure Julia implementations (in
`base/iterators.jl` and other `.jl` files), compiler fallthrough decisions in
`compile/expr/builtin_hof.rs`, and a small set of retained Rust VM instructions
for operations that still need a runtime boundary or cache compatibility.

### Pure Julia / Method-Dispatch HOFs

These are implemented in Julia and reached by ordinary method dispatch. Many
public names explicitly return `Ok(None)` from `compile_builtin_hof()`, causing
fallthrough to the method table; names not matched by `compile_builtin_hof()`
also dispatch normally.

| HOF | Location | Implementation Pattern |
|-----|----------|----------------------|
| `map(f, A)` | `iterators.jl` | `collect(Generator(f, A))` |
| `map(f, A, B)` | `iterators.jl` | Iterate `zip(A, B)`, apply `f(pair[1], pair[2])` |
| `map(f, x::Number)` | `number.jl` | Scalar map: `f(x)` (multiple dispatch, 1-4 args) |
| `map(f, s::String)` | `strings/basic.jl` | IOBuffer-based, returns `String` |
| `filter(f, A)` | `iterators.jl` | `collect(Filter(f, A))` |
| `filter(f, s::String)` | `strings/basic.jl` | IOBuffer-based, returns `String` |
| `reduce(op, itr)` | `iterators.jl` | iterate-based left fold |
| `reduce(op, itr, init)` | `iterators.jl` | iterate-based left fold with init |
| `foldl(op, itr)` | `iterators.jl` | Alias for `reduce` |
| `foldl(op, itr, init)` | `iterators.jl` | Alias for `reduce` with init |
| `foldr(op, itr)` | `iterators.jl` | Collect then right-fold |
| `foldr(op, itr, init)` | `iterators.jl` | Collect then right-fold with init |
| `mapfoldl(f, op, itr)` | `iterators.jl` | Map-then-left-fold |
| `mapfoldl(f, op, itr, init)` | `iterators.jl` | Map-then-left-fold with init |
| `mapfoldr(f, op, itr)` | `iterators.jl` | Map-then-right-fold (collect first) |
| `mapfoldr(f, op, itr, init)` | `iterators.jl` | Map-then-right-fold with init |
| `mapreduce(f, op, itr)` | `iterators.jl` | Alias for `mapfoldl` |
| `mapreduce(f, op, itr, init)` | `iterators.jl` | Alias for `mapfoldl` with init |
| `foreach(f, itr)` | `abstractarray.jl` | `for x in itr; f(x); end` |
| `foreach(f, itr1, itr2)` | `abstractarray.jl` | `for (x,y) in zip(itr1,itr2); f(x,y); end` |
| `broadcast(f, args...)` | `broadcast.jl` | Pure Julia broadcast infrastructure |
| `accumulate(op, A)` | `accumulate.jl` | iterate-based cumulative operation |
| `accumulate(op, A, init)` | `accumulate.jl` | With initial value |

### Rust Builtin HOFs (compiled to VM instructions)

These are compiled directly to specialized VM instructions for performance or because they require compile-time function resolution:

| HOF | Instruction | Return Type | Notes |
|-----|-------------|-------------|-------|
| `ntuple(f, n)` | `NtupleFunc` / `NtupleRuntime` | `Tuple` | Static callables use `NtupleFunc`; runtime callables use `NtupleRuntime` |
| `compose(f, g)` | `CallBuiltin(Compose, 2)` | `Function` | Function composition |
| `sprint(f, args...)` | `SprintFunc` | `String` | 1-arg `sprint(x)` uses IOBuffer; the Pure Julia `sprint(f, args...)` does not invoke `f(io, args...)` because the VM cannot pass IOBuffer to user functions yet, so the Rust builtin is retained (documented blocker, Issue #3731) |

### Migrated HOFs that previously had Rust priority

After Issues #3731 and #3728 the following HOFs always dispatch to Pure Julia methods. The corresponding VM instructions remain in the codebase as cache-compatibility fallbacks but are no longer emitted by new IR — `compile_builtin_hof` returns `Ok(None)` for the public names so method dispatch wins.

| HOF | Pure Julia file | Issue | Retained VM instruction |
|-----|----------------|-------|------------------------|
| `map!(f, a::Array)`              | `base/array.jl` | #3731 | `MapFuncInPlace` |
| `map!(f, dest::Array, src::Array)` | `base/array.jl` | #3731 | `MapFuncInPlace` |
| `filter!(f, a::Array)`           | `base/array.jl` | #3731 | `FilterFuncInPlace` |
| `mapfoldl(f, op, itr [, init])`  | `base/iterators.jl` | #3731 | `MapReduceFunc`, `MapReduceFuncWithInit` |
| `mapfoldr(f, op, itr [, init])`  | `base/iterators.jl` | #3731 | `MapFoldrFunc`, `MapFoldrFuncWithInit` |
| `mapreduce(f, op, itr [, init])` | `base/iterators.jl` | #3731 | `MapReduceFunc(WithInit)` |
| `findfirst(f::Function, ::Array)` | `base/array.jl` | #3728 | `FindFirstFunc` |
| `findlast(f::Function, ::Array)`  | `base/array.jl` | #3728 | `FindLastFunc` |
| `findall(f::Function, ::Array)`   | `base/reduce.jl` | #3728 | `FindAllFunc` |
| `any(f::Function, arr)`           | `base/reduce.jl` | #3728 | `AnyFunc` |
| `all(f::Function, arr)`           | `base/reduce.jl` | #3728 | `AllFunc` |
| `count(f::Function, ::Array)`     | `base/reduce.jl` | #3728 | `CountFunc` |
| `sum(f::Function, ::Array)`       | `base/reduce.jl` | #3728 | `SumFunc` |

The keyword-argument form (`mapreduce(f, op, arr; init=val)` etc.) is rewritten
to positional form at compile time and then dispatched normally. See the kwargs
handling block in `compile/expr/call/mod.rs`.

### Current Routing Summary

After Issues #3731 and #3728, most public HOF names route through Pure Julia
methods. `sprint` is the remaining public mixed path in this guide; migrated VM
instructions are retained as compatibility fallbacks but are not emitted for
new IR.

| Function | Pure Julia path | Rust builtin path |
|----------|----------------|-------------------|
| `map` | `map(f, A)`, `map(f, A, B)`, `map(f, s::String)` | -- |
| `filter` | `filter(f, A)`, `filter(f, s::String)` | -- |
| `findall` | All `(::Function, ::Array)` (Issue #3728) and `(::String, ::String)` / `(::Char, ::String)` (Issue #3726) forms | -- |
| `count` | Predicate forms for `Array` (Issue #3728), `Tuple` (Issue #5681), `AbstractRange`, `Memory`, and `String` (#2081), plus `(::String, ::String)` / `(::Char, ::String)` search forms (Issue #3726) | -- |
| `sprint` | `sprint(x)` (1-arg, `io.jl`) | `sprint(f, args...)` (builtin — invokes `f(io, args...)` which the Pure Julia version cannot yet do) |

### Keyword Argument Handling

`reduce`, `foldl`, `foldr`, `mapfoldl`, `mapfoldr`, and `mapreduce` support
`init` as a keyword argument. The compiler (`compile/expr/call/mod.rs`) converts
keyword argument forms to positional argument forms at compile time (Issues
#2077, #2084):

```julia
reduce(+, [1,2,3]; init=0)      # -> reduce(+, [1,2,3], 0)
foldl(*, arr; init=1)            # -> foldl(*, arr, 1)
mapreduce(f, op, arr; init=val)  # -> mapreduce(f, op, arr, val)
```

## HOF Implementation Pattern (iterate-based)

All HOF functions that process iterables should follow the standard `iterate()` pattern for generality. This pattern works with any iterable type (arrays, tuples, ranges, etc.):

```julia
function my_hof(op, A)
    # Step 1: Get first element
    iter = iterate(A)
    iter === nothing && error("empty collection")
    (val, state) = iter
    result = init_from(val)

    # Step 2: Loop with iterate(A, state)
    iter = iterate(A, state)
    while iter !== nothing
        (val, state) = iter
        result = op(result, val)
        iter = iterate(A, state)
    end

    return result
end
```

Reference implementations:
- `reduce`/`foldl`/`foldr` in `subset_julia_vm/src/julia/base/iterators.jl`
- `accumulate` in `subset_julia_vm/src/julia/base/accumulate.jl`

## Iterator-Based HOFs in `iterators.jl`

The following HOFs are implemented directly in `iterators.jl` alongside the iterator wrapper types:

| Function | Signature | Description |
|----------|-----------|-------------|
| `map(f, A)` | `map(f::Function, A)` | `collect(Generator(f, A))` |
| `map(f, A, B)` | `map(f::Function, A, B)` | Binary map via `zip` |
| `filter(f, A)` | `filter(f::Function, A)` | `collect(Filter(f, A))` |
| `reduce(op, itr)` | `reduce(op::Function, itr)` | Left fold without init |
| `reduce(op, itr, init)` | `reduce(op::Function, itr, init)` | Left fold with init |
| `foldl(op, itr)` | `foldl(op::Function, itr)` | Alias for `reduce` |
| `foldr(op, itr)` | `foldr(op::Function, itr)` | Right fold (collects first) |
| `mapfoldl(f, op, itr)` | `mapfoldl(f::Function, op::Function, itr)` | Map then left fold |
| `mapfoldr(f, op, itr)` | `mapfoldr(f::Function, op::Function, itr)` | Map then right fold |
| `mapreduce(f, op, itr)` | `mapreduce(f::Function, op::Function, itr)` | Alias for `mapfoldl` |

## HOF Testing Checklist

When adding or modifying HOF implementations:

- [ ] Use `iterate()` pattern (not raw array indexing) for iterable processing
- [ ] Test with lambda functions: `(a, b) -> a + b`
- [ ] Test with named functions: `myadd(a, b) = a + b`
- [ ] Test with bare operators (`+`, `*`) as function arguments (supported since Issue #1985)
- [ ] Define helper functions and types **OUTSIDE** `@testset` blocks
- [ ] Verify `push!` works with the result type (for accumulating results)
- [ ] Add the function to `exports.jl` and verify with `base_exports_do_not_exceed_upstream`
- [ ] Test edge cases: empty collections, single-element collections, mixed types
- [ ] For String HOFs: verify return type is `String`, not `Vector{Char}`

## Function Type Inference Strategy (Issue #1671)

HOFs require correct type inference for function arguments to enable method dispatch. The compiler resolves function arguments through several paths:

### 1. FunctionRef -> Callable type

Callable expressions must infer to a Julia callable type from
`compile/expr/infer/julia_type.rs`. A bare `Expr::FunctionRef` currently infers
as `JuliaType::Struct("typeof(name)")`; the shared type lattice treats these
singleton callable structs as subtypes of `Function`, so generic HOF methods
still match while callable-specific methods can win.

- `Expr::FunctionRef { name, .. }` -> `JuliaType::Struct("typeof(name)")`
- Lambdas are lowered to `FunctionRef` by the lowering stage
- Bare operators in argument position are lowered to `FunctionRef` (Issue #1985)

**Key rule:** When adding new `Expr` variants that represent callable values,
ensure `infer_julia_type()` returns a type that dispatch treats as callable.

### 2. Pure Julia fallthrough pattern

`compile_builtin_hof()` returns `Ok(None)` for public Pure Julia HOF names such
as `map`, `filter`, `reduce`, `foldl`, `foldr`, `map!`, `filter!`, `findall`,
`foreach`, and `broadcast`. This causes the compiler to fall through to method
table dispatch, where Pure Julia implementations are found:

```
compile_builtin_hof("map") -> Ok(None)
  -> method table lookup -> map(f::Function, A) in iterators.jl
  -> Pure Julia: collect(Generator(f, A))
```

### 3. Call-site type specialization

For HOF calls, return type inference uses call-site specialization in
`compile/expr/infer/hof.rs`:

- `infer_map_call_return_type()` -- Extracts function name + array element type -> dispatches in method table -> returns `ArrayOf(result_element_type)`
- `infer_filter_call_return_type()` -- Returns same element type as input array
- `infer_reduce_call_return_type()` -- Infers result type from operator + element type

### 4. Function resolution with arity preference

`resolve_function_ref()` and `resolve_function_ref_with_arity()` in
`compile/expr/builtin.rs` resolve function references for HOF arguments. Arity
preference is critical for operators: `mapreduce(f, +, arr)` must resolve `+`
as binary, not unary (Issue #2004).

## HOF Code Review Checklist

When reviewing PRs that add HOF functions:

- [ ] Does the implementation use `iterate()` instead of direct indexing?
- [ ] Are tests using bare operators, named functions, and/or lambdas as HOF arguments?
- [ ] Is the function exported in the appropriate `exports.jl`?
- [ ] Does the implementation match the official Julia behavior? (Check `julia/base/`)
- [ ] For public HOF names with retained Rust instructions: are the Pure Julia path and compatibility fallback consistent?
- [ ] Is the Pure Julia implementation in the correct file? (`iterators.jl` for core HOFs, `abstractarray.jl` for `foreach`, `accumulate.jl` for cumulative ops)

## String Specialization Rule (Issue #2622)

HOFs on String types must return `String`, not `Vector{Char}` or `Vector{Any}`. In official Julia, `map(f, s::String)` and `filter(pred, s::String)` both return `String` by using an `IOBuffer` internally to collect characters and then converting to `String`.

### Pattern: IOBuffer-Based String HOFs

```julia
# Official Julia pattern (simplified from julia/base/strings/basic.jl):
function map(f, s::AbstractString)
    out = IOBuffer(sizehint=sizeof(s))
    for c in s
        c2 = f(c)
        write(out, c2)
    end
    return String(take!(out))
end

function filter(f, s::AbstractString)
    out = IOBuffer(sizehint=sizeof(s))
    for c in s
        f(c) && write(out, c)
    end
    return String(take!(out))
end
```

### Why This Matters for Migration

When implementing `map`/`filter` in Pure Julia, the generic iterate-based pattern returns `Vector{Any}` because it uses `push!` to collect results. String inputs need **explicit specializations** that return `String`.

### Current Status

SubsetJuliaVM has String specializations in `base/strings/basic.jl` for:
- `map(f::Function, s::String)` -- returns `String` (tested in `strings/map_string.jl`)
- `filter(f::Function, s::String)` -- returns `String` (tested in `strings/filter_string.jl`)
- `count(f::Function, s::String)` -- returns `Int64` (counts characters matching predicate)

### HOF String Specialization Checklist

When adding new HOFs that process iterables, check if a String specialization is needed:
- [ ] Does the HOF return a collection? (e.g., `map` returns array/string)
- [ ] If yes, add a `(f, s::AbstractString)` or `(f, s::String)` method that returns `String`
- [ ] Test with `isa(result, String)` assertion
- [ ] Verify empty string edge case

## Accumulate Operations

Cumulative/accumulate HOFs are implemented in Pure Julia in `base/accumulate.jl`:

| Function | In-place | Description |
|----------|----------|-------------|
| `cumsum(arr)` | `cumsum!(B, A)` | Cumulative sum |
| `cumprod(arr)` | `cumprod!(B, A)` | Cumulative product |
| `accumulate(op, A)` | `accumulate!(op, B, A)` | Generalized cumulative op (iterate-based) |
| `accumulate(op, A, init)` | -- | With initial value |

These use the `iterate()` protocol for `accumulate` (works with any iterable) and direct indexing for `cumsum`/`cumprod` (array-specific).

## Reduction Operations in `base/reduce.jl`

Non-HOF reductions (no function argument):

| Function | Description |
|----------|-------------|
| `count(arr)` | Count truthy values in array |
| `extrema(arr)` | `(min, max)` tuple |
| `extrema(f, arr)` | `(min(f(x)), max(f(x)))` |
| `findmax(arr)` / `findmin(arr)` | `(value, index)` |
| `findmax(f, arr)` / `findmin(f, arr)` | `(f(x), index)` |
| `argmax(arr)` / `argmin(arr)` | Index of extremum |
| `argmax(f, arr)` / `argmin(f, arr)` | Element that extremizes f |
| `diff(arr)` | Consecutive differences |
| `any(arr)` / `all(arr)` | Boolean reduction |

## Related Documentation

- `CLAUDE.md` - Top-level contributor guidelines
- `LOWERING.md` - Lowering details including function definition forms
- `COLLECTIONS.md` - Collection type dispatch and iterator types
- `PURE_JULIA_DESIGN.md` - Pure Julia design philosophy
