# Issue #9440: div/rem/mod between a Rational and an Unsigned integer must follow
# upstream's signedness rules, which the sjulia `x - div(x,y)*y` / 2-arg `div`
# formulas did not. Upstream (julia/base/rational.jl) routes div through the
# 3-arg integer `div(a, b, RoundToZero)` (which promotes a mixed Signed/Unsigned
# pair to the common unsigned type *before* dividing) and computes rem/mod
# directly as `Rational(rem/mod(cross_num, den), den)` (whose element type
# follows the integer rem/mod signedness rule). Result-type only bug: the values
# were already correct. Types below verified against upstream julia 1.12.6.

using Test

@testset "Rational x Unsigned div/rem/mod result type (Issue #9440)" begin
    # div: mixed Signed/Unsigned promotes to the unsigned type (not Int64/Int128).
    @test typeof(div(1 // 1, UInt64(3))) === UInt64
    @test typeof(div(1 // 1, UInt128(3))) === UInt128
    @test typeof(div(UInt64(3), 1 // 1)) === UInt64
    @test typeof(div(UInt128(3), 1 // 1)) === UInt128

    # rem(Rational, Unsigned): rem(Int, UInt) is signed -> Rational{Int}.
    @test typeof(rem(1 // 1, UInt64(3))) === Rational{Int64}
    @test typeof(rem(1 // 1, UInt128(3))) === Rational{Int128}
    # rem(Unsigned, Rational): rem(UInt, Int) is unsigned -> Rational{UInt}.
    @test typeof(rem(UInt64(3), 1 // 1)) === Rational{UInt64}
    @test typeof(rem(UInt128(3), 1 // 1)) === Rational{UInt128}

    # mod(Rational, Unsigned): mod(Int, UInt) is unsigned -> Rational{UInt}.
    @test typeof(mod(1 // 1, UInt64(3))) === Rational{UInt64}
    @test typeof(mod(1 // 1, UInt128(3))) === Rational{UInt128}
    # mod(Unsigned, Rational): mod(UInt, Int) is signed -> Rational{Int}.
    @test typeof(mod(UInt64(3), 1 // 1)) === Rational{Int64}
    @test typeof(mod(UInt128(3), 1 // 1)) === Rational{Int128}

    # Values stay correct across the type change.
    @test div(3 // 4, UInt64(5)) == 0
    @test rem(3 // 4, UInt64(5)) == 3 // 4
    @test mod(UInt64(5), 3 // 1) == 2 // 1
    @test div(7 // 2, 3) == 1              # ordinary signed case unchanged
    @test rem(7 // 2, 3) == 1 // 2
    @test mod(-7 // 4, 3) == 5 // 4

    # fld/cld are unaffected and keep their upstream signedness.
    @test typeof(fld(1 // 1, UInt64(3))) === Int64
    @test typeof(cld(1 // 1, UInt64(3))) === Int64
    @test typeof(fld(UInt64(3), 1 // 1)) === UInt64
    @test typeof(cld(UInt64(3), 1 // 1)) === UInt64
end

true
