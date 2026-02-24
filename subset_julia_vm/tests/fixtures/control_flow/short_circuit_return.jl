# Test short-circuit && return pattern

using Test

function test_and_return(x)
    x > 0 && return 42
    -1
end

@testset "Short-circuit && return pattern" begin

    # Sum the results: 42 + (-1) = 41
    @test (test_and_return(5) + test_and_return(-5)) == 41.0
end

true  # Test passed
