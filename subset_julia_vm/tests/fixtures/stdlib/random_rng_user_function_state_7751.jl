using Random
using Test

# Issue #7751: RNGs are mutable Julia objects. Passing an RNG through a user
# function must advance the caller-visible state, not a detached copy.

draw_uniform_7751(rng) = rand(rng)
draw_normal_7751(rng::AbstractRNG) = randn(rng)

function user_function_advances_untyped_rng_7751()
    rng = Xoshiro(7)
    a = draw_uniform_7751(rng)
    b = draw_uniform_7751(rng)
    fresh = Xoshiro(7)
    c = rand(fresh)
    d = rand(fresh)
    return a == c && b == d && a != b
end

function user_function_advances_abstractrng_7751()
    rng = Xoshiro(11)
    a = draw_normal_7751(rng)
    b = draw_normal_7751(rng)
    fresh = Xoshiro(11)
    c = randn(fresh)
    d = randn(fresh)
    return a == c && b == d && a != b
end

@testset "RNG user-function state sharing (Issue #7751)" begin
    @test user_function_advances_untyped_rng_7751()
    @test user_function_advances_abstractrng_7751()
end

true
