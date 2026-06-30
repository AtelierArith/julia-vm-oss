# Issue #6249: repeated T across Type{T} and AbstractArray{T,N} is a
# diagonal specificity relation for concrete Vector and Matrix actuals.

using Test

type_abstract_array_rank_typevar_diagonal_6249(::Type{T}, ::AbstractArray{T,N}) where {T<:Real,N} = :type_absarray_rankvar_same
type_abstract_array_rank_typevar_diagonal_6249(::Type{Integer}, ::AbstractArray{<:Real,N}) where {N} = :type_integer_absarray_rankvar_real

function type_abstract_array_rank_typevar_diagonal_via_any_6249(t, x)
    tt::Any = t
    xx::Any = x
    type_abstract_array_rank_typevar_diagonal_6249(tt, xx)
end

type_abstract_array_rank_typevar_diagonal_exact_6249(::Type{T}, ::AbstractArray{T,N}) where {T<:Real,N} = :type_absarray_rankvar_same
type_abstract_array_rank_typevar_diagonal_exact_6249(::Type{Int64}, ::AbstractArray{Int64,N}) where {N} = :type_int_absarray_rankvar_int

@testset "Type/AbstractArray rank-TypeVar diagonal specificity (Issue #6249)" begin
    @test type_abstract_array_rank_typevar_diagonal_6249(Int64, [1, 2]) === :type_absarray_rankvar_same
    @test type_abstract_array_rank_typevar_diagonal_6249(Integer, [1, 2]) === :type_integer_absarray_rankvar_real
    @test type_abstract_array_rank_typevar_diagonal_6249(Float64, [1.0, 2.0]) === :type_absarray_rankvar_same
    @test type_abstract_array_rank_typevar_diagonal_6249(Int64, [1 2]) === :type_absarray_rankvar_same

    @test type_abstract_array_rank_typevar_diagonal_via_any_6249(Int64, [1, 2]) === :type_absarray_rankvar_same
    @test type_abstract_array_rank_typevar_diagonal_via_any_6249(Integer, [1, 2]) === :type_integer_absarray_rankvar_real
    @test type_abstract_array_rank_typevar_diagonal_via_any_6249(Float64, [1.0, 2.0]) === :type_absarray_rankvar_same
    @test type_abstract_array_rank_typevar_diagonal_via_any_6249(Int64, [1 2]) === :type_absarray_rankvar_same

    @test type_abstract_array_rank_typevar_diagonal_exact_6249(Int64, [1, 2]) === :type_int_absarray_rankvar_int
    @test type_abstract_array_rank_typevar_diagonal_exact_6249(Int64, [1 2]) === :type_int_absarray_rankvar_int
end

type_abstract_array_rank_typevar_diagonal_6249(Int64, [1, 2]) === :type_absarray_rankvar_same &&
    type_abstract_array_rank_typevar_diagonal_6249(Integer, [1, 2]) === :type_integer_absarray_rankvar_real &&
    type_abstract_array_rank_typevar_diagonal_6249(Float64, [1.0, 2.0]) === :type_absarray_rankvar_same &&
    type_abstract_array_rank_typevar_diagonal_6249(Int64, [1 2]) === :type_absarray_rankvar_same &&
    type_abstract_array_rank_typevar_diagonal_via_any_6249(Int64, [1, 2]) === :type_absarray_rankvar_same &&
    type_abstract_array_rank_typevar_diagonal_via_any_6249(Integer, [1, 2]) === :type_integer_absarray_rankvar_real &&
    type_abstract_array_rank_typevar_diagonal_via_any_6249(Float64, [1.0, 2.0]) === :type_absarray_rankvar_same &&
    type_abstract_array_rank_typevar_diagonal_via_any_6249(Int64, [1 2]) === :type_absarray_rankvar_same &&
    type_abstract_array_rank_typevar_diagonal_exact_6249(Int64, [1, 2]) === :type_int_absarray_rankvar_int &&
    type_abstract_array_rank_typevar_diagonal_exact_6249(Int64, [1 2]) === :type_int_absarray_rankvar_int
