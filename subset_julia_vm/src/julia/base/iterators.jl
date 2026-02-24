# =============================================================================
# iterators.jl - Iterator types and utilities
# =============================================================================
# Based on Julia's base/iterators.jl
#
# The iterate protocol:
#   iterate(collection) -> (element, state) | nothing
#   iterate(collection, state) -> (element, state) | nothing
#
# Note: Builtin types (Array, Tuple, Range, String) use VM instructions
# for iteration (IterateFirst/IterateNext). This file only defines iterate
# methods for custom iterator wrapper types.

# =============================================================================
# Enumerate - counter-based iteration wrapper
# =============================================================================
# Based on Julia's base/iterators.jl
#
# enumerate(iter) yields (i, x) where i is a counter starting at 1

struct Enumerate{I}
    itr::I
end

enumerate(iter) = Enumerate(iter)

function iterate(e::Enumerate)
    next = iterate(e.itr)
    if next === nothing
        return nothing
    end
    return ((1, next[1]), (2, next[2]))
end

function iterate(e::Enumerate, state)
    i = state[1]
    inner_state = state[2]
    next = iterate(e.itr, inner_state)
    if next === nothing
        return nothing
    end
    return ((i, next[1]), (i + 1, next[2]))
end

function length(e::Enumerate)
    return length(e.itr)
end

# =============================================================================
# Zip - parallel iteration over multiple collections
# =============================================================================
# Based on Julia's base/iterators.jl
#
# zip(a, b) yields (a[i], b[i]) until either is exhausted

struct Zip{I1, I2}
    itr1::I1
    itr2::I2
end

zip(a, b) = Zip(a, b)

function iterate(z::Zip)
    next1 = iterate(z.itr1)
    next2 = iterate(z.itr2)
    if next1 === nothing || next2 === nothing
        return nothing
    end
    return ((next1[1], next2[1]), (next1[2], next2[2]))
end

function iterate(z::Zip, state)
    state1 = state[1]
    state2 = state[2]
    next1 = iterate(z.itr1, state1)
    next2 = iterate(z.itr2, state2)
    if next1 === nothing || next2 === nothing
        return nothing
    end
    return ((next1[1], next2[1]), (next1[2], next2[2]))
end

function length(z::Zip)
    return min(length(z.itr1), length(z.itr2))
end

# =============================================================================
# Zip3 - parallel iteration over 3 collections (Issue #1990)
# =============================================================================

struct Zip3{I1, I2, I3}
    itr1::I1
    itr2::I2
    itr3::I3
end

zip(a, b, c) = Zip3(a, b, c)

function iterate(z::Zip3)
    next1 = iterate(z.itr1)
    next2 = iterate(z.itr2)
    next3 = iterate(z.itr3)
    if next1 === nothing || next2 === nothing || next3 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1]), (next1[2], next2[2], next3[2]))
end

function iterate(z::Zip3, state)
    next1 = iterate(z.itr1, state[1])
    next2 = iterate(z.itr2, state[2])
    next3 = iterate(z.itr3, state[3])
    if next1 === nothing || next2 === nothing || next3 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1]), (next1[2], next2[2], next3[2]))
end

function length(z::Zip3)
    return min(min(length(z.itr1), length(z.itr2)), length(z.itr3))
end

# =============================================================================
# Zip4 - parallel iteration over 4 collections (Issue #1990)
# =============================================================================

struct Zip4{I1, I2, I3, I4}
    itr1::I1
    itr2::I2
    itr3::I3
    itr4::I4
end

zip(a, b, c, d) = Zip4(a, b, c, d)

function iterate(z::Zip4)
    next1 = iterate(z.itr1)
    next2 = iterate(z.itr2)
    next3 = iterate(z.itr3)
    next4 = iterate(z.itr4)
    if next1 === nothing || next2 === nothing || next3 === nothing || next4 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1], next4[1]), (next1[2], next2[2], next3[2], next4[2]))
end

function iterate(z::Zip4, state)
    next1 = iterate(z.itr1, state[1])
    next2 = iterate(z.itr2, state[2])
    next3 = iterate(z.itr3, state[3])
    next4 = iterate(z.itr4, state[4])
    if next1 === nothing || next2 === nothing || next3 === nothing || next4 === nothing
        return nothing
    end
    return ((next1[1], next2[1], next3[1], next4[1]), (next1[2], next2[2], next3[2], next4[2]))
end

function length(z::Zip4)
    return min(min(min(length(z.itr1), length(z.itr2)), length(z.itr3)), length(z.itr4))
end

# =============================================================================
# Take - iterate first N elements
# =============================================================================
# Based on Julia's base/iterators.jl
#
# take(iter, n) yields at most the first n elements from iter

struct Take{I}
    xs::I
    n::Int64
end

take(xs, n::Integer) = Take(xs, Int64(n))

function iterate(it::Take)
    if it.n <= 0
        return nothing
    end
    next = iterate(it.xs)
    if next === nothing
        return nothing
    end
    return (next[1], (it.n - 1, next[2]))
end

function iterate(it::Take, state)
    n = state[1]
    if n <= 0
        return nothing
    end
    inner_state = state[2]
    next = iterate(it.xs, inner_state)
    if next === nothing
        return nothing
    end
    return (next[1], (n - 1, next[2]))
end

function length(it::Take)
    return it.n
end

# =============================================================================
# Drop - skip first N elements
# =============================================================================
# Based on Julia's base/iterators.jl
#
# drop(iter, n) skips the first n elements and yields the rest

struct Drop{I}
    xs::I
    n::Int64
end

drop(xs, n::Integer) = Drop(xs, Int64(n))

