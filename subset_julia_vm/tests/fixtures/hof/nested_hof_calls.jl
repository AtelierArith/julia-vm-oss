# Nested HOF calls (Issue #2072)
# Tests that higher-order functions can be nested as arguments
# to other higher-order functions without stack corruption.

using Test

add1(x) = x + 1
gt2(x) = x > 2
square(x) = x^2

@testset "Nested HOF calls" begin
    # filter(f, map(g, arr)) - basic nested HOF
    @test filter(x -> x > 5, map(x -> x^2, [1, 2, 3, 4])) == [9, 16]

    # map(f, filter(g, arr)) - reversed nesting
    @test map(x -> x * 2, filter(x -> x > 2, [1, 2, 3, 4])) == [6, 8]

    # Nested with named functions
    @test filter(gt2, map(add1, [1, 2, 3])) == [3, 4]

    # map(f, map(g, arr)) - double map
    @test map(x -> x + 1, map(x -> x * 2, [1, 2, 3])) == [3, 5, 7]

    # Split version still works (regression check)
    inner = map(x -> x^2, [1, 2, 3, 4])
    @test filter(x -> x > 5, inner) == [9, 16]
end

true
