# Test: Module with parametric struct, operators, and functions
# Tests module struct definition, Base operator extension, struct field comparison,
# and Vector{Point{T}} type annotation for method dispatch with where clause.

using Test

module MyGeometry
using Statistics: mean

export distance, centroid
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

# Vector{Point{T}} type annotation with where clause and dynamic type parameters
function centroid(points::Vector{Point{T}}) where T <: Real
    x = mean([point.x for point in points])
    y = mean([point.y for point in points])
    # Use promote_type with typeof to get the correct result type
    Tnew = promote_type(typeof(x), typeof(y))
    return Point{Tnew}(x, y)
end

end # module

using .MyGeometry

# Test Point creation
p = Point(3, 4)
q = Point(0, 0)

# Test distance function
d = distance(p, q)
@assert d == 5.0

# Test operator overloading
r = p + q
@assert r == Point(3, 4)

s = p - q
@assert s == Point(3, 4)

# Test centroid function with Vector{Point{T}} dispatch
c = centroid([Point(1, 2), Point(3, 4), Point(5, 6)])
@assert c == Point(3.0, 4.0)

# Test with Float64 points
pf = Point(1.5, 2.5)
qf = Point(0.5, 0.5)
@assert pf + qf == Point(2.0, 3.0)

42

@testset "Module with parametric struct, operators, dynamic type parameters (Point{Tnew})" begin
end

true  # Test passed
