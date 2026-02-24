using Test

# Cross-type Rational comparisons: Rational{BigInt} vs Rational{Int64}
# Issue #2511: Previously failed with "expected I64, got BigInt" due to
# CallDynamicBinary using break-on-first-match instead of scored dispatch.

r_big = Rational{BigInt}(big(7), big(3))
r_int = 7//3
r_big2 = Rational{BigInt}(big(1), big(2))
r_int2 = 1//2

@testset "Cross-type Rational equality" begin
    # Rational{BigInt} == Rational{Int64}
    @test r_big == r_int
    @test r_int == r_big

    # Rational{BigInt} != Rational{Int64}
    @test r_big != r_int2
    @test r_int2 != r_big
end

@testset "Cross-type Rational ordering" begin
    # Rational{BigInt} < Rational{Int64}
    @test r_big2 < r_int
    @test !(r_big < r_int)

    # Rational{BigInt} > Rational{Int64}
    @test r_big > r_int2
    @test !(r_big2 > r_int)

    # Rational{BigInt} <= Rational{Int64}
    @test r_big <= r_int
    @test r_big2 <= r_int
    @test !(r_big > r_int)

    # Rational{BigInt} >= Rational{Int64}
    @test r_big >= r_int
    @test !(r_big2 >= r_int)
end

@testset "Cross-type Rational reverse ordering" begin
    # Rational{Int64} < Rational{BigInt}
    @test r_int2 < r_big
    @test !(r_int < r_big)

    # Rational{Int64} > Rational{BigInt}
    @test r_int > r_big2
    @test !(r_int > r_big)

    # Rational{Int64} <= Rational{BigInt}
    @test r_int <= r_big
    @test r_int2 <= r_big

    # Rational{Int64} >= Rational{BigInt}
    @test r_int >= r_big
    @test !(r_int2 >= r_big)
end

true
