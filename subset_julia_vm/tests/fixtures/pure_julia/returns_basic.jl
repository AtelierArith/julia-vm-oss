# Test Returns functor - constant value wrapper

using Test

@testset "Returns - functor that returns constant value" begin

    # Create Returns instance with integer
    r = Returns(42)
    @assert r.value == 42

    # Test the helper function
    @assert call_returns(r) == 42

    # Different numeric value
    r2 = Returns(3.14)
    @assert r2.value == 3.14
    @assert call_returns(r2) == 3.14

    @test (true)
end

true  # Test passed
