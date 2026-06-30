# Issue #6233: repeated T across Type{T} and a value argument is a diagonal
# specificity relation.

using Test

type_value_diagonal_6233(::Type{T}, ::T) where {T<:Real} = :type_value_same
type_value_diagonal_6233(::Type{Integer}, ::Integer) = :type_integer_integer

function type_value_diagonal_via_any_6233(t, x)
    tt::Any = t
    xx::Any = x
    type_value_diagonal_6233(tt, xx)
end

@testset "Type/value diagonal specificity (Issue #6233)" begin
    @test type_value_diagonal_6233(Int64, 1) === :type_value_same
    @test type_value_diagonal_6233(Integer, 1) === :type_integer_integer
    @test type_value_diagonal_6233(Float64, 1.0) === :type_value_same

    @test type_value_diagonal_via_any_6233(Int64, 1) === :type_value_same
    @test type_value_diagonal_via_any_6233(Integer, 1) === :type_integer_integer
    @test type_value_diagonal_via_any_6233(Float64, 1.0) === :type_value_same
end

type_value_diagonal_6233(Int64, 1) === :type_value_same &&
    type_value_diagonal_6233(Integer, 1) === :type_integer_integer &&
    type_value_diagonal_6233(Float64, 1.0) === :type_value_same &&
    type_value_diagonal_via_any_6233(Int64, 1) === :type_value_same &&
    type_value_diagonal_via_any_6233(Integer, 1) === :type_integer_integer &&
    type_value_diagonal_via_any_6233(Float64, 1.0) === :type_value_same
