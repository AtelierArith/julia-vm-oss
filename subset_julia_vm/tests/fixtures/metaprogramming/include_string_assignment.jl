# Test include_string assignment support (Issue #1433)
# Tests that eval/include_string supports assignment expressions

using Test

@testset "include_string assignment" begin
    # Simple assignment - returns the assigned value
    result = include_string(Main, "x = 10")
    @test result == 10

    # Assignment with expression
    result2 = include_string(Main, "y = 5 + 3")
    @test result2 == 8

    # Assignment with function call
    result3 = include_string(Main, "z = sqrt(16)")
    @test result3 == 4.0

    # Multiple assignments in sequence - returns last value
    result4 = include_string(Main, "a = 1\nb = 2\nc = a + b")
    @test result4 == 3

    # Assignment with negative value
    result5 = include_string(Main, "neg = -42")
    @test result5 == -42

    # Assignment with abs
    result6 = include_string(Main, "pos = abs(-100)")
    @test result6 == 100
end

true
