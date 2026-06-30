# Issue #6806 (PR B): indexing a MemoryRef-backed `Array{T,N}` wrapper through an
# untyped parameter (raw `IndexLoad`) reads the element directly from storage for
# both index modes `ArrayValue::linear_index` accepts — a single linear index of
# any rank, or one index per dimension (column-major) — instead of dispatching
# `getindex` per index. This extends the rank-1 fast path to multi-dimensional
# reads. Characterization of value and bounds semantics; verified against
# upstream Julia 1.12.
using Test

mid(m, i, j) = m[i, j]      # full N-index
lin(a, k) = a[k]            # single linear index (any rank)

@testset "multi-dim wrapper indexing via untyped param (Issue #6806)" begin
    m = [10i + j for i in 1:3, j in 1:4]   # 3x4 Matrix{Int64}, column-major

    # full per-dimension indexing
    @test mid(m, 1, 1) == 11
    @test mid(m, 3, 4) == 34
    @test mid(m, 2, 3) == 23

    # single linear index into a matrix (column-major order)
    @test lin(m, 1) == 11          # m[1,1]
    @test lin(m, 2) == 21          # m[2,1] (column-major)
    @test lin(m, 4) == 12          # m[1,2]
    @test lin(m, 12) == 34         # m[3,4]

    # 3-D array: full indexing and single linear index
    t = [100i + 10j + k for i in 1:2, j in 1:2, k in 1:2]
    @test t[1, 1, 1] == 111
    @test t[2, 2, 2] == 222
    @test lin(t, 1) == 111
    @test lin(t, 8) == 222

    # bounds errors preserved (type) for both modes
    @test_throws BoundsError mid(m, 4, 1)
    @test_throws BoundsError mid(m, 1, 5)
    @test_throws BoundsError lin(m, 13)
    @test_throws BoundsError lin(m, 0)

    # values stay typed
    mf = [Float64(i + j) for i in 1:2, j in 1:2]
    @test mid(mf, 2, 2) === 4.0
end

true
