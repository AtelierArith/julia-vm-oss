# =============================================================================
# accumulate.jl - Cumulative operations on collections
# =============================================================================
# Based on Julia's base/accumulate.jl

function _cumsum_into!(result, arr)
    n = length(arr)
    if n == 0
        return result
    end
    result[1] = arr[1]
    for i in 2:n
        result[i] = result[i-1] + arr[i]
    end
    return result
end

function _cumprod_into!(result, arr)
    n = length(arr)
    if n == 0
        return result
    end
    result[1] = arr[1]
    for i in 2:n
        result[i] = result[i-1] * arr[i]
    end
    return result
end

function _cumsum_bool_into!(result, arr)
    n = length(arr)
    if n == 0
        return result
    end
    acc = Int64(arr[1])
    result[1] = acc
    for i in 2:n
        acc = acc + Int64(arr[i])
        result[i] = acc
    end
    return result
end

function _cumprod_bool_into!(result, arr)
    n = length(arr)
    if n == 0
        return result
    end
    acc = arr[1]
    result[1] = acc
    for i in 2:n
        acc = acc && arr[i]
        result[i] = acc
    end
    return result
end

# cumsum: cumulative sum
function cumsum(arr)
    n = length(arr)
    return _cumsum_into!(_array_undef_from_dims(Float64, (n,)), arr)
end

cumsum(arr::Vector{Int64}) = _cumsum_into!(_array_undef_from_dims(Int64, (length(arr),)), arr)
cumsum(arr::Vector{Int8}) = _cumsum_into!(_array_undef_from_dims(Int64, (length(arr),)), arr)
cumsum(arr::Vector{Int16}) = _cumsum_into!(_array_undef_from_dims(Int64, (length(arr),)), arr)
cumsum(arr::Vector{Int32}) = _cumsum_into!(_array_undef_from_dims(Int64, (length(arr),)), arr)
cumsum(arr::Vector{Bool}) = _cumsum_bool_into!(_array_undef_from_dims(Int64, (length(arr),)), arr)
cumsum(arr::Vector{UInt8}) = _cumsum_into!(_array_undef_from_dims(UInt64, (length(arr),)), arr)
cumsum(arr::Vector{UInt16}) = _cumsum_into!(_array_undef_from_dims(UInt64, (length(arr),)), arr)
cumsum(arr::Vector{UInt32}) = _cumsum_into!(_array_undef_from_dims(UInt64, (length(arr),)), arr)
cumsum(arr::Vector{UInt64}) = _cumsum_into!(_array_undef_from_dims(UInt64, (length(arr),)), arr)
cumsum(arr::Vector{Float32}) = _cumsum_into!(similar(arr, length(arr)), arr)
cumsum(arr::Vector{Float64}) = _cumsum_into!(similar(arr, length(arr)), arr)

# cumprod: cumulative product
function cumprod(arr)
    n = length(arr)
    return _cumprod_into!(_array_undef_from_dims(Float64, (n,)), arr)
end

cumprod(arr::Vector{Int64}) = _cumprod_into!(_array_undef_from_dims(Int64, (length(arr),)), arr)
cumprod(arr::Vector{Int8}) = _cumprod_into!(_array_undef_from_dims(Int64, (length(arr),)), arr)
cumprod(arr::Vector{Int16}) = _cumprod_into!(_array_undef_from_dims(Int64, (length(arr),)), arr)
cumprod(arr::Vector{Int32}) = _cumprod_into!(_array_undef_from_dims(Int64, (length(arr),)), arr)
cumprod(arr::Vector{Bool}) = _cumprod_bool_into!(similar(arr, length(arr)), arr)
cumprod(arr::Vector{UInt8}) = _cumprod_into!(_array_undef_from_dims(UInt64, (length(arr),)), arr)
cumprod(arr::Vector{UInt16}) = _cumprod_into!(_array_undef_from_dims(UInt64, (length(arr),)), arr)
cumprod(arr::Vector{UInt32}) = _cumprod_into!(_array_undef_from_dims(UInt64, (length(arr),)), arr)
cumprod(arr::Vector{UInt64}) = _cumprod_into!(_array_undef_from_dims(UInt64, (length(arr),)), arr)
cumprod(arr::Vector{Float32}) = _cumprod_into!(similar(arr, length(arr)), arr)
cumprod(arr::Vector{Float64}) = _cumprod_into!(similar(arr, length(arr)), arr)

