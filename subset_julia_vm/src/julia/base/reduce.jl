# =============================================================================
# Reduce - Reduction operations on collections
# =============================================================================
# Based on Julia's base/reduce.jl

# count(itr) - Count truthy values (identity predicate)
# Based on Julia's base/reduce.jl:750
# count(itr; init=0) = count(identity, itr; init)
# For boolean arrays, counts true values
function count(arr::Array)
    c = 0
    for i in 1:length(arr)
        # Direct truthiness check for Bool values
        if arr[i]
            c = c + 1
        end
    end
    return c
end

# Note: count(predicate, arr) is implemented as a builtin higher-order function
# Use: count(x -> x > 0, arr) or count(isodd, arr)

# extrema: return (minimum, maximum) as a tuple
# With dims keyword: (min, max) along specified dimension
function extrema(arr; dims=0)
    if dims == 0
        n = length(arr)
        minval = arr[1]
        maxval = arr[1]
        for i in 2:n
            if arr[i] < minval
                minval = arr[i]
            end
            if arr[i] > maxval
                maxval = arr[i]
            end
        end
        return (minval, maxval)
    end
    m = size(arr, 1)
    n = size(arr, 2)
    if dims == 1
        minval = arr[1, 1]
        maxval = arr[1, 1]
        for i in 2:m
            if arr[i, 1] < minval
                minval = arr[i, 1]
            end
            if arr[i, 1] > maxval
                maxval = arr[i, 1]
            end
        end
        first_result = (minval, maxval)
        result = _array_undef_from_dims(typeof(first_result), (1, n))
        result[1, 1] = first_result
        for j in 1:n
            if j == 1
                continue
            end
            minval = arr[1, j]
            maxval = arr[1, j]
            for i in 2:m
                if arr[i, j] < minval
                    minval = arr[i, j]
                end
                if arr[i, j] > maxval
                    maxval = arr[i, j]
                end
            end
            result[1, j] = (minval, maxval)
        end
        return result
    elseif dims == 2
        minval = arr[1, 1]
        maxval = arr[1, 1]
        for j in 2:n
            if arr[1, j] < minval
                minval = arr[1, j]
            end
            if arr[1, j] > maxval
                maxval = arr[1, j]
            end
        end
        first_result = (minval, maxval)
        result = _array_undef_from_dims(typeof(first_result), (m, 1))
        result[1, 1] = first_result
        for i in 1:m
            if i == 1
                continue
            end
            minval = arr[i, 1]
            maxval = arr[i, 1]
            for j in 2:n
                if arr[i, j] < minval
                    minval = arr[i, j]
                end
                if arr[i, j] > maxval
                    maxval = arr[i, j]
                end
            end
            result[i, 1] = (minval, maxval)
        end
        return result
    else
        error("extrema: dims must be 1 or 2 for matrices")
    end
end

# extrema(f, arr) -> (min(f(x)), max(f(x)))
# Return (minimum, maximum) of f applied to each element.
# Based on Julia's base/reduce.jl:797
function extrema(f::Function, arr)
    n = length(arr)
    fval = f(arr[1])
    minval = fval
    maxval = fval
    for i in 2:n
        fval = f(arr[i])
        if fval < minval
            minval = fval
        end
        if fval > maxval
            maxval = fval
        end
    end
    return (minval, maxval)
end

# Note: count(predicate, arr) is implemented as a builtin higher-order function
# Use: count(x -> x > 0, arr) or count(isodd, arr)

# findmax: return (maximum value, index)
function findmax(arr)
    n = length(arr)
    maxval = arr[1]
    maxidx = 1
    for i in 2:n
        if arr[i] > maxval
            maxval = arr[i]
            maxidx = i
        end
    end
    return (maxval, maxidx)
end

# findmax(f, domain) -> (f(x), index)
# Return the maximum value of f applied to elements, and its index.
# Based on Julia's base/reduce.jl:842
function findmax(f::Function, arr)
    n = length(arr)
    maxfval = f(arr[1])
    maxidx = 1
    for i in 2:n
        fval = f(arr[i])
        if fval > maxfval
            maxfval = fval
            maxidx = i
        end
    end
    return (maxfval, maxidx)
