# Test with unique struct name
struct TestPoint
    x::Float64
    y::Float64
end

function Base.:+(a::TestPoint, b::TestPoint)
    return TestPoint(a.x + b.x, a.y + b.y)
end

p1 = TestPoint(1.0, 2.0)
p2 = TestPoint(3.0, 4.0)
p3 = p1 + p2
p3.x
