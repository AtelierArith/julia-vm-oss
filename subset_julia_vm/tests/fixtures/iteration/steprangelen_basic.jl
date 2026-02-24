# Test: StepRangeLen basic construction and iteration
# StepRangeLen creates values with exact step size

using Test

function test_steprangelen_basic()
    r = StepRangeLen(1.0, 0.5, 5)
    n = 0.0
    for x in r
        n += 1.0
    end
    return n
end

@testset "StepRangeLen basic construction (Issue #529)" begin
    @test (test_steprangelen_basic()) == 5.0
end

true  # Test passed
