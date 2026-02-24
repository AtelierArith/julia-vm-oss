using Test

function for_add!(out, a, b, iters)
    t0 = time_ns()
    for _ in 1:iters
        for i in 1:length(a)
            out[i] = a[i] + b[i]
        end
    end
    return (time_ns() - t0) / 1.0e9
end

function bcast_add!(out, a, b, iters)
    t0 = time_ns()
    for _ in 1:iters
        out .= a .+ b
    end
    return (time_ns() - t0) / 1.0e9
end

@testset "broadcast fast path regression" begin
    n = 1000
    iters = 10
    a = [Float64(i) for i in 1:n]
    b = [Float64(2 * i) for i in 1:n]
    out = [0.0 for _ in 1:n]

    # Warmup
    for_add!(out, a, b, 1)
    bcast_add!(out, a, b, 1)

    tf = for_add!(out, a, b, iters)
    tb = bcast_add!(out, a, b, iters)
    println("broadcast/for ratio=", tb / tf)

    # Current baseline is ~40x slower; fast path should bring this down substantially.
    @test (tb / tf) < 2.0
end

true
