# Test cis, cispi, reim functions (Issue #1881)

using Test

@testset "cis basic" begin
    # cis(0) = cos(0) + i*sin(0) = 1+0i
    r0 = cis(0.0)
    @test abs(r0.re - 1.0) < 1e-14
    @test abs(r0.im) < 1e-14

    # cis(pi/2) = cos(pi/2) + i*sin(pi/2) = 0+1i
    r1 = cis(pi / 2.0)
    @test abs(r1.re) < 1e-14
    @test abs(r1.im - 1.0) < 1e-14

    # cis(pi) = cos(pi) + i*sin(pi) = -1+0i
    r2 = cis(pi)
    @test abs(r2.re - (-1.0)) < 1e-14
    @test abs(r2.im) < 1e-14
end

@testset "cis unit circle" begin
    # |cis(x)| = 1 for all x
    r = cis(1.23)
    @test abs(abs(r) - 1.0) < 1e-14
end

@testset "cispi basic" begin
    # cispi(0) = cos(0) + i*sin(0) = 1+0i
    r0 = cispi(0.0)
    @test abs(r0.re - 1.0) < 1e-14
    @test abs(r0.im) < 1e-14

    # cispi(0.5) = cos(pi/2) + i*sin(pi/2) = 0+1i
    r1 = cispi(0.5)
    @test abs(r1.re) < 1e-14
    @test abs(r1.im - 1.0) < 1e-14

    # cispi(1) = cos(pi) + i*sin(pi) = -1+0i
    r2 = cispi(1.0)
    @test abs(r2.re - (-1.0)) < 1e-14
    @test abs(r2.im) < 1e-14
end

@testset "reim complex" begin
    z = Complex{Float64}(3.0, 4.0)
    r, i = reim(z)
    @test r == 3.0
    @test i == 4.0
end

@testset "reim real" begin
    r, i = reim(5.0)
    @test r == 5.0
    @test i == 0.0
end

@testset "reim int" begin
    r, i = reim(7)
    @test r == 7
    @test i == 0
end

true
