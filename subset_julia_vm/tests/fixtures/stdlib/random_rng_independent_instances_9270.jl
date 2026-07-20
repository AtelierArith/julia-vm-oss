using Random
using Test

# Issue #9270: two same-seed RNG values bound to separate variables must be
# INDEPENDENT — drawing from one must not advance the other. The construction
# path had a CSE bug: `m2 = MersenneTwister(123)` reused m1's freshly-built
# engine (the RNG constructor was mis-classified as a pure/consistent expression
# and value-numbered away), so the four interleaved draws formed one shared
# stream instead of two identical independent streams.
#
# The stream VALUES are NOT upstream-parity for MersenneTwister (dSFMT is
# deferred, #8998); these tests assert the backend-independent contract:
# same-seed cross-instance reproducibility, and that assignment aliases (Julia
# RNGs are mutable heap objects) while two constructor calls do not. Covered
# here for the two stdlib-`Random` RNGs (MersenneTwister, Xoshiro); the same
# fix also covers StableRNG (an external package upstream), locked by the Rust
# unit test `rng_constructors_are_not_pure_issue_9270` in compile::ir_opt.

# Two same-seed MersenneTwister, draws interleaved, must repeat pairwise.
function mt_two_instances_interleaved_9270()
    m1 = MersenneTwister(123)
    m2 = MersenneTwister(123)
    ok = true
    for _ in 1:20
        if rand(m1) != rand(m2)
            ok = false
        end
    end
    return ok
end

# Same for Xoshiro (same latent aliasing bug, same fix).
function xoshiro_two_instances_interleaved_9270()
    x1 = Xoshiro(123)
    x2 = Xoshiro(123)
    ok = true
    for _ in 1:20
        if rand(x1) != rand(x2)
            ok = false
        end
    end
    return ok
end

# randn draws are likewise independent across two same-seed instances.
function mt_two_instances_randn_9270()
    m1 = MersenneTwister(99)
    m2 = MersenneTwister(99)
    ok = true
    for _ in 1:10
        if randn(m1) != randn(m2)
            ok = false
        end
    end
    return ok
end

# Assignment aliases: `m2 = m1` shares one engine, so interleaved draws diverge
# (upstream RNGs are mutable objects — assignment binds the same object).
function mt_assignment_aliases_9270()
    m1 = MersenneTwister(123)
    m2 = m1
    a = rand(m1)
    b = rand(m2)  # advances the SAME engine
    return a != b
end

# Two independently-constructed same-seed instances stay in lock-step even when
# one is advanced first: reseeding-free, construction is a true fresh engine.
function mt_first_draws_match_9270()
    m1 = MersenneTwister(2024)
    m2 = MersenneTwister(2024)
    return rand(m1) == rand(m2)
end

@testset "RNG independent instances (Issue #9270)" begin
    @test mt_two_instances_interleaved_9270()
    @test xoshiro_two_instances_interleaved_9270()
    @test mt_two_instances_randn_9270()
    @test mt_assignment_aliases_9270()
    @test mt_first_draws_match_9270()
end

true
