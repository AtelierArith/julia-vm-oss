# Issue #6220: conflicting tuple-vararg specificity dimensions must stay
# ambiguous instead of falling through to the scalar score's fixed-prefix bias.

using Test

tuple_vararg_ambiguity_6220(::Tuple{Vararg{Integer}}) = :integer
tuple_vararg_ambiguity_6220(::Tuple{Int64,Vararg{Any}}) = :prefix

function tuple_vararg_ambiguity_caught_6220(x)
    try
        tuple_vararg_ambiguity_6220(x)
        return false
    catch
        return true
    end
end

@testset "tuple vararg ambiguity (Issue #6220)" begin
    @test tuple_vararg_ambiguity_6220(()) == :integer
    @test_throws MethodError tuple_vararg_ambiguity_6220((1,))
    @test_throws MethodError tuple_vararg_ambiguity_6220((1, 2))
    @test tuple_vararg_ambiguity_6220((1, "x")) == :prefix

    @test tuple_vararg_ambiguity_caught_6220((1,))
    @test tuple_vararg_ambiguity_caught_6220((1, 2))
end

tuple_vararg_ambiguity_6220(()) == :integer &&
    tuple_vararg_ambiguity_caught_6220((1,)) &&
    tuple_vararg_ambiguity_caught_6220((1, 2)) &&
    tuple_vararg_ambiguity_6220((1, "x")) == :prefix
