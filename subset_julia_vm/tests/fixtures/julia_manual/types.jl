# Julia Manual: Types
# https://docs.julialang.org/en/v1/manual/types/
# Tests type system, abstract types, composite types, and type hierarchy.

using Test

# Abstract type hierarchy
abstract type Shape end
abstract type Shape2D <: Shape end

# Composite types (structs)
struct Point
    x::Float64
    y::Float64
end

struct Circle <: Shape2D
    center::Point
    radius::Float64
end

struct Rectangle <: Shape2D
    origin::Point
    width::Float64
    height::Float64
end

# Mutable struct
mutable struct Counter
    value::Int64
end

# Functions using types
function area(c::Circle)
    pi * c.radius^2
end

function area(r::Rectangle)
    r.width * r.height
end

function increment!(c::Counter)
    c.value = c.value + 1
end

@testset "Primitive types" begin
    @test typeof(1) == Int64
    @test typeof(1.0) == Float64
    @test typeof(true) == Bool
    @test typeof("hello") == String
end

@testset "Struct construction" begin
    p = Point(1.0, 2.0)
    @test p.x == 1.0
    @test p.y == 2.0

    c = Circle(Point(0.0, 0.0), 5.0)
    @test c.center.x == 0.0
    @test c.radius == 5.0
end

@testset "Type hierarchy" begin
    c = Circle(Point(0.0, 0.0), 1.0)
    @test isa(c, Circle)
    @test isa(c, Shape2D)
    @test isa(c, Shape)
end

@testset "Multiple dispatch on types" begin
    c = Circle(Point(0.0, 0.0), 2.0)
    r = Rectangle(Point(0.0, 0.0), 3.0, 4.0)
    @test area(c) == 4.0 * pi
    @test area(r) == 12.0
end

@testset "Mutable structs" begin
    cnt = Counter(0)
    @test cnt.value == 0
    increment!(cnt)
    @test cnt.value == 1
    increment!(cnt)
    @test cnt.value == 2
end

@testset "Type checks" begin
    @test isa(1, Int64)
    @test isa(1.0, Float64)
    @test isa("hello", String)
    @test isa(1, Number)
    @test isa(1.0, Number)
end

true
