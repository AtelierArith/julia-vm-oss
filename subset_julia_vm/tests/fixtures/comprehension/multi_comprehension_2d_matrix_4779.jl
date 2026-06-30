# Issue #4779: Multi-variable array comprehensions
# `[expr for i in R1, j in R2]` executed (the symbol resolution from
# the now-closed #2143 worked) but produced a 1D `Vector{T}` of
# length `length(R1) * length(R2)` instead of upstream's 2D
# `Matrix{T}` of shape `(length(R1), length(R2))`.
#
# Root cause: `compile_multi_comprehension` allocated a 1D result
# array and ArrayPush'd elements in column-major order, but never
# reshaped to the multi-dimensional layout that upstream's
# `[expr for ..., ...]` produces.
#
# Fix: at the end of `compile_multi_comprehension`, for n>=2
# iterations, load each `len_var` and emit
# `Instr::CallBuiltin(BuiltinId::Reshape, 1 + n)`. The nested-loop
# iteration order is already column-major (outermost loop = last
# iteration), so the flat element order matches what column-major
# reshape expects.

using Test

@testset "Multi-comp 2D returns Matrix (Issue #4779)" begin
    m = [i + j for i in 1:2, j in 1:3]
    @test typeof(m) === Matrix{Int64}
    @test size(m) == (2, 3)
    @test ndims(m) == 2
    @test m == [2 3 4; 3 4 5]
end

@testset "Multi-comp 2D with Float source (Issue #4779)" begin
    m = [i * j for i in 1.0:3.0, j in 1.0:2.0]
    @test typeof(m) === Matrix{Float64}
    @test size(m) == (3, 2)
end

@testset "Multi-comp 2D iteration order (Issue #4779)" begin
    # Column-major: (1,1), (2,1), (1,2), (2,2), (1,3), (2,3)
    # → values at row-major positions: m[1,1]=1+1, m[2,1]=2+1,
    #   m[1,2]=1+2, m[2,2]=2+2, m[1,3]=1+3, m[2,3]=2+3
    m = [i + j for i in 1:2, j in 1:3]
    @test m[1, 1] == 2
    @test m[2, 1] == 3
    @test m[1, 2] == 3
    @test m[2, 2] == 4
    @test m[1, 3] == 4
    @test m[2, 3] == 5
end

@testset "Multi-comp 3D returns Array (Issue #4779)" begin
    # `i` body keeps inference clean; multi-op body inference is a
    # separate pre-existing limitation.
    a = [i for i in 1:2, j in 1:3, k in 1:2]
    @test typeof(a) === Array{Int64, 3}
    @test size(a) == (2, 3, 2)
    @test ndims(a) == 3
end

@testset "Single-iter comprehension regression — still 1D Vector (Issue #4779)" begin
    v = [i * 2 for i in 1:5]
    @test typeof(v) === Vector{Int64}
    @test size(v) == (5,)
    @test v == [2, 4, 6, 8, 10]
end

true
