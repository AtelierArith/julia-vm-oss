# Test: LinRange iteration with for loop

using Test

function test_linrange_iteration()
    r = LinRange(1.0, 5.0, 5)
    total = 0.0
    for x in r
        total += x
    end
    return total
end

@testset "LinRange iteration with for loop (Issue #529)" begin
    @test (test_linrange_iteration()) == 15.0
end

true  # Test passed
