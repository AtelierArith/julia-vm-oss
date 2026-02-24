using Test

abstract type Shape end
struct Circle <: Shape
    r::Float64
end
struct Rectangle <: Shape
    w::Float64
    h::Float64
end

area(c::Circle) = 3.14159 * c.r * c.r
area(r::Rectangle) = r.w * r.h

@testset "abstract type dispatch" begin
    c = Circle(2.0)
    r = Rectangle(3.0, 4.0)
    @test area(c) > 12.0
    @test area(c) < 13.0
    @test area(r) == 12.0
    @test isa(c, Shape)
    @test isa(r, Shape)
end

true
