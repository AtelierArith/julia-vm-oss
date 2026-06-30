# Issue #4836: `compile_multi_comprehension` had the same F64
# fallback bug that `compile_comprehension` had — when body
# inference yielded `Any` or any non-matched `ValueType`, the result
# silently became `Array{Float64, N}` even when the body produced
# non-numeric or Int values at runtime. Surfaced by PR #4835
# (#4779) which fixed the shape but left the eltype inference path
# untouched.
#
# Sibling of #4822 (PR #4823 fixed the single-iterator case by
# changing the fallback to `ArrayElementType::Any`).
#
# Fix: change the `_ => ArrayElementType::F64` fallback in
# `compile_multi_comprehension` to `_ => ArrayElementType::Any`,
# and pair with a body-compile fallback that uses StoreAny/LoadAny
# instead of forcing F64 coercion.

using Test

@testset "Multi-comp Any-body preserves Int values (Issue #4836)" begin
    # `i+j+k` body inference falls through to Any (multi-op chain
    # crosses the current inference resolution limit). With the
    # F64 fallback this used to coerce values to Float64.
    m = [i + j + k for i in 1:2, j in 1:3, k in 1:2]
    @test ndims(m) == 3
    @test size(m) == (2, 3, 2)
    @test m[1, 1, 1] === 3
    @test m[2, 3, 2] === 7
    # Eltype must not silently widen to Float64.
    @test eltype(m) !== Float64
end

@testset "Multi-comp Any-body preserves String values (Issue #4836)" begin
    m = [string(i, ",", j) for i in 1:2, j in 1:2]
    @test ndims(m) == 2
    @test size(m) == (2, 2)
    @test m[1, 1] == "1,1"
    @test m[2, 2] == "2,2"
    @test eltype(m) !== Float64
end

@testset "Multi-comp 2D Int-body regression — still Matrix{Int64} (Issue #4836)" begin
    # Simple-var body inference still resolves to I64.
    m = [i for i in 1:2, j in 1:3]
    @test typeof(m) === Matrix{Int64}
    @test size(m) == (2, 3)
end

@testset "Multi-comp 2D Float-body regression — still Matrix{Float64} (Issue #4836)" begin
    # Float body inference still resolves correctly.
    m = [Float64(i + j) for i in 1:2, j in 1:3]
    @test typeof(m) === Matrix{Float64}
    @test size(m) == (2, 3)
end

true
