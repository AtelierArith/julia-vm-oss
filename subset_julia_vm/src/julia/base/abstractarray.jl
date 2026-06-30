# =============================================================================
# abstractarray.jl - Abstract array utilities
# =============================================================================
# Based on Julia's base/abstractarray.jl
#
# This file contains array-related utilities that work with any iterable.

# =============================================================================
# eltype - iterator element type protocol
# =============================================================================
# Based on Julia's base/abstractarray.jl:243-245. Type-specific eltype methods
# should be defined on `::Type{T}`; the value fallback delegates through
# `typeof(x)` so custom iterators work with Base.IteratorEltype's default.

function eltype(::Type)
    return Any
end

function eltype(x)
    return eltype(typeof(x))
end

function length(A::AbstractArray)
    dims = size(A)
    n = 1
    for d in dims
        n *= d
    end
    return n
end

function size(A::AbstractArray, d::Int64)
    return size(A)[d]
end

function getindex(A::AbstractMatrix, k::Integer)
    rows = size(A, 1)
    i = ((k - 1) % rows) + 1
    j = div(k - 1, rows) + 1
    return A[i, j]
end

# =============================================================================
# foreach - apply function to each element for side effects
# =============================================================================
# Based on Julia's base/abstractarray.jl
#
# foreach(f, c...) -> nothing
#
# Call function f on each element of iterable c.
# For multiple iterable arguments, f is called elementwise, and iteration
# stops when any iterator is finished.
#
# foreach should be used instead of map when the results of f are not
# needed, for example in foreach(println, array).

"""
    foreach(f, itr) -> nothing

Call function `f` on each element of iterable `itr`.

# Examples
```julia
julia> foreach(println, [1, 2, 3])
1
2
3
```
"""
function foreach(f::Function, itr)
    for x in itr
        f(x)
    end
    return nothing
end

"""
    foreach(f, itr1, itr2) -> nothing

Call function `f` on corresponding elements from `itr1` and `itr2`.
Iteration stops when either iterator is exhausted.

# Examples
```julia
julia> foreach((x, y) -> println(x, " -> ", y), [1, 2], ["a", "b"])
1 -> a
2 -> b
```
"""
function foreach(f::Function, itr1, itr2)
    for (x, y) in zip(itr1, itr2)
        f(x, y)
    end
    return nothing
end

# =============================================================================
# sizehint! - hint for expected collection size (no-op)
# =============================================================================
# Based on Julia's base/abstractarray.jl
#
# sizehint!(v, n) -> v
#
# Suggest that collection v reserve capacity for at least n elements.
# This is a performance hint only and has no effect on behavior.
# Returns the collection unchanged.

"""
    sizehint!(v, n) -> v

Suggest that collection `v` reserve capacity for at least `n` elements.
This can improve performance of subsequent `push!` operations.
Returns `v` unchanged.

# Examples
```julia
julia> a = Int64[]; sizehint!(a, 100); push!(a, 1); length(a)
1
```
"""
sizehint!(a, _) = a

# =============================================================================
# Generic AbstractArray element-wise equality (Issue #8229)
# =============================================================================
# Based on Julia's base/abstractarray.jl:3085 (`isequal`) and :3126 (`==`),
# which compare two AbstractArrays element-wise by iterating both with
# `zip(A, B)` after a shape check.
#
# `isequal(A::AbstractArray, B::AbstractArray)` element-compares two arrays
# through the `size`/`getindex` protocol. It is needed for AbstractArray
# subtypes the equality builtin cannot read — a user `struct <: AbstractArray`
# (generic struct ref) or a `SubArray` view — for which the generic
# `isequal(x, y) = x === y` fallback otherwise wins dispatch and returns `false`.
# It also backs the `isequal` Rust builtin's dispatch fallback for those same
# operands when they reach the builtin via the `==` operator (Issue #8229).
#
# A native `Array`/`Memory` carrier is also `<: AbstractArray`, but a statically
# typed native-array `isequal`/`==` is routed straight to the Rust builtin fast
# path by the compiler and never enters this method; it is reached only for the
# generic struct carriers. There is deliberately NO matching `==(::AbstractArray,
# ::AbstractArray)` method: the binary-op codegen statically resolves `==` and
# would coerce a `Memory`/native operand to `Array`, so `==` on these structs is
# instead routed through the `isequal` builtin by the gate in
# `compile_binary_op`. Element count is computed locally from `size` (via
# `_abstractarray_count`, NOT `length`); arrays in this VM are 1-based, so the
# `size` equality check is equivalent to upstream's `axes` check.

function _abstractarray_count(A::AbstractArray)
    n = 1
    for d in size(A)
        n *= d
    end
    return n
end

function isequal(A::AbstractArray, B::AbstractArray)
    if size(A) != size(B)
        return false
    end
    for i in 1:_abstractarray_count(A)
        if !isequal(A[i], B[i])
            return false
        end
    end
    return true
end

# =============================================================================
# stride / strides - Memory stride for column-major arrays
# =============================================================================
# Based on Julia's base/abstractarray.jl:577-607
#
# For column-major arrays (Julia's default storage order):
#   stride(A, 1) = 1
#   stride(A, k) = prod(size(A, i) for i in 1:k-1)

# strides(A): return tuple of strides for each dimension
function strides(A)
    nd = ndims(A)
    if nd == 1
        return (1,)
    elseif nd == 2
        return (1, size(A, 1))
    elseif nd == 3
        return (1, size(A, 1), size(A, 1) * size(A, 2))
    else
        error("strides: only 1D, 2D, and 3D arrays are supported")
    end
end

# stride(A, k): return stride for dimension k
function stride(A, k::Int64)
    nd = ndims(A)
    if k == 1
        return 1
    elseif k == 2
        return size(A, 1)
    elseif k == 3
        return size(A, 1) * size(A, 2)
    elseif k > nd
        # For dimensions beyond ndims, stride is the total number of elements
        s = 1
        for i in 1:nd
            s = s * size(A, i)
        end
        return s
    else
        error("stride: dimension $k out of range for $(nd)D array")
    end
end
