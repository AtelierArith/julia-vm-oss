# Float utility functions: exponent, significand, frexp, nextfloat, prevfloat
# Issue #2101

using Test

@testset "exponent" begin
    @test exponent(8.0) == 3
    @test exponent(1.0) == 0
    @test exponent(0.5) == -1
    @test exponent(15.2) == 3
    @test exponent(-3.0) == 1
    @test exponent(2.0) == 1
    @test exponent(4.0) == 2
    @test exponent(0.25) == -2
end

@testset "significand" begin
    @test significand(8.0) == 1.0
    @test significand(15.2) == 1.9
    @test significand(-15.2) == -1.9
    @test significand(1.0) == 1.0
    @test significand(0.5) == 1.0
    @test significand(2.0) == 1.0
    @test significand(3.0) == 1.5
    @test significand(0.0) == 0.0
    @test isinf(significand(Inf))
    @test isnan(significand(NaN))
end

@testset "frexp" begin
    @test frexp(6.0) == (0.75, 3)
    @test frexp(8.0) == (0.5, 4)
    @test frexp(1.0) == (0.5, 1)
    @test frexp(0.5) == (0.5, 0)
    @test frexp(-3.0) == (-0.75, 2)
    @test frexp(0.0) == (0.0, 0)
    # Verify frexp identity: x == frac * 2^exp
    (frac, exp) = frexp(15.2)
    @test frac * 2.0^exp == 15.2
    (frac2, exp2) = frexp(-7.5)
    @test frac2 * 2.0^exp2 == -7.5
end

@testset "nextfloat" begin
    @test nextfloat(0.0) == 5.0e-324
    @test nextfloat(1.0) == 1.0 + eps()
    @test nextfloat(Inf) == Inf
    @test nextfloat(-Inf) == -floatmax()
    @test nextfloat(1.0) > 1.0
    @test nextfloat(-1.0) > -1.0
    @test nextfloat(2.0) == 2.0 + eps(2.0)
end

@testset "prevfloat" begin
    @test prevfloat(0.0) == -5.0e-324
    @test prevfloat(Inf) == floatmax()
    @test prevfloat(-Inf) == -Inf
    @test prevfloat(1.0) < 1.0
    @test prevfloat(-1.0) < -1.0
end

@testset "nextfloat/prevfloat inverse" begin
    @test prevfloat(nextfloat(1.0)) == 1.0
    @test nextfloat(prevfloat(1.0)) == 1.0
    @test prevfloat(nextfloat(2.0)) == 2.0
    @test nextfloat(prevfloat(2.0)) == 2.0
    @test prevfloat(nextfloat(0.25)) == 0.25
    @test nextfloat(prevfloat(0.25)) == 0.25
    @test prevfloat(nextfloat(-1.0)) == -1.0
    @test nextfloat(prevfloat(-1.0)) == -1.0
end

true
