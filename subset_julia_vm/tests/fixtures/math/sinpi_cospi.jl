# Test sinpi(), cospi(), sincospi() functions (Issue #1861)
# Accuracy/exactness at integer & half-integer args, and large x (Issue #8309):
# sinpi/cospi must be exact there, not the naive sin(pi*x)/cos(pi*x).

using Test

@testset "sinpi basic" begin
    @test sinpi(0.0) == 0.0
    @test sinpi(0.5) == 1.0
    @test sinpi(1.0) == 0.0
    @test sinpi(-0.5) == -1.0
end

@testset "cospi basic" begin
    @test cospi(0.0) == 1.0
    @test cospi(0.5) == 0.0
    @test cospi(1.0) == -1.0
end

@testset "sincospi basic" begin
    s, c = sincospi(0.0)
    @test s == 0.0
    @test c == 1.0

    s2, c2 = sincospi(0.5)
    @test s2 == 1.0
    @test c2 == 0.0
end

# Issue #8309: exactness at integer / half-integer arguments.
@testset "sinpi exact at integers and half-integers" begin
    @test sinpi(2.0) == 0.0
    @test sinpi(3.0) == 0.0
    @test sinpi(10.0) == 0.0
    @test sinpi(1.5) == -1.0
    @test sinpi(2.5) == 1.0
    @test cospi(2.0) == 1.0
    @test cospi(3.0) == -1.0
    @test cospi(10.0) == 1.0
    @test cospi(1.5) == 0.0
    @test cospi(2.5) == 0.0
    # sign of zero matches upstream
    @test sinpi(-2.0) === -0.0
    @test sinpi(2.0) === 0.0
end

# Issue #8309: accurate for large x where naive sin(pi*x) loses precision.
@testset "sinpi/cospi accurate for large x" begin
    @test sinpi(123456.5) == 1.0
    @test cospi(123456.5) == 0.0
    @test sinpi(1.0e15) == 0.0
    @test cospi(1.0e15) == 1.0
end

# Integer arguments route through exact methods.
@testset "sinpi/cospi integer args" begin
    @test sinpi(2) == 0.0
    @test sinpi(-3) === -0.0
    @test cospi(3) == -1.0
    @test cospi(4) == 1.0
end

# Non-special values still match sin(pi*x)/cos(pi*x) closely.
@testset "sinpi/cospi reference values" begin
    @test sinpi(0.25) ≈ 0.7071067811865476 atol=1e-15
    @test cospi(0.25) ≈ 0.7071067811865476 atol=1e-15
    @test sinpi(0.1) ≈ 0.30901699437494745 atol=1e-15
    @test cospi(0.1) ≈ 0.9510565162951535 atol=1e-15
end

true
