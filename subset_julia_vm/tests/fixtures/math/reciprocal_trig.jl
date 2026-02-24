# Test reciprocal trigonometric functions: sec, csc, cot, asec, acsc, acot (Issue #1863)

using Test

@testset "sec basic" begin
    @test sec(0.0) == 1.0
    @test sec(pi) ≈ -1.0 atol=1e-14
end

@testset "csc basic" begin
    @test csc(pi / 2.0) ≈ 1.0 atol=1e-14
    @test csc(pi / 6.0) ≈ 2.0 atol=1e-14
end

@testset "cot basic" begin
    @test cot(pi / 4.0) ≈ 1.0 atol=1e-14
end

@testset "asec basic" begin
    @test asec(1.0) == 0.0
    @test asec(-1.0) ≈ pi atol=1e-14
end

@testset "acsc basic" begin
    @test acsc(1.0) ≈ pi / 2.0 atol=1e-14
end

@testset "acot basic" begin
    @test acot(1.0) ≈ pi / 4.0 atol=1e-14
    @test acot(0.0) ≈ pi / 2.0 atol=1e-14
end

true
