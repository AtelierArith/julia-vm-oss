# Test extrema() and diff() reduction functions (Issue #1874)

using Test

@testset "extrema basic" begin
    @test extrema([3, 1, 4, 1, 5, 9]) == (1, 9)
    @test extrema([1.0, 2.0, 3.0]) == (1.0, 3.0)
    @test extrema([-5, -1, 0, 3]) == (-5, 3)
    @test extrema([42]) == (42, 42)
end

@testset "diff basic" begin
    result = diff([1, 3, 6, 10])
    @test typeof(result) === Vector{Int64}
    @test eltype(result) === Int64
    @test result[1] == 2.0
    @test result[2] == 3.0
    @test result[3] == 4.0
    @test length(result) == 3
end

@testset "diff float" begin
    result = diff([1.0, 2.5, 4.0])
    @test typeof(result) === Vector{Float64}
    @test eltype(result) === Float64
    @test result[1] == 1.5
    @test result[2] == 1.5
end

@testset "diff preserves non-Float64 result eltypes (#4018, #4600)" begin
    small_ints = diff(Int8[1, 3, 6])
    @test typeof(small_ints) === Vector{Int8}
    @test eltype(small_ints) === Int8
    @test length(small_ints) == 2
    @test typeof(small_ints[1]) === Int8
    @test small_ints[1] == Int8(2)
    @test small_ints[2] == Int8(3)

    floats = diff(Float32[1, 2.5, 4])
    @test typeof(floats) === Vector{Float32}
    @test eltype(floats) === Float32
    @test length(floats) == 2
    @test typeof(floats[1]) === Float32
    @test floats[1] == Float32(1.5)
    @test floats[2] == Float32(1.5)
end

true
