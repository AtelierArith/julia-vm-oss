# Test StepRangeLen basic operations (Issue #325)
# StepRangeLen provides lazy range parameterized by reference, step, and length

using Test

@testset "StepRangeLen basic operations" begin
    r = StepRangeLen(1.0, 0.5, 5, 1)

    # Type check
    @test isa(r, StepRangeLen)

    # Length
    @test length(r) == 5

    # First element: ref + (1 - offset) * step = 1.0 + (1 - 1) * 0.5 = 1.0
    @test first(r) == 1.0

    # Last element: ref + (len - offset) * step = 1.0 + (5 - 1) * 0.5 = 3.0
    @test last(r) == 3.0

    # Step
    @test step(r) == 0.5
end

@testset "StepRangeLen iteration" begin
    r = StepRangeLen(0.0, 1.0, 4, 1)
    values = Float64[]
    for x in r
        push!(values, x)
    end

    @test length(values) == 4
    @test values[1] == 0.0
    @test values[2] == 1.0
    @test values[3] == 2.0
    @test values[4] == 3.0
end

@testset "StepRangeLen getindex" begin
    r = StepRangeLen(0.0, 0.25, 5, 1)

    @test r[1] == 0.0
    @test r[2] == 0.25
    @test r[5] == 1.0
end

@testset "StepRangeLen from range_start_step_length" begin
    # range(start; step=s, length=n) should return StepRangeLen
    r = range_start_step_length(1.0, 2.0, 4)

    @test isa(r, StepRangeLen)
    @test first(r) == 1.0
    @test step(r) == 2.0
    @test length(r) == 4
end

true
