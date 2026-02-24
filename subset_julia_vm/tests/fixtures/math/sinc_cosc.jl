# Test sinc() and cosc() functions (Issue #1861)

using Test

@testset "sinc basic" begin
    @test sinc(0) == 1.0
    @test sinc(0.0) == 1.0
    @test sinc(1.0) ≈ 0.0 atol=1e-15
    @test sinc(-1.0) ≈ 0.0 atol=1e-15
end

@testset "cosc basic" begin
    @test cosc(0) == 0.0
    @test cosc(0.0) == 0.0
    @test cosc(1.0) ≈ -1.0 atol=1e-10
end

true
