# findlast(f, A) - find last index where predicate returns true (Issue #1996)

using Test

@testset "findlast with predicate" begin
    # Lambda function - basic
    @test findlast(x -> x > 3, [1, 2, 5, 4]) == 4
    @test findlast(x -> x < 0, [-1, 2, -3]) == 3

    # Lambda - first element matches only
    @test findlast(x -> x > 5, [6, 2, 4, 1]) == 1

    # Lambda - last element matches
    @test findlast(x -> x > 6, [2, 4, 6, 7]) == 4

    # No match returns nothing
    @test findlast(x -> x > 10, [2, 4, 6, 8]) === nothing

    # Single element array - match
    @test findlast(x -> x > 0, [3]) == 1

    # Single element array - no match
    @test findlast(x -> x > 5, [3]) === nothing

    # isodd/iseven via lambda
    @test findlast(x -> x % 2 != 0, [2, 4, 3, 6]) == 3
    @test findlast(x -> x % 2 == 0, [1, 3, 4, 5, 6]) == 5

    # Multiple matches - returns last
    @test findlast(x -> x > 2, [1, 3, 5, 2, 4]) == 5
end

true
