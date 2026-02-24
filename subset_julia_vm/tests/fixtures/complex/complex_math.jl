# Test complex number math functions: abs, abs2, angle, exp, sqrt, log (Issue #1881)

using Test

@testset "abs complex" begin
    z = Complex{Float64}(3.0, 4.0)
    @test abs(z) == 5.0

    z2 = Complex{Float64}(0.0, 1.0)
    @test abs(z2) == 1.0

    z3 = Complex{Float64}(0.0, 0.0)
    @test abs(z3) == 0.0
end

@testset "abs2 complex" begin
    z = Complex{Float64}(3.0, 4.0)
    @test abs2(z) == 25.0

    z2 = Complex{Float64}(1.0, 1.0)
    @test abs2(z2) == 2.0
end

@testset "angle complex" begin
    # angle of 1+0i = 0
    z1 = Complex{Float64}(1.0, 0.0)
    @test abs(angle(z1)) < 1e-14

    # angle of 0+1i = pi/2
    z2 = Complex{Float64}(0.0, 1.0)
    @test abs(angle(z2) - pi / 2.0) < 1e-14

    # angle of -1+0i = pi
    z3 = Complex{Float64}(-1.0, 0.0)
    @test abs(angle(z3) - pi) < 1e-14
end

@testset "exp complex" begin
    # exp(0+0i) = 1+0i
    z0 = Complex{Float64}(0.0, 0.0)
    r0 = exp(z0)
    @test abs(r0.re - 1.0) < 1e-14
    @test abs(r0.im) < 1e-14

    # exp(i*pi) = -1 (Euler's formula)
    z1 = Complex{Float64}(0.0, pi)
    r1 = exp(z1)
    @test abs(r1.re - (-1.0)) < 1e-14
    @test abs(r1.im) < 1e-14
end

@testset "sqrt complex" begin
    # sqrt(4+0i) = 2+0i
    z1 = Complex{Float64}(4.0, 0.0)
    r1 = sqrt(z1)
    @test abs(r1.re - 2.0) < 1e-14
    @test abs(r1.im) < 1e-14

    # sqrt(-1+0i) = 0+1i
    z2 = Complex{Float64}(-1.0, 0.0)
    r2 = sqrt(z2)
    @test abs(r2.re) < 1e-14
    @test abs(r2.im - 1.0) < 1e-14

    # sqrt(0+0i) = 0+0i
    z3 = Complex{Float64}(0.0, 0.0)
    r3 = sqrt(z3)
    @test r3.re == 0.0
    @test r3.im == 0.0
end

@testset "log complex" begin
    # log(1+0i) = 0+0i
    z1 = Complex{Float64}(1.0, 0.0)
    r1 = log(z1)
    @test abs(r1.re) < 1e-14
    @test abs(r1.im) < 1e-14

    # log(e+0i) = 1+0i
    z2 = Complex{Float64}(exp(1.0), 0.0)
    r2 = log(z2)
    @test abs(r2.re - 1.0) < 1e-14
    @test abs(r2.im) < 1e-14
end

true
