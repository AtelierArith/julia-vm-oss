# Test: Basic struct definition inside module

using Test
using .MyGeometry

module MyGeometry

export Point

struct Point
    x::Float64
    y::Float64
end

end

@testset "Basic struct definition inside module" begin


    p = Point(3.0, 4.0)
    @test (p.x + p.y) == 7.0
end

true  # Test passed
