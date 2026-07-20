using Random

# Issue #8998: MersenneTwister(seed) random stream diverges from upstream.
#
# sjulia's MersenneTwister is backed by MT19937-64 (vm/rng.rs); upstream Julia
# uses dSFMT (SIMD-oriented Fast MT, julia/stdlib/Random/src/RNGs.jl).
# The same seed therefore produces different bit-level streams.
#
# This fixture PINS the current sjulia MT19937-64 stream values so that
# accidental changes to the RNG implementation are detected. These values
# intentionally DO NOT match upstream Julia.
#
# Upstream Julia output for seed=42: 0.7108238673434464, 0.0644852510983267
# sjulia output (MT19937-64) for seed=42:
const EXPECTED_SEED42_1 = 0.755155532954539
const EXPECTED_SEED42_2 = 0.6390313938546974

# Upstream Julia output for seed=1234: 0.5383210129299967, 0.9973545274591418, 0.027541876868637738
# sjulia output (MT19937-64) for seed=1234:
const EXPECTED_SEED1234_1 = 0.9472316166078043
const EXPECTED_SEED1234_2 = 0.052223374792334964
const EXPECTED_SEED1234_3 = 0.9743182754802404

function mt_stream_seed42_8998()
    rng = MersenneTwister(42)
    v1 = rand(rng)
    v2 = rand(rng)
    return v1 == EXPECTED_SEED42_1 && v2 == EXPECTED_SEED42_2
end

function mt_stream_seed1234_8998()
    rng = MersenneTwister(1234)
    v1 = rand(rng)
    v2 = rand(rng)
    v3 = rand(rng)
    return v1 == EXPECTED_SEED1234_1 && v2 == EXPECTED_SEED1234_2 && v3 == EXPECTED_SEED1234_3
end

# Sanity: values must be in [0, 1) regardless of MT backend.
function mt_stream_in_range_8998()
    rng = MersenneTwister(42)
    all(0.0 <= rand(rng) < 1.0 for _ in 1:10)
end

println(mt_stream_seed42_8998())
println(mt_stream_seed1234_8998())
println(mt_stream_in_range_8998())
mt_stream_seed42_8998() && mt_stream_seed1234_8998() && mt_stream_in_range_8998()
