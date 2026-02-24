# Test: Function returning Range with Float64 parameters (issue #354)
# This tests the fix where MakeRangeLazy incorrectly required Int64 values,
# but specialization could emit Float64 values for function parameters.

using Test

function myrange(start, stop)
    return start:stop
end

@testset "Function returning Range with Float64 parameters (issue #354)" begin


    # Test with Float64 arguments - just verify it returns a valid range
    r = myrange(1.0, 5.0)

    # Collect the range and sum it to verify correctness
    arr = collect(r)
    @test (sum(arr)) == 15.0
end

true  # Test passed
