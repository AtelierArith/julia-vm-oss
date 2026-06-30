# Test range(start; step, length) element-type parity with upstream (Issue #5135)
#
# Upstream `range_start_step_length` (julia/base/range.jl:222-229) returns
# `StepRange{typeof(stop),typeof(step)}` for integer start/step, preserving the
# integer element type. sjulia previously float-promoted via
# `StepRangeLen(start * 1.0, step * 1.0, ...)`, so `range(1, step=2, length=5)`
# yielded `[1.0, 3.0, 5.0, 7.0, 9.0]` instead of `[1, 3, 5, 7, 9]`. Float
# start/step still produces a `StepRangeLen{Float64}`, matching upstream values.

using Test

@testset "range(start; step, length) preserves integer eltype" begin
    r = range(1, step=2, length=5)
    # Upstream: StepRange{Int64, Int64}, NOT a float StepRangeLen.
    @test r isa StepRange
    @test eltype(r) == Int64
    c = collect(r)
    @test eltype(c) == Int64
    @test c == [1, 3, 5, 7, 9]
    @test c[1] === 1
    @test c[5] === 9
end

@testset "range(start; step, length) with negative integer step" begin
    r = range(10, step=-2, length=5)
    @test r isa StepRange
    @test eltype(r) == Int64
    @test collect(r) == [10, 8, 6, 4, 2]
end

@testset "range(start; step, length) length=1 integer" begin
    r = range(3, step=2, length=1)
    @test r isa StepRange
    @test collect(r) == [3]
    @test collect(r)[1] === 3
end

@testset "range(start; step, length) float step stays StepRangeLen" begin
    r = range(1.0, step=0.5, length=4)
    @test r isa StepRangeLen
    @test eltype(r) == Float64
    @test collect(r) == [1.0, 1.5, 2.0, 2.5]
end

@testset "range(start; step, length) int start float step" begin
    r = range(1, step=0.5, length=4)
    @test r isa StepRangeLen
    @test collect(r) == [1.0, 1.5, 2.0, 2.5]
end

true
