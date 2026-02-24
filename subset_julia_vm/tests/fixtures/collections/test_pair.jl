# Test Pair type
# Pair is a simple key-value container

using Test

@testset "Pair type - key-value container with first and second fields (Issue #531)" begin

    # Test basic Pair construction
    p1 = Pair(1, 2)
    @assert typeof(p1) == Pair
    @assert p1.first == 1
    @assert p1.second == 2

    # Test Pair with different types
    p2 = Pair(100, 42)
    @assert p2.first == 100
    @assert p2.second == 42

    # Test Pair with mixed types (int and float)
    p3 = Pair(5, 3.14)
    @assert p3.first == 5
    @assert p3.second == 3.14

    @test (true)
end

true  # Test passed
