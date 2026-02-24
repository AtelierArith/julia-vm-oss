# Test: Module qualified function call in inline comparison
# Tests that MyGeometry.centroid([...]) == Point(...) works correctly
# without requiring variable assignment first.

using Test

module MyGeometry
using Statistics: mean

export distance
export Point

struct Point{T<:Real}
    x::T
    y::T
end

Base.:+(p::Point{T}, q::Point{T}) where T <: Real = Point{T}(p.x + q.x, p.y + q.y)
Base.:-(p::Point{T}, q::Point{T}) where T <: Real = Point{T}(p.x - q.x, p.y - q.y)

function distance(p::Point{T}, q::Point{T}) where T <: Real
    return sqrt((q.x - p.x)^2 + (q.y - p.y)^2)
end

function centroid(points::Vector{Point{T}}) where T <: Real
    x = mean([point.x for point in points])
    y = mean([point.y for point in points])
    Tnew = promote_type(typeof(x), typeof(y))
    return Point{Tnew}(convert(Tnew, x), convert(Tnew, y))
end

end # module

using .MyGeometry

p = Point(3, 4)
q = Point(0, 0)

# Test distance with exported function
@assert distance(p, q) == 5.0

# Test qualified access in inline comparison (the bug fix)
@assert MyGeometry.centroid([Point(1, 2), Point(3, 4), Point(5, 6)]) == Point(3.0, 4.0)

# Additional tests for qualified access patterns
@assert MyGeometry.distance(p, q) == 5.0
@assert MyGeometry.centroid([Point(0, 0), Point(2, 2)]) == Point(1.0, 1.0)

42

@testset "Module qualified function call (MyGeometry.centroid) in inline assert comparison" begin
end

true  # Test passed
