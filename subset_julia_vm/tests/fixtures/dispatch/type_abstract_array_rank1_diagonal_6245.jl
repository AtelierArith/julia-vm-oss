# Issue #6245: repeated T across Type{T} and AbstractArray{T,1} is a
# diagonal specificity relation for concrete Vector actuals.

using Test

type_abstract_array_rank1_diagonal_6245(::Type{T}, ::AbstractArray{T,1}) where {T<:Real} = :type_absarray1_same
type_abstract_array_rank1_diagonal_6245(::Type{Integer}, ::AbstractArray{<:Real,1}) = :type_integer_absarray1_real

function type_abstract_array_rank1_diagonal_via_any_6245(t, x)
    tt::Any = t
    xx::Any = x
    type_abstract_array_rank1_diagonal_6245(tt, xx)
end

type_abstract_array_rank1_diagonal_exact_6245(::Type{T}, ::AbstractArray{T,1}) where {T<:Real} = :type_absarray1_same
type_abstract_array_rank1_diagonal_exact_6245(::Type{Int64}, ::AbstractArray{Int64,1}) = :type_int_absarray1_int

@testset "Type/AbstractArray rank-1 diagonal specificity (Issue #6245)" begin
    @test type_abstract_array_rank1_diagonal_6245(Int64, [1, 2]) === :type_absarray1_same
    @test type_abstract_array_rank1_diagonal_6245(Integer, [1, 2]) === :type_integer_absarray1_real
    @test type_abstract_array_rank1_diagonal_6245(Float64, [1.0, 2.0]) === :type_absarray1_same

    @test type_abstract_array_rank1_diagonal_via_any_6245(Int64, [1, 2]) === :type_absarray1_same
    @test type_abstract_array_rank1_diagonal_via_any_6245(Integer, [1, 2]) === :type_integer_absarray1_real
    @test type_abstract_array_rank1_diagonal_via_any_6245(Float64, [1.0, 2.0]) === :type_absarray1_same

    @test type_abstract_array_rank1_diagonal_exact_6245(Int64, [1, 2]) === :type_int_absarray1_int
end

type_abstract_array_rank1_diagonal_6245(Int64, [1, 2]) === :type_absarray1_same &&
    type_abstract_array_rank1_diagonal_6245(Integer, [1, 2]) === :type_integer_absarray1_real &&
    type_abstract_array_rank1_diagonal_6245(Float64, [1.0, 2.0]) === :type_absarray1_same &&
    type_abstract_array_rank1_diagonal_via_any_6245(Int64, [1, 2]) === :type_absarray1_same &&
    type_abstract_array_rank1_diagonal_via_any_6245(Integer, [1, 2]) === :type_integer_absarray1_real &&
    type_abstract_array_rank1_diagonal_via_any_6245(Float64, [1.0, 2.0]) === :type_absarray1_same &&
    type_abstract_array_rank1_diagonal_exact_6245(Int64, [1, 2]) === :type_int_absarray1_int