end

# findmax!: in-place version that stores result in pre-allocated arrays
# Based on Julia's base/reducedim.jl:1149
# Simplified for 1D arrays: stores max value in rval[1] and index in rind[1]
function findmax!(rval, rind, arr)
    result = findmax(arr)
    rval[1] = result[1]
    rind[1] = result[2]
    return (rval, rind)
end

# findmin: return (minimum value, index)
function findmin(arr)
    n = length(arr)
    minval = arr[1]
    minidx = 1
    for i in 2:n
        if arr[i] < minval
            minval = arr[i]
            minidx = i
        end
    end
    return (minval, minidx)
end

# findmin(f, domain) -> (f(x), index)
# Return the minimum value of f applied to elements, and its index.
# Based on Julia's base/reduce.jl:908
function findmin(f::Function, arr)
    n = length(arr)
    minfval = f(arr[1])
    minidx = 1
    for i in 2:n
        fval = f(arr[i])
        if fval < minfval
            minfval = fval
            minidx = i
        end
    end
    return (minfval, minidx)
end

# findmin!: in-place version that stores result in pre-allocated arrays
# Based on Julia's base/reducedim.jl:1076
# Simplified for 1D arrays: stores min value in rval[1] and index in rind[1]
function findmin!(rval, rind, arr)
    result = findmin(arr)
    rval[1] = result[1]
    rind[1] = result[2]
    return (rval, rind)
end

# diff: compute differences between consecutive elements
function diff(arr)
    n = length(arr)
    result = similar(arr, n - 1)
    for i in 1:(n-1)
        result[i] = arr[i+1] - arr[i]
    end
    return result
end

# argmax: return the index of the maximum element
# Based on Julia's base/reduce.jl:993
function argmax(arr)
    return findmax(arr)[2]
end

# argmax(f, domain) -> x
# Return the element x from domain that maximizes f(x).
# Based on Julia's base/reduce.jl:964
function argmax(f::Function, arr)
    idx = findmax(f, arr)[2]
    return arr[idx]
end

# argmin: return the index of the minimum element
# Based on Julia's base/reduce.jl:1051
function argmin(arr)
    return findmin(arr)[2]
end

# argmin(f, domain) -> x
# Return the element x from domain that minimizes f(x).
# Based on Julia's base/reduce.jl:1022
function argmin(f::Function, arr)
    idx = findmin(f, arr)[2]
    return arr[idx]
end

# Note: accumulate(op, arr) requires a function argument in Julia
# Use cumsum(arr) for cumulative sum instead (defined in functional.jl)

# Note: foldl, foldr, mapfoldl, mapfoldr functions
# These are implemented as builtins since they require calling function arguments
# (op parameter) which is not supported in Pure Julia due to SubsetJuliaVM's
# compile-time function resolution.
#
# Available functions:
#   - foldl(op, arr): left-associative fold: op(op(op(a, b), c), d)
#   - foldr(op, arr): right-associative fold: op(a, op(b, op(c, d)))
#   - mapfoldl(f, op, arr): map then left fold
#   - mapfoldr(f, op, arr): map then right fold
#
# See: foldl, foldr, mapfoldl, mapfoldr are exported from exports.jl
# Implementation is in the Rust VM: src/compile/expr/builtin_hof.rs and src/vm/exec/hof.rs

# =============================================================================
# any / all - Boolean reduction (non-HOF versions)
# =============================================================================
# Based on Julia's base/reduce.jl
# Note: any(f, arr) and all(f, arr) are implemented as builtin HOFs

# any: check if any element is true (non-HOF version)
function any(arr)
    for x in arr
        if x
            return true
        end
    end
    return false
end

# all: check if all elements are true (non-HOF version)
function all(arr)
    for x in arr
        if !x
            return false
        end
    end
    return true
end

