using Random
using Test

# Issue #7306: MersenneTwister(seed) construction.
# Before #7306 only Xoshiro/StableRNG were constructible; the MersenneTwister
# type ANNOTATION/isa/dispatch were already mapped (#7231) but construction
# errored with "Unknown function: MersenneTwister".
#
# NOTE: upstream Julia's MersenneTwister is backed by dSFMT. The sjulia VM
# backs it with a deterministic MT19937-64 engine, so the generated stream is
# NOT bit-identical to upstream julia. These tests assert structural and
# seed-reproducibility properties (constructible, finite, reproducible across
# two constructions, distinct seeds differ, isa AbstractRNG, threads through a
# user function), NOT exact upstream values.

# Construction and scalar randn.
function mt_construct_and_randn_7306()
    m = MersenneTwister(7)
    x = randn(m)
    return x isa Float64 && isfinite(x)
end

# Qualified construction (Random.MersenneTwister) works too.
function mt_qualified_construct_7306()
    m = Random.MersenneTwister(7)
    x = randn(m)
    return x isa Float64 && isfinite(x)
end

# rand(m) is a Float64 in [0, 1).
function mt_scalar_rand_in_range_7306()
    m = MersenneTwister(7)
    x = rand(m)
    return x isa Float64 && 0.0 <= x < 1.0
end

# Same seed -> same first draw (reproducible across two constructions).
function mt_same_seed_reproducible_7306()
    a = randn(MersenneTwister(7))
    b = randn(MersenneTwister(7))
    return a == b
end

# A whole sequence is reproducible for the same seed.
function mt_sequence_reproducible_7306()
    m1 = MersenneTwister(123)
    m2 = MersenneTwister(123)
    same = true
    for _ in 1:10
        if rand(m1) != rand(m2)
            same = false
        end
    end
    return same
end

# Distinct seeds produce different streams (first draw differs).
function mt_distinct_seeds_differ_7306()
    a = rand(MersenneTwister(7))
    b = rand(MersenneTwister(8))
    return a != b
end

# rand(m, n) returns a vector of the right length with values in [0, 1).
function mt_rand_vector_7306()
    m = MersenneTwister(7)
    x = rand(m, 3)
    return length(x) == 3 && all(v -> 0.0 <= v < 1.0, x)
end

# randn(m, n) returns a vector of finite Float64 of the right length.
function mt_randn_vector_7306()
    m = MersenneTwister(7)
    x = randn(m, 3)
    return length(x) == 3 && all(v -> v isa Float64 && isfinite(v), x)
end

# A MersenneTwister value satisfies isa AbstractRNG, and typeof reports it.
function mt_isa_abstractrng_7306()
    return MersenneTwister(7) isa AbstractRNG
end

function mt_typeof_name_7306()
    return string(typeof(MersenneTwister(7))) == "MersenneTwister"
end

# Threads through an untyped user function.
draw_mt_untyped(rng) = rand(rng)
function mt_thread_untyped_param_7306()
    a = draw_mt_untyped(MersenneTwister(7))
    return a isa Float64 && 0.0 <= a < 1.0
end

# Threads through a ::MersenneTwister-typed user function.
draw_mt_typed(rng::MersenneTwister) = randn(rng)
function mt_thread_typed_param_7306()
    a = draw_mt_typed(MersenneTwister(7))
    return a isa Float64 && isfinite(a)
end

# Threads through an ::AbstractRNG-typed user function.
draw_mt_abstract(rng::AbstractRNG) = randn(rng)
function mt_thread_abstract_param_7306()
    a = draw_mt_abstract(MersenneTwister(7))
    return a isa Float64 && isfinite(a)
end

@testset "MersenneTwister(seed) construction (Issue #7306)" begin
    @test mt_construct_and_randn_7306()
    @test mt_qualified_construct_7306()
    @test mt_scalar_rand_in_range_7306()
    @test mt_same_seed_reproducible_7306()
    @test mt_sequence_reproducible_7306()
    @test mt_distinct_seeds_differ_7306()
    @test mt_rand_vector_7306()
    @test mt_randn_vector_7306()
    @test mt_isa_abstractrng_7306()
    @test mt_typeof_name_7306()
    @test mt_thread_untyped_param_7306()
    @test mt_thread_typed_param_7306()
    @test mt_thread_abstract_param_7306()
end

true
