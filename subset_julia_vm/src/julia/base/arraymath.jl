# Array arithmetic helpers
# Based on Julia's base/arraymath.jl

function _arraymath_check_same_shape(A::Vector, B::Vector)
    if length(A) != length(B)
        error("DimensionMismatch: arrays could not be broadcast to a common size")
    end
    return nothing
end

function _arraymath_check_same_shape(A::Matrix, B::Matrix)
    if size(A, 1) != size(B, 1) || size(A, 2) != size(B, 2)
        error("DimensionMismatch: arrays could not be broadcast to a common size")
    end
    return nothing
end

function Base.:+(A::Vector, B::Vector)
    _arraymath_check_same_shape(A, B)
    return map(+, A, B)
end

function Base.:-(A::Vector, B::Vector)
    _arraymath_check_same_shape(A, B)
    return map(-, A, B)
end

function _arraymath_matrix_binary(f, A::Matrix, B::Matrix)
    _arraymath_check_same_shape(A, B)
    m = size(A, 1)
    n = size(A, 2)
    if length(A) == 0
        return similar(A, m, n)
    end

    first_value = f(A[1], B[1])
    result = _array_undef_from_dims(typeof(first_value), (m, n))
    result[1] = first_value
    for i in 2:length(A)
        result[i] = f(A[i], B[i])
    end
    return result
end

function Base.:+(A::Matrix, B::Matrix)
    return _arraymath_matrix_binary(+, A, B)
end

function Base.:-(A::Matrix, B::Matrix)
    return _arraymath_matrix_binary(-, A, B)
end
