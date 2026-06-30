using Test

@testset "typed tuple-dims allocation follows Base array dispatch (Issue #4018)" begin
    zi = zeros(Int64, (2, 3))
    @test typeof(zi) === Matrix{Int64}
    @test size(zi) == (2, 3)
    @test zi[1, 1] == 0
    @test zi[2, 3] == 0

    oi = ones(Int64, (2, 3))
    @test typeof(oi) === Matrix{Int64}
    @test size(oi) == (2, 3)
    @test oi[1, 1] == 1
    @test oi[2, 3] == 1

    zc = zeros(Complex{Float64}, (2, 2))
    @test typeof(zc) === Matrix{Complex{Float64}}
    @test size(zc) == (2, 2)
    @test zc[1, 1] == 0.0 + 0.0im

    oc = ones(Complex{Float64}, (2, 2))
    @test typeof(oc) === Matrix{Complex{Float64}}
    @test size(oc) == (2, 2)
    @test oc[2, 2] == 1.0 + 0.0im

    filled = fill(7, (2, 3))
    @test typeof(filled) === Matrix{Int64}
    @test size(filled) == (2, 3)
    @test filled[1, 2] == 7
    @test filled[2, 3] == 7

    cube = fill(3, (2, 2, 2))
    @test typeof(cube) === Array{Int64, 3}
    @test size(cube) == (2, 2, 2)
    @test cube[2, 2, 2] == 3

    hyper_fill = fill(1, (1, 1, 1, 1))
    @test typeof(hyper_fill) === Array{Int64, 4}
    @test size(hyper_fill) == (1, 1, 1, 1)
    @test hyper_fill[1, 1, 1, 1] == 1

    hyper_zero = zeros(Int64, (1, 1, 1, 1))
    @test typeof(hyper_zero) === Array{Int64, 4}
    @test size(hyper_zero) == (1, 1, 1, 1)
    @test hyper_zero[1, 1, 1, 1] == 0

    real_similar = similar(Array{Real}, (2, 2))
    @test typeof(real_similar) === Matrix{Real}
    @test eltype(real_similar) === Real
    @test size(real_similar) == (2, 2)

    undef_matrix = Array{Int64}(undef, 2, 3)
    @test typeof(undef_matrix) === Matrix{Int64}
    @test eltype(undef_matrix) === Int64
    @test size(undef_matrix) == (2, 3)
    @test length(undef_matrix) == 6

    undef_vector = Array{Float64}(undef, (2,))
    @test typeof(undef_vector) === Vector{Float64}
    @test eltype(undef_vector) === Float64
    @test size(undef_vector) == (2,)
    @test length(undef_vector) == 2

    function generic_array_undef(T)
        result = Array{T}(undef, (2,))
        return (eltype(result), length(result), size(result))
    end

    generic_undef = generic_array_undef(Float64)
    @test generic_undef[1] === Float64
    @test generic_undef[2] == 2
    @test generic_undef[3] == (2,)

    function generic_similar_tuple_dims_4569(::Type{T}, dims::Tuple, expected_type, expected_eltype, expected_len) where T
        result = similar(Array{T}, dims)
        runtime_rank_type = Array{T,length(dims)}
        return typeof(result) === expected_type &&
               typeof(result) === runtime_rank_type &&
               eltype(result) === expected_eltype &&
               size(result) == dims &&
               length(result) == expected_len
    end

    @test generic_similar_tuple_dims_4569(Int64, (2,), Vector{Int64}, Int64, 2)
    @test generic_similar_tuple_dims_4569(Float64, (2, 3), Matrix{Float64}, Float64, 6)
    @test generic_similar_tuple_dims_4569(Bool, (1, 2), Matrix{Bool}, Bool, 2)
    @test generic_similar_tuple_dims_4569(Complex{Float64}, (2, 2), Matrix{Complex{Float64}}, Complex{Float64}, 4)

    function generic_similar_untyped_dims_4643(::Type{T}, dims, expected_type, expected_eltype, expected_len) where T
        result = similar(Array{T}, dims)
        runtime_rank_type = Array{T,length(dims)}
        return typeof(result) === expected_type &&
               typeof(result) === runtime_rank_type &&
               eltype(result) === expected_eltype &&
               size(result) == dims &&
               length(result) == expected_len
    end

    @test generic_similar_untyped_dims_4643(Int64, (2,), Vector{Int64}, Int64, 2)
    @test generic_similar_untyped_dims_4643(Float64, (2, 3), Matrix{Float64}, Float64, 6)
    @test generic_similar_untyped_dims_4643(Bool, (1, 2), Matrix{Bool}, Bool, 2)
    @test generic_similar_untyped_dims_4643(Complex{Float64}, (2, 2), Matrix{Complex{Float64}}, Complex{Float64}, 4)

    symbol_fill = fill(:ok, (1, 2))
    @test typeof(symbol_fill) === Matrix{Symbol}
    @test size(symbol_fill) == (1, 2)
    @test symbol_fill[1, 2] === :ok

    generic_symbols = fill(:ok, (1, 1, 1, 1))
    @test typeof(generic_symbols) === Array{Symbol, 4}
    @test size(generic_symbols) == (1, 1, 1, 1)
    @test generic_symbols[1, 1, 1, 1] === :ok
end

true
