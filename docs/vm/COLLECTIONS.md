# Collection Type Dispatch Guide

*Last updated: 2026-06-11*

This document describes the pattern for implementing generic functions that work with multiple collection types (Array, Dict, Set, Tuple) while preserving type information. It also covers the iterator wrapper types and the Dict{K,V} Pure Julia infrastructure.

## Collect Native Fallback Inventory (Issue #4052)

Public `collect(itr)` should reach Julia Base-shaped method dispatch first. The retained
native fallbacks below are compatibility boundaries for VM-native values that cannot yet
participate directly in Pure Julia field access or continuation semantics. Every retained
boundary carries a `CollectFallback:` tag in code and is checked by
`scripts/check_collect_fallback_inventory.sh`.

| Tag | Owner classification | Upstream reference | Current reason |
|-----|----------------------|--------------------|----------------|
| `CollectFallback: builtin-rangecollect-final-boundary` | Compatibility fallback | `julia/base/range.jl` `collect(r::AbstractRange) = Array(r)`; `julia/base/array.jl` `_collect` | Final fallback for VM-native Array/Range/Tuple/Memory/Generator values after dispatch-first routes are exhausted. |
| `CollectFallback: collect-similar-generator-compile-boundary` | Compatibility fallback | `julia/base/array.jl` `collect_similar(cont, itr)` / `collect(itr::Generator)` | Non-Memory `collect_similar(cont, generator)` reuses the VM generator bridge while `Memory` containers dispatch to Pure Julia. Generator bridge is narrowed for #4265; final removal: #4568. |
| `CollectFallback: public-collect-primitive-compile-boundary` | Compatibility fallback | `julia/base/array.jl` `collect(itr)`; `julia/base/range.jl` range collect | Static VM-native Range/Generator and selected `Any` calls enter `BuiltinOp::Collect` so primitive representations avoid generic `collect(::Any)` erasure. Range/Generator bridges are narrowed for #4265/#4266; final removal: #4568. |
| `CollectFallback: rangecollect-builtin-entry` | Bootstrap intrinsic | Julia runtime builtin/array allocation boundary plus `julia/base/array.jl` collect surface | Single VM builtin entry for retained native collect materialization. No new public Rust intrinsic should be added for #4052 paths. |
| `CollectFallback: runtime-generator-pre-score-boundary` | Compatibility fallback | `julia/base/array.jl` `collect(itr::Generator)` | Runtime `collect(x::Any)` where `x` is a VM-native generator uses generic `_collect` for nonempty Vector/Matrix/Memory/Range-backed named-function cases; unsafe empty/type-constructor/tuple-splat/filtered/eager cases remain native. Generator bridge is narrowed for #4265; final removal: #4568. |
| `CollectFallback: runtime-range-pre-score-boundary` | Compatibility fallback | `julia/base/range.jl` `collect(r::AbstractRange) = Array(r)` | Runtime `Value::Range` is not a Pure Julia range struct; integer UnitRange/StepRange candidates now score first, while field-access-dependent shapes such as runtime floating StepRangeLen remain on native materialization. Range bridge is narrowed for #4266; final removal: #4568. |
| `CollectFallback: runtime-simplevector-pre-score-boundary` | Compatibility fallback | `julia/base/array.jl` `collect(::Core.SimpleVector)` | Runtime `Core.SimpleVector` values collect as `Vector{Any}` before normal candidate scoring so heterogeneous type/value parameter vectors do not enter generic `_collect` widening. |
| `CollectFallback: native-collect-sentinel-boundary` | Compatibility fallback | `julia/base/iterators.jl` zip wrappers; `julia/base/array.jl` collect | Shared runtime sentinel for retained native collect after real user/Pure Julia candidates are scored first. |
| `CollectFallback: collect-similar-generator-runtime-boundary` | Compatibility fallback | `julia/base/array.jl` `collect_similar` / generator collect | Runtime `collect_similar` keeps non-Memory generator values on the VM bridge; Memory-specific methods remain Pure Julia. Generator bridge is narrowed for #4265; final removal: #4568. |
| `CollectFallback: array-wrapper-copy-materialization` | Compatibility fallback | `julia/base/array.jl` `collect(A::AbstractArray)` | Pure Julia `Array` wrappers can still arrive at the native collect sentinel from runtime fallback paths; this materializes an independent Memory-first copy from `_mem` / `_size` until public wrapper collect fully owns the path. |
| `CollectFallback: array-copy-materialization` | Compatibility fallback | `julia/base/array.jl` `collect(A::AbstractArray)` | Native-array compatibility copy materializes through Memory-first `ArrayValue` until public Array wrappers fully own the path. |
| `CollectFallback: range-value-materialization` | Compatibility fallback | `julia/base/range.jl` `Array(r)` | VM-native `Value::Range` value materialization preserves existing typed range results. Range bridge is narrowed for #4266; final removal: #4568. |
| `CollectFallback: tuple-value-materialization` | Compatibility fallback | `julia/base/array.jl` `_collect(..., ::EltypeUnknown, ...)` / `collect_to_with_first!` | Runtime tuple values use typejoin materialization when the Pure Julia tuple method cannot be selected statically. |
| `CollectFallback: simplevector-value-materialization` | Compatibility fallback | `julia/base/array.jl` `collect(::Core.SimpleVector)` | VM-native `Core.SimpleVector` values materialize through Memory-first `Vector{Any}` while preserving heterogeneous elements. |
| `CollectFallback: memory-value-materialization` | Compatibility fallback | `julia/base/array.jl` `collect_similar` and `genericmemory.jl` storage | VM-native `Memory` values materialize through Memory-first array helpers while wrapper Array migration continues. |
| `CollectFallback: struct-native-iterator-materialization` | Compatibility fallback | `julia/base/iterators.jl` wrappers | Last native handler for selected struct-backed wrappers after dispatch-first candidates have had priority. |
| `CollectFallback: zip-struct-materialization` | Compatibility fallback | `julia/base/iterators.jl` `zip` / `zip_iteratorsize` | Zip has a native fallback only as a sentinel; user/Pure Julia `collect(::Zip)` candidates score first. |
| `CollectFallback: enumerate-struct-legacy-materialization` | Compatibility fallback | `julia/base/iterators.jl` `Enumerate` | Legacy direct `RangeCollect` support remains, but public static/runtime `collect(::Enumerate)` now dispatches to Pure Julia first. |
| `CollectFallback: rest-struct-legacy-materialization` | Compatibility fallback | `julia/base/iterators.jl` `Rest` | Legacy direct `RangeCollect` support remains, but public static/runtime `collect(::Rest)` now dispatches to Pure Julia first. |
| `CollectFallback: logrange-struct-materialization` | Compatibility fallback | `julia/base/range.jl` `LogRange` collection | LogRange has a native value materializer for representation cases that cannot run Pure Julia field access safely. |
| `CollectFallback: eager-generator-iterable-materialization` | Compatibility fallback | `julia/base/array.jl` eager generator collection | Eager generator wrappers already hold materialized values in their iterable field; collecting them re-materializes that iterable through the native iterator boundary. Final removal: #4568. |
| `CollectFallback: generator-eager-copy-boundary` | Compatibility fallback | `julia/base/array.jl` generator collect | Eager generator wrappers already hold the collected array; the fallback returns an independent copy. Final removal: #4568. |
| `CollectFallback: generator-typeobject-boundary` | Compatibility fallback | `julia/base/array.jl` `@default_eltype`; callable `Type` behavior | VM bridge applies type-object callables while preserving empty eltype metadata. Final removal: #4568. |
| `CollectFallback: generator-tuplesplat-typeobject-boundary` | Compatibility fallback | `julia/base/generator.jl` vararg `Generator` construction | Tuple-splat type-object generators remain on the VM bridge for supported constructor cases. Final removal: #4568. |
| `CollectFallback: generator-function-index-boundary` | Compatibility fallback | `julia/base/generator.jl` `iterate(g::Generator)` | Function-index generators apply `f` through the VM HOF frame path after materializing wrapped values. Final removal: #4568. |
| `CollectFallback: generator-filtered-function-boundary` | Compatibility fallback | `julia/base/iterators.jl` filtered generators | Filtered generator collection still uses a VM HOF path for supported predicate/map function indices. Final removal: #4568. |
| `CollectFallback: generator-tuplesplat-function-boundary` | Compatibility fallback | `julia/base/generator.jl` vararg generator iteration | Tuple-splat function generators apply the callable through the VM HOF path. Final removal: #4568. |
| `CollectFallback: generator-runtime-callable-boundary` | Compatibility fallback | `julia/base/generator.jl` callable value iteration | Runtime callable generators use VM runtime callable dispatch while preserving supported result element types. Final removal: #4568. |
| `CollectFallback: generator-tuplesplat-runtime-callable-boundary` | Compatibility fallback | `julia/base/generator.jl` callable value iteration | Runtime tuple-splat callables remain on the VM bridge for supported value-call cases. Final removal: #4568. |

