# Test quoting of arrow functions (anonymous functions)
# :(x -> expr) should create Expr(:(->), :x, body)

using Test

@testset "Quote of arrow function: :(x -> expr) becomes Expr(:(->), ...)" begin

    # Test basic arrow function quote
    ex = :(x -> x + 1)

    # Check it produces an Expr containing arrow function syntax
    s = string(ex)
    @assert occursin("->", s)
    @assert occursin("x", s)

    # Test .head field access
    @assert string(ex.head) == "->"

    # Check the parameter (use string comparison since Symbol == Symbol not implemented)
    @assert string(ex.args[1]) == "x"

    # Test with tuple parameters
    ex2 = :((x, y) -> x + y)
    s2 = string(ex2)
    @assert occursin("->", s2)
    @assert occursin("x", s2)
    @assert occursin("y", s2)
    @assert string(ex2.head) == "->"

    # Return success
    @test (1.0) == 1.0
end

true  # Test passed
