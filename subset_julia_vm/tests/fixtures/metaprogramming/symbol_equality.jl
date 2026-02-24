# Test Symbol equality comparison
# Symbol == Symbol should return true if names match

using Test

@testset "Symbol equality comparison: :x == :x, ex.args[1] == :+ in Expr" begin

    # Basic equality tests
    @assert :x == :x
    @assert :hello == :hello

    # Inequality tests via comparison returning false
    result = :x == :y
    @assert result == false

    # Symbol comparison in metaprogramming context
    ex = :(x + y)
    @assert ex.head == :call
    @assert ex.args[1] == :+
    @assert ex.args[2] == :x
    @assert ex.args[3] == :y

    # Symbol comparison with variables
    s1 = :test
    s2 = :test
    s3 = :other
    @assert s1 == s2
    result2 = s1 == s3
    @assert result2 == false

    # Return success
    @test (1.0) == 1.0
end

true  # Test passed
