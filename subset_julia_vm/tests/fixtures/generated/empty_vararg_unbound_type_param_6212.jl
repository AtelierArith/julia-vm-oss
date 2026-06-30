using Test

# Issue #6212 / #5074: an empty `xs::T...` call does not constrain T.
# Reading T must throw like Julia, while value-only empty vararg calls still
# return the empty tuple.

function plain_empty_vararg_type_param_6212(xs::T...) where T
    T
end

function plain_empty_vararg_values_6212(xs::T...) where T
    xs
end

@generated function generated_empty_vararg_type_param_6212(xs::T...) where T
    return :(T)
end

@generated function generated_empty_vararg_values_6212(xs::T...) where T
    return :(xs)
end

@testset "empty vararg unbound type parameter (Issue #6212)" begin
    @test_throws UndefVarError plain_empty_vararg_type_param_6212()
    @test plain_empty_vararg_type_param_6212(1, 2) == Int64
    @test plain_empty_vararg_values_6212() == ()
    @test plain_empty_vararg_values_6212(1, 2) == (1, 2)

    @test_throws UndefVarError generated_empty_vararg_type_param_6212()
    @test generated_empty_vararg_type_param_6212(1, 2) == Int64
    @test generated_empty_vararg_values_6212() == ()
    @test generated_empty_vararg_values_6212(1, 2) == (1, 2)
end

true
