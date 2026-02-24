# Struct field type inference test
# Tests that field access on user-defined structs correctly infers field types

using Test

# Define test structs OUTSIDE @testset block per project guidelines
struct Point
    x::Float64
    y::Float64
end

struct Person
    name::String
    age::Int64
end

struct Container{T}
    value::T
end

mutable struct MutablePoint
    x::Float64
    y::Float64
end

@testset "User-defined struct field access type inference" begin
    # Test basic struct field access
    p = Point(1.0, 2.0)
    @test p.x == 1.0
    @test p.y == 2.0

    # Test different field types
    person = Person("Alice", 30)
    @test person.name == "Alice"
    @test person.age == 30

    # Test field access in arithmetic expressions
    p2 = Point(3.0, 4.0)
    dist_squared = p.x * p.x + p.y * p.y
    @test dist_squared == 5.0

    # Test field access in function calls
    function point_magnitude(pt)
        return sqrt(pt.x * pt.x + pt.y * pt.y)
    end
    @test point_magnitude(p2) == 5.0

    # Test mutable struct field access
    mp = MutablePoint(1.0, 2.0)
    @test mp.x == 1.0
    mp.x = 10.0
    @test mp.x == 10.0

    # Test nested field access
    function swap_xy(pt::Point)
        return Point(pt.y, pt.x)
    end
    swapped = swap_xy(Point(3.0, 4.0))
    @test swapped.x == 4.0
    @test swapped.y == 3.0

    # Test field access in loops
    points = [Point(1.0, 1.0), Point(2.0, 2.0), Point(3.0, 3.0)]
    sum_x = 0.0
    for pt in points
        sum_x += pt.x
    end
    @test sum_x == 6.0

    # Test parametric struct field access
    c_int = Container{Int64}(42)
    @test c_int.value == 42

    c_float = Container{Float64}(3.14)
    @test c_float.value == 3.14
end

true
