using Test

# Issue #9343: two number-tower-edge deviations from upstream Julia.
#   (1) Bool "strong zero" multiply: `*(x::Bool, y::AbstractFloat)` returns
#       `copysign(zero(y), y)` when x is false — stronger than IEEE NaN
#       propagation — so `false * Inf == 0.0` and `false * -Inf == -0.0`.
#   (2) Char is NOT a Number: `'a' == 97` is `false` (identity `===` fallback),
#       not a codepoint comparison; char arithmetic ('a'+1, 'b'-'a') is kept.

@testset "Bool strong-zero multiply (Issue #9343)" begin
    # false absorbs any float to a (signed) zero, even Inf/NaN.
    @test false * Inf === 0.0
    @test false * NaN === 0.0
    @test false * (-Inf) === -0.0
    @test Inf * false === 0.0
    @test (-Inf) * false === -0.0

    # true is the multiplicative identity and preserves the float exactly.
    @test true * 2.5 === 2.5
    @test true * Inf === Inf

    # Ordinary finite products are unchanged.
    @test false * 2.5 === 0.0

    # Float32 keeps its type and the strong-zero semantics.
    @test false * Inf32 === 0.0f0
    @test false * (-Inf32) === -0.0f0
    @test true * 2.5f0 === 2.5f0

    # Works through a variable / non-literal path too.
    b = false
    y = Inf
    @test b * y === 0.0

    # And through a typed function body (specializer path).
    f(x::Bool, z::Float64) = x * z
    @test f(false, Inf) === 0.0
    @test f(false, -Inf) === -0.0
    @test f(true, 3.5) === 3.5
end

@testset "Char is not numerically equal to Integer (Issue #9343)" begin
    # 'a' has codepoint 97 but Char != Int (no upstream method -> === -> false).
    @test ('a' == 97) === false
    @test (97 == 'a') === false
    @test ('a' == 97.0) === false
    @test isequal('a', 97) === false
    @test ('a' != 97) === true

    # Char-Char comparisons still work (both operands are Char).
    @test ('a' == 'a') === true
    @test ('a' < 'b') === true

    # Char arithmetic specials are preserved.
    @test ('a' + 1) === 'b'
    @test ('b' - 'a') === 1
    @test ('z' - 'a') === 25
    @test ('a' + 2) === 'c'
end

true
