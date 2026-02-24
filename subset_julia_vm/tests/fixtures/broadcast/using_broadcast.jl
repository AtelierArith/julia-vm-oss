# Test using Broadcast module
# Issue #760: Broadcast module implementation

using Test
using Broadcast

# Define helper functions for testing
add(x, y) = x + y
mul(x, y) = x * y

@testset "Using Broadcast module" begin
    # Test that broadcast is accessible via Broadcast module
    arr1 = [1.0, 2.0, 3.0]
    arr2 = [4.0, 5.0, 6.0]

    # Test broadcast with addition function
    result = broadcast(add, arr1, arr2)
    @test result[1] == 5.0
    @test result[2] == 7.0
    @test result[3] == 9.0

    # Test broadcast with scalar
    result2 = broadcast(mul, arr1, 2.0)
    @test result2[1] == 2.0
    @test result2[2] == 4.0
    @test result2[3] == 6.0

    # Test broadcast! (in-place)
    dest = [0.0, 0.0, 0.0]
    broadcast!(add, dest, arr1, arr2)
    @test dest[1] == 5.0
    @test dest[2] == 7.0
    @test dest[3] == 9.0
end

true