function iterate(it::Drop)
    y = iterate(it.xs)
    for i in 1:it.n
        if y === nothing
            return y
        end
        y = iterate(it.xs, y[2])
    end
    return y
end

function iterate(it::Drop, state)
    return iterate(it.xs, state)
end

function length(it::Drop)
    n = length(it.xs) - it.n
    if n < 0
        return 0
    end
    return n
end

# =============================================================================
# TakeWhile - yield elements while predicate is true
# =============================================================================
# Based on Julia's base/iterators.jl
#
# takewhile(pred, iter) yields elements from iter as long as pred returns true,
# then stops. Once pred returns false, no more elements are yielded.

struct TakeWhile{I, P}
    pred::P
    xs::I
end

"""
    takewhile(pred, iter)

An iterator that yields elements from `iter` as long as predicate `pred`
is true, afterwards drops every element.

# Examples
```julia
collect(takewhile(x -> x < 4, [1, 2, 3, 4, 5]))
# => [1, 2, 3]
```
"""
takewhile(pred, xs) = TakeWhile(pred, xs)

function iterate(ibl::TakeWhile)
    next = iterate(ibl.xs)
    if next === nothing
        return nothing
    end
    if !ibl.pred(next[1])
        return nothing
    end
    return next
end

function iterate(ibl::TakeWhile, state)
    next = iterate(ibl.xs, state)
    if next === nothing
        return nothing
    end
    if !ibl.pred(next[1])
        return nothing
    end
    return next
end

# =============================================================================
# DropWhile - skip elements while predicate is true, then yield the rest
# =============================================================================
# Based on Julia's base/iterators.jl
#
# dropwhile(pred, iter) skips elements from iter as long as pred returns true,
# then yields all remaining elements (even if pred becomes true again later).

struct DropWhile{I, P}
    pred::P
    xs::I
end

"""
    dropwhile(pred, iter)

An iterator that drops elements from `iter` as long as predicate `pred`
is true, afterwards returns every element.

# Examples
```julia
collect(dropwhile(x -> x < 3, [1, 2, 3, 4, 1]))
# => [3, 4, 1]
```
"""
dropwhile(pred, xs) = DropWhile(pred, xs)

function iterate(ibl::DropWhile)
    next = iterate(ibl.xs)
    while next !== nothing
        if !ibl.pred(next[1])
            return next
        end
        next = iterate(ibl.xs, next[2])
    end
    return nothing
end

function iterate(ibl::DropWhile, state)
    return iterate(ibl.xs, state)
end

# =============================================================================
# Collect - materialize iterator to array
# =============================================================================
# Generic collect using iterate protocol.
# Works for any iterator type that implements iterate().
#
# Note: For numeric iterators (Range, numeric arrays), the Rust builtin
# collect handles type preservation. This Pure Julia version is a fallback
# for custom iterators and uses Vector{Any} to preserve element types.

function collect(itr)
    # Use Vector{Any}() to preserve element types
    result = Vector{Any}()
    next = iterate(itr)
    while next !== nothing
        x, state = next
        # Preserve original element type
        push!(result, x)
        next = iterate(itr, state)
    end
    return result
end

# =============================================================================
# CartesianIndex - multi-dimensional index wrapper
# =============================================================================
# Based on Julia's base/multidimensional.jl
#
# CartesianIndex(i, j, k...) creates a multi-dimensional index
# A[I] is equivalent to A[i, j, k...]

struct CartesianIndex
    I
end

# Note: The struct constructor CartesianIndex(tuple) is auto-generated.
# We rely on the caller to pass a tuple directly, e.g., CartesianIndex((1, 2))
# For convenience in iteration, use CartesianIndex(args...) splat form.

# Access to index tuple
Tuple(ci::CartesianIndex) = ci.I

# Length (number of dimensions)
length(ci::CartesianIndex) = length(ci.I)

# Indexing into CartesianIndex
getindex(ci::CartesianIndex, i::Int64) = ci.I[i]

# Equality
==(a::CartesianIndex, b::CartesianIndex) = a.I == b.I

# Workaround: CartesianIndex arithmetic disabled — bare operators cannot be passed as function arguments (Issue #1985)
# +(a::CartesianIndex, b::CartesianIndex) = CartesianIndex(map(+, a.I, b.I)...)
# -(a::CartesianIndex, b::CartesianIndex) = CartesianIndex(map(-, a.I, b.I)...)

# Show
function show(io::IO, ci::CartesianIndex)
    print(io, "CartesianIndex(")
    for i in 1:length(ci.I)
        if i > 1
            print(io, ", ")
        end
        print(io, ci.I[i])
    end
    print(io, ")")
end

# =============================================================================
# CartesianIndices - iterator over all CartesianIndex in a region
# =============================================================================
# Based on Julia's base/multidimensional.jl
#
# CartesianIndices((m, n)) iterates over all (i, j) where 1 <= i <= m, 1 <= j <= n
# in column-major order (i varies fastest)

struct CartesianIndices
    dims
end

# Constructor from array
CartesianIndices(A::Array) = CartesianIndices(size(A))

# Size and length
size(ci::CartesianIndices) = ci.dims
function length(ci::CartesianIndices)
    n = length(ci.dims)
    if n == 0
        return 1  # Scalar case
    elseif n == 1
        return Int64(ci.dims[1])
    elseif n == 2
        return Int64(ci.dims[1]) * Int64(ci.dims[2])
    elseif n == 3
        return Int64(ci.dims[1]) * Int64(ci.dims[2]) * Int64(ci.dims[3])
    else
        return Int64(prod(ci.dims))
    end
end

# First and last indices
function first(ci::CartesianIndices)
    return CartesianIndex(_ones_tuple(length(ci.dims)))
end

