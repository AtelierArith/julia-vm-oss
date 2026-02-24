# Test fieldcount function
# fieldcount(T::Type) returns the number of fields for a type (Julia semantics)

using Test

struct Point
    x::Float64
    y::Float64
end

struct Person
    name::String
    age::Int64
    height::Float64
end

struct Empty
end

@testset "fieldcount - number of fields in type" begin

    # Test fieldcount on types (standard Julia usage)
    @test fieldcount(Point) == 2
    @test fieldcount(Person) == 3
    @test fieldcount(Empty) == 0

    # Test nfields on instances (Julia's nfields works on values)
    p = Point(1.0, 2.0)
    @test nfields(p) == 2

    per = Person("Alice", 30, 5.6)
    @test nfields(per) == 3
end

true  # Test passed
