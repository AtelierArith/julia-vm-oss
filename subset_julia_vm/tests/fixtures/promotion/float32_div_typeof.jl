# Test Float32 div() type preservation (Issue #1970)

using Test

@testset "div(Float32, Float32) type preservation" begin
    x = Float32(5.0)
    y = Float32(2.0)
    result = div(x, y)
    @test result == Float32(2.0)
    @test typeof(result) == Float32
end

@testset "div(Float32, Int64) returns Float32" begin
    x = Float32(7.0)
    y = 2
    result = div(x, y)
    @test result == Float32(3.0)
    @test typeof(result) == Float32
end

@testset "div(Int64, Float32) returns Float32" begin
    x = 7
    y = Float32(2.0)
    result = div(x, y)
    @test result == Float32(3.0)
    @test typeof(result) == Float32
end

@testset "div(Float64, Float64) returns Float64" begin
    x = 7.0
    y = 2.0
    result = div(x, y)
    @test result == 3.0
    @test typeof(result) == Float64
end

@testset "div(Float32, Float64) promotes to Float64" begin
    x = Float32(7.0)
    y = 2.0
    result = div(x, y)
    @test result == 3.0
    @test typeof(result) == Float64
end

true
