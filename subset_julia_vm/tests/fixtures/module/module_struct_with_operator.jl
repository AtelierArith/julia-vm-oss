# Test: Struct with operator overloading in module

using Test
using .MyGeometry

module MyGeometry

export Point

struct Point{T<:Real}
    x::T
    y::T
end

Base.:+(p::Point{T}, q::Point{T}) where T <: Real = Point{T}(p.x + q.x, p.y + q.y)

end

@testset "Struct with operator overloading inside module" begin


    p = Point(1, 2)
    q = Point(3, 4)
    r = p + q
    @assert r.x == 4 && r.y == 6
    @test (r.x + r.y) == 10.0
end

true  # Test passed
