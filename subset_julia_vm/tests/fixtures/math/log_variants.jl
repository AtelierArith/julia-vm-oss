# Test log2(), log10(), log1p() functions (Issue #1861)

using Test

@testset "log2 basic" begin
    @test log2(1.0) == 0.0
    @test log2(2.0) == 1.0
    @test log2(4.0) == 2.0
    @test log2(8.0) == 3.0
end

@testset "log10 basic" begin
    @test log10(1.0) == 0.0
    @test log10(10.0) == 1.0
    @test log10(100.0) == 2.0
    @test log10(1000.0) ≈ 3.0 atol=1e-15
end

@testset "log1p basic" begin
    @test log1p(0.0) == 0.0
    @test log1p(1.0) ≈ log(2.0) atol=1e-15
end

true
