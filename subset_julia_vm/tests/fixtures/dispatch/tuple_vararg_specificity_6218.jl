# Issue #6218: tuple-vararg method specificity must use the actual tuple shape.
# `Tuple{Vararg{Int64}}` beats `Tuple{Int64,Vararg{Any}}` for all-Int tuples,
# but the fixed-prefix fallback still handles mixed tails.

using Test

tuple_vararg_specificity_6218(::Tuple{Vararg{Int64}}) = :varint
tuple_vararg_specificity_6218(::Tuple{Int64,Vararg{Any}}) = :prefix

tuple_vararg_specificity_reversed_6218(::Tuple{Int64,Vararg{Any}}) = :prefix
tuple_vararg_specificity_reversed_6218(::Tuple{Vararg{Int64}}) = :varint

tuple_vararg_same_tail_6218(::Tuple{Vararg{Int64}}) = :varint
tuple_vararg_same_tail_6218(::Tuple{Int64,Vararg{Int64}}) = :prefix

@testset "tuple vararg specificity (Issue #6218)" begin
    @test tuple_vararg_specificity_6218(()) == :varint
    @test tuple_vararg_specificity_6218((1,)) == :varint
    @test tuple_vararg_specificity_6218((1, 2)) == :varint
    @test tuple_vararg_specificity_6218((1, "x")) == :prefix

    @test tuple_vararg_specificity_reversed_6218((1,)) == :varint
    @test tuple_vararg_specificity_reversed_6218((1, 2)) == :varint
    @test tuple_vararg_specificity_reversed_6218((1, "x")) == :prefix

    @test tuple_vararg_same_tail_6218((1,)) == :prefix
    @test tuple_vararg_same_tail_6218((1, 2)) == :prefix
end

tuple_vararg_specificity_6218(()) == :varint &&
    tuple_vararg_specificity_6218((1,)) == :varint &&
    tuple_vararg_specificity_6218((1, 2)) == :varint &&
    tuple_vararg_specificity_6218((1, "x")) == :prefix &&
    tuple_vararg_specificity_reversed_6218((1,)) == :varint &&
    tuple_vararg_specificity_reversed_6218((1, 2)) == :varint &&
    tuple_vararg_specificity_reversed_6218((1, "x")) == :prefix &&
    tuple_vararg_same_tail_6218((1,)) == :prefix &&
    tuple_vararg_same_tail_6218((1, 2)) == :prefix
