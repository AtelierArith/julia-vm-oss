# Issue #6239: repeated T across Type{T} and AbstractVector{T} is a
# diagonal specificity relation for concrete Vector actuals.

using Test

type_abstract_vector_diagonal_6239(::Type{T}, ::AbstractVector{T}) where {T<:Real} = :type_absvec_same
type_abstract_vector_diagonal_6239(::Type{Integer}, ::AbstractVector{<:Real}) = :type_integer_absvec_real

function type_abstract_vector_diagonal_via_any_6239(t, x)
    tt::Any = t
    xx::Any = x
    type_abstract_vector_diagonal_6239(tt, xx)
end

type_abstract_vector_diagonal_exact_6239(::Type{T}, ::AbstractVector{T}) where {T<:Real} = :type_absvec_same
type_abstract_vector_diagonal_exact_6239(::Type{Int64}, ::AbstractVector{Int64}) = :type_int_absvec_int

@testset "Type/AbstractVector diagonal specificity (Issue #6239)" begin
    @test type_abstract_vector_diagonal_6239(Int64, [1, 2]) === :type_absvec_same
    @test type_abstract_vector_diagonal_6239(Integer, [1, 2]) === :type_integer_absvec_real
    @test type_abstract_vector_diagonal_6239(Float64, [1.0, 2.0]) === :type_absvec_same

    @test type_abstract_vector_diagonal_via_any_6239(Int64, [1, 2]) === :type_absvec_same
    @test type_abstract_vector_diagonal_via_any_6239(Integer, [1, 2]) === :type_integer_absvec_real
    @test type_abstract_vector_diagonal_via_any_6239(Float64, [1.0, 2.0]) === :type_absvec_same

    @test type_abstract_vector_diagonal_exact_6239(Int64, [1, 2]) === :type_int_absvec_int
end

type_abstract_vector_diagonal_6239(Int64, [1, 2]) === :type_absvec_same &&
    type_abstract_vector_diagonal_6239(Integer, [1, 2]) === :type_integer_absvec_real &&
    type_abstract_vector_diagonal_6239(Float64, [1.0, 2.0]) === :type_absvec_same &&
    type_abstract_vector_diagonal_via_any_6239(Int64, [1, 2]) === :type_absvec_same &&
    type_abstract_vector_diagonal_via_any_6239(Integer, [1, 2]) === :type_integer_absvec_real &&
    type_abstract_vector_diagonal_via_any_6239(Float64, [1.0, 2.0]) === :type_absvec_same &&
    type_abstract_vector_diagonal_exact_6239(Int64, [1, 2]) === :type_int_absvec_int
