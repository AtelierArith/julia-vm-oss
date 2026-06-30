# Issue #6216: `f()` for competing unbounded vararg methods must still use the
# declared vararg element type for specificity. Expanding `xs::T...` to the
# runtime argument list erases that information when the vararg is empty.

using Test

empty_vararg_specificity_6216(xs::Int64...) = :int
empty_vararg_specificity_6216(xs::Integer...) = :integer

empty_vararg_specificity_reversed_6216(xs::Integer...) = :integer
empty_vararg_specificity_reversed_6216(xs::Int64...) = :int

empty_prefixed_vararg_specificity_6216(head::String, xs::Integer...) = :integer
empty_prefixed_vararg_specificity_6216(head::String, xs::Int64...) = :int

@testset "empty vararg specificity (Issue #6216)" begin
    @test empty_vararg_specificity_6216() == :int
    @test empty_vararg_specificity_6216(1, 2) == :int

    @test empty_vararg_specificity_reversed_6216() == :int
    @test empty_vararg_specificity_reversed_6216(1, 2) == :int

    @test empty_prefixed_vararg_specificity_6216("x") == :int
    @test empty_prefixed_vararg_specificity_6216("x", 1, 2) == :int
end

empty_vararg_specificity_6216() == :int &&
    empty_vararg_specificity_6216(1, 2) == :int &&
    empty_vararg_specificity_reversed_6216() == :int &&
    empty_vararg_specificity_reversed_6216(1, 2) == :int &&
    empty_prefixed_vararg_specificity_6216("x") == :int &&
    empty_prefixed_vararg_specificity_6216("x", 1, 2) == :int
