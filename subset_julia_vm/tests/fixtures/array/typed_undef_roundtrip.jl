# Write-read round-trip tests for all typed undef array constructors (Issue #1804)
# Pattern: construct -> write -> read -> verify for each element type

using Test

@testset "Float64 undef array round-trip" begin
    v = Vector{Float64}(undef, 3)
    @test length(v) == 3

    v[1] = 1.5
    v[2] = 2.5
    v[3] = 3.5

    @test v[1] == 1.5
    @test v[2] == 2.5
    @test v[3] == 3.5

    # Overwrite and verify
    v[2] = 99.0
    @test v[2] == 99.0
    @test v[1] == 1.5  # unchanged
end

@testset "Int64 undef array round-trip" begin
    v = Vector{Int64}(undef, 3)
    @test length(v) == 3

    v[1] = 10
    v[2] = 20
    v[3] = 30

    @test v[1] == 10
    @test v[2] == 20
    @test v[3] == 30

    # Overwrite and verify
    v[1] = 999
    @test v[1] == 999
    @test v[3] == 30  # unchanged
end

@testset "Bool undef array allocation" begin
    v = Vector{Bool}(undef, 4)
    @test length(v) == 4
    # Bool undef arrays are initialized to false
    @test v[1] == false
    @test v[4] == false
end

@testset "Complex{Float64} undef array round-trip" begin
    v = Vector{Complex{Float64}}(undef, 3)
    @test length(v) == 3

    v[1] = Complex(1.0, 2.0)
    v[2] = Complex(3.0, 4.0)
    v[3] = Complex(5.0, 6.0)

    @test real(v[1]) == 1.0
    @test imag(v[1]) == 2.0
    @test real(v[2]) == 3.0
    @test imag(v[2]) == 4.0
    @test real(v[3]) == 5.0
    @test imag(v[3]) == 6.0

    # Overwrite and verify
    v[2] = Complex(30.0, 40.0)
    @test real(v[2]) == 30.0
    @test imag(v[2]) == 40.0
    @test real(v[1]) == 1.0  # unchanged
    @test imag(v[3]) == 6.0  # unchanged
end

@testset "2D Float64 undef array round-trip" begin
    arr = Array{Float64}(undef, 2, 3)
    @test size(arr) == (2, 3)

    arr[1, 1] = 1.0
    arr[2, 1] = 2.0
    arr[1, 2] = 3.0
    arr[2, 2] = 4.0
    arr[1, 3] = 5.0
    arr[2, 3] = 6.0

    @test arr[1, 1] == 1.0
    @test arr[2, 1] == 2.0
    @test arr[1, 2] == 3.0
    @test arr[2, 2] == 4.0
    @test arr[1, 3] == 5.0
    @test arr[2, 3] == 6.0
end

true
