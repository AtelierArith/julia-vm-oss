# Test for src/base/math.jl
# Based on Julia's test/math.jl
using Test

@testset "sign" begin
    @test sign(1) == 1
    @test sign(-1) == -1
    @test sign(0) == 0
    @test sign(5.5) == 1
    @test sign(-5.5) == -1
end

@testset "clamp" begin
    # From Julia's test/math.jl
    @test clamp(0, 1, 3) == 1
    @test clamp(1, 1, 3) == 1
    @test clamp(2, 1, 3) == 2
    @test clamp(3, 1, 3) == 3
    @test clamp(4, 1, 3) == 3

    @test clamp(0.0, 1, 3) == 1.0
    @test clamp(1.0, 1, 3) == 1.0
    @test clamp(2.0, 1, 3) == 2.0
    @test clamp(3.0, 1, 3) == 3.0
    @test clamp(4.0, 1, 3) == 3.0
end

@testset "mod and rem" begin
    @test mod(7, 3) == 1
    @test mod(-7, 3) == 2
    @test mod(7, -3) == -2
    @test mod(-7, -3) == -1

    @test rem(7, 3) == 1
    @test rem(-7, 3) == -1
    @test rem(7, -3) == 1
end

@testset "div and fld" begin
    @test div(7, 3) == 2
    @test div(9, 3) == 3
    @test div(-7, 3) == -3

    @test fld(7, 3) == 2
    @test fld(-7, 3) == -3
    @test fld(9, 3) == 3
end

@testset "hypot" begin
    @test hypot(3, 4) == 5.0
    @test hypot(5, 12) == 13.0
    @test hypot(0, 5) == 5.0
    @test hypot(3, 0) == 3.0
end

@testset "deg2rad and rad2deg" begin
    @test isapprox(deg2rad(0), 0.0) == true
    @test isapprox(deg2rad(90), pi / 2) == true
    @test isapprox(deg2rad(180), pi) == true
    @test isapprox(deg2rad(360), 2 * pi) == true

    @test isapprox(rad2deg(0), 0.0) == true
    @test isapprox(rad2deg(pi / 2), 90.0) == true
    @test isapprox(rad2deg(pi), 180.0) == true
end

@testset "iseven and isodd" begin
    @test iseven(0) == true
    @test iseven(1) == false
    @test iseven(2) == true
    @test iseven(-2) == true

    @test isodd(0) == false
    @test isodd(1) == true
    @test isodd(2) == false
    @test isodd(-1) == true
end

@testset "trigonometric functions" begin
    # sinpi and cospi
    @test isapprox(sinpi(0), 0.0) == true
    @test isapprox(sinpi(0.5), 1.0) == true
    @test isapprox(sinpi(1), 0.0) == true

    @test isapprox(cospi(0), 1.0) == true
    @test isapprox(cospi(0.5), 0.0) == true
    @test isapprox(cospi(1), -1.0) == true

    # sinc
    @test sinc(0) == 1.0
end

@testset "degree-based trig" begin
    # sind, cosd, tand
    @test isapprox(sind(0), 0.0) == true
    @test isapprox(sind(90), 1.0) == true
    @test isapprox(sind(180), 0.0) == true

    @test isapprox(cosd(0), 1.0) == true
    @test isapprox(cosd(90), 0.0) == true
    @test isapprox(cosd(180), -1.0) == true

    @test isapprox(tand(45), 1.0) == true
end

@testset "reciprocal trig" begin
    @test isapprox(sec(0), 1.0) == true
end


@testset "mod1 and fld1" begin
    @test mod1(0, 3) == 3
    @test mod1(1, 3) == 1
    @test mod1(2, 3) == 2
    @test mod1(3, 3) == 3
    @test mod1(4, 3) == 1
    @test mod1(6, 3) == 3
end

@testset "mod2pi" begin
    @test mod2pi(0) == 0.0
    @test isapprox(mod2pi(pi), pi) == true
end

@testset "evalpoly" begin
    # p(x) = 1 + 2x + 3x^2 at x=2: 1 + 4 + 12 = 17
    @test evalpoly(2, [1, 2, 3]) == 17
    # p(x) = 1 at x=5: 1
    @test evalpoly(5, [1]) == 1
    # p(x) = 2 + 3x at x=4: 2 + 12 = 14
    @test evalpoly(4, [2, 3]) == 14
end

println("test_math.jl: All tests passed!")
