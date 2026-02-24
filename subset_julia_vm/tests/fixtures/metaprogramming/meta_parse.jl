# Test Meta.parse functionality

using Test

@testset "Meta.parse: parse strings to Expr AST nodes" begin

    # Basic parsing of literals
    expr1 = Meta.parse("42")
    @assert expr1 == 42

    expr2 = Meta.parse("3.14")
    @assert expr2 == 3.14

    # Parse binary expression: a + b -> Expr(:call, :+, :a, :b)
    expr5 = Meta.parse("a + b")
    @assert Meta.isexpr(expr5, :call)

    # Parse function call: f(x) -> Expr(:call, :f, :x)
    expr8 = Meta.parse("f(x)")
    @assert Meta.isexpr(expr8, :call)

    # Return true to indicate success
    @test (true)
end

true  # Test passed
