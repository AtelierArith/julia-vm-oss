# Test: StepRangeLen iteration with for loop

using Test

function test_steprangelen_iteration()
    r = StepRangeLen(1.0, 0.5, 5)
    # Elements: 1.0, 1.5, 2.0, 2.5, 3.0
    total = 0.0
    for x in r
        total += x
    end
    return total
end

@testset "StepRangeLen iteration with for loop (Issue #529)" begin
    @test (test_steprangelen_iteration()) == 10.0
end

true  # Test passed
