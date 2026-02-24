# Test degree-based reciprocal trig functions: secd, cscd, cotd, asecd, acscd, acotd (Issue #1863)

using Test

@testset "secd basic" begin
    @test secd(0.0) ≈ 1.0 atol=1e-14
    @test secd(60.0) ≈ 2.0 atol=1e-14
end

@testset "cscd basic" begin
    @test cscd(90.0) ≈ 1.0 atol=1e-14
    @test cscd(30.0) ≈ 2.0 atol=1e-14
end

@testset "cotd basic" begin
    @test cotd(45.0) ≈ 1.0 atol=1e-14
end

@testset "asecd basic" begin
    @test asecd(1.0) ≈ 0.0 atol=1e-14
    @test asecd(2.0) ≈ 60.0 atol=1e-12
end

@testset "acscd basic" begin
    @test acscd(1.0) ≈ 90.0 atol=1e-12
    @test acscd(2.0) ≈ 30.0 atol=1e-12
end

@testset "acotd basic" begin
    @test acotd(1.0) ≈ 45.0 atol=1e-12
end

true
