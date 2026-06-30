using Random
using Test

function rand_rng_scalar_7227()
    rng = Xoshiro(123)
    x = rand(rng)
    return x isa Float64 && 0.0 <= x < 1.0
end

function rand_rng_vector_7227()
    rng = Xoshiro(123)
    x = rand(rng, 3)
    return length(x) == 3 && all(v -> 0.0 <= v < 1.0, x)
end

function rand_rng_matrix_7227()
    rng = Xoshiro(123)
    x = rand(rng, 2, 3)
    return size(x) == (2, 3)
end

function rand_rng_int_vector_7227()
    rng = Xoshiro(123)
    x = rand(rng, Int, 4)
    return length(x) == 4
end

function randn_rng_vector_7227()
    rng = Xoshiro(123)
    x = randn(rng, 5)
    return length(x) == 5 && all(v -> v isa Float64, x)
end

function randn_rng_matrix_7227()
    rng = Xoshiro(123)
    x = randn(rng, 2, 2)
    return size(x) == (2, 2)
end

function rand_rng_advances_stream_7227()
    rng = Xoshiro(123)
    a = rand(rng)
    b = rand(rng)
    return a != b
end

@testset "explicit-RNG array sampling (Issue #7227)" begin
    @test rand_rng_scalar_7227()
    @test rand_rng_vector_7227()
    @test rand_rng_matrix_7227()
    @test rand_rng_int_vector_7227()
    @test randn_rng_vector_7227()
    @test randn_rng_matrix_7227()
    @test rand_rng_advances_stream_7227()
end

true
