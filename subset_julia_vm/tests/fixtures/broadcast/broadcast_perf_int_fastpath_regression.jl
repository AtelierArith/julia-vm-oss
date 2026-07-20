# Correctness companion of the Int same-shape broadcast fast path.
#
# This fixture used to assert a wall-clock ratio (broadcast vs for-loop).
# Wall-clock ratio @tests in the shared fixture suite are structurally
# load-sensitive: the sibling Float64 fixture flaked under full-suite
# saturation once the #9360 @testset gate made fixture @test failures gating
# (Issue #9489). Per repo policy (Issue #3210) the performance guard now lives
# in a Criterion benchmark with stable statistical methodology:
#
#     cargo bench -p subset_julia_vm --bench vm_broadcast_fastpath_benchmark
#
# Here we keep only the load-robust correctness check: the in-place broadcast
# `out .= a .+ b` on Int vectors must produce exactly the same result as the
# explicit for loop, including after repeated in-place reuse.

using Test

function for_add!(out, a, b, iters)
    for _ in 1:iters
        for i in 1:length(a)
            out[i] = a[i] + b[i]
        end
    end
    return out
end

function bcast_add!(out, a, b, iters)
    for _ in 1:iters
        out .= a .+ b
    end
    return out
end

@testset "broadcast int same-shape fast path correctness" begin
    n = 1000
    iters = 10
    a = [i for i in 1:n]
    b = [2 * i for i in 1:n]
    out_loop = [0 for _ in 1:n]
    out_bcast = [0 for _ in 1:n]

    for_add!(out_loop, a, b, iters)
    bcast_add!(out_bcast, a, b, iters)

    @test out_bcast == out_loop
    @test out_bcast[1] == 3
    @test out_bcast[n] == 3 * n
    @test eltype(out_bcast) == Int
end

true
