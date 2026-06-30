# Issue #7172: a struct defined inside a module can be constructed via its
# module-qualified name (`M.Point(...)`), not only the unqualified name brought
# into scope by `using`. Covers both non-parametric and parametric structs.
using Test

module Shapes
struct Point
    x::Int
    y::Int
end

struct Vec2{T}
    x::T
    y::T
end
end

@testset "Issue #7172: module-qualified constructor" begin
    p = Shapes.Point(3, 4)
    @test p.x == 3
    @test p.y == 4

    v = Shapes.Vec2(1.5, 2.5)
    @test v.x == 1.5
    @test v.y == 2.5
    @test typeof(v.x) == Float64
end

true
