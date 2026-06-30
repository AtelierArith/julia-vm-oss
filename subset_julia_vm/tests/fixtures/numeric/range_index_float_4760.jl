# Issue #4760: whole-number Float64 range like 0.0:1.0:5.0
# narrowed first/step/last/[i] to Int64 because the range
# `getindex` heuristic at vm/exec/array_index.rs picked the
# narrowing branch whenever the computed element was a whole
# number, regardless of the range's `is_float` flag.
#
# Fix: the narrowing branch now also requires `!range.is_float`.
# Non-float ranges keep their existing "narrow to I64 when
# integer-valued" behavior (so `(1:5)[1] === Int64(1)` still
# holds); float ranges route through `typed_element`, which
# preserves the Float64 element type.

using Test

@testset "repr(Float64 whole-number range) preserves Float (Issue #4760)" begin
    @test repr(0.0:1.0:5.0) == "0.0:1.0:5.0"
    @test repr(0.0:1.0:2.0) == "0.0:1.0:2.0"
    @test repr(-2.0:1.0:2.0) == "-2.0:1.0:2.0"
end

@testset "first/last/step on Float64 whole-number range (Issue #4760)" begin
    r = 0.0:1.0:5.0
    @test first(r) === 0.0
    @test last(r) === 5.0
    @test step(r) === 1.0
    @test r[1] === 0.0
    @test r[6] === 5.0
end

@testset "Int range regression — narrow Int64 stays Int64 (Issue #4760)" begin
    r = 1:5
    @test first(r) === 1
    @test last(r) === 5
    @test r[1] === 1
    @test r[3] === 3
    @test typeof(first(r)) === Int64
end

@testset "Mixed float range (non-whole step) unchanged (Issue #4760)" begin
    # Already worked — regression guard.
    @test first(0.5:1.0:5.5) === 0.5
    @test repr(0.5:1.0:5.5) == "0.5:1.0:5.5"
end

@testset "collect Float64 whole-number range (Issue #4760)" begin
    @test collect(0.0:1.0:2.0) == [0.0, 1.0, 2.0]
    @test eltype(collect(0.0:1.0:2.0)) === Float64
end

true
