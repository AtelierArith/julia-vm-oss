using Test

module AbstractMatrixIntegerGetindexSpecificity8400
import Base: getindex, size

struct LinearOnlyMatrix <: AbstractMatrix{Int}
    data::Vector{Int}
end

size(A::LinearOnlyMatrix) = (length(A.data), 1)

function getindex(A::LinearOnlyMatrix, i::Integer)
    return A.data[Int(i)]
end
end

A = AbstractMatrixIntegerGetindexSpecificity8400.LinearOnlyMatrix([10, 20, 30])

@test getindex(A, 1) == 10
@test A[2] == 20

boxed = Any[A][1]
@test getindex(boxed, 3) == 30

true