function last(ci::CartesianIndices)
    return CartesianIndex(ci.dims)
end

# Helper: create tuple of ones
function _ones_tuple(n::Int64)
    if n == 0
        return ()
    elseif n == 1
        return (1,)
    elseif n == 2
        return (1, 1)
    elseif n == 3
        return (1, 1, 1)
    elseif n == 4
        return (1, 1, 1, 1)
    elseif n == 5
        return (1, 1, 1, 1, 1)
    elseif n == 6
        return (1, 1, 1, 1, 1, 1)
    elseif n == 7
        return (1, 1, 1, 1, 1, 1, 1)
    elseif n == 8
        return (1, 1, 1, 1, 1, 1, 1, 1)
    else
        error("CartesianIndices supports up to 8 dimensions")
    end
end

# Iteration protocol for CartesianIndices
# NOTE: iterate(::CartesianIndices) is handled by VM builtins in type_ops.rs
# for better performance and to avoid method dispatch issues during base loading.
# The VM builtin returns (CartesianIndex(indices), state) tuples.

# Show
function show(io::IO, ci::CartesianIndices)
    print(io, "CartesianIndices(")
    show(io, ci.dims)
    print(io, ")")
end

# =============================================================================
# eachindex - iterate over array indices
# =============================================================================
# For multi-dimensional arrays, returns CartesianIndices
# Note: The basic eachindex(arr) = 1:length(arr) is defined in range.jl for linear indexing.
# This version returns CartesianIndices for multi-dimensional iteration.

# eachindex for CartesianIndices-style iteration over arrays
function eachindex(::IndexCartesian, A::Array)
    return CartesianIndices(size(A))
end

# =============================================================================
# IndexStyle - Array indexing trait types
# =============================================================================
# Based on Julia's base/indices.jl
#
# IndexStyle is an abstract type used to describe the optimal indexing style
# for arrays. IndexLinear and IndexCartesian are its two subtypes.

"""
    IndexStyle

Abstract type for describing the optimal indexing style for arrays.
Subtypes are `IndexLinear` and `IndexCartesian`.
"""
abstract type IndexStyle end

"""
    IndexLinear()

Subtype of `IndexStyle` used to describe arrays which are optimally
indexed by one linear index.

A linear indexing style uses one integer index to describe the position
in the array (even if it's a multidimensional array).
"""
struct IndexLinear <: IndexStyle end

"""
    IndexCartesian()

Subtype of `IndexStyle` used to describe arrays which are optimally
indexed by a Cartesian index. This is the default for new custom
`AbstractArray` subtypes.

A Cartesian indexing style uses multiple integer indices to describe
the position in a multidimensional array, with exactly one index per dimension.
"""
struct IndexCartesian <: IndexStyle end

# =============================================================================
# LinearIndices - linear index iterator
# =============================================================================
# Based on Julia's base/indices.jl
#
# LinearIndices(A) returns 1:length(A) for iteration over linear indices
# Simplified implementation that stores the total length directly

struct LinearIndices
    len::Int64
end

# Constructor from tuple (dims) - compute product of dimensions
# Use explicit handling to avoid compile issues with tuple iteration
function LinearIndices(dims::Tuple)
    n = length(dims)
    if n == 0
        return LinearIndices(0)
    elseif n == 1
        return LinearIndices(Int64(dims[1]))
    elseif n == 2
        return LinearIndices(Int64(dims[1]) * Int64(dims[2]))
    elseif n == 3
        return LinearIndices(Int64(dims[1]) * Int64(dims[2]) * Int64(dims[3]))
    elseif n == 4
        return LinearIndices(Int64(dims[1]) * Int64(dims[2]) * Int64(dims[3]) * Int64(dims[4]))
    else
        # Fallback to prod for higher dimensions
        return LinearIndices(Int64(prod(dims)))
    end
end

# Length
length(li::LinearIndices) = li.len

# Iteration protocol - return linear indices 1:length
function iterate(li::LinearIndices)
    if li.len == 0
        return nothing
    end
    return (1, 2)
end

function iterate(li::LinearIndices, state::Int64)
    if state > li.len
        return nothing
    end
    return (state, state + 1)
end

# First and last indices
function first(li::LinearIndices)
    return 1
end

function last(li::LinearIndices)
    return li.len
end

# getindex for linear indices - just return the index
function getindex(li::LinearIndices, i::Int64)
    if i < 1 || i > li.len
        error("BoundsError")
    end
    return i
end

# Show
function show(io::IO, li::LinearIndices)
    print(io, "LinearIndices((1:", li.len, ",))")
end

# =============================================================================
# only - return single element from collection
# =============================================================================
# Based on Julia's base/iterators.jl
#
# only(x) returns the one and only element of collection x, and throws
# an ArgumentError if the collection has zero or more than one element.

function only(x)
    n = length(x)
    if n == 0
        error("ArgumentError: Collection is empty, must contain exactly one element")
    elseif n > 1
        error("ArgumentError: Collection has multiple elements, must contain exactly one element")
    end
    return x[1]
end

# =============================================================================
# EachCol - iterate over columns of a matrix
# =============================================================================
# Based on Julia's base/iterators.jl
#
# eachcol(A) yields each column of matrix A as a 1D array

struct EachCol
    mat
end

eachcol(A::Array) = EachCol(A)

function iterate(ec::EachCol)
    s = size(ec.mat)
    if length(s) < 2
        # 1D array: treat as single column
        return (ec.mat, 2)
    end
    ncols = s[2]
    if ncols == 0
        return nothing
    end
    # Return first column
    col = ec.mat[:, 1]
    return (col, 2)
end

