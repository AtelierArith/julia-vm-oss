using Test

# Issue #7863: a *standalone* quoted subtype/supertype constraint must use the
# operator as its `head` — `Expr(:<:, :S, :Real)` / `Expr(:>:, :S, :Int)` —
# matching upstream Julia, not the `Expr(:call, :<:, :S, :Real)` shape that the
# generic BinaryExpression quote path produced before the fix. The bounded
# `where` path (Issue #7845) already produced the correct shape; this guards the
# standalone expression that never reached `subtype_constraint_constructor`.
@testset "standalone quoted subtype constraint uses :<: head (Issue #7863)" begin
    # Subtype constraint `S<:Real`.
    ex = :(S<:Real)
    @test ex isa Expr
    @test ex.head === :(<:)
    @test ex.args == [:S, :Real]

    # Supertype constraint `S>:Int`.
    ey = :(S>:Int)
    @test ey isa Expr
    @test ey.head === :(>:)
    @test ey.args == [:S, :Int]

    # Nested operands are still lowered recursively (head stays :<:).
    ez = :(Vector{T}<:AbstractArray)
    @test ez.head === :(<:)
    @test ez.args == [:(Vector{T}), :AbstractArray]
end

true
