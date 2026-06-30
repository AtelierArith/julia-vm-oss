# A multi-iterator comprehension produces an N-dimensional array whose rank
# equals the number of iterator clauses: `[f(i,j) for i in r1, j in r2]` is a
# `Matrix`, `[f(i) for i in r]` is a `Vector`. sjulia previously inferred the
# rank-free `Array` for every comprehension, so typed multiple dispatch
# mis-selected a `::Vector` method for a 2-D `Matrix` argument and `view` on a
# comprehension-built matrix failed with a `view(::Vector, ...)` MethodError
# (Issue #6817). The runtime value, `typeof`, `ndims`, and `isa` were always
# correct — only the compile-time rank inference was wrong.
#
# All expectations verified against upstream Julia 1.12.

using Test

f(::Matrix) = "matrix"
f(::Vector) = "vector"

g(::Matrix{Int64}) = "mi"
g(::Matrix{Float64}) = "mf"

@testset "multi-iterator comprehension rank dispatch (Issue #6817)" begin
    # --- rank dispatch: 2-D comprehension is a Matrix, 1-D is a Vector ---
    @test f([i + j for i in 1:3, j in 1:3]) == "matrix"     # inline 2-D
    @test f([i for i in 1:4]) == "vector"                   # inline 1-D
    m = [i + j for i in 1:3, j in 1:3]
    v = [k * k for k in 1:5]
    @test f(m) == "matrix"                                  # variable 2-D
    @test f(v) == "vector"                                  # variable 1-D

    # --- view on a comprehension-built Matrix (the original symptom) ---
    @test sum(view(m, 1:2, 1:3)) == 21                      # first two rows
    @test sum(view(m, :, 1)) == 9                           # colon view, first column

    # --- runtime value stays fully correct ---
    @test typeof(m) == Matrix{Int64}
    @test ndims(m) == 2
    @test size(m) == (3, 3)
    @test m isa Matrix
    @test !(m isa Vector)

    # --- 3-D comprehension ---
    t = [i + j + k for i in 1:2, j in 1:2, k in 1:2]
    @test typeof(t) == Array{Int64,3}
    @test ndims(t) == 3

    # --- element-specific dispatch still resolves on the concrete value ---
    @test g(m) == "mi"
    mf = [Float64(i + j) for i in 1:2, j in 1:2]
    @test g(mf) == "mf"
end

true
