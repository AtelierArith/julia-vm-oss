# Test @kwdef with all default values

using Test

@kwdef struct Point
    x::Float64 = 0.0
    y::Float64 = 0.0
end

@testset "@kwdef basic with defaults" begin
    # Test default constructor (no args)
    p1 = Point()
    @test p1.x == 0.0
    @test p1.y == 0.0

    # Test partial kwargs
    p2 = Point(x=5.0)
    @test p2.x == 5.0
    @test p2.y == 0.0

    # Test all kwargs
    p3 = Point(x=1.0, y=2.0)
    @test p3.x == 1.0
    @test p3.y == 2.0

    # Test kwargs in different order
    p4 = Point(y=3.0, x=4.0)
    @test p4.x == 4.0
    @test p4.y == 3.0
end

true
