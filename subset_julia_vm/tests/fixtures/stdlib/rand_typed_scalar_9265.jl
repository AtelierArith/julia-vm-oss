using Random

# Issue #9265: scalar typed `rand([rng], ::Type{T})` for a concrete bits-numeric
# / Bool type T must return one scalar of type T, sampling integers over their
# full range and floats in [0, 1). Before the fix, `rand(rng, UInt32)` /
# `rand(UInt32)` / `rand(rng, Bool)` errored, and `rand(rng, Int)` / `rand(Int)`
# silently returned a Float64 / a 0-dimensional Array{Float64}.
#
# The exact bit stream is NOT upstream-parity for MersenneTwister (the dSFMT port
# stays deferred, Issue #8998), so this fixture asserts the *type* and behavioral
# contract (correct type, determinism for a fixed seed, float range) rather than
# exact values. Every assertion below is also `true` under upstream julia 1.12.

const INT_TYPES = (Int8, Int16, Int32, Int64, Int128,
                   UInt8, UInt16, UInt32, UInt64, UInt128)

# rand(rng, T) returns a value of type T for every bits-integer type.
function typed_rand_rng_int_types_9265()
    ok = true
    for T in INT_TYPES
        rng = MersenneTwister(42)
        x = rand(rng, T)
        ok = ok && (x isa T)
    end
    return ok
end

# rand(rng, Bool) and rand(rng, Float{16,32,64}) return the right type.
function typed_rand_rng_bool_float_9265()
    rng = MersenneTwister(123)
    b = rand(rng, Bool)
    f16 = rand(rng, Float16)
    f32 = rand(rng, Float32)
    f64 = rand(rng, Float64)
    return (b isa Bool) && (f16 isa Float16) && (f32 isa Float32) && (f64 isa Float64)
end

# rand(Int) / rand(UInt32) / rand(Bool) with no explicit RNG use the global RNG
# and still return a scalar of the requested type (previously a 0-d array/error).
function typed_rand_global_9265()
    return (rand(Int) isa Int) && (rand(UInt32) isa UInt32) &&
           (rand(Bool) isa Bool) && (rand(Float64) isa Float64) &&
           (rand(Int8) isa Int8) && (rand(UInt) isa UInt)
end

# A fixed seed produces the same scalar draw across two constructions.
function typed_rand_deterministic_9265()
    a = rand(MersenneTwister(7), UInt32)
    b = rand(MersenneTwister(7), UInt32)
    c = rand(MersenneTwister(7), Int64)
    d = rand(MersenneTwister(7), Int64)
    return (a == b) && (c == d)
end

# Float typed draws land in [0, 1) at full granularity. Float16 (11-bit
# significand) and Float32 (24-bit) are the sharp cases: a naive
# `f16::from_f64(rand())` / `rand() as f32` rounds f64 values just below 1.0 up
# to exactly 1.0 at a ~2^-12 / ~2^-25 rate, breaking the upper bound
# (Issue #9275). Draw many samples per type and assert the observed maximum stays
# strictly below 1.0 (and reaches near-1.0, so granularity is not lost) and the
# minimum stays >= 0. 50k F16 draws would surface ~12 out-of-range values if the
# rounding bug were present.
function typed_rand_float_range_9265()
    rng = MersenneTwister(99)
    n = 50000
    ok = true
    max16 = zero(Float16); min16 = one(Float16)
    max32 = 0.0f0; min32 = 1.0f0
    max64 = 0.0; min64 = 1.0
    for _ in 1:n
        x16 = rand(rng, Float16)
        ok = ok && (0.0f0 <= Float32(x16) < 1.0f0)
        max16 = max(max16, x16); min16 = min(min16, x16)
        x32 = rand(rng, Float32)
        ok = ok && (0.0f0 <= x32 < 1.0f0)
        max32 = max(max32, x32); min32 = min(min32, x32)
        x64 = rand(rng, Float64)
        ok = ok && (0.0 <= x64 < 1.0)
        max64 = max(max64, x64); min64 = min(min64, x64)
    end
    return ok &&
           (max16 < 1) && (min16 >= 0) && (max16 > 0.9) &&
           (max32 < 1.0f0) && (min32 >= 0.0f0) && (max32 > 0.99f0) &&
           (max64 < 1.0) && (min64 >= 0.0) && (max64 > 0.99)
end

println(typed_rand_rng_int_types_9265())
println(typed_rand_rng_bool_float_9265())
println(typed_rand_global_9265())
println(typed_rand_deterministic_9265())
println(typed_rand_float_range_9265())

typed_rand_rng_int_types_9265() && typed_rand_rng_bool_float_9265() &&
    typed_rand_global_9265() && typed_rand_deterministic_9265() &&
    typed_rand_float_range_9265()
