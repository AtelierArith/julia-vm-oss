# Test sincos() function (Issue #1877)

using Test

@testset "sincos basic" begin
    s, c = sincos(0.0)
    @test s == 0.0
    @test c == 1.0
end

@testset "sincos pi/2" begin
    s, c = sincos(pi / 2.0)
    @test abs(s - 1.0) < 1e-14
    @test abs(c) < 1e-14
end

@testset "sincos pi" begin
    s, c = sincos(pi)
    @test abs(s) < 1e-14
    @test abs(c - (-1.0)) < 1e-14
end

@testset "sincos negative" begin
    s, c = sincos(-pi / 2.0)
    @test abs(s - (-1.0)) < 1e-14
    @test abs(c) < 1e-14
end

@testset "sincos consistency" begin
    x = 1.23
    s, c = sincos(x)
    @test abs(s - sin(x)) < 1e-14
    @test abs(c - cos(x)) < 1e-14
end

true
