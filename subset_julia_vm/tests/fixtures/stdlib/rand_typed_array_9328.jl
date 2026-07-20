using Random

# Issue #9328: `rand([rng], ::Type{T}, dims...)` for a concrete scalar type T
# must return an array whose element type is T — `rand(rng, Int, 2, 2)` is a
# `Matrix{Int64}`, `rand(rng, Float32, 3)` a `Vector{Float32}`, `rand(Int, 2)` a
# `Vector{Int64}`, etc. Before the fix, the typed-array wiring dropped the
# element type: the leading-`Int` form produced a Float64-backed array and every
# other type (`Float32`, `Bool`, `Int32`, …) raised a runtime
# "DynamicToI64: expected numeric, got DataType" error on the static path.
#
# The exact bit stream is NOT upstream-parity for MersenneTwister (the dSFMT port
# stays deferred, Issue #8998), so this fixture asserts the element *type* and
# behavioral contract (correct typeof/eltype, integer-ness, float range,
# determinism per seed) rather than exact values. Every assertion below is also
# `true` under upstream julia 1.12.
#
# (sjulia additionally guarantees a k-element typed array consumes exactly the
# scalar `rand(rng, T)` stream — verified in the PR — but that intra-sjulia
# invariant is deliberately NOT asserted here because upstream julia's SIMD
# array-fill path draws differently from k scalar draws, so it is not
# parity-safe.)

# rand(rng, T, dims...) is faithfully typed for every type sjulia represents,
# with the requested rank (Vector for 1 dim, Matrix for 2).
function typed_rand_array_rng_types_9328()
    rng = MersenneTwister(42)
    return typeof(rand(rng, Int, 2, 2)) == Matrix{Int64} &&
           typeof(rand(rng, Int, 3)) == Vector{Int64} &&
           typeof(rand(rng, Int32, 3)) == Vector{Int32} &&
           typeof(rand(rng, Int8, 4)) == Vector{Int8} &&
           typeof(rand(rng, UInt8, 3)) == Vector{UInt8} &&
           typeof(rand(rng, UInt64, 2)) == Vector{UInt64} &&
           typeof(rand(rng, Int128, 2)) == Vector{Int128} &&
           typeof(rand(rng, UInt128, 2)) == Vector{UInt128} &&
           typeof(rand(rng, Bool, 3)) == Vector{Bool} &&
           typeof(rand(rng, Float32, 3)) == Vector{Float32} &&
           typeof(rand(rng, Float64, 3)) == Vector{Float64} &&
           typeof(rand(rng, Float32, 2, 2)) == Matrix{Float32}
end

# rand(T, dims...) with the global RNG (no explicit rng) is faithfully typed too.
function typed_rand_array_global_types_9328()
    return typeof(rand(Int, 2)) == Vector{Int64} &&
           typeof(rand(Int, 2, 2)) == Matrix{Int64} &&
           typeof(rand(Int32, 3)) == Vector{Int32} &&
           typeof(rand(UInt8, 3)) == Vector{UInt8} &&
           typeof(rand(Bool, 3)) == Vector{Bool} &&
           typeof(rand(Float32, 3)) == Vector{Float32} &&
           typeof(rand(Float64, 3)) == Vector{Float64}
end

# eltype and element-level type/range are correct.
function typed_rand_array_elements_9328()
    rng = MersenneTwister(1)
    ai = rand(rng, Int, 5)
    af = rand(rng, Float32, 5)
    ab = rand(rng, Bool, 5)
    ah = rand(rng, Float16, 5)  # sjulia has no native Float16 array storage, but
                                # the drawn values are still Float16 (parity-safe)
    return eltype(ai) == Int64 && all(x -> x isa Int64, ai) &&
           eltype(af) == Float32 && all(x -> (x isa Float32) && 0.0f0 <= x < 1.0f0, af) &&
           all(x -> x isa Bool, ab) &&
           all(x -> x isa Float16, ah)
end

# A fixed seed reproduces the same typed array across two constructions.
function typed_rand_array_deterministic_9328()
    a = rand(MersenneTwister(7), Int, 4)
    b = rand(MersenneTwister(7), Int, 4)
    c = rand(MersenneTwister(7), Float32, 3)
    d = rand(MersenneTwister(7), Float32, 3)
    return (a == b) && (c == d)
end

# A captured RNG inside a comprehension body reaches the runtime `RandMaybeRng`
# branch; its typed-array form must be faithful too (it mirrored the same
# dropped-eltype behavior before the fix).
function typed_rand_array_captured_rng_9328()
    rng = MersenneTwister(11)
    v = [rand(rng, Int, 2) for _ in 1:3]
    return all(a -> typeof(a) == Vector{Int64} && all(x -> x isa Int64, a), v)
end

println(typed_rand_array_rng_types_9328())
println(typed_rand_array_global_types_9328())
println(typed_rand_array_elements_9328())
println(typed_rand_array_deterministic_9328())
println(typed_rand_array_captured_rng_9328())

typed_rand_array_rng_types_9328() && typed_rand_array_global_types_9328() &&
    typed_rand_array_elements_9328() && typed_rand_array_deterministic_9328() &&
    typed_rand_array_captured_rng_9328()
