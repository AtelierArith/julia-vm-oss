using Test

# Issue #5667: 1:1:5 (explicit step 1) is a StepRange in upstream, but sjulia
# collapsed any step-1 range to UnitRange. An explicit step now yields a StepRange
# even when the step is 1, distinguished from the 2-argument UnitRange 1:5.

@testset "explicit-step range is a StepRange (Issue #5667)" begin
    # typeof distinguishes explicit step from unit range.
    @test typeof(1:1:5) == StepRange{Int64, Int64}
    @test typeof(1:5) == UnitRange{Int64}
    @test typeof(0:1:10) == StepRange{Int64, Int64}
    @test typeof(1:2:9) == StepRange{Int64, Int64}

    # isa
    @test isa(1:1:5, StepRange)
    @test !isa(1:1:5, UnitRange)
    @test isa(1:5, UnitRange)

    # show renders the explicit step.
    @test string(1:1:5) == "1:1:5"
    @test string(1:5) == "1:5"
    @test repr(1:1:5) == "1:1:5"

    # Values/iteration unaffected.
    @test collect(1:1:5) == [1, 2, 3, 4, 5]
    @test collect(1:1:5) == collect(1:5)
    @test (1:1:5)[2] == 2
    @test step(1:1:5) == 1
    @test first(1:1:5) == 1
    @test last(1:1:5) == 5
    @test length(1:1:5) == 5
    @test sum(1:1:5) == 15

    # == is element-wise (so 1:1:5 == 1:5), but === distinguishes... values equal.
    @test (1:1:5) == (1:5)
    @test (1:1:5) == (1:1:5)
end

true
