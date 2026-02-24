# Test hasfield function

using Test

struct Point
    x::Float64
    y::Float64
end

struct Person
    name::String
    age::Int64
end

@testset "hasfield - check if type has a field" begin

    # Define struct


    # Test hasfield with symbol - exists
    @assert hasfield(Point, :x)
    @assert hasfield(Point, :y)

    @assert hasfield(Person, :name)
    @assert hasfield(Person, :age)

    # Test hasfield - doesn't exist (use negation)
    @assert !hasfield(Point, :z)
    @assert !hasfield(Person, :height)

    @test (true)
end

true  # Test passed
