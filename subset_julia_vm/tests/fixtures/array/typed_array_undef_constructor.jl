# Test Vector{T}(undef, n) and Array{T}(undef, dims...) constructors
# (Issue #1586, Issue #4047)

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

    # Array{T}(undef, dims::Tuple) mirrors Julia's boot.jl tuple constructor.
    arr_tuple = Array{Float64}(undef, (2, 3))
    @test typeof(arr_tuple) == Matrix{Float64}
    @test eltype(arr_tuple) == Float64
    @test size(arr_tuple) == (2, 3)
    @test length(arr_tuple) == 6

    # Explicit-rank Array{T,N}(undef, dims::Tuple) unpacks dims as d...
    arr_rank_tuple = Array{Float32,2}(undef, (2, 2))
    @test typeof(arr_rank_tuple) == Matrix{Float32}
    @test eltype(arr_rank_tuple) == Float32
    @test ndims(arr_rank_tuple) == 2
    @test size(arr_rank_tuple) == (2, 2)

    function make_rank_tuple(T, N)
        Array{T,N}(undef, (2, 2))
    end

    arr_runtime_rank = make_rank_tuple(Float32, 2)
    @test typeof(arr_runtime_rank) == Matrix{Float32}
    @test eltype(arr_runtime_rank) == Float32
    @test ndims(arr_runtime_rank) == 2
    @test size(arr_runtime_rank) == (2, 2)

    function make_vec_tuple(T)
        Array{T,1}(undef, (2,))
    end

    arr_runtime_vec = make_vec_tuple(Complex{Float64})
    @test typeof(arr_runtime_vec) == Vector{Complex{Float64}}
    @test eltype(arr_runtime_vec) == Complex{Float64}
    @test ndims(arr_runtime_vec) == 1
    @test length(arr_runtime_vec) == 2

    dims_from_var = (2, 2)
    arr_dims_var = Array{Int64,2}(undef, dims_from_var)
    @test typeof(arr_dims_var) == Matrix{Int64}
    @test size(arr_dims_var) == (2, 2)

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
