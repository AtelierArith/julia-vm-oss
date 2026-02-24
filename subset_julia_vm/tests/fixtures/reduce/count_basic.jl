# Test count function - counts truthy values in an array
# count(arr) counts true elements in a boolean array
# count(f, arr) counts elements where f returns true (builtin HOF)

using Test

# Helper predicates for count(f, arr) tests
_ispositive(x) = x > 0
_gt2(x) = x > 2
_isnegative(x) = x < 0
_nonzero(x) = x != 0

@testset "count function (Issue #762)" begin
    # count(arr) - count true values in boolean array
    @test (count([true, false, true, true, false]) == 3)
    @test (count([false, false, false]) == 0)
    @test (count([true, true, true]) == 3)

    # count with integer array: use predicate version
    # Note: In Julia, count(Int[]) throws TypeError - use count(f, arr) instead
    @test (count(_nonzero, [0, 1, 0, 3, 0, 5]) == 3)
    @test (count(_nonzero, [1, 2, 3, 4, 5]) == 5)
    @test (count(_nonzero, [0, 0, 0]) == 0)

    # count(f, arr) - count elements where predicate is true
    @test (count(_ispositive, [1, -2, 3, -4, 5]) == 3)
    @test (count(_gt2, [1, 2, 3, 4, 5]) == 3)
    @test (count(iseven, [1, 2, 3, 4, 5, 6]) == 3)
    @test (count(isodd, [1, 2, 3, 4, 5, 6]) == 3)
    @test (count(_isnegative, [1, 2, 3]) == 0)
end

true  # Test passed