## The Problem: Type Erasure (Issue #1821)

When implementing generic functions like `copy`, a naive implementation may lose type information:

```julia
# WRONG: Generic fallback that erases type
function copy(arr)
    return collect(arr)  # Returns Vector for ALL iterable types!
end

# Result:
copy([1, 2, 3])           # => Vector{Int64} ✓
copy(Dict("a" => 1))      # => Vector{Pair} ✗ (should be Dict)
copy(Set([1, 2, 3]))      # => Vector{Int64} ✗ (should be Set)
```

The `collect()` function iterates over any iterable and produces a `Vector`, which destroys the original collection's type identity.

## The Solution: Multiple Dispatch

Julia's standard library uses **multiple dispatch** to provide type-specific implementations:

```julia
# CORRECT: Type-specific methods
copy(arr::Array) = collect(arr)           # Array → Array (via collect)
copy(d::Dict) = merge(d, Dict())          # Dict → Dict
copy(s::Set) = union(s, Set())            # Set → Set
```

Each collection type gets its own method that preserves type.

## Implementation Checklist

When implementing a generic function from Julia Base:

### 1. Search Official Julia Source

```bash
# Find all method signatures for the function
rg -n "^functionname\\(" julia/base/
```

### 2. Identify Type-Specific Methods

Look for patterns like:
- `functionname(a::Array)` - Array-specific
- `functionname(d::AbstractDict)` or `functionname(d::Dict)` - Dict-specific
- `functionname(s::AbstractSet)` or `functionname(s::Set)` - Set-specific
- `functionname(t::Tuple)` - Tuple-specific

