# Test degree-based trigonometric functions: sind, cosd, tand, asind, acosd, atand, sincosd (Issue #1863)

using Test

@testset "sind basic" begin
    @test sind(0.0) ≈ 0.0 atol=1e-15
    @test sind(30.0) ≈ 0.5 atol=1e-14
    @test sind(90.0) ≈ 1.0 atol=1e-14
end

@testset "cosd basic" begin
    @test cosd(0.0) ≈ 1.0 atol=1e-15
    @test cosd(60.0) ≈ 0.5 atol=1e-14
    @test cosd(90.0) ≈ 0.0 atol=1e-14
end

@testset "tand basic" begin
    @test tand(0.0) ≈ 0.0 atol=1e-15
    @test tand(45.0) ≈ 1.0 atol=1e-14
end

@testset "asind basic" begin
    @test asind(0.0) ≈ 0.0 atol=1e-14
    @test asind(0.5) ≈ 30.0 atol=1e-12
    @test asind(1.0) ≈ 90.0 atol=1e-12
end

@testset "acosd basic" begin
    @test acosd(1.0) ≈ 0.0 atol=1e-14
    @test acosd(0.5) ≈ 60.0 atol=1e-12
    @test acosd(0.0) ≈ 90.0 atol=1e-12
end

@testset "atand basic" begin
    @test atand(0.0) ≈ 0.0 atol=1e-14
    @test atand(1.0) ≈ 45.0 atol=1e-12
end

@testset "sincosd basic" begin
    s, c = sincosd(0.0)
    @test s ≈ 0.0 atol=1e-15
    @test c ≈ 1.0 atol=1e-15

    s2, c2 = sincosd(30.0)
    @test s2 ≈ 0.5 atol=1e-14
    @test c2 ≈ sqrt(3.0) / 2.0 atol=1e-14
end

true
