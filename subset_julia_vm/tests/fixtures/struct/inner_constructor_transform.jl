# Inner constructor that transforms inputs

using Test

struct AbsPoint
    x::Float64
    y::Float64
    AbsPoint(x, y) = new(abs(x), abs(y))
end

@testset "Inner constructor that transforms inputs" begin

    p = AbsPoint(-3.0, -4.0)
    @test (p.x + p.y) == 7.0
end

true  # Test passed