### 3. Implement Type-Specific Methods First

Add implementations in the appropriate files:

| Type | Implementation File | Example |
|------|---------------------|---------|
| Array | `base/array.jl` | `copy(arr::Array) = collect(arr)` |
| Dict | `base/dict.jl` | `copy(d::Dict) = merge(d, Dict())` |
| Set | `base/set.jl` | `copy(s::Set) = union(s, Set())` |

### 4. Add Type Preservation Test

```julia
# tests/fixtures/collections/functionname_type_preservation.jl
using Test

@testset "functionname() preserves type for all collections" begin
    @testset "Array" begin
        arr = [1, 2, 3]
        result = functionname(arr)
        @test length(result) == expected_length
        # Verify it's the correct type (can't use typeof in some cases)
    end

    @testset "Dict" begin
        dict = Dict("a" => 1)
        result = functionname(dict)
        @test length(result) == expected_length
    end

    @testset "Set" begin
        set = Set([1, 2, 3])
        result = functionname(set)
        @test length(result) == expected_length
    end

    @testset "Tuple" begin
        tup = (1, 2, 3)
        result = functionname(tup)
        @test result == expected_value
    end
end
```

### 5. Only Add Generic Fallback If Truly Needed

Generic fallbacks should be:
- Last resort, not primary implementation
- Documented with clear behavior expectations
- Tested to ensure they don't break type-specific methods

## Functions Requiring Type-Specific Dispatch

### Implemented

| Function | Array | Dict | Set | Tuple | Notes |
|----------|-------|------|-----|-------|-------|
| `copy` | array.jl | dict.jl | set.jl | tuple.jl (identity) | Issue #1821 |
| `empty` | array.jl | array.jl | set.jl | array.jl | Issue #2389 |
| `empty!` | array.jl | dict.jl | set.jl | N/A (immutable) | |
| `length` | builtin + Dict{K,V} | builtin + Dict{K,V} | set.jl | builtin | |
| `iterate` | builtin | builtin + Dict{K,V} | builtin | builtin | |
| `collect` | iterators.jl | N/A | N/A | N/A | Generic iterate-based fallback |
| `keys` | builtin | dict.jl + Dict{K,V} | N/A | N/A | |
| `values` | builtin | dict.jl + Dict{K,V} | N/A | N/A | |
| `pairs` | builtin | dict.jl + Dict{K,V} | N/A | N/A | |
| `haskey` | N/A | dict.jl + Dict{K,V} | N/A | N/A | |
| `get` | N/A | dict.jl + Dict{K,V} | N/A | N/A | |
| `getkey` | N/A | dict.jl | N/A | N/A | |
| `merge` | N/A | dict.jl | N/A | N/A | |
| `mergewith` / `mergewith!` | N/A | dict.jl | N/A | N/A | |
| `delete!` | N/A | Dict{K,V} | set.jl | N/A | |
| `pop!` | builtin | Dict{K,V} | N/A | N/A | |
| `isempty` | builtin | Dict{K,V} | N/A | N/A | |
| `push!` | builtin | N/A | set.jl | N/A | |
| `union` / `union!` | N/A | N/A | set.jl | N/A | |
| `intersect` / `intersect!` | N/A | N/A | set.jl | N/A | |
| `setdiff` / `setdiff!` | N/A | N/A | set.jl | N/A | |
| `symdiff` / `symdiff!` | N/A | N/A | set.jl | N/A | |
| `issubset` | N/A | N/A | set.jl | N/A | |
| `isdisjoint` | N/A | N/A | set.jl | N/A | |
| `issetequal` | N/A | N/A | set.jl | N/A | |
| `in!` | N/A | N/A | set.jl | N/A | |
| `unique` / `unique!` | set.jl | N/A | N/A | N/A | Also `unique(f, itr)` |
| `allunique` | set.jl | N/A | N/A | N/A | |
| `allequal` | set.jl | N/A | N/A | N/A | |
| `filter!` | array.jl | N/A | N/A | N/A | |
| `first` | builtin | N/A | N/A | tuple.jl | |
| `last` | builtin | N/A | N/A | tuple.jl | |
| `reverse` | array.jl | N/A | N/A | tuple.jl | |
| `similar` | builtin | N/A | N/A | N/A | All forms — `similar(a)`, `similar(a, n)`, `similar(a, n, m, ...)`, `similar(a, T, dims...)`. Issues #2129 / #3648 / #3751 |

