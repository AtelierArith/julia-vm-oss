# =============================================================================
# accumulate.jl - Cumulative operations on collections
# =============================================================================
# Based on Julia's base/accumulate.jl

# cumsum: cumulative sum
function cumsum(arr)
    n = length(arr)
    result = zeros(n)
    result[1] = arr[1]
    for i in 2:n
        result[i] = result[i-1] + arr[i]
    end
    return result
end

# cumprod: cumulative product
function cumprod(arr)
    n = length(arr)
    result = zeros(n)
    result[1] = arr[1]
    for i in 2:n
        result[i] = result[i-1] * arr[i]
    end
    return result
end

# accumulate: generalized cumulative operation (Issue #1839)
# accumulate(op, A) applies op cumulatively to elements of A,
# returning a vector of all intermediate values.
# This is the generalization of cumsum (op=+) and cumprod (op=*).

function accumulate(op::Function, A)
    y = iterate(A)
    if y === nothing
        return []
    end
    acc = y[1]
    result = [acc]
    y = iterate(A, y[2])
    while y !== nothing
        acc = op(acc, y[1])
        push!(result, acc)
        y = iterate(A, y[2])
    end
    return result
end

function accumulate(op::Function, A, init)
    result = []
    acc = init
    y = iterate(A)
    while y !== nothing
        acc = op(acc, y[1])
        push!(result, acc)
        y = iterate(A, y[2])
    end
    return result
end

# =============================================================================
# In-place cumulative operations
# =============================================================================
# Based on Julia's base/accumulate.jl

# cumsum!: cumulative sum of A, storing result in B
function cumsum!(B, A)
    n = length(A)
    B[1] = A[1]
    for i in 2:n
        B[i] = B[i-1] + A[i]
    end
    return B
end

# cumprod!: cumulative product of A, storing result in B
function cumprod!(B, A)
    n = length(A)
    B[1] = A[1]
    for i in 2:n
        B[i] = B[i-1] * A[i]
    end
    return B
end

# accumulate!: generalized in-place cumulative operation
# accumulate!(op, B, A) applies op cumulatively to elements of A,
# storing all intermediate values in B.
function accumulate!(op::Function, B, A)
    n = length(A)
    B[1] = A[1]
    for i in 2:n
        B[i] = op(B[i-1], A[i])
    end
    return B
end
