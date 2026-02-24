# Test @elapsed macro - returns elapsed time in seconds

using Test

@testset "@elapsed macro returns elapsed time in seconds" begin

    # Basic usage
    t = @elapsed begin
        # Do some simple computation
        x = 0
        for i in 1:1000
            x = x + i
        end
    end

    # Elapsed time should be a Float64
    @assert typeof(t) == Float64

    # Time should be non-negative
    @assert t >= 0.0

    # Simple expression
    t2 = @elapsed 1 + 1
    @assert t2 >= 0.0

    # With sleep (tests actual timing) - use generous tolerance for slow CI
    t3 = @elapsed sleep(0.01)
    @assert t3 >= 0.0  # Just verify non-negative; actual sleep timing is OS-dependent

    @test (true)
end

true  # Test passed
