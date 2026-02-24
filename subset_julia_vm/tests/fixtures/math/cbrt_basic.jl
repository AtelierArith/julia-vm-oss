# Test cbrt() cube root function (Issue #1857)

using Test

@testset "cbrt positive values" begin
    @test cbrt(27.0) == 3.0
    @test cbrt(8.0) == 2.0
    @test cbrt(1.0) == 1.0
    @test cbrt(0.0) == 0.0
end

@testset "cbrt negative values" begin
    @test cbrt(-27.0) == -3.0
    @test cbrt(-8.0) == -2.0
    @test cbrt(-1.0) == -1.0
end

@testset "cbrt integer argument" begin
    @test cbrt(27) == 3.0
    @test cbrt(-27) == -3.0
    @test cbrt(0) == 0.0
end

true