# =============================================================================
# Predicate HOF reducers (Issue #3728)
# =============================================================================
# Public 2-arg HOF forms `any(f, arr)`, `all(f, arr)`, `count(f, arr)`,
# `findall(f, arr)`, `sum(f, arr)` previously routed to Rust builtins
# (`AnyFunc`, `AllFunc`, `CountFunc`, `FindAllFunc`, `SumFunc`). They are
# now Pure Julia methods that call the function value through normal
# dispatch — the VM already supports calling Function/Closure values from
# sjulia in this context, as demonstrated by `mapfoldl(f, op, arr)` in
# `iterators.jl`.

# any(f, arr): true if `f(x)` is truthy for any `x` in `arr`.
function any(f::Function, arr)
    for x in arr
        if f(x)
            return true
        end
    end
    return false
end

# all(f, arr): true if `f(x)` is truthy for every `x` in `arr`.
function all(f::Function, arr)
    for x in arr
        if !f(x)
            return false
        end
    end
    return true
end

# count(f, arr): count elements where `f(x)` is true.
# (Single-argument count(arr) and count(f, ::String) live elsewhere.)
function count(f::Function, arr::Array)
    n = 0
    for x in arr
        if f(x)
            n = n + 1
        end
    end
    return n
end

# count(f, t::Tuple): count elements for which `f` is true, by iteration. The
# builtin/Array `count` HOF does not accept a tuple, so `count(iseven, (1,2,3,4))`
# was an "Unknown function: count" error (Issue #5681).
function count(f::Function, t::Tuple)
    n = 0
    for x in t
        if f(x)
            n = n + 1
        end
    end
    return n
end

# count(f, itr): iterator-generic fallback for non-indexable collections such as
# Dict values/keys and Set (Issue #8816). More-specific Array/Tuple methods above
# retain their existing behavior.
function count(f::Function, itr)
    n = 0
    for x in itr
        if f(x)
            n = n + 1
        end
    end
    return n
end

# count(itr): count `true` elements of a boolean iterator, e.g.
# `count(x % 2 == 0 for x in 1:4)` (Issue #9103). Mirrors upstream
# `count(itr; init=0) = count(identity, itr; init)` (julia/base/reduce.jl).
# The Array method above retains its existing fast path.
function count(itr)
    return count(identity, itr)
end

# findall(f, arr): vector of 1-based indices where `f(arr[i])` is true.
function findall(f::Function, arr::Array)
    result = Int64[]
    n = length(arr)
    for i in 1:n
        if f(arr[i])
            push!(result, i)
        end
    end
    return result
end

# sum(f, arr): sum of `f(x)` for `x` in `arr`.
# Mirrors Julia: errors on empty input (no zero element to start with).
function sum(f::Function, arr::Array)
    n = length(arr)
    if n == 0
        throw(ArgumentError("reducing over an empty collection is not allowed"))
    end
    acc = f(arr[1])
    for i in 2:n
        acc = acc + f(arr[i])
    end
    return acc
end

function sum(f::Function, arr::AbstractArray)
    n = length(arr)
    if n == 0
        throw(ArgumentError("reducing over an empty collection is not allowed"))
    end
    acc = f(arr[1])
    for i in 2:n
        acc = acc + f(arr[i])
    end
    return acc
end

function sum(f::Function, t::Tuple)
    n = length(t)
    if n == 0
        throw(ArgumentError("reducing over an empty collection is not allowed"))
    end
    acc = f(t[1])
    for i in 2:n
        acc = acc + f(t[i])
    end
    return acc
end

function _sum_iterable(itr, init)
    if init !== nothing
        result = init
        for x in itr
            result = result + x
        end
        return result
    end

    result = zero(eltype(itr))
    for x in itr
        result = result + x
    end
    return result
end

function _prod_empty_iterable_value(itr)
    T = eltype(itr)
    if T == Bool
        return true
    elseif T == Int8 || T == Int16 || T == Int32 || T == Int64
        return Int64(1)
    elseif T == UInt8 || T == UInt16 || T == UInt32 || T == UInt64
        return UInt64(1)
    elseif T == Float32
        return Float32(1)
    elseif T == Float64
        return Float64(1)
    elseif T == String
        return ""
    end
    return 1
