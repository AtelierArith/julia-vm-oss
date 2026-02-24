# Test using Iterators module
# Issue #759: Iterators module implementation

using Test
using Iterators

@testset "Using Iterators module" begin
    # Test that enumerate is accessible via Iterators
    arr = [10, 20, 30]
    result = collect(enumerate(arr))
    @test result[1] == (1, 10)
    @test result[2] == (2, 20)
    @test result[3] == (3, 30)

    # Test that zip is accessible
    a = [1, 2, 3]
    b = [4, 5, 6]
    zipped = collect(zip(a, b))
    @test zipped[1] == (1, 4)
    @test zipped[2] == (2, 5)
    @test zipped[3] == (3, 6)

    # Test that take is accessible
    taken = collect(take([1, 2, 3, 4, 5], 3))
    @test length(taken) == 3
    @test taken[1] == 1
    @test taken[3] == 3
end

true
