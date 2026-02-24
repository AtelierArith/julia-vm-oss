# Multiple inner constructors

using Test

struct Point2
    x::Float64
    y::Float64
    Point2(x, y) = new(x, y)
    Point2(xy) = new(xy, xy)
end

@testset "Multiple inner constructors" begin

    p1 = Point2(3.0, 4.0)
    p2 = Point2(5.0)
    @test (p1.x + p1.y + p2.x + p2.y) == 17.0
end

true  # Test passed
