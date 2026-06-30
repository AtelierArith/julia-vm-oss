# Issue #5416 (inference half): a keyword argument with a `nothing` default does
# not constrain the kwarg — a caller may pass any value. PR #5424 fixed the slot
# metadata (the `LoadSlotNothing` crash); this guards the remaining inference bug
# where the abstract interpreter inferred `by::Nothing` (a singleton) and
# constant-folded `return by` to the constant `nothing`, so `f(1, by = 10)`
# silently returned `nothing` instead of the passed value. Verified against
# upstream Julia 1.12.
#
# Long-form definitions used deliberately: two short-form keyword-only methods
# sharing a kwarg name currently fail to lower (separate bug, #5422).

using Test

function passthrough(x; by = nothing)
    return by
end

@testset "nothing-default kwarg value is not dropped (#5416)" begin
    @testset "passed value is returned, not folded to nothing" begin
        @test passthrough(1, by = 10) == 10
        @test typeof(passthrough(1, by = 10)) === Int64
        @test passthrough(1, by = "hi") == "hi"
        @test passthrough(1, by = 2.5) == 2.5
        @test passthrough(1, by = [1, 2]) == [1, 2]
    end

    @testset "omitted kwarg keeps the nothing default" begin
        @test passthrough(1) === nothing
    end

    @testset "stdlib sort with by kwarg" begin
        @test sort([3, 1, 2]; by = x -> -x) == [3, 2, 1]
    end
end

true