function iterate(ec::EachCol, state::Int64)
    s = size(ec.mat)
    if length(s) < 2
        # 1D array: only one "column"
        return nothing
    end
    ncols = s[2]
    if state > ncols
        return nothing
    end
    col = ec.mat[:, state]
    return (col, state + 1)
end

function length(ec::EachCol)
    s = size(ec.mat)
    if length(s) < 2
        return 1  # 1D array has 1 "column"
    end
    return s[2]
end

# =============================================================================
# EachRow - iterate over rows of a matrix
# =============================================================================
# Based on Julia's base/iterators.jl
#
# eachrow(A) yields each row of matrix A as a 1D array

struct EachRow
    mat
end

eachrow(A::Array) = EachRow(A)

function iterate(er::EachRow)
    s = size(er.mat)
    if length(s) < 2
        # 1D array: each element is a "row"
        return iterate(er.mat)
    end
    nrows = s[1]
    if nrows == 0
        return nothing
    end
    # Return first row
    row = er.mat[1, :]
    return (row, 2)
end

function iterate(er::EachRow, state::Int64)
    s = size(er.mat)
    if length(s) < 2
        # 1D array: delegate to array iteration
        return iterate(er.mat, state)
    end
    nrows = s[1]
    if state > nrows
        return nothing
    end
    row = er.mat[state, :]
    return (row, state + 1)
end

function length(er::EachRow)
    s = size(er.mat)
    if length(s) < 2
        return length(er.mat)  # 1D array: each element is a "row"
    end
    return s[1]
end

# =============================================================================
# EachSlice - iterate over slices of an array along a specified dimension
# =============================================================================
# Based on Julia's Base.eachslice
#
# eachslice(A; dims) generalizes eachrow (dims=1) and eachcol (dims=2)

struct EachSlice
    mat
    dim::Int64
end

eachslice(A; dims) = EachSlice(A, dims)

function iterate(es::EachSlice)
    s = size(es.mat)
    if length(s) < 2
        if es.dim == 1
            return iterate(es.mat)
        else
            return (es.mat, 2)
        end
    end
    n = s[es.dim]
    if n == 0
        return nothing
    end
    if es.dim == 1
        slice = es.mat[1, :]
    else
        slice = es.mat[:, 1]
    end
    return (slice, 2)
end

function iterate(es::EachSlice, state::Int64)
    s = size(es.mat)
    if length(s) < 2
        if es.dim == 1
            return iterate(es.mat, state)
        else
            return nothing
        end
    end
    n = s[es.dim]
    if state > n
        return nothing
    end
    if es.dim == 1
        slice = es.mat[state, :]
    else
        slice = es.mat[:, state]
    end
    return (slice, state + 1)
end

function length(es::EachSlice)
    s = size(es.mat)
    if length(s) < 2
        if es.dim == 1
            return length(es.mat)
        else
            return 1
        end
    end
    return s[es.dim]
end

# =============================================================================
# SkipMissing - skip missing values in iteration
# =============================================================================
# Based on Julia's base/missing.jl
#
# skipmissing(itr) wraps an iterator to skip all missing values

struct SkipMissing
    x
end

skipmissing(itr) = SkipMissing(itr)

function iterate(itr::SkipMissing)
    next = iterate(itr.x)
    if next === nothing
        return nothing
    end
    val = next[1]
    state = next[2]
    if ismissing(val)
        return iterate(itr, state)
    end
    return (val, state)
end

function iterate(itr::SkipMissing, state)
    next = iterate(itr.x, state)
    if next === nothing
        return nothing
    end
    val = next[1]
    newstate = next[2]
    if ismissing(val)
        return iterate(itr, newstate)
    end
    return (val, newstate)
end

# Length is unknown without iteration
# (would need to count non-missing elements)

# =============================================================================
# Flatten - flatten nested iterables
# =============================================================================
# Based on Julia's Iterators.flatten
#
# flatten(iter) iterates over all elements of each element in iter
# Example: flatten([[1,2], [3,4]]) yields 1, 2, 3, 4

struct Flatten
    it
end

flatten(itr) = Flatten(itr)

function iterate(f::Flatten)
    outer_next = iterate(f.it)
    if outer_next === nothing
        return nothing
    end
    inner = outer_next[1]
    outer_state = outer_next[2]
    inner_next = iterate(inner)
    while inner_next === nothing
        outer_next = iterate(f.it, outer_state)
        if outer_next === nothing
            return nothing
        end
        inner = outer_next[1]
        outer_state = outer_next[2]
        inner_next = iterate(inner)
    end
    return (inner_next[1], (inner, inner_next[2], outer_state))
end

function iterate(f::Flatten, state)
    inner = state[1]
    inner_state = state[2]
    outer_state = state[3]
    inner_next = iterate(inner, inner_state)
    while inner_next === nothing
        outer_next = iterate(f.it, outer_state)
        if outer_next === nothing
            return nothing
        end
        inner = outer_next[1]
        outer_state = outer_next[2]
        inner_next = iterate(inner)
    end
    return (inner_next[1], (inner, inner_next[2], outer_state))
end

# =============================================================================
# flatmap - map then flatten
# =============================================================================
# Based on Julia's Iterators.flatmap (julia/base/iterators.jl:1371)
#
# flatmap(f, itr) applies f to each element then flattens the results
# In official Julia: flatmap(f, c...) = flatten(map(f, c...))
# Workaround: use FlatMap struct due to map transposition bug (Issue #2119)
# Issue #2115

struct FlatMap
    f
    itr
end

flatmap(f, itr) = FlatMap(f, itr)

