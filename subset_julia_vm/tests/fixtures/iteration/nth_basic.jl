# Test nth function for iterators
# nth(itr, n) returns the nth element of an iterator

using Test
using Iterators

@testset "nth function" begin
    # Test with arrays
    @test nth([10, 20, 30], 1) == 10
    @test nth([10, 20, 30], 2) == 20
    @test nth([10, 20, 30], 3) == 30

    # Test with ranges
    @test nth(1:10, 5) == 5
    @test nth(2:2:10, 4) == 8  # [2, 4, 6, 8, 10], 4th element is 8

    # Test with StepRange
    @test nth(1:2:9, 3) == 5  # [1, 3, 5, 7, 9], 3rd element is 5

    # Test with enumerate
    result = nth(enumerate([10, 20, 30]), 2)
    @test result[1] == 2
    @test result[2] == 20

    # Test with take
    @test nth(take(1:100, 5), 3) == 3

    # Test with zip
    result = nth(zip([1, 2, 3], [10, 20, 30]), 2)
    @test result[1] == 2
    @test result[2] == 20
end

true
