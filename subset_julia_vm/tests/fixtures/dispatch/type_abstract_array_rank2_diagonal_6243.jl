# Issue #6243: repeated T across Type{T} and AbstractArray{T,2} is a
# diagonal specificity relation for concrete Matrix actuals.

using Test

type_abstract_array_rank2_diagonal_6243(::Type{T}, ::AbstractArray{T,2}) where {T<:Real} = :type_absarray2_same
type_abstract_array_rank2_diagonal_6243(::Type{Integer}, ::AbstractArray{<:Real,2}) = :type_integer_absarray2_real

function type_abstract_array_rank2_diagonal_via_any_6243(t, x)
    tt::Any = t
    xx::Any = x
    type_abstract_array_rank2_diagonal_6243(tt, xx)
end

type_abstract_array_rank2_diagonal_exact_6243(::Type{T}, ::AbstractArray{T,2}) where {T<:Real} = :type_absarray2_same
type_abstract_array_rank2_diagonal_exact_6243(::Type{Int64}, ::AbstractArray{Int64,2}) = :type_int_absarray2_int

@testset "Type/AbstractArray rank-2 diagonal specificity (Issue #6243)" begin
    @test type_abstract_array_rank2_diagonal_6243(Int64, [1 2]) === :type_absarray2_same
    @test type_abstract_array_rank2_diagonal_6243(Integer, [1 2]) === :type_integer_absarray2_real
    @test type_abstract_array_rank2_diagonal_6243(Float64, [1.0 2.0]) === :type_absarray2_same

    @test type_abstract_array_rank2_diagonal_via_any_6243(Int64, [1 2]) === :type_absarray2_same
    @test type_abstract_array_rank2_diagonal_via_any_6243(Integer, [1 2]) === :type_integer_absarray2_real
    @test type_abstract_array_rank2_diagonal_via_any_6243(Float64, [1.0 2.0]) === :type_absarray2_same

    @test type_abstract_array_rank2_diagonal_exact_6243(Int64, [1 2]) === :type_int_absarray2_int
end

type_abstract_array_rank2_diagonal_6243(Int64, [1 2]) === :type_absarray2_same &&
    type_abstract_array_rank2_diagonal_6243(Integer, [1 2]) === :type_integer_absarray2_real &&
    type_abstract_array_rank2_diagonal_6243(Float64, [1.0 2.0]) === :type_absarray2_same &&
    type_abstract_array_rank2_diagonal_via_any_6243(Int64, [1 2]) === :type_absarray2_same &&
    type_abstract_array_rank2_diagonal_via_any_6243(Integer, [1 2]) === :type_integer_absarray2_real &&
    type_abstract_array_rank2_diagonal_via_any_6243(Float64, [1.0 2.0]) === :type_absarray2_same &&
    type_abstract_array_rank2_diagonal_exact_6243(Int64, [1 2]) === :type_int_absarray2_int
