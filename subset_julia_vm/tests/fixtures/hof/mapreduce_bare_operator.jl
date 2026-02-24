# Regression test: mapreduce/mapfoldl/mapfoldr with bare operators (+, -, *)
# Issue #2004: resolve_function_ref incorrectly resolved + and - to their
# unary forms instead of binary forms when used as reduce operators.

using Test

@testset "mapreduce with bare operators (Issue #2004)" begin
    # mapreduce(f, +, arr) - sum of squares
    # 1^2 + 2^2 + 3^2 = 1 + 4 + 9 = 14
    @test mapreduce(x -> x^2, +, [1, 2, 3]) == 14.0

    # mapreduce(f, *, arr) - product of incremented values
    # (1+1) * (2+1) * (3+1) = 2 * 3 * 4 = 24
    @test mapreduce(x -> x + 1, *, [1, 2, 3]) == 24.0

    # mapreduce(f, -, arr) - left-fold subtraction of squares
    # (1^2 - 2^2) - 3^2 = (1 - 4) - 9 = -12
    @test mapreduce(x -> x^2, -, [1, 2, 3]) == -12.0

    # mapfoldl with bare + operator
    @test mapfoldl(x -> x^2, +, [1, 2, 3]) == 14.0

    # mapfoldr with bare + operator
    @test mapfoldr(x -> x^2, +, [1, 2, 3]) == 14.0

    # mapreduce with identity and bare +
    @test mapreduce(x -> x, +, [1, 2, 3]) == 6.0

    # Single element - should just apply f
    @test mapreduce(x -> x^2, +, [5]) == 25.0
end

true