### Not Yet Implemented

| Function | Notes |
|----------|-------|
| `sizehint!` | No-op stub exists in `abstractarray.jl` |

## Pure Julia Dict{K,V} Infrastructure (Milestone 9)

Dict{K,V} is now a **Pure Julia mutable struct** (Issues #2738, #2747, #2748), coexisting with the Rust-backed `Value::Dict` in a **dual dispatch** model.

### Architecture: Dual Dispatch Model

| Aspect | Rust-backed `Value::Dict` | Pure Julia `Dict{K,V}` struct |
|--------|--------------------------|-------------------------------|
| Construction | `Dict()` / `Dict{K,V}()` with pair/empty args | struct constructor (non-pair args) |
| Storage | Rust `HashMap<DictKey, Value>` | `Vector{Int64}` (slots) / `Vector{Any}` (keys, vals) |
| Dispatch | `::Dict` bare annotation | `::Dict{K,V} where {K,V}` annotation |
| Hash function | Rust `Hash` trait | Julia `hash()` + open-addressing linear probing |

### Dict{K,V} Struct Definition (in `base/dict.jl`)

```julia
mutable struct Dict{K,V} <: AbstractDict{K,V}
    slots     # Vector{Int64} - slot metadata (0=empty, 127=deleted, 128+=filled)
    keys      # Vector{Any}   - keys storage
    vals      # Vector{Any}   - values storage
    ndel      # Int64 - number of deleted entries
    count     # Int64 - number of active entries
    age       # Int64 - modification counter
    idxfloor  # Int64 - smallest index that might be occupied
    maxprobe  # Int64 - max probe distance used
end
```

### Core Hash Table Algorithms

Implemented in Pure Julia in `base/dict.jl`:

| Algorithm | Function | Reference |
|-----------|----------|-----------|
| Key lookup | `ht_keyindex(h, key)` | julia/base/dict.jl:238-260 |
| Insertion slot | `ht_keyindex2!(h, key)` | julia/base/dict.jl:267-319 |
| Internal insert | `_setindex!(h, v, key, index, sh)` | julia/base/dict.jl:324-342 |
| Internal delete | `_delete!(h, index)` | julia/base/dict.jl:626-651 |
| Rehash / resize | `rehash!(h, newsz)` | julia/base/dict.jl:138-192 |
| Skip deleted slots | `skip_deleted(h, i)` | julia/base/dict.jl:684-699 |
| Table size rounding | `_tablesz(x)` | julia/base/abstractdict.jl:580 |
| Short hash extraction | `_shorthash7(hsh)` | julia/base/dict.jl:122 |
| Hash + index | `hashindex(key, sz)` | julia/base/dict.jl:127-132 |

### Public API Methods (Dict{K,V} where {K,V})

| Method | Description |
|--------|-------------|
| `setindex!(h, v, key)` | Insert or update key-value pair |
| `getindex(h, key)` | Retrieve value by key (throws on missing) |
| `haskey(h, key)` | Check if key exists |
| `get(h, key, default)` | Retrieve value or return default |
| `length(h)` | Number of active entries |
| `isempty(h)` | Check if dict is empty |
| `delete!(h, key)` | Remove key-value pair |
| `empty!(h)` | Remove all entries |
| `pop!(h, key)` | Remove and return value |
| `pop!(h, key, default)` | Remove and return value, or default |
| `iterate(h)` / `iterate(h, state)` | Iteration protocol (yields `Pair`) |
| `keys(h)` | Return all keys as `Vector{Any}` |
| `values(h)` | Return all values as `Vector{Any}` |
| `pairs(h)` | Return all pairs as `Vector{Any}` of tuples |

### Value::Dict Wrapper Methods (bare `::Dict`)

These dispatch on Rust-backed `Value::Dict` and delegate to internal intrinsics:

| Method | Location |
|--------|----------|
| `haskey(d::Dict, key)` | dict.jl (wraps `_dict_haskey`) |
| `get(d::Dict, key, default)` | dict.jl (wraps `_dict_haskey` + `_dict_get`) |
| `getkey(d::Dict, key, default)` | dict.jl |
| `keys(d::Dict)` | dict.jl (wraps `_dict_keys`) |
| `values(d::Dict)` | dict.jl (wraps `_dict_values`) |
| `pairs(d::Dict)` | dict.jl (wraps `_dict_pairs`) |
| `merge(d1::Dict, d2::Dict)` | dict.jl |
| `copy(d::Dict)` | dict.jl (via `merge`) |
| `mergewith!(combine, d1, d2)` | dict.jl |
| `mergewith(combine, d1, d2)` | dict.jl |

## Set Operations (Pure Julia)

All Set operations are implemented in Pure Julia (`base/set.jl`), wrapping internal Rust intrinsics for core operations:

### Core Operations (Rust intrinsic wrappers)

| Function | Intrinsic |
|----------|-----------|
| `push!(s::Set, x)` | `_set_push!` |
| `delete!(s::Set, x)` | `_set_delete!` |
| `empty!(s::Set)` | `_set_empty!` |
| `length(s::Set)` | `_set_length` |
| `in(x, s::Set)` | Handled by compiler/VM (`BuiltinId::SetIn`) |

### Set Algebra (Pure Julia, Issue #2575)

| Function | In-place variant | Description |
|----------|-----------------|-------------|
| `union(s1, s2)` | `union!(s, itr)` | Set union |
| `intersect(s1, s2)` | `intersect!(s, itr)` | Set intersection |
| `setdiff(s1, s2)` | `setdiff!(s, itr)` | Set difference |
| `symdiff(s1, s2)` | `symdiff!(s, itr)` | Symmetric difference |
| `issubset(a, b)` | -- | Subset check |
| `isdisjoint(a, b)` | -- | Disjoint check |
| `issetequal(a, b)` | -- | Set equality |
| `copy(s)` | -- | Shallow copy (via `union`) |
| `empty(s)` | -- | Empty Set of same type |
| `in!(x, s)` | -- | Check membership and insert |

### Array-as-Set Operations (in `base/set.jl`)

| Function | Description |
|----------|-------------|
| `unique(arr)` | Remove duplicates |
| `unique(f, arr)` | Remove duplicates by `f(x)` value |
| `unique!(arr)` | Remove duplicates in-place |
| `allunique(arr)` | Check if all elements are unique |
| `allequal(arr)` | Check if all elements are equal |

## Iterator Wrapper Types (Pure Julia)

SubsetJuliaVM implements 33 iterator/indexing-related structs in `base/iterators.jl`; `base/generator.jl` adds the `Generator` wrapper plus iterator size/eltype trait markers. All public iterator wrappers implement the `iterate()` protocol.

### Iterator Types in `base/iterators.jl`

| Struct | Constructor | Description | `length` |
|--------|-------------|-------------|----------|
| `Enumerate{I}` | `enumerate(iter)` | Yields `(i, x)` counter pairs | Yes |
| `Zip{I1,I2}` | `zip(a, b)` | Parallel 2-collection iteration | Yes |
| `Zip3{I1,I2,I3}` | `zip(a, b, c)` | Parallel 3-collection iteration | Yes |
| `Zip4{I1,I2,I3,I4}` | `zip(a, b, c, d)` | Parallel 4-collection iteration | Yes |
| `Zip5{I1,I2,I3,I4,I5}` | `zip(a, b, c, d, e)` | Parallel 5-collection iteration | Yes |
| `Zip6{I1,I2,I3,I4,I5,I6}` | `zip(a, b, c, d, e, f)` | Parallel 6-collection iteration | Yes |
| `Zip7{I1,I2,I3,I4,I5,I6,I7}` | `zip(a, b, c, d, e, f, g)` | Parallel 7-collection iteration | Yes |
| `Take{I}` | `take(xs, n)` | First N elements | Yes |
| `Drop{I}` | `drop(xs, n)` | Skip first N elements | Yes |
| `TakeWhile{I,P}` | `takewhile(pred, xs)` | Yield while predicate true | No |
| `DropWhile{I,P}` | `dropwhile(pred, xs)` | Skip while predicate true | No |
| `CartesianIndex` | `CartesianIndex(tuple)` | Multi-dimensional index | Yes |
| `CartesianIndices` | `CartesianIndices(dims)` | All indices in region | Yes |
| `IndexLinear` | -- | Linear indexing style trait | No |
| `IndexCartesian` | -- | Cartesian indexing style trait | No |
| `LinearIndices` | `LinearIndices(dims)` | Linear index iterator | Yes |
| `Pairs{K,V,I,A}` | `pairs(A)` | Index/value dictionary view for indexable collections | Yes |
| `EachCol` | `eachcol(A)` | Iterate matrix columns | Yes |
| `EachRow` | `eachrow(A)` | Iterate matrix rows | Yes |
| `EachSlice` | `eachslice(A; dims)` | Iterate array slices | Yes |
| `SkipMissing` | `skipmissing(itr)` | Skip missing values | No |
| `Flatten` | `flatten(itr)` | Flatten nested iterables | No |
| `FlatMap` | `flatmap(f, itr)` | Map then flatten | No |
| `Rest` | `rest(itr)` / `rest(itr, state)` | Iterator from second element | No |
| `Cycle` | `cycle(itr)` | Infinite cyclic repetition | No (infinite) |
| `Repeated` | `repeated(x)` / `repeated(x, n)` | Repeat value (finite or infinite) | Finite only |
| `Partition` | `partition(itr, n)` | Group into chunks of N | No |
| `Product` | `product(a, b)` | Cartesian product | Yes |
| `ProductIterator` | `product(iters...)` | Vararg Cartesian product | Yes |
| `EachSplit` | `eachsplit(str, delim)` | String split iterator | No |
| `EachRSplit` | `eachrsplit(str, delim)` | Reverse string split iterator | No |
| `Count{T}` | `countfrom(start, step)` | Infinite counting iterator | No (infinite) |
| `Filter` | (internal, used by `filter()`) | Filter by predicate | No |

### Iterator Types in `base/generator.jl`

| Struct | Constructor | Description |
|--------|-------------|-------------|
| `Generator` | `Generator(f, iter)` | Lazy map: yields `f(x)` for each `x` |

### Iterator Size/Eltype Traits (in `base/generator.jl`)

| Trait Type | Subtypes | Description |
|------------|----------|-------------|
| `IteratorSize` | `HasLength`, `HasShape{N}`, `SizeUnknown`, `IsInfinite` | Describes size capability |
| `IteratorEltype` | `HasEltype`, `EltypeUnknown` | Describes element type knowledge |

### Additional Iterator-like Types (in `base/broadcast.jl`)

| Struct | Description |
|--------|-------------|
| `Broadcasted` | Lazy broadcast wrapper |
| `Extruded` | Broadcast indexing helper |
| `DefaultArrayStyle` | Array broadcast style |
| `TupleBroadcastStyle` | Tuple broadcast style |

### Standalone Iterator Functions

| Function | Location | Description |
|----------|----------|-------------|
| `collect(itr)` | iterators.jl | Materialize iterator to `Vector{Any}` |
| `only(x)` | iterators.jl | Return single element or throw |
| `nth(itr, n)` | iterators.jl | Get nth element (optimized for Array/Range) |
| `peel(itr)` | iterators.jl | Split into `(first, rest)` |
| `eachindex(::IndexCartesian, A)` | iterators.jl | CartesianIndices iteration |

## Code Review Checklist

When reviewing PRs that add generic functions:

- [ ] Does the function delegate to `collect()`? If so, verify it only applies to Array
- [ ] Are there type-specific methods for Dict, Set, and Tuple?
- [ ] Is there a type preservation test?
- [ ] Does the implementation match official Julia behavior?
- [ ] Are the methods in the correct files (dict.jl for Dict, set.jl for Set, etc.)?
- [ ] For Dict operations: does the method handle both `Value::Dict` and `Dict{K,V}` struct?

## Audit Command

Find potential type-erasing patterns in the codebase:

```bash
# Find functions that return collect() result
rg -n "return collect\\(" subset_julia_vm/src/julia/base/

# Find generic fallbacks that might need type-specific overloads
rg -n "function (copy|empty|similar)" subset_julia_vm/src/julia/base/

# List all iterator wrapper structs
rg -n "^struct " subset_julia_vm/src/julia/base/iterators.jl subset_julia_vm/src/julia/base/generator.jl
```

## AbstractArray Interface Completeness (Issue #2706)

Julia's `AbstractArray` interface defines a set of required methods that all array-like types must implement. When SubsetJuliaVM adds new array-like types (Range, LinRange, StepRangeLen, etc.), these methods must be available.

### Required Interface Methods

| Method | Description | Status |
|--------|-------------|--------|
| `size(A)` | Returns tuple of dimensions | Implemented for Array, Range |
| `size(A, d)` | Returns size along dimension `d` | Implemented for Array |
| `length(A)` | Total number of elements | Implemented for Array, Range, Tuple, Dict, Set, String |
| `axes(A)` | Returns tuple of valid index ranges | Implemented for Array (up to 16D) |
| `axes(A, d)` | Returns valid index range for dimension `d` | Implemented for Array |
| `getindex(A, i)` | Element access | Implemented for Array, Range, Tuple, Dict |
| `setindex!(A, v, i)` | Element mutation | Implemented for Array, Dict |
| `iterate(A)` | Iterator protocol | Implemented for Array, Range, Tuple, Dict, Set |
| `eltype(A)` | Element type | Implemented for Array |
| `ndims(A)` | Number of dimensions | Implemented for Array |
| `firstindex(A)` | First valid index | Implemented for Array |
| `lastindex(A)` | Last valid index | Implemented for Array |

### Range Types

Range types (`UnitRange`, `StepRange`, `LinRange`, `StepRangeLen`, `OneTo`, `LogRange`) are array-like and should support:
- `length(r)` -- number of elements
- `size(r)` -- `(length(r),)` for 1D ranges
- `axes(r)` -- `(1:length(r),)` for 1D ranges
- `getindex(r, i)` -- element at index
- `iterate(r)` / `iterate(r, state)` -- iteration

### SubArray (View) Support

`SubArray{T}` in `base/subarray.jl` provides lightweight array views via `view(A, indices)`. Supports `Float64`, `Int64`, and `Bool` element types with reading, writing, `length`, `size`, `firstindex`, `lastindex`.

### Audit Command

```bash
# Check which array-like types implement size/length/axes
rg -n "function (size|length|axes)" subset_julia_vm/src/julia/base/
rg -n "BuiltinId::(Size|Length|Axes)" subset_julia_vm/src/vm/
```

## Array Manipulation Functions (Pure Julia)

The following array operations are implemented in Pure Julia in `base/array.jl`:

### Aggregation

| Function | `dims` keyword | `f` overload | In-place |
|----------|---------------|-------------|----------|
| `sum(arr)` | Yes (1, 2) | `sum(g::Generator)` | `sum!(r, A)` |
| `prod(arr)` | Yes (1, 2) | `prod(f, arr)` | `prod!(r, A)` |
| `minimum(arr)` | Yes (1, 2) | `minimum(f, arr)` | `minimum!(r, A)` |
| `maximum(arr)` | Yes (1, 2) | `maximum(f, arr)` | `maximum!(r, A)` |
| `extrema(arr)` | Yes (1, 2) | `extrema(f, arr)` | -- |

### Search

| Function | Description |
|----------|-------------|
| `argmin(arr)` / `argmin(f, arr)` | Index/element of minimum |
| `argmax(arr)` / `argmax(f, arr)` | Index/element of maximum |
| `findmin(arr)` / `findmin(f, arr)` | `(value, index)` of minimum |
| `findmax(arr)` / `findmax(f, arr)` | `(value, index)` of maximum |
| `findmin!(rval, rind, arr)` | In-place findmin |
| `findmax!(rval, rind, arr)` | In-place findmax |
| `findfirst(value, arr)` | First index of value |
| `findlast(value, arr)` | Last index of value |
| `findnext(f, A, start)` | Next index where predicate true |
| `findprev(f, A, start)` | Previous index where predicate true |
| `findall(A::Array)` | All truthy indices |
| `findall(x::Bool)` | Scalar boolean |
| `indexin(a, b)` | Indices of `a` elements in `b` |

### Mutation

| Function | Description |
|----------|-------------|
| `push!`, `pop!`, `pushfirst!`, `popfirst!` | Deque operations |
| `insert!(a, i, item)` | Insert at index |
| `deleteat!(a, i)` | Delete at index |
| `popat!(a, i)` / `popat!(a, i, default)` | Pop at index |
| `splice!(a, i)` / `splice!(a, i, v)` | Remove/replace at index |
| `append!(a, items)` | Append collection |
| `prepend!(a, items)` | Prepend collection |
| `resize!(a, n)` | Resize vector |
| `empty!(a)` | Remove all elements |
| `fill!(arr, value)` | Fill with value |
| `keepat!(a, inds)` | Keep only specified indices |
| `filter!(f, a)` | Filter in-place |
| `map!(f, a)` / `map!(f, dest, src)` | Map in-place |
| `clamp!(a, lo, hi)` | Clamp values in-place |
| `circshift!(arr, shift)` | Circular shift in-place |
| `copy!(dest, src)` | Copy with resize |
| `copyto!(dest, src)` | Copy elements (multiple overloads) |

### Transformation

| Function | Description |
|----------|-------------|
| `reverse(arr)` | Reverse array |
| `circshift(arr, k)` | Circular shift |
| `repeat(arr, n)` / `repeat(arr, m, n)` | Repeat array |
| `vcat(a, b)` / `vcat(args...)` | Vertical concatenation |
| `hcat(a, b)` / `hcat(args...)` | Horizontal concatenation |
| `cat(A, B; dims)` | General concatenation |
| `vec(arr)` | Flatten to 1D |
| `stack(arrays)` | Stack arrays as matrix columns |
| `permutedims(arr)` / `permutedims(arr, perm)` | Dimension permutation (up to 4D) |
| `permutedims!(dest, src, perm)` | In-place permutation |
| `transpose(arr)` | Matrix transpose |
| `adjoint(arr)` | Conjugate transpose |
| `rotl90(mat)` / `rotr90(mat)` / `rot180(mat)` | Matrix rotation |
| `selectdim(A, d, i)` | Select slice along dimension |
| `dropdims(A; dims)` | Remove singleton dimension |
| `insertdims(A; dims)` | Insert singleton dimension |
| `mapslices(f, A; dims)` | Apply function to slices |
| `sortslices(A; dims)` | Sort slices lexicographically |
| `diff(arr)` | Consecutive differences |

### Construction

| Function | Description |
|----------|-------------|
| `fill(value, dims...)` | Create filled array (up to 3D, type-preserving) |
| `trues(dims...)` / `falses(dims...)` | Boolean arrays (up to 3D) |
| `checkbounds(Bool, A, i)` / `checkbounds(A, i)` | Bounds checking |
| `checkindex(Bool, inds, i)` | Index range checking |
| `isassigned(a, i)` | Check index assignment |
| `isperm(p)` / `invperm(p)` | Permutation utilities |
| `issorted(arr)` | Check sorted order |

## Accumulate Operations (Pure Julia, `base/accumulate.jl`)

| Function | In-place | Description |
|----------|----------|-------------|
| `cumsum(arr)` | `cumsum!(B, A)` | Cumulative sum |
| `cumprod(arr)` | `cumprod!(B, A)` | Cumulative product |
| `accumulate(op, A)` | `accumulate!(op, B, A)` | Generalized cumulative operation |
| `accumulate(op, A, init)` | -- | With initial value |

## Reduction Operations (Pure Julia, `base/reduce.jl`)

| Function | Description |
|----------|-------------|
| `count(arr)` | Count truthy values |
| `extrema(arr)` / `extrema(f, arr)` | `(min, max)` tuple |
| `findmax(arr)` / `findmin(arr)` | `(value, index)` |
| `argmax(arr)` / `argmin(arr)` | Index of extremum |
| `diff(arr)` | Consecutive differences |
| `any(arr)` / `all(arr)` | Boolean reduction (non-HOF) |

## Memory{T} Migration Status

`Memory{T}` is Julia's low-level typed memory backing for arrays (replacing `Array` internals). Migration is tracked across multiple phases:

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1 | `Value::Memory` variant added | ✅ Complete |
| Phase 2 | VM instructions for Memory operations | ✅ Complete |
| Phase 3 | `genericmemory.jl` connection | ✅ Complete |
| Phase 4 | FFI updates | ✅ Complete |
| Phase 5 | Retire literal `Value::Array` usage and make native-array compatibility explicit (Issues #3908/#4568) | ✅ `Value::Array(ArrayRef)` retired. `scripts/check_value_array_allowlist.sh` is now a zero-match audit; remaining host/runtime compatibility uses explicit `Value::NativeArray` converters. |
| Phase 6 | Dict migrated to Memory-based hash table | ✅ Complete |
| Phase 7 | (reserved) | — |
| Phase 8 | FFI consumer updates | 🔧 Pending |
| Phase 9 | Final cleanup | 🔧 Pending |

### ArrayData Element Types (16 variants)

The `ArrayData` enum represents homogeneous array storage. These types can be stored in typed arrays:

| Category | Types |
|----------|-------|
| Floating point | `F32`, `F64` |
| Signed integers | `I8`, `I16`, `I32`, `I64` |
| Unsigned integers | `U8`, `U16`, `U32`, `U64` |
| Other | `Bool`, `BitPackedBool`, `String`, `Char`, `StructRefs`, `Any` |

**Not in ArrayData**: `F16`, `I128`, `U128`, `BigInt`, `BigFloat` — these are stored directly in `Value` enum variants, not in homogeneous arrays.

## Dict Dual Dispatch Model

Two parallel Dict representations coexist:

| Aspect | Path 1: `Value::Dict` (Rust-backed) | Path 2: `Dict{K,V}` struct (Pure Julia) |
|--------|--------------------------------------|------------------------------------------|
| Creation | `Dict()` / `Dict{K,V}()` with pair/empty args | Non-pair constructors |
| Storage | Rust `HashMap<DictKey, Value>` | `Vector{Int64}` slots + `Vector{Any}` keys/vals |
| Dispatch | `::Dict` bare annotation | `::Dict{K,V} where {K,V}` annotation |
| Hash | Rust `Hash` trait | Julia `hash()` + open-addressing linear probing |

Both paths support standard operations (`getindex`, `setindex!`, `haskey`, `iterate`, etc.) through separate method definitions.

## Set Implementation

| Layer | Description |
|-------|-------------|
| Internal | `Value::Set` — Rust `HashSet`-backed primitive |
| Public API | Pure Julia wrappers in `src/julia/base/set.jl` (9.2KB) |
| Dispatch | 16 `BuiltinId` variants in dispatch chain |

## Broadcast Infrastructure

Broadcast operations are implemented in `src/julia/base/broadcast.jl` (68KB):

| Component | Description |
|-----------|-------------|
| `BroadcastStyle` hierarchy | Dispatch style selection for broadcasting |
| `Broadcasted` | Lazy broadcast wrapper |
| `Extruded` | Broadcast indexing helper |
| `DefaultArrayStyle` / `TupleBroadcastStyle` | Style types |

**Known workarounds**: Issues #2531, #2534, #2535-#2543.

## Range Types (6 types)

| Type | Description |
|------|-------------|
| `UnitRange{T}` | Consecutive integers (e.g., `1:10`) |
| `StepRange{T}` | Stepped range (e.g., `1:2:10`) |
| `LinRange` | Linearly spaced range |
| `StepRangeLen` | Stepped range with offset |
| `OneTo` | 1-based range (e.g., `Base.OneTo(5)`) |
| `LogRange` | Logarithmically spaced range |

All range types support `length`, `size`, `axes`, `getindex`, and `iterate`.

## Related Documentation

- `TYPE_SYSTEM.md` - Type representation and conversion
- `HOF_GUIDE.md` - Higher-order functions with collections
- `DICT_INDEXING.md` - Dict-specific indexing behavior
- `PURE_JULIA_DESIGN.md` - Pure Julia design philosophy
- `STATUS.md` - Milestone 9 (Dict{K,V}) status details

## Fixture Tests

- `subset_julia_vm/tests/fixtures/collections/copy_type_preservation.jl` - Copy type preservation
- `subset_julia_vm/tests/fixtures/collections/empty_basic.jl` - Empty function for all types
- `subset_julia_vm/tests/fixtures/collections/copy_set.jl` - Set-specific copy tests
