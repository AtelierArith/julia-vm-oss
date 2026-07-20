using Random

# Issue #9285: a scalar typed `rand(rng, T)` (and the dimension form
# `rand(rng, n)`) INSIDE a generator / comprehension body, where the RNG is a
# captured variable of static type `Any`, previously mis-lowered: the compiler
# read the type/dimension argument as an array dimension and emitted
# `RandArray(2)` with a `DynamicToI64` applied to the RNG value, so the call
# raised `Type error: DynamicToI64: expected numeric, got Rng(...)`.
#
# The fix routes `rand(a, dims...)` / `randn(a, dims...)` whose first argument is
# statically `Any` (any arity) through the runtime-branching `RandMaybeRng`
# instruction: `a` is decided at runtime to be an explicit RNG (scalar/vector/
# N-D draw) or a leading array dimension (N-D array), so the captured RNG is no
# longer coerced to a length. This covers the 3-arg matrix, the higher-rank, and
# the `randn` forms in addition to the original 2-arg case.
#
# Assertions are type / behavioral (parity-safe: each is ALSO `true` under
# upstream julia 1.12). The MersenneTwister bit stream is NOT upstream-parity
# (dSFMT port deferred, Issue #8998), so no exact random value is printed. Two
# unrelated pre-existing gaps are deliberately AVOIDED here rather than worked
# around: `collect` over a Float16 generator widening to `Vector{Any}`
# (Issue #9301) and chained broadcast comparison `0 .<= v .< 1` returning
# all-false (Issue #9300); this fixture uses per-element predicates instead.

# rand(rng, Float16) in a generator: every draw is a Float16 in [0, 1).
function gen_rand_rng_float16_9285()
    rng = MersenneTwister(7)
    xs = collect(rand(rng, Float16) for _ in 1:3)
    return length(xs) == 3 &&
           all(x -> x isa Float16, xs) &&
           all(x -> 0 <= x < 1, xs)
end

# Comprehension form of the same call.
function comp_rand_rng_float16_9285()
    rng = MersenneTwister(11)
    xs = [rand(rng, Float16) for _ in 1:4]
    return length(xs) == 4 &&
           all(x -> x isa Float16, xs) &&
           all(x -> 0 <= x < 1, xs)
end

# rand(rng, Bool) in a generator: every draw is a Bool.
function gen_rand_rng_bool_9285()
    rng = MersenneTwister(13)
    xs = collect(rand(rng, Bool) for _ in 1:8)
    return length(xs) == 8 && all(x -> x isa Bool, xs)
end

# rand(rng, T) for an integer type in a generator returns scalars of that type.
function gen_rand_rng_int_types_9285()
    ok = true
    for T in (Int8, UInt32, Int64, UInt128)
        rng = MersenneTwister(21)
        xs = collect(rand(rng, T) for _ in 1:3)
        ok = ok && length(xs) == 3 && all(x -> x isa T, xs)
    end
    return ok
end

# Dimension-second form rand(rng, n) in a generator (same captured-RNG root
# cause): each draw is a length-n Vector{Float64} in [0, 1).
function gen_rand_rng_dim_9285()
    rng = MersenneTwister(1)
    ys = collect(rand(rng, n) for n in 1:3)
    return length.(ys) == [1, 2, 3] &&
           all(y -> eltype(y) == Float64, ys) &&
           all(y -> all(v -> 0 <= v < 1, y), ys)
end

# N-dimension form rand(rng, dims...) in a generator (Issue #9285 generalized to
# any arity): rand(rng, 2, 3) is a 2x3 Float64 matrix; each element is in [0, 1).
function gen_rand_rng_matrix_9285()
    rng = MersenneTwister(2)
    ms = collect(rand(rng, 2, 3) for _ in 1:2)
    return length(ms) == 2 &&
           all(A -> size(A) == (2, 3), ms) &&
           all(A -> eltype(A) == Float64, ms) &&
           all(A -> all(v -> 0 <= v < 1, A), ms)
