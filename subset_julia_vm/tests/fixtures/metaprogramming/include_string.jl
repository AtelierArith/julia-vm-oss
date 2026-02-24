# Test include_string function - evaluate code strings dynamically
# Note: SubsetJuliaVM's eval only supports limited expressions:
#       - Arithmetic: +, -, *, /, div, mod, ^
#       - Math functions: sqrt, abs, sin, cos
#       - Comparisons: ==, !=, <, >, <=, >=
#       - Boolean: !

using Test

@testset "include_string basic" begin
    # Simple arithmetic expression
    result = include_string(Main, "1 + 2")
    @test result == 3

    # More complex arithmetic
    result2 = include_string(Main, "10 * 5 + 3")
    @test result2 == 53

    # Whitespace only - returns nothing
    result3 = include_string(Main, "   ")
    @test result3 === nothing

    # Empty string - returns nothing
    result4 = include_string(Main, "")
    @test result4 === nothing

    # Single expression with trailing whitespace
    result5 = include_string(Main, "42\n")
    @test result5 == 42

    # Parenthesized expression
    result6 = include_string(Main, "(2 + 3) * 4")
    @test result6 == 20
end

@testset "include_string multiple expressions" begin
    # Multiple expressions separated by newlines - returns last value
    # Note: Each expression is evaluated independently
    result = include_string(Main, "1 + 1\n2 + 2\n3 + 3")
    @test result == 6
end

@testset "include_string with supported functions" begin
    # abs function works
    result = include_string(Main, "abs(-5)")
    @test result == 5

    # sqrt function works
    result2 = include_string(Main, "sqrt(16)")
    @test result2 == 4.0

    # Combination
    result3 = include_string(Main, "abs(-3) + sqrt(4)")
    @test result3 == 5.0
end

true
