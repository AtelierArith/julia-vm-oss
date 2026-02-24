# Test sinpi(), cospi(), sincospi() functions (Issue #1861)

using Test

@testset "sinpi basic" begin
    @test sinpi(0.0) == 0.0
    @test sinpi(0.5) == 1.0
    @test sinpi(1.0) ≈ 0.0 atol=1e-15
    @test sinpi(-0.5) == -1.0
end

@testset "cospi basic" begin
    @test cospi(0.0) == 1.0
    @test cospi(0.5) ≈ 0.0 atol=1e-15
    @test cospi(1.0) == -1.0
end

@testset "sincospi basic" begin
    s, c = sincospi(0.0)
    @test s == 0.0
    @test c == 1.0

    s2, c2 = sincospi(0.5)
    @test s2 == 1.0
    @test c2 ≈ 0.0 atol=1e-15
end

true
