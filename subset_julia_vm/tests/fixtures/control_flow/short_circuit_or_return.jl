# Test short-circuit || return pattern

using Test

function test_or_return(x)
    x < 0 || return 99
    0
end

@testset "Short-circuit || return pattern" begin

    # Sum the results: 0 + 99 = 99
    @test (test_or_return(-5) + test_or_return(5)) == 99.0
end

true  # Test passed
