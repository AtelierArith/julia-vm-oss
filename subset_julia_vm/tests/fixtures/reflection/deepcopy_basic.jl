# Test deepcopy with mutable struct

using Test

mutable struct Point
    x::Float64
    y::Float64
end

@testset "deepcopy - recursive deep copy of mutable struct" begin

    p1 = Point(1.0, 2.0)
    p2 = deepcopy(p1)

    # Modify original
    p1.x = 100.0

    # p2 should be unchanged
    @test (p2.x + p2.y) == 3.0
end

true  # Test passed
