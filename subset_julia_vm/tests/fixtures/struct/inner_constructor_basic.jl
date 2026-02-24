# Basic inner constructor with new()

using Test

struct Point
    x::Float64
    y::Float64
    Point(x, y) = new(x, y)
end

@testset "Basic inner constructor with new()" begin

    p = Point(1.0, 2.0)
    @test (p.x + p.y) == 3.0
end

true  # Test passed
