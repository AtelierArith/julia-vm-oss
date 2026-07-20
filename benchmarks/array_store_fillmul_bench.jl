# Array-store / map-copy benchmark (Issue #10566 blocker (a)).
#
# `fill_mul!` / `map_copy!` are the canonical shapes targeted by #10566: an
# untyped function that stores into a `Vector{Int64}` / `Vector{Float64}`
# argument in a hot loop.
#
# Before the #10566 (a) fix, the `CallSpecialize` site for these calls
# permanently fell back to the generic interpreter, because a MemoryRef-backed
# `Vector` argument presented as `ValueType::Struct` instead of
# `ValueType::ArrayOf`, so `compile_index_assign`'s `IndexAssign`
# specialization (Issue #6346) never matched and the whole specialization was
# rejected (and negatively cached, Issue #8603 — so it never retried).
#
# NOTE: this benchmarks the *specialized body* path only. The typed-loop
# native block (#10566 blockers (b) `StoreSlotArray` narrowing and (c)
# transactional write-through) is still open, so these loops do not yet run as
# a native typed block.
#
# Run: ./target/release-fast/sjulia benchmarks/array_store_fillmul_bench.jl

function fill_mul!(a, n)
    for i in 1:n
        a[i] = i * 2
    end
    return a
end

function map_copy!(y, x, n)
    for i in 1:n
        y[i] = x[i] + 1.0
    end
    return y
end

function bench_fill(n, iters)
    a = zeros(Int64, n)
    for _ in 1:iters
        fill_mul!(a, n)
    end
    return a[n]
end

function bench_map(n, iters)
    x = zeros(Float64, n)
    y = zeros(Float64, n)
    for _ in 1:iters
        map_copy!(y, x, n)
    end
    return y[n]
end

const N = 10_000
const ITERS = 500

t0 = time()
r1 = bench_fill(N, ITERS)
t1 = time()
r2 = bench_map(N, ITERS)
t2 = time()

println("fill_mul!  result=", r1, " time=", round(t1 - t0; digits = 3), "s")
println("map_copy!  result=", r2, " time=", round(t2 - t1; digits = 3), "s")
println("total=", round(t2 - t0; digits = 3), "s")