end

# Higher-rank form rand(rng, 2, 2, 2): a 2x2x2 Float64 array per draw.
function gen_rand_rng_3d_9285()
    rng = MersenneTwister(3)
    ms = collect(rand(rng, 2, 2, 2) for _ in 1:2)
    return length(ms) == 2 &&
           all(A -> size(A) == (2, 2, 2), ms) &&
           all(A -> eltype(A) == Float64, ms) &&
           all(A -> all(v -> 0 <= v < 1, A), ms)
end

# randn with a captured RNG shares the identical root cause. randn draws are
# unbounded normals, so assert type and finiteness rather than a [0, 1) range.
function gen_randn_rng_vector_9285()
    rng = MersenneTwister(4)
    vs = collect(randn(rng, 2) for _ in 1:3)
    return length(vs) == 3 &&
           all(v -> length(v) == 2, vs) &&
           all(v -> eltype(v) == Float64, vs) &&
           all(v -> all(isfinite, v), vs)
end

function gen_randn_rng_matrix_9285()
    rng = MersenneTwister(5)
    ms = collect(randn(rng, 2, 3) for _ in 1:2)
    return length(ms) == 2 &&
           all(A -> size(A) == (2, 3), ms) &&
           all(A -> eltype(A) == Float64, ms) &&
           all(A -> all(isfinite, A), ms)
end

# Regression: randn(m, n) with a captured (statically Any) leading dimension
# still yields an m x n Float64 matrix (not-an-RNG branch of RandMaybeRng).
function captured_dim_randn_matrix_9285()
    m = 2
    ms = collect(randn(m, n) for n in 1:2)
    return size(ms[1]) == (2, 1) && size(ms[2]) == (2, 2) &&
           all(A -> eltype(A) == Float64, ms)
end

# Regression: the plain global-RNG dimension call rand(n) is untouched.
function plain_rand_dims_9285()
    r = rand(3)
    return typeof(r) == Vector{Float64} && length(r) == 3
end

# Regression: rand(m, n) with a captured (statically Any) leading dimension
# still yields an m x n Float64 matrix (routed through the same instruction's
# not-an-RNG branch).
function captured_dim_matrix_9285()
    m = 2
    ys = collect(rand(m, n) for n in 1:2)
    return size(ys[1]) == (2, 1) && size(ys[2]) == (2, 2) &&
           all(A -> eltype(A) == Float64, ys)
end

# Regression: the direct call rand(rng, Float16) with a statically-typed RNG
# still takes the fast scalar path and returns a Float16.
function direct_rand_rng_float16_9285()
    rng = MersenneTwister(1)
    return rand(rng, Float16) isa Float16
end

println(gen_rand_rng_float16_9285())
println(comp_rand_rng_float16_9285())
println(gen_rand_rng_bool_9285())
println(gen_rand_rng_int_types_9285())
println(gen_rand_rng_dim_9285())
println(gen_rand_rng_matrix_9285())
println(gen_rand_rng_3d_9285())
println(gen_randn_rng_vector_9285())
println(gen_randn_rng_matrix_9285())
println(captured_dim_randn_matrix_9285())
println(plain_rand_dims_9285())
println(captured_dim_matrix_9285())
println(direct_rand_rng_float16_9285())

gen_rand_rng_float16_9285() && comp_rand_rng_float16_9285() &&
    gen_rand_rng_bool_9285() && gen_rand_rng_int_types_9285() &&
    gen_rand_rng_dim_9285() && gen_rand_rng_matrix_9285() &&
    gen_rand_rng_3d_9285() && gen_randn_rng_vector_9285() &&
    gen_randn_rng_matrix_9285() && captured_dim_randn_matrix_9285() &&
    plain_rand_dims_9285() && captured_dim_matrix_9285() &&
    direct_rand_rng_float16_9285()
