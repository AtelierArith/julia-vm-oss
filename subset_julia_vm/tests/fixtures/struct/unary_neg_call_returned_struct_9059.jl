# Static unary negation must dispatch when the operand is a call-returned struct
# (Issue #9059).

using Test

module StaticUnaryNegCall9059
export Box, wrap

struct Box <: Real
    x::Int
end

wrap(x)::Box = Box(x)
Base.:-(b::Box) = Box(-b.x)
Base.sin(b::Box)::Box = b
end

using .StaticUnaryNegCall9059

@testset "Static unary negation of call-returned structs" begin
    b = wrap(7)

    @test (-sin(b)).x == -7

    y = sin(b)
    @test (-y).x == -7
end

true
