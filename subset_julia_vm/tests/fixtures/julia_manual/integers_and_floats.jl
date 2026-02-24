# Julia Manual: Integers and Floating-Point Numbers
# https://docs.julialang.org/en/v1/manual/integers-and-floating-point-numbers/
# Tests integer types, float types, arithmetic, and special values.

using Test

@testset "Integer types" begin
    @test typeof(1) == Int64
    @test typeof(true) == Bool
    @test typeof(false) == Bool
end

@testset "Integer arithmetic" begin
    @test 1 + 2 == 3
    @test 10 - 3 == 7
    @test 3 * 4 == 12
    @test div(7, 2) == 3
    @test 7 % 3 == 1
    @test 2^10 == 1024
end

@testset "Boolean as numeric" begin
    @test true == 1
    @test false == 0
    @test true + true == 2
    @test 3 * false == 0
    @test true + 1 == 2
end

@testset "Floating-point types" begin
    @test typeof(1.0) == Float64
    @test typeof(1.0f0) == Float32
end

@testset "Floating-point arithmetic" begin
    @test 1.0 + 2.0 == 3.0
    @test 1.5 * 2.0 == 3.0
    @test 7.0 / 2.0 == 3.5
    @test 2.0^3.0 == 8.0
end

@testset "Floating-point special values" begin
    @test Inf > 0
    @test -Inf < 0
    @test isnan(NaN)
    @test isinf(Inf)
    @test !isfinite(Inf)
    @test isfinite(1.0)
end

@testset "Type conversions" begin
    @test Float64(1) == 1.0
    @test Int64(2.0) == 2
    @test Float32(1.5) == 1.5f0
end

true
