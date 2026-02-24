# Test map for scalar Numbers (Issue #1646)
# Based on Julia's base/number.jl:328
# map(f, x::Number, ys::Number...) = f(x, ys...)
#
# Note: Using concrete types (Int64, Float64) to avoid dispatch conflicts with map(f, Array)

using Test

# Define helper functions outside @testset
add2(a, b) = a + b
add3(a, b, c) = a + b + c
add4(a, b, c, d) = a + b + c + d
mul2(a, b) = a * b

@testset "map for scalar Numbers" begin
    @testset "Single argument - Int64" begin
        # map(f, x::Int64) = f(x)
        @test map(abs, -5) == 5.0
        @test map(x -> x * 2, 3) == 6.0
        @test map(x -> x + 1, 10) == 11.0
    end

    @testset "Single argument - Float64" begin
        # map(f, x::Float64) = f(x)
        @test map(abs, -3.5) == 3.5
        @test map(x -> x * 2.0, 3.0) == 6.0
    end

    @testset "Two arguments - Int64" begin
        # map(f, x::Int64, y::Int64) = f(x, y)
        @test map(add2, 1, 2) == 3.0
        @test map(mul2, 3, 4) == 12.0
        @test map((a, b) -> a - b, 10, 3) == 7.0
    end

    @testset "Two arguments - Float64" begin
        # map(f, x::Float64, y::Float64) = f(x, y)
        @test map(add2, 1.5, 2.5) == 4.0
        @test map((a, b) -> a * b, 2.0, 3.0) == 6.0
    end

    @testset "Two arguments - mixed Int64/Float64" begin
        # map(f, x::Int64, y::Float64) and map(f, x::Float64, y::Int64)
        @test map(add2, 1, 2.5) == 3.5
        @test map(add2, 1.5, 2) == 3.5
    end

    @testset "Three arguments - Int64" begin
        # map(f, x::Int64, y::Int64, z::Int64) = f(x, y, z)
        @test map(add3, 1, 2, 3) == 6.0
        @test map((a, b, c) -> a * b + c, 2, 3, 4) == 10.0
    end

    @testset "Three arguments - Float64" begin
        # map(f, x::Float64, y::Float64, z::Float64) = f(x, y, z)
        @test map(add3, 1.0, 2.0, 3.0) == 6.0
    end

    @testset "Four arguments - Int64" begin
        # map(f, x::Int64, y::Int64, z::Int64, w::Int64) = f(x, y, z, w)
        @test map(add4, 1, 2, 3, 4) == 10.0
        @test map((a, b, c, d) -> a + b + c + d, 10, 20, 30, 40) == 100.0
    end

    @testset "Four arguments - Float64" begin
        # map(f, x::Float64, y::Float64, z::Float64, w::Float64) = f(x, y, z, w)
        @test map(add4, 1.0, 2.0, 3.0, 4.0) == 10.0
    end
end

true
