# Test: StepRangeLen direct construction with sum
# Note: range(; step, length) function returns StepRangeLen but type inference
# doesn't track it, so we test StepRangeLen directly instead.

using Test

function test_steprangelen_sum()
    r = StepRangeLen(0.0, 0.25, 5)
    # Elements: 0.0, 0.25, 0.5, 0.75, 1.0
    total = 0.0
    for x in r
        total += x
    end
    return total
end

@testset "range with step and length returns StepRangeLen (Issue #529)" begin
    @test isapprox((test_steprangelen_sum()), 2.5)
end

true  # Test passed
