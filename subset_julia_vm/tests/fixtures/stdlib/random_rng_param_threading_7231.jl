using Random
using Test

# Issue #7231: an explicit RNG argument threaded through a user function.
# A param that is untyped, ::Xoshiro, or ::AbstractRNG must let randn(rng) /
# rand(rng) resolve to the scalar-from-rng form (not randn(dims...)).
# The VM RNG does not bit-match upstream julia, so we assert structural and
# seed-reproducibility properties rather than exact values.

# Untyped param carrying a Value::Rng.
f_untyped(rng) = randn(rng)

# ::Xoshiro typed param.
g_xoshiro(rng::Xoshiro) = randn(rng)

# ::AbstractRNG typed param (abstract supertype must accept a concrete Rng).
h_abstract(rng::AbstractRNG) = randn(rng)

function untyped_param_scalar_randn_7231()
    x = f_untyped(Xoshiro(7))
    return x isa Float64 && isfinite(x)
end

function xoshiro_param_scalar_randn_7231()
    x = g_xoshiro(Xoshiro(7))
    return x isa Float64 && isfinite(x)
end

function abstractrng_param_scalar_randn_7231()
    x = h_abstract(Xoshiro(7))
    return x isa Float64 && isfinite(x)
end

# All three forms see the same RNG state -> same first draw for the same seed.
function rng_param_forms_agree_7231()
    a = f_untyped(Xoshiro(7))
    b = g_xoshiro(Xoshiro(7))
    c = h_abstract(Xoshiro(7))
    return a == b && b == c
end

# rand through an explicit-RNG param.
rand_untyped(rng) = rand(rng)
function untyped_param_scalar_rand_7231()
    x = rand_untyped(Xoshiro(7))
    return x isa Float64 && 0.0 <= x < 1.0
end

# rand(rng, dims...) reachable through a typed param.
randvec(rng::Xoshiro, n) = rand(rng, n)
function rng_param_rand_vector_7231()
    x = randvec(Xoshiro(7), 4)
    return length(x) == 4 && all(v -> 0.0 <= v < 1.0, x)
end

# randn(rng, dims...) reachable through an abstract param.
randnvec(rng::AbstractRNG, n) = randn(rng, n)
function rng_param_randn_vector_7231()
    x = randnvec(Xoshiro(7), 3)
    return length(x) == 3 && all(v -> v isa Float64, x)
end

# Threading default_rng() through a user function shares the global stream.
draw(rng) = rand(rng)
function thread_default_rng_7231()
    Random.seed!(2024)
    a = rand()
    Random.seed!(2024)
    c = draw(Random.default_rng())
    return a == c
end

# An AbstractRNG-typed param accepts the global default_rng() handle and shares
# the global stream.
draw_tl(rng::AbstractRNG) = rand(rng)
function abstractrng_param_takes_default_rng_7231()
    Random.seed!(55)
    a = rand()
    Random.seed!(55)
    c = draw_tl(Random.default_rng())
    return a == c
end

# A Value::Rng argument satisfies an ::AbstractRNG param (isa relation).
function rng_arg_isa_abstractrng_7231()
    return Xoshiro(7) isa AbstractRNG
end

@testset "explicit RNG param threading (Issue #7231)" begin
    @test untyped_param_scalar_randn_7231()
    @test xoshiro_param_scalar_randn_7231()
    @test abstractrng_param_scalar_randn_7231()
    @test rng_param_forms_agree_7231()
    @test untyped_param_scalar_rand_7231()
    @test rng_param_rand_vector_7231()
    @test rng_param_randn_vector_7231()
    @test thread_default_rng_7231()
    @test abstractrng_param_takes_default_rng_7231()
    @test rng_arg_isa_abstractrng_7231()
end

true