function iterate(fm::FlatMap)
    # Get first outer element
    outer_next = iterate(fm.itr)
    if outer_next === nothing
        return nothing
    end
    outer_val = outer_next[1]
    outer_state = outer_next[2]
    # Apply f to get inner iterable
    inner = fm.f(outer_val)
    inner_next = iterate(inner)
    # Skip empty inner iterables
    while inner_next === nothing
        outer_next = iterate(fm.itr, outer_state)
        if outer_next === nothing
            return nothing
        end
        outer_val = outer_next[1]
        outer_state = outer_next[2]
        inner = fm.f(outer_val)
        inner_next = iterate(inner)
    end
    return (inner_next[1], (inner, inner_next[2], outer_state, fm))
end

function iterate(fm::FlatMap, state)
    inner = state[1]
    inner_state = state[2]
    outer_state = state[3]
    inner_next = iterate(inner, inner_state)
    # If inner exhausted, advance outer
    while inner_next === nothing
        outer_next = iterate(fm.itr, outer_state)
        if outer_next === nothing
            return nothing
        end
        outer_val = outer_next[1]
        outer_state = outer_next[2]
        inner = fm.f(outer_val)
        inner_next = iterate(inner)
    end
    return (inner_next[1], (inner, inner_next[2], outer_state, fm))
end

# =============================================================================
# Rest - return iterator skipping first element
# =============================================================================
# Based on Julia's Iterators.rest
#
# rest(iter) returns an iterator starting from the second element
# rest(iter, state) returns an iterator starting from the given state

struct Rest
    itr
    state
end

function rest(itr)
    first_next = iterate(itr)
    if first_next === nothing
        return Rest(itr, nothing)
    end
    return Rest(itr, first_next[2])
end

function rest(itr, state)
    return Rest(itr, state)
end

function iterate(r::Rest)
    if r.state === nothing
        return nothing
    end
    return iterate(r.itr, r.state)
end

function iterate(r::Rest, state)
    return iterate(r.itr, state)
end

# =============================================================================
# Cycle - infinite cyclic iterator
# =============================================================================
# Based on Julia's Iterators.cycle
#
# cycle(iter) repeats iter forever
# Warning: Creates infinite iterator! Use with take() or break

struct Cycle
    xs
end

cycle(itr) = Cycle(itr)

function iterate(c::Cycle)
    next = iterate(c.xs)
    if next === nothing
        return nothing  # Empty collection
    end
    return (next[1], next[2])
end

function iterate(c::Cycle, state)
    next = iterate(c.xs, state)
    if next === nothing
        # Restart from beginning
        next = iterate(c.xs)
        if next === nothing
            return nothing
        end
    end
    return (next[1], next[2])
end

# =============================================================================
# Repeated - repeat a value
# =============================================================================
# Based on Julia's Iterators.repeated
#
# repeated(x) repeats x forever
# repeated(x, n) repeats x exactly n times

struct Repeated
    x
    n::Int64  # -1 means infinite
end

repeated(x) = Repeated(x, -1)
repeated(x, n::Integer) = Repeated(x, Int64(n))

function iterate(r::Repeated)
    if r.n == 0
        return nothing
    end
    if r.n < 0
        return (r.x, -1)  # Infinite: state doesn't matter
    end
    return (r.x, r.n - 1)
end

function iterate(r::Repeated, remaining::Int64)
    if remaining == 0
        return nothing
    end
    if remaining < 0
        return (r.x, -1)  # Infinite
    end
    return (r.x, remaining - 1)
end

function length(r::Repeated)
    if r.n < 0
        error("Infinite iterator has no length")
    end
    return r.n
end

# =============================================================================
# Partition - group elements into chunks
# =============================================================================
# Based on Julia's Iterators.partition
#
# partition(iter, n) yields tuples/arrays of n consecutive elements
# Example: partition([1,2,3,4,5], 2) yields [1,2], [3,4], [5]

struct Partition
    xs
    n::Int64
end

partition(itr, n::Integer) = Partition(itr, Int64(n))

function iterate(p::Partition)
    chunk = []
    next = iterate(p.xs)
    if next === nothing
        return nothing
    end
    for i in 1:p.n
        if next === nothing
            break
        end
        push!(chunk, next[1])
        state = next[2]
        next = iterate(p.xs, state)
    end
    if length(chunk) == 0
        return nothing
    end
    if next === nothing
        return (chunk, nothing)
    end
    return (chunk, next)
end

function iterate(p::Partition, state)
    if state === nothing
        return nothing
    end
    chunk = []
    next = state
    for i in 1:p.n
        if next === nothing
            break
        end
        push!(chunk, next[1])
        inner_state = next[2]
        next = iterate(p.xs, inner_state)
    end
    if length(chunk) == 0
        return nothing
    end
    if next === nothing
        return (chunk, nothing)
    end
    return (chunk, next)
end

# =============================================================================
# Product - Cartesian product of iterables
# =============================================================================
# Based on Julia's Iterators.product
#
# product(a, b) yields all (x, y) where x in a, y in b
# Example: product([1,2], [3,4]) yields (1,3), (1,4), (2,3), (2,4)

struct Product
    a
    b
end

product(a, b) = Product(a, b)

function iterate(p::Product)
    a_next = iterate(p.a)
    if a_next === nothing
        return nothing
    end
    b_next = iterate(p.b)
    if b_next === nothing
        return nothing
    end
    # State: (a_val, a_state, b_state, b_first_state)
    # b_first_state is used to restart b when advancing a
    return ((a_next[1], b_next[1]), (a_next[1], a_next[2], b_next[2], b_next[2]))
end

