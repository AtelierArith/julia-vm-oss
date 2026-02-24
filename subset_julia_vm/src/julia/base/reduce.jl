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
        result_min = zeros(1, n)
        result_max = zeros(1, n)
        for j in 1:n
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
            result_min[1, j] = minval
            result_max[1, j] = maxval
        end
        result = []
        for j in 1:n
            push!(result, (result_min[1, j], result_max[1, j]))
        end
        return result
    elseif dims == 2
        result = []
        for i in 1:m
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
            push!(result, (minval, maxval))
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
    result = zeros(n - 1)
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
# Note: any(f, arr) is implemented as a builtin HOF
function any(arr)
    n = length(arr)
    for i in 1:n
        if arr[i]
            return true
        end
    end
    return false
end

# all: check if all elements are true (non-HOF version)
# Note: all(f, arr) is implemented as a builtin HOF
function all(arr)
    n = length(arr)
    for i in 1:n
        if !arr[i]
            return false
        end
    end
    return true
end
