# Performance regression test for Float64-array × Int-scalar broadcasting.
#
# Issue #7587: `x .^ 2` (and `.+ 2`, `.* 2`, …) on a `Vector{Float64}` used to be
# ~8-14x slower than the same operation with a Float64 scalar (`x .^ 2.0`),
# because the per-element mixed-type call `^(::Float64, ::Int)` fell through to
# the generic `^(::Number, ::Number)` promote() fallback in promotion.jl instead
# of a concrete fast method. Re-adding concrete mixed Float/Int arithmetic
# methods (base/float.jl) closes the gap.
#
# This compares the Int-scalar form against the Float64-scalar form and asserts
# they run within a generous ratio, catching a severe regression while tolerating
# CI timing noise (cf. broadcast_perf_int_fastpath_regression.jl).

using Test

function bcast_time(f, iters)
    t0 = time_ns()
    local r
    for _ in 1:iters
        r = f()
    end
    return (time_ns() - t0) / 1.0e9
end

@testset "Float64-array × Int-scalar broadcast is not pathologically slow (#7587)" begin
    n = 500
    iters = 10
    x = [Float64(i) * 0.01 for i in 1:n]

    # Warmup (compile / cache paths)
    bcast_time(() -> x .^ 2, 1)
    bcast_time(() -> x .^ 2.0, 1)
    bcast_time(() -> x .+ 2, 1)
    bcast_time(() -> x .+ 2.0, 1)
    bcast_time(() -> x .* 2, 1)
    bcast_time(() -> x .* 2.0, 1)

    t_pow_int = bcast_time(() -> x .^ 2, iters)
    t_pow_flt = bcast_time(() -> x .^ 2.0, iters)
    t_add_int = bcast_time(() -> x .+ 2, iters)
    t_add_flt = bcast_time(() -> x .+ 2.0, iters)
    t_mul_int = bcast_time(() -> x .* 2, iters)
    t_mul_flt = bcast_time(() -> x .* 2.0, iters)

    # Before the fix these ratios were ~8-14. A margin of 4.0 leaves ample room
    # for CI noise while still failing on the promote()-fallback regression.
    @test (t_pow_int / t_pow_flt) < 4.0
    @test (t_add_int / t_add_flt) < 4.0
    @test (t_mul_int / t_mul_flt) < 4.0
end

true
