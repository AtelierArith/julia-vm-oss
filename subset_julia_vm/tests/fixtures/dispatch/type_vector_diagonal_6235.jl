# Issue #6235: repeated T across Type{T} and Vector{T} is a diagonal
# specificity relation.

using Test

type_vector_diagonal_6235(::Type{T}, ::Vector{T}) where {T<:Real} = :type_vec_same
type_vector_diagonal_6235(::Type{Integer}, ::Vector{<:Real}) = :type_integer_vec_real

function type_vector_diagonal_via_any_6235(t, x)
    tt::Any = t
    xx::Any = x
    type_vector_diagonal_6235(tt, xx)
end

@testset "Type/vector diagonal specificity (Issue #6235)" begin
    @test type_vector_diagonal_6235(Int64, [1]) === :type_vec_same
    @test type_vector_diagonal_6235(Integer, [1]) === :type_integer_vec_real
    @test type_vector_diagonal_6235(Float64, [1.0]) === :type_vec_same

    @test type_vector_diagonal_via_any_6235(Int64, [1]) === :type_vec_same
    @test type_vector_diagonal_via_any_6235(Integer, [1]) === :type_integer_vec_real
    @test type_vector_diagonal_via_any_6235(Float64, [1.0]) === :type_vec_same
end

type_vector_diagonal_6235(Int64, [1]) === :type_vec_same &&
    type_vector_diagonal_6235(Integer, [1]) === :type_integer_vec_real &&
    type_vector_diagonal_6235(Float64, [1.0]) === :type_vec_same &&
    type_vector_diagonal_via_any_6235(Int64, [1]) === :type_vec_same &&
    type_vector_diagonal_via_any_6235(Integer, [1]) === :type_integer_vec_real &&
    type_vector_diagonal_via_any_6235(Float64, [1.0]) === :type_vec_same