# accumulate: generalized cumulative operation (Issue #1839)
# accumulate(op, A) applies op cumulatively to elements of A,
# returning a vector of all intermediate values.
# This is the generalization of cumsum (op=+) and cumprod (op=*).

function accumulate(op::Function, A; init=nothing)
    # `init` keyword (Issue #5701): seed the accumulation, delegating to the
    # positional `accumulate(op, A, init)` (the most specific 3-arg method is
    # picked for the concrete `A`). Without `init`, the existing behavior runs.
    if init !== nothing
        return accumulate(op, A, init)
    end
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

function _accumulate_runtime_eltype(first_acc, next_acc)
    first_type = typeof(first_acc)
    next_type = typeof(next_acc)
    if first_type === next_type
        return first_type
    end
    if next_type === Int64 &&
       (first_type === Int8 || first_type === Int16 || first_type === Int32 ||
        first_type === UInt8 || first_type === UInt16 || first_type === UInt32 ||
        first_type === UInt64)
        return first_type
    end
    return promote_type(first_type, next_type)
end

function _accumulate_sample_value(::Type{Bool})
    return false
end

function _accumulate_sample_value(::Type{Int8})
    return Int8(0)
end

function _accumulate_sample_value(::Type{Int16})
    return Int16(0)
end

function _accumulate_sample_value(::Type{Int32})
    return Int32(0)
end

function _accumulate_sample_value(::Type{Int64})
    return Int64(0)
end

function _accumulate_sample_value(::Type{UInt8})
    return UInt8(0)
end

function _accumulate_sample_value(::Type{UInt16})
    return UInt16(0)
end

function _accumulate_sample_value(::Type{UInt32})
    return UInt32(0)
end

function _accumulate_sample_value(::Type{UInt64})
    return UInt64(0)
end

function _accumulate_sample_value(::Type{Float32})
    return Float32(0)
end

function _accumulate_sample_value(::Type{Float64})
    return Float64(0)
end

function _accumulate_sample_value(::Type{String})
    return ""
end

function _accumulate_sample_value(::Type{T}) where T
    return nothing
end

function _accumulate_promoted_eltype(op, sample)
    if sample === nothing
        return Any
    end
    next_acc = op(sample, sample)
    return _accumulate_runtime_eltype(sample, next_acc)
end

function _accumulate_promoted_eltype(op, sample, init)
    if sample === nothing
        return Any
    end
    first_acc = op(init, sample)
    next_acc = op(first_acc, sample)
    return _accumulate_runtime_eltype(first_acc, next_acc)
end

function accumulate(op::Function, A::Array; init=nothing)
    # `init` keyword (Issue #5701): delegate to the positional 3-arg Array method.
    if init !== nothing
        return accumulate(op, A, init)
    end
    n = length(A)
    if n == 0
        sample = _accumulate_sample_value(eltype(A))
        return _array_undef_from_dims(_accumulate_promoted_eltype(op, sample), (0,))
    end
    acc = A[1]
    if n == 1
        result_type = _accumulate_promoted_eltype(op, acc)
        result = _array_undef_from_dims(result_type, (n,))
        result[1] = acc
        return result
    end
    first_acc = acc
    acc = op(acc, A[2])
    result = _array_undef_from_dims(_accumulate_runtime_eltype(first_acc, acc), (n,))
    result[1] = first_acc
    result[2] = acc
    for i in 3:n
        acc = op(acc, A[i])
        result[i] = acc
    end
    return result
end

function accumulate(op::Function, A::Array, init)
    n = length(A)
    if n == 0
        sample = _accumulate_sample_value(eltype(A))
        return _array_undef_from_dims(_accumulate_promoted_eltype(op, sample, init), (0,))
    end
    acc = op(init, A[1])
    if n == 1
        result_type = _accumulate_promoted_eltype(op, A[1], init)
        result = _array_undef_from_dims(result_type, (n,))
        result[1] = acc
        return result
    end
    first_acc = acc
    acc = op(acc, A[2])
    result = _array_undef_from_dims(_accumulate_runtime_eltype(first_acc, acc), (n,))
    result[1] = first_acc
    result[2] = acc
    for i in 3:n
        acc = op(acc, A[i])
        result[i] = acc
    end
    return result
