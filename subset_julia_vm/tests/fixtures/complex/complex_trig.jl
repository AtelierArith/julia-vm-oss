# Test complex trigonometric functions: sin, cos, tan (Issue #1881)

using Test

@testset "sin complex" begin
    # sin(0+0i) = 0+0i
    z0 = Complex{Float64}(0.0, 0.0)
    r0 = sin(z0)
    @test abs(r0.re) < 1e-14
    @test abs(r0.im) < 1e-14

    # sin(pi/2 + 0i) = 1+0i
    z1 = Complex{Float64}(pi / 2.0, 0.0)
    r1 = sin(z1)
    @test abs(r1.re - 1.0) < 1e-14
    @test abs(r1.im) < 1e-14
end

@testset "cos complex" begin
    # cos(0+0i) = 1+0i
    z0 = Complex{Float64}(0.0, 0.0)
    r0 = cos(z0)
    @test abs(r0.re - 1.0) < 1e-14
    @test abs(r0.im) < 1e-14

    # cos(pi + 0i) = -1+0i
    z1 = Complex{Float64}(pi, 0.0)
    r1 = cos(z1)
    @test abs(r1.re - (-1.0)) < 1e-14
    @test abs(r1.im) < 1e-14
end

@testset "tan complex" begin
    # tan(0+0i) = 0+0i
    z0 = Complex{Float64}(0.0, 0.0)
    r0 = tan(z0)
    @test abs(r0.re) < 1e-14
    @test abs(r0.im) < 1e-14

    # tan(pi/4 + 0i) = 1+0i
    z1 = Complex{Float64}(pi / 4.0, 0.0)
    r1 = tan(z1)
    @test abs(r1.re - 1.0) < 1e-14
    @test abs(r1.im) < 1e-14
end

@testset "sin cos identity" begin
    # sin^2(z) + cos^2(z) = 1
    z = Complex{Float64}(1.5, 0.8)
    s = sin(z)
    c = cos(z)
    sum_sq = s * s + c * c
    @test abs(sum_sq.re - 1.0) < 1e-12
    @test abs(sum_sq.im) < 1e-12
end

true