function iterate(p::Product, state)
    a_val = state[1]
    a_state = state[2]
    b_state = state[3]
    b_first_state = state[4]

    # Try advancing b
    b_next = iterate(p.b, b_state)
    if b_next !== nothing
        return ((a_val, b_next[1]), (a_val, a_state, b_next[2], b_first_state))
    end

    # b exhausted, advance a and restart b
    a_next = iterate(p.a, a_state)
    if a_next === nothing
        return nothing
    end

    # Restart b from beginning
    b_restart = iterate(p.b)
    if b_restart === nothing
        return nothing
    end

    return ((a_next[1], b_restart[1]), (a_next[1], a_next[2], b_restart[2], b_restart[2]))
end

function length(p::Product)
    return length(p.a) * length(p.b)
end

# =============================================================================
# EachSplit - string split iterator
# =============================================================================
# Based on Julia's base/strings/util.jl
#
# eachsplit(str, delim) yields substrings split by delimiter
# Simplified version without limit/keepempty options

struct EachSplit
    str::String
    delim::String
end

eachsplit(str::String, delim::String) = EachSplit(str, delim)
eachsplit(str::String, delim::Char) = EachSplit(str, string(delim))

# Default: split on whitespace
eachsplit(str::String) = EachSplit(str, " ")

function iterate(es::EachSplit)
    n = length(es.str)
    if n == 0
        return nothing
    end
    # Find first delimiter
    idx = findfirst(es.delim, es.str)
    if idx === nothing
        # No delimiter, return whole string
        return (es.str, n + 1)
    end
    # Return substring before delimiter
    i = first(idx)
    if i == 1
        # Empty first part, skip to next
        start = length(es.delim) + 1
        if start > n
            return nothing
        end
        rest = es.str[start:n]
        return iterate(EachSplit(rest, es.delim))
    end
    substr = es.str[1:i-1]
    return (substr, i + length(es.delim))
end

function iterate(es::EachSplit, state::Int64)
    n = length(es.str)
    if state > n
        return nothing
    end
    rest = es.str[state:n]
    if length(rest) == 0
        return nothing
    end
    # Find next delimiter
    idx = findfirst(es.delim, rest)
    if idx === nothing
        # No more delimiters, return rest
        return (rest, n + 1)
    end
    i = first(idx)
    if i == 1
        # Empty part, skip
        new_start = state + length(es.delim)
        if new_start > n
            return nothing
        end
        return iterate(es, new_start)
    end
    substr = rest[1:i-1]
    return (substr, state + i - 1 + length(es.delim))
end

# =============================================================================
# EachRSplit - reverse string split iterator (Issue #1994)
# =============================================================================
# Based on Julia's base/strings/util.jl (lines 806-898)
#
# eachrsplit(str, delim) yields substrings split by delimiter,
# iterating from right to left.
# Unlike eachsplit which yields left-to-right, eachrsplit yields
# the rightmost substring first.

struct EachRSplit
    str::String
    delim::String
end

eachrsplit(str::String, delim::String) = EachRSplit(str, delim)
eachrsplit(str::String, delim::Char) = EachRSplit(str, string(delim))

# Default: split on whitespace
eachrsplit(str::String) = EachRSplit(str, " ")

function iterate(ers::EachRSplit)
    n = length(ers.str)
    if n == 0
        return nothing
    end
    # Find last delimiter
    idx = findlast(ers.delim, ers.str)
    if idx === nothing
        # No delimiter, return whole string and signal done
        return (ers.str, 0)
    end
    i = first(idx)
    dlen = length(ers.delim)
    # Return substring after the last delimiter
    start = i + dlen
    if start > n
        # Empty trailing part, skip to searching in remaining string
        return iterate(ers, i - 1)
    end
    substr = ers.str[start:n]
    return (substr, i - 1)
end

function iterate(ers::EachRSplit, state::Int64)
    if state <= 0
        if state == 0
            return nothing
        end
        return nothing
    end
    # Search within str[1:state]
    part = ers.str[1:state]
    idx = findlast(ers.delim, part)
    if idx === nothing
        # No more delimiters, return remaining string
        return (part, 0)
    end
    i = first(idx)
    dlen = length(ers.delim)
    start = i + dlen
    if start > state
        # Empty part between delimiters, skip
        return iterate(ers, i - 1)
    end
    substr = ers.str[start:state]
    return (substr, i - 1)
end

# =============================================================================
# Count - infinite counting iterator
# =============================================================================
# Based on Julia's base/iterators.jl
#
# countfrom(start, step) yields start, start+step, start+2*step, ...
# Warning: Creates infinite iterator! Use with take() or break

struct Count{T}
    start::T
    step::T
end

countfrom(start::Int64, step::Int64) = Count{Int64}(start, step)
countfrom(start::Float64, step::Float64) = Count{Float64}(start, step)
countfrom(start::Int64, step::Float64) = Count{Float64}(Float64(start), step)
countfrom(start::Float64, step::Int64) = Count{Float64}(start, Float64(step))
countfrom(start::Int64) = Count{Int64}(start, Int64(1))
countfrom(start::Float64) = Count{Float64}(start, 1.0)
countfrom() = Count{Int64}(Int64(1), Int64(1))

function iterate(c::Count{Int64})
    return (c.start, c.start + c.step)
end

function iterate(c::Count{Int64}, state::Int64)
    return (state, state + c.step)
end

function iterate(c::Count{Float64})
    return (c.start, c.start + c.step)
end

function iterate(c::Count{Float64}, state::Float64)
    return (state, state + c.step)
end

# =============================================================================
# Peel - split iterator into first element and rest
# =============================================================================
# Based on Julia's base/iterators.jl
#
# peel(iter) returns (first_element, rest_iterator) or nothing if empty
# This is useful for extracting the first element while keeping the rest
# as an iterator.
#
# Examples:
#   peel([1, 2, 3]) => (1, Rest([1,2,3], state))
#   peel([]) => nothing

