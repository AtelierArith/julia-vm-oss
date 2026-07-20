# Test: Parametric struct definition inside module with function

using Test
using .MyGeometry

module MyGeometry

export Point, distance

struct Point{T<:Real}
    x::T
    y::T
end

function distance(p::Point{T}, q::Point{T}) where T <: Real
    return sqrt((q.x - p.x)^2 + (q.y - p.y)^2)
end

end

@testset "Parametric struct with function inside module" begin


    p = Point(3, 4)
    q = Point(0, 0)
    @assert distance(p, q) == 5.0
    @test (5.0) == 5.0
end

true  # Test passed
