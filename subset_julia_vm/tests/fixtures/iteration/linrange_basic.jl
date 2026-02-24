# Test: LinRange basic construction and iteration
# LinRange creates linearly spaced values between start and stop

using Test

function test_linrange_basic()
    r = LinRange(1.0, 5.0, 5)
    n = 0.0
    for x in r
        n += 1.0
    end
    return n
end

@testset "LinRange basic construction (Issue #529)" begin
    @test (test_linrange_basic()) == 5.0
end

true  # Test passed
