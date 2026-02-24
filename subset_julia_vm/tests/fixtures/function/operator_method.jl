# Test operator method definitions (short form)
struct Point2D
    x::Float64
    y::Float64
end

# Operator method definition using short form
+(a::Point2D, b::Point2D) = Point2D(a.x + b.x, a.y + b.y)
-(a::Point2D, b::Point2D) = Point2D(a.x - b.x, a.y - b.y)
*(a::Point2D, s::Float64) = Point2D(a.x * s, a.y * s)
==(a::Point2D, b::Point2D) = a.x == b.x && a.y == b.y

p1 = Point2D(1.0, 2.0)
p2 = Point2D(3.0, 4.0)
p3 = p1 + p2
result = p3.x + p3.y  # Should be 10.0
