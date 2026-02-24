# Test Vector{T}(undef, n) and Array{T}(undef, dims...) constructors (Issue #1586)

using Test

@testset "Typed array undef constructor" begin
    # Vector{Float64}(undef, n)
    v_f64 = Vector{Float64}(undef, 5)
    @test length(v_f64) == 5

    # Vector{Int64}(undef, n)
    v_i64 = Vector{Int64}(undef, 3)
    @test length(v_i64) == 3

    # Vector{Bool}(undef, n)
    v_bool = Vector{Bool}(undef, 4)
    @test length(v_bool) == 4

    # Vector{Complex{Float64}}(undef, n)
    v_complex = Vector{Complex{Float64}}(undef, 2)
    @test length(v_complex) == 2

    # Array{Float64}(undef, m, n) - 2D array
    arr_2d = Array{Float64}(undef, 3, 4)
    @test size(arr_2d) == (3, 4)
    @test length(arr_2d) == 12

    # Array{Int64}(undef, m, n, k) - 3D array
    arr_3d = Array{Int64}(undef, 2, 3, 4)
    @test size(arr_3d) == (2, 3, 4)
    @test length(arr_3d) == 24

    # Can write to undef arrays
    v_f64[1] = 1.5
    v_f64[2] = 2.5
    @test v_f64[1] == 1.5
    @test v_f64[2] == 2.5

    v_i64[1] = 10
    v_i64[2] = 20
    @test v_i64[1] == 10
    @test v_i64[2] == 20
end

true
