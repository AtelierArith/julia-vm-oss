# Issue #6247: repeated T across Type{T} and rank-omitted AbstractArray{T}
# is a diagonal specificity relation for concrete Vector and Matrix actuals.

using Test

type_abstract_array_rank_omitted_diagonal_6247(::Type{T}, ::AbstractArray{T}) where {T<:Real} = :type_absarray_same
type_abstract_array_rank_omitted_diagonal_6247(::Type{Integer}, ::AbstractArray{<:Real}) = :type_integer_absarray_real

function type_abstract_array_rank_omitted_diagonal_via_any_6247(t, x)
    tt::Any = t
    xx::Any = x
    type_abstract_array_rank_omitted_diagonal_6247(tt, xx)
end

type_abstract_array_rank_omitted_diagonal_exact_6247(::Type{T}, ::AbstractArray{T}) where {T<:Real} = :type_absarray_same
type_abstract_array_rank_omitted_diagonal_exact_6247(::Type{Int64}, ::AbstractArray{Int64}) = :type_int_absarray_int

@testset "Type/AbstractArray rank-omitted diagonal specificity (Issue #6247)" begin
    @test type_abstract_array_rank_omitted_diagonal_6247(Int64, [1, 2]) === :type_absarray_same
    @test type_abstract_array_rank_omitted_diagonal_6247(Integer, [1, 2]) === :type_integer_absarray_real
    @test type_abstract_array_rank_omitted_diagonal_6247(Float64, [1.0, 2.0]) === :type_absarray_same
    @test type_abstract_array_rank_omitted_diagonal_6247(Int64, [1 2]) === :type_absarray_same

    @test type_abstract_array_rank_omitted_diagonal_via_any_6247(Int64, [1, 2]) === :type_absarray_same
    @test type_abstract_array_rank_omitted_diagonal_via_any_6247(Integer, [1, 2]) === :type_integer_absarray_real
    @test type_abstract_array_rank_omitted_diagonal_via_any_6247(Float64, [1.0, 2.0]) === :type_absarray_same
    @test type_abstract_array_rank_omitted_diagonal_via_any_6247(Int64, [1 2]) === :type_absarray_same

    @test type_abstract_array_rank_omitted_diagonal_exact_6247(Int64, [1, 2]) === :type_int_absarray_int
    @test type_abstract_array_rank_omitted_diagonal_exact_6247(Int64, [1 2]) === :type_int_absarray_int
end

type_abstract_array_rank_omitted_diagonal_6247(Int64, [1, 2]) === :type_absarray_same &&
    type_abstract_array_rank_omitted_diagonal_6247(Integer, [1, 2]) === :type_integer_absarray_real &&
    type_abstract_array_rank_omitted_diagonal_6247(Float64, [1.0, 2.0]) === :type_absarray_same &&
    type_abstract_array_rank_omitted_diagonal_6247(Int64, [1 2]) === :type_absarray_same &&
    type_abstract_array_rank_omitted_diagonal_via_any_6247(Int64, [1, 2]) === :type_absarray_same &&
    type_abstract_array_rank_omitted_diagonal_via_any_6247(Integer, [1, 2]) === :type_integer_absarray_real &&
    type_abstract_array_rank_omitted_diagonal_via_any_6247(Float64, [1.0, 2.0]) === :type_absarray_same &&
    type_abstract_array_rank_omitted_diagonal_via_any_6247(Int64, [1 2]) === :type_absarray_same &&
    type_abstract_array_rank_omitted_diagonal_exact_6247(Int64, [1, 2]) === :type_int_absarray_int &&
    type_abstract_array_rank_omitted_diagonal_exact_6247(Int64, [1 2]) === :type_int_absarray_int
