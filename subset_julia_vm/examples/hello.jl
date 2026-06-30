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

println("Hello, World! from SubsetJuliaVM")
@show distance(p, q)
