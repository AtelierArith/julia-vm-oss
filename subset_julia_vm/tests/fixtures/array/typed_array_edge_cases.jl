# Typed array constructor edge cases
# Tests zero-length, single-element, and high-dimensional arrays.
# Related: Issue #1607

using Test

@testset "Zero-length arrays" begin
    v_f64 = Vector{Float64}(undef, 0)
    @test length(v_f64) == 0

    v_i64 = Vector{Int64}(undef, 0)
    @test length(v_i64) == 0

    v_bool = Vector{Bool}(undef, 0)
    @test length(v_bool) == 0
end

@testset "Single-element arrays" begin
    v_f64 = Vector{Float64}(undef, 1)
    @test length(v_f64) == 1
    v_f64[1] = 42.0
    @test v_f64[1] == 42.0

    v_i64 = Vector{Int64}(undef, 1)
    @test length(v_i64) == 1
    v_i64[1] = 99
    @test v_i64[1] == 99
end

@testset "2D array read-write" begin
    arr = Array{Float64}(undef, 3, 3)
    @test size(arr) == (3, 3)
    @test length(arr) == 9

    # Write to all elements
    for i in 1:3
        for j in 1:3
            arr[i, j] = Float64(i * 10 + j)
        end
    end

    # Read back
    @test arr[1, 1] == 11.0
    @test arr[2, 3] == 23.0
    @test arr[3, 3] == 33.0
end

@testset "3D array dimensions" begin
    arr = Array{Int64}(undef, 2, 3, 4)
    @test size(arr) == (2, 3, 4)
    @test length(arr) == 24
end

@testset "zeros and ones typed" begin
    # zeros with type
    z_f64 = zeros(Float64, 3)
    @test length(z_f64) == 3
    @test z_f64[1] == 0.0
    @test z_f64[2] == 0.0
    @test z_f64[3] == 0.0

    z_i64 = zeros(Int64, 4)
    @test length(z_i64) == 4
    @test z_i64[1] == 0

    # ones with type
    o_f64 = ones(Float64, 3)
    @test length(o_f64) == 3
    @test o_f64[1] == 1.0
    @test o_f64[2] == 1.0

    o_i64 = ones(Int64, 2)
    @test length(o_i64) == 2
    @test o_i64[1] == 1
end

@testset "Complex array undef length" begin
    v = Vector{Complex{Float64}}(undef, 3)
    @test length(v) == 3

    v_zero = Vector{Complex{Float64}}(undef, 0)
    @test length(v_zero) == 0
end

true
