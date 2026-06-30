using Test

# Issue #5661: `reverse` of a range is itself a lazy range, not a materialized
# Vector. `reverse(1:5)` is `5:-1:1` (a StepRange), not `[5, 4, 3, 2, 1]`.
# sjulia fell through to the generic array `reverse`, which collected the range.
#
# NOTE: range `==` / `===` are separately broken in sjulia (filed as a follow-up),
# so this fixture checks the reversed range via `collect`, `typeof`/`isa`, and the
# `first`/`last`/`step`/`length` accessors rather than comparing ranges directly.

@testset "reverse(::AbstractRange) returns a range, not a Vector (Issue #5661)" begin
    r = reverse(1:5)
    @test r isa StepRange
    @test typeof(r) === StepRange{Int64,Int64}
    @test collect(r) == [5, 4, 3, 2, 1]
    @test first(r) == 5
    @test last(r) == 1
    @test step(r) == -1
    @test length(r) == 5

    # Stepped integer range.
    r2 = reverse(1:2:9)
    @test r2 isa StepRange
    @test collect(r2) == [9, 7, 5, 3, 1]
    @test step(r2) == -2

    r3 = reverse(2:2:10)
    @test collect(r3) == [10, 8, 6, 4, 2]

    # Float range reverses to a StepRangeLen.
    rf = reverse(0.0:0.5:2.0)
    @test collect(rf) == [2.0, 1.5, 1.0, 0.5, 0.0]
    @test step(rf) == -0.5
end

@testset "reverse on an Array still materializes (Issue #5661)" begin
    @test reverse([1, 2, 3]) == [3, 2, 1]
    @test reverse([1, 2, 3]) isa Vector{Int64}
end

true
