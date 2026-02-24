# Test: @generated fallback basic
# Phase 1 implementation: execute fallback branch only

using Test

function square_fallback(x)
    result = 0
    if @generated
        # Ignored in Phase 1
        result = -1  # This should never execute
    else
        # Fallback is executed
        result = x * x
    end
    result
end

@testset "@generated with fallback: executes else branch (Phase 1)" begin


    r1 = square_fallback(3)
    @assert r1 == 9

    r2 = square_fallback(5)
    @assert r2 == 25

    # Return true if all tests passed
    @test (true)
end

true  # Test passed
