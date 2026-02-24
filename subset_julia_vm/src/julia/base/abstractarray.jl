# =============================================================================
# abstractarray.jl - Abstract array utilities
# =============================================================================
# Based on Julia's base/abstractarray.jl
#
# This file contains array-related utilities that work with any iterable.

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