end

function _accumulate_into!(result, op, A)
    n = length(A)
    if n == 0
        return result
    end
    acc = A[1]
    result[1] = acc
    for i in 2:n
        acc = op(acc, A[i])
        result[i] = acc
    end
    return result
end

function _accumulate_with_init_into!(result, op, A, init)
    acc = init
    for i in 1:length(A)
        acc = op(acc, A[i])
        result[i] = acc
    end
    return result
end

function _accumulate_bool_sum_into!(result, A)
    n = length(A)
    if n == 0
        return result
    end
    acc = Int64(A[1])
    result[1] = acc
    for i in 2:n
        acc = acc + Int64(A[i])
        result[i] = acc
    end
    return result
end

function _accumulate_bool_sum_with_init_into!(result, A, init)
    acc = Int64(init)
    for i in 1:length(A)
        acc = acc + Int64(A[i])
        result[i] = acc
    end
    return result
end

accumulate(::typeof(+), A::Vector{Int64}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), +, A)) : accumulate(+, A, init)
accumulate(::typeof(+), A::Vector{Int8}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), +, A)) : accumulate(+, A, init)
accumulate(::typeof(+), A::Vector{Int16}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), +, A)) : accumulate(+, A, init)
accumulate(::typeof(+), A::Vector{Int32}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), +, A)) : accumulate(+, A, init)
accumulate(::typeof(+), A::Vector{UInt8}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), +, A)) : accumulate(+, A, init)
accumulate(::typeof(+), A::Vector{UInt16}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), +, A)) : accumulate(+, A, init)
accumulate(::typeof(+), A::Vector{UInt32}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), +, A)) : accumulate(+, A, init)
accumulate(::typeof(+), A::Vector{UInt64}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), +, A)) : accumulate(+, A, init)
accumulate(::typeof(+), A::Vector{Float32}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), +, A)) : accumulate(+, A, init)
accumulate(::typeof(+), A::Vector{Float64}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), +, A)) : accumulate(+, A, init)
accumulate(::typeof(+), A::Vector{Bool}; init=nothing) = init === nothing ? (_accumulate_bool_sum_into!(_array_undef_from_dims(Int64, (length(A),)), A)) : accumulate(+, A, init)

accumulate(::typeof(*), A::Vector{Int64}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), *, A)) : accumulate(*, A, init)
accumulate(::typeof(*), A::Vector{Int8}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), *, A)) : accumulate(*, A, init)
accumulate(::typeof(*), A::Vector{Int16}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), *, A)) : accumulate(*, A, init)
accumulate(::typeof(*), A::Vector{Int32}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), *, A)) : accumulate(*, A, init)
accumulate(::typeof(*), A::Vector{UInt8}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), *, A)) : accumulate(*, A, init)
accumulate(::typeof(*), A::Vector{UInt16}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), *, A)) : accumulate(*, A, init)
accumulate(::typeof(*), A::Vector{UInt32}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), *, A)) : accumulate(*, A, init)
accumulate(::typeof(*), A::Vector{UInt64}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), *, A)) : accumulate(*, A, init)
accumulate(::typeof(*), A::Vector{Float32}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), *, A)) : accumulate(*, A, init)
accumulate(::typeof(*), A::Vector{Float64}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), *, A)) : accumulate(*, A, init)
accumulate(::typeof(*), A::Vector{Bool}; init=nothing) = init === nothing ? (_accumulate_into!(similar(A, length(A)), *, A)) : accumulate(*, A, init)

accumulate(::typeof(+), A::Vector{Float32}, init::Float32) = _accumulate_with_init_into!(similar(A, length(A)), +, A, init)
accumulate(::typeof(*), A::Vector{Float32}, init::Float32) = _accumulate_with_init_into!(similar(A, length(A)), *, A, init)
accumulate(::typeof(+), A::Vector{Bool}, init) = _accumulate_bool_sum_with_init_into!(_array_undef_from_dims(Int64, (length(A),)), A, init)
accumulate(::typeof(*), A::Vector{Bool}, init::Bool) = _accumulate_with_init_into!(similar(A, length(A)), *, A, init)

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