end

function _prod_iterable(itr, init)
    if init !== nothing
        result = init
        for x in itr
            result = result * x
        end
        return result
    end

    y = iterate(itr)
    if y === nothing
        return _prod_empty_iterable_value(itr)
    end

    result = y[1]
    state = y[2]
    y = iterate(itr, state)
    while y !== nothing
        result = result * y[1]
        state = y[2]
        y = iterate(itr, state)
    end
    return result
end

function _minimum_iterable(itr, init)
    if init !== nothing
        result = init
    else
        y = iterate(itr)
        if y === nothing
            error("minimum: empty collection")
        end
        result = y[1]
        state = y[2]
        y = iterate(itr, state)
        while y !== nothing
            if y[1] < result
                result = y[1]
            end
            state = y[2]
            y = iterate(itr, state)
        end
        return result
    end

    for x in itr
        if x < result
            result = x
        end
    end
    return result
end

function _maximum_iterable(itr, init)
    if init !== nothing
        result = init
    else
        y = iterate(itr)
        if y === nothing
            error("maximum: empty collection")
        end
        result = y[1]
        state = y[2]
        y = iterate(itr, state)
        while y !== nothing
            if y[1] > result
                result = y[1]
            end
            state = y[2]
            y = iterate(itr, state)
        end
        return result
    end

    for x in itr
        if x > result
            result = x
        end
    end
    return result
end

function _any_unary_predicate(f, A)
    n = length(A)
    for i in 1:n
        if f(A[i])
            return true
        end
    end
    return false
end

function _all_unary_predicate(f, A)
    n = length(A)
    for i in 1:n
        if !f(A[i])
            return false
        end
    end
    return true
end

function _count_unary_predicate(f, A)
    c = 0
    n = length(A)
    for i in 1:n
        if f(A[i])
            c = c + 1
        end
    end
    return c
end

function _findall_unary_predicate(f, A)
    result = Int64[]
    n = length(A)
    for i in 1:n
        if f(A[i])
            push!(result, i)
        end
    end
    return result
end

any(::typeof(iszero), A::Vector{Int32}) = _any_unary_predicate(iszero, A)
any(::typeof(isone), A::Vector{Int32}) = _any_unary_predicate(isone, A)
any(::typeof(signbit), A::Vector{Int32}) = _any_unary_predicate(signbit, A)
any(::typeof(iseven), A::Vector{Int32}) = _any_unary_predicate(iseven, A)
any(::typeof(isodd), A::Vector{Int32}) = _any_unary_predicate(isodd, A)

all(::typeof(iszero), A::Vector{Int32}) = _all_unary_predicate(iszero, A)
all(::typeof(isone), A::Vector{Int32}) = _all_unary_predicate(isone, A)
all(::typeof(signbit), A::Vector{Int32}) = _all_unary_predicate(signbit, A)
all(::typeof(iseven), A::Vector{Int32}) = _all_unary_predicate(iseven, A)
all(::typeof(isodd), A::Vector{Int32}) = _all_unary_predicate(isodd, A)

count(::typeof(iszero), A::Vector{Int32}) = _count_unary_predicate(iszero, A)
count(::typeof(isone), A::Vector{Int32}) = _count_unary_predicate(isone, A)
count(::typeof(signbit), A::Vector{Int32}) = _count_unary_predicate(signbit, A)
count(::typeof(iseven), A::Vector{Int32}) = _count_unary_predicate(iseven, A)
count(::typeof(isodd), A::Vector{Int32}) = _count_unary_predicate(isodd, A)

findall(::typeof(iszero), A::Vector{Int32}) = _findall_unary_predicate(iszero, A)
findall(::typeof(isone), A::Vector{Int32}) = _findall_unary_predicate(isone, A)
findall(::typeof(signbit), A::Vector{Int32}) = _findall_unary_predicate(signbit, A)
findall(::typeof(iseven), A::Vector{Int32}) = _findall_unary_predicate(iseven, A)
findall(::typeof(isodd), A::Vector{Int32}) = _findall_unary_predicate(isodd, A)
