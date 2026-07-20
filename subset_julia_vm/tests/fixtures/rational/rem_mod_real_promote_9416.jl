# Issue #9416: rem/mod between Rational and Float/BigFloat threw MethodError
# (TypeError for the BigFloat pair) instead of promoting. Upstream reaches these
# pairs through `rem(x::Real, y::Real) = rem(promote(x,y)...)` in
# julia/base/promotion.jl; sjulia adds the Rational-scoped equivalents in
# base/rational.jl.
#
# Issue #9422: the cross-multiplication inside rem/mod(Rational, Integer) used a
# plain (wrapping) `*`, so a typemax(UInt128) operand silently wrapped — and in
# multi-cell runs could reach a Rust-side UInt128→Int64 conversion that aborted
# with an UNCATCHABLE OverflowError. It now uses `checked_mul` (ported from
# julia/base/checked.jl), raising upstream's catchable OverflowError.
#
# All expected values/types verified against upstream julia 1.12.

using Test

@testset "rem/mod Rational x Real promotes (Issue #9416)" begin
    # Float64 x Rational
    @test rem(2.5, 3 // 4) == 0.25
    @test rem(2.5, 3 // 4) isa Float64
    @test mod(2.5, 3 // 4) == 0.25
    @test rem(3 // 4, 2.5) == 0.75
    @test rem(3 // 4, 2.5) isa Float64
    @test mod(3 // 4, 2.5) == 0.75

    # Float32 x Rational promotes to Float32
    @test mod(Float32(2.5), 3 // 4) === Float32(0.25)
    @test rem(Float32(2.5), 3 // 4) === Float32(0.25)
    @test rem(3 // 4, Float32(2.5)) === Float32(0.75)

    # BigFloat x Rational promotes to BigFloat
    @test mod(big"2.5", 3 // 4) == 0.25
    @test mod(big"2.5", 3 // 4) isa BigFloat
    @test rem(big"2.5", 3 // 4) == 0.25
    @test rem(3 // 4, big"2.5") == 0.75

    # negative operands keep float rem/mod semantics after promotion
    @test rem(-2.5, 3 // 4) == -0.25
    @test mod(-2.5, 3 // 4) == 0.5

    # the more-specific Rational x Rational / Rational x Integer methods still win
    @test rem(3 // 4, 1 // 3) == 1 // 12
    @test rem(7 // 2, 3) == 1 // 2
    @test mod(-7 // 4, 3) == 5 // 4
end

@testset "rem/mod Rational x typemax(UInt128) raises catchable OverflowError (Issue #9422)" begin
    @test_throws OverflowError rem(3 // 4, typemax(UInt128))
    @test_throws OverflowError rem(typemax(UInt128), 3 // 4)
    @test_throws OverflowError mod(3 // 4, typemax(UInt128))
    @test_throws OverflowError mod(typemax(UInt128), 3 // 4)

    # The #9422 abort escaped try/catch — assert an ordinary catch works.
    caught = try
        rem(3 // 4, typemax(UInt128))
        false
    catch e
        e isa OverflowError
    end
    @test caught

    # In-range unsigned operands are unaffected (types per Issue #9440 rules).
    @test rem(3 // 4, UInt128(5)) == 3 // 4
    @test typeof(rem(3 // 4, UInt128(5))) === Rational{Int128}
    @test mod(UInt128(5), 3 // 1) == 2 // 1
end

true
