# Test @kwdef with default values for all fields

using Test

@kwdef struct Point
    x::Float64 = 0.0
    y::Float64 = 0.0
end

@testset "@kwdef with default values" begin
    p1 = Point()
    @test p1.x == 0.0
    @test p1.y == 0.0

    p2 = Point(x=1.0)
    @test p2.x == 1.0
    @test p2.y == 0.0

    p3 = Point(y=2.0)
    @test p3.x == 0.0
    @test p3.y == 2.0

    p4 = Point(x=1.0, y=2.0)
    @test p4.x == 1.0
    @test p4.y == 2.0
end

true
