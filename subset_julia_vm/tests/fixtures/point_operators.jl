# Define a 2D Point struct
struct Point
    x::Float64
    y::Float64
end

# Overload the + operator for Point
function Base.:+(a::Point, b::Point)
    return Point(a.x + b.x, a.y + b.y)
end

# Overload the - operator for Point
function Base.:-(a::Point, b::Point)
    return Point(a.x - b.x, a.y - b.y)
end

# Create two points
p1 = Point(1.0, 2.0)
p2 = Point(3.0, 4.0)

# Use the overloaded operators
p3 = p1 + p2
println("p1 = (", p1.x, ", ", p1.y, ")")
println("p2 = (", p2.x, ", ", p2.y, ")")
println("p1 + p2 = (", p3.x, ", ", p3.y, ")")

p4 = p2 - p1
println("p2 - p1 = (", p4.x, ", ", p4.y, ")")

# Chain operations
p5 = p1 + p2 + Point(10.0, 10.0)
println("p1 + p2 + (10,10) = (", p5.x, ", ", p5.y, ")")

p3.x + p3.y