# NOTE: Due to Issue #777 (Union{Nothing, Tuple} return type bug), this function
# does NOT work correctly for empty iterators. When called with an empty iterator,
# the VM has type inference issues.
# Workaround: Check if the iterator is empty before calling peel.
# Example: if iterate(itr) !== nothing; result = peel(itr); end
function peel(itr)
    y = iterate(itr)
    result = nothing
    if y !== nothing
        val = y[1]
        s = y[2]
        result = (val, rest(itr, s))
    end
    return result
end

# =============================================================================
# Nth - get the nth element of an iterator
# =============================================================================
# Based on Julia's base/iterators.jl
#
# nth(itr, n) returns the nth element of the iterator, or throws BoundsError
# if the iterator has fewer than n elements.
#
# This is a simplified implementation that works with any iterable.
# Unlike Julia's full implementation, we don't use IteratorSize traits.
#
# Examples:
#   nth([1, 2, 3], 2) => 2
#   nth(1:10, 5) => 5
#   nth(enumerate([10, 20, 30]), 2) => (2, 20)

"""
    nth(itr, n::Integer)

Get the `n`th element of an iterable collection.
Throws a `BoundsError` if the iterator doesn't have `n` elements.

# Examples
```julia
julia> nth(2:2:10, 4)
8

julia> nth([10, 20, 30], 2)
20

julia> nth(enumerate([5, 6, 7]), 2)
(2, 6)
```

See also: [`first`](@ref), [`last`](@ref)
"""
function nth(itr, n::Int64)
    n > 0 || error("BoundsError: nth index must be positive")
    y = iterate(itr)
    i = 1
    while i < n
        if y === nothing
            error("BoundsError: iterator exhausted before reaching index $n")
        end
        y = iterate(itr, y[2])
        i = i + 1
    end
    if y === nothing
        error("BoundsError: iterator exhausted before reaching index $n")
    end
    return y[1]
end

# Optimized version for arrays using direct indexing
function nth(arr::Array, n::Int64)
    n > 0 || error("BoundsError: nth index must be positive")
    n <= length(arr) || error("BoundsError: index $n out of bounds for array of length $(length(arr))")
    return arr[n]
end

# Optimized version for ranges using direct indexing
function nth(r::UnitRange, n::Int64)
    n > 0 || error("BoundsError: nth index must be positive")
    n <= length(r) || error("BoundsError: index $n out of bounds for range of length $(length(r))")
    return first(r) + n - 1
end

function nth(r::StepRange, n::Int64)
    n > 0 || error("BoundsError: nth index must be positive")
    n <= length(r) || error("BoundsError: index $n out of bounds for range of length $(length(r))")
    return first(r) + (n - 1) * step(r)
end

# =============================================================================
# Higher-Order Functions - Pure Julia implementations
# =============================================================================
# Based on Julia's base/abstractarray.jl
#
# These implementations use Generator and collect to transform collections
# using user-defined functions.
#
# Note: This requires the field function call feature (Issue #1357) to work.

# =============================================================================
# map - apply function to each element
# =============================================================================
# Based on Julia's base/abstractarray.jl:3420
#
# map(f, A) returns a new collection with f applied to each element of A
#
# Examples:
#   map(x -> x * 2, [1, 2, 3]) => [2, 4, 6]
#   map(abs, [-1, 2, -3]) => [1, 2, 3]

"""
    map(f, A)

Apply function `f` to each element of collection `A`, returning a new collection
with the results.

# Examples
```julia
julia> map(x -> x * 2, [1, 2, 3])
[2, 4, 6]

julia> map(abs, [-1, 2, -3])
[1, 2, 3]
```
"""
map(f::Function, A) = collect(Generator(f, A))

# map(f, A, B) - apply binary function to corresponding elements of two collections
# Based on Julia's base/abstractarray.jl
function map(f::Function, A, B)
    result = []
    iter = iterate(zip(A, B))
    while iter !== nothing
        (pair, state) = iter
        push!(result, f(pair[1], pair[2]))
        iter = iterate(zip(A, B), state)
    end
    return result
end

# =============================================================================
# Filter - iterator wrapper for filtering
# =============================================================================
# Based on Julia's base/iterators.jl
#
# Filter wraps an iterator and yields only elements for which the predicate
# function returns true.

struct Filter
    flt::Function
    itr
end

function iterate(f::Filter)
    y = iterate(f.itr)
    while y !== nothing
        if f.flt(y[1])
            return (y[1], y[2])
        end
        y = iterate(f.itr, y[2])
    end
    return nothing
end

function iterate(f::Filter, state)
    y = iterate(f.itr, state)
    while y !== nothing
        if f.flt(y[1])
            return (y[1], y[2])
        end
        y = iterate(f.itr, y[2])
    end
    return nothing
end

# =============================================================================
# filter - select elements satisfying predicate
# =============================================================================
# Based on Julia's base/array.jl
#
# filter(f, A) returns a new collection containing only elements x for which f(x) is true
#
# Examples:
#   filter(iseven, [1, 2, 3, 4, 5]) => [2, 4]
#   filter(x -> x > 0, [-1, 2, -3, 4]) => [2, 4]

"""
    filter(f, A)

Return a new collection containing only elements of `A` for which `f` returns `true`.

# Examples
```julia
julia> filter(iseven, [1, 2, 3, 4, 5])
[2, 4]

julia> filter(x -> x > 0, [-1, 2, -3, 4])
[2, 4]
```
"""
filter(f::Function, A) = collect(Filter(f, A))

# =============================================================================
# reduce/foldl - reduce collection to single value
# =============================================================================
# Based on Julia's base/reduce.jl
#
# reduce(op, itr) combines elements using the binary operator op
# foldl(op, itr) is left-fold (same as reduce)
#
# Examples:
#   reduce(+, [1, 2, 3, 4]) => 10
#   reduce(*, [1, 2, 3, 4]) => 24

"""
    reduce(op, itr)
    reduce(op, itr, init)

Reduce `itr` using the binary operator `op`. The optional `init` argument
provides the initial value.

# Examples
```julia
julia> reduce(+, [1, 2, 3, 4])
10

julia> reduce(*, [1, 2, 3, 4])
24
```
"""
function reduce(op::Function, itr)
    y = iterate(itr)
    if y === nothing
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = y[1]
    y = iterate(itr, y[2])
    while y !== nothing
        acc = op(acc, y[1])
        y = iterate(itr, y[2])
    end
    return acc
end

function reduce(op::Function, itr, init)
    acc = init
    y = iterate(itr)
    while y !== nothing
        acc = op(acc, y[1])
        y = iterate(itr, y[2])
    end
    return acc
end

# Keyword argument form: reduce(op, itr; init=val) is handled at compiler level
# in call.rs by converting to reduce(op, itr, val) (Issue #2077, #2084)

# foldl is an alias for reduce (left-fold)
foldl(op::Function, itr) = reduce(op, itr)
foldl(op::Function, itr, init) = reduce(op, itr, init)
# Keyword argument form: foldl(op, itr; init=val) is handled at compiler level
# in call.rs by converting to foldl(op, itr, val) (Issue #2077, #2084)

# =============================================================================
# foldr - right fold
# =============================================================================
# Based on Julia's base/reduce.jl
#
# foldr(op, itr) combines elements from right to left
#
# Examples:
#   foldr(-, [1, 2, 3]) => 1 - (2 - 3) = 2

"""
    foldr(op, itr)
    foldr(op, itr, init)

Right-fold `itr` using the binary operator `op`.

# Examples
```julia
julia> foldr(-, [1, 2, 3])
2  # = 1 - (2 - 3) = 1 - (-1) = 2
```
"""
function foldr(op::Function, itr)
    # Collect to array first, then fold from right
    arr = collect(itr)
    n = length(arr)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = arr[n]
    i = n - 1
    while i >= 1
        acc = op(arr[i], acc)
        i = i - 1
    end
    return acc
end

function foldr(op::Function, itr, init)
    arr = collect(itr)
    n = length(arr)
    acc = init
    i = n
    while i >= 1
        acc = op(arr[i], acc)
        i = i - 1
    end
    return acc
end

# Keyword argument form: foldr(op, itr; init=val) is handled at compiler level
# in call.rs by converting to foldr(op, itr, val) (Issue #2077, #2084)

# =============================================================================
# mapfoldl - left fold with transformation
# =============================================================================
# Based on Julia's base/reduce.jl
#
# mapfoldl(f, op, itr) applies f to each element, then left-folds with op
# mapfoldl(f, op, itr, init) starts accumulation from init
#
# Examples:
#   mapfoldl(x -> x^2, +, [1, 2, 3]) => 1 + 4 + 9 = 14
#   mapfoldl(x -> x^2, -, [1, 2, 3]) => (1 - 4) - 9 = -12

function mapfoldl(f::Function, op::Function, itr)
    y = iterate(itr)
    if y === nothing
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = f(y[1])
    y = iterate(itr, y[2])
    while y !== nothing
        acc = op(acc, f(y[1]))
        y = iterate(itr, y[2])
    end
    return acc
end

function mapfoldl(f::Function, op::Function, itr, init)
    acc = init
    y = iterate(itr)
    while y !== nothing
        acc = op(acc, f(y[1]))
        y = iterate(itr, y[2])
    end
    return acc
end

# Keyword argument form: mapfoldl(f, op, itr; init=val) is handled at compiler level
# in call.rs by converting to mapfoldl(f, op, itr, val) (Issue #2077)

# =============================================================================
# mapfoldr - right fold with transformation
# =============================================================================
# Based on Julia's base/reduce.jl
#
# mapfoldr(f, op, itr) applies f to each element, then right-folds with op
# mapfoldr(f, op, itr, init) starts accumulation from init
#
# Examples:
#   mapfoldr(x -> x^2, -, [1, 2, 3]) => 1 - (4 - 9) = 6
#   mapfoldr(x -> x + 1, +, [1, 2, 3]) => 2 + (3 + 4) = 9

function mapfoldr(f::Function, op::Function, itr)
    arr = collect(itr)
    n = length(arr)
    if n == 0
        error("ArgumentError: reducing over an empty collection is not allowed")
    end
    acc = f(arr[n])
    i = n - 1
    while i >= 1
        acc = op(f(arr[i]), acc)
        i = i - 1
    end
    return acc
end

function mapfoldr(f::Function, op::Function, itr, init)
    arr = collect(itr)
    n = length(arr)
    acc = init
    i = n
    while i >= 1
        acc = op(f(arr[i]), acc)
        i = i - 1
    end
    return acc
end

# Keyword argument form: mapfoldr(f, op, itr; init=val) is handled at compiler level
# in call.rs by converting to mapfoldr(f, op, itr, val) (Issue #2077)

# =============================================================================
# mapreduce - map and reduce (alias for mapfoldl)
# =============================================================================
# Based on Julia's base/reduce.jl:305
# mapreduce(f, op, itr) is an alias for mapfoldl(f, op, itr)

mapreduce(f::Function, op::Function, itr) = mapfoldl(f, op, itr)
mapreduce(f::Function, op::Function, itr, init) = mapfoldl(f, op, itr, init)

# Keyword argument form: mapreduce(f, op, itr; init=val) is handled at compiler level
# in call.rs by converting to mapreduce(f, op, itr, val) (Issue #2077)
