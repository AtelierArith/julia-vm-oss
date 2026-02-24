# Test nextfloat and prevfloat functions

using Test

@testset "nextfloat and prevfloat - adjacent floating point values" begin

    # Basic tests: nextfloat increases, prevfloat decreases
    x = 1.0
    if nextfloat(x) <= x
        @assert false
    end
    if prevfloat(x) >= x
        @assert false
    end

    # nextfloat and prevfloat are inverses
    @assert prevfloat(nextfloat(1.0)) == 1.0
    @assert nextfloat(prevfloat(1.0)) == 1.0

    # eps relationship: nextfloat(x) - x == eps(x) for x = 1.0
    diff = nextfloat(1.0) - 1.0
    @assert diff == eps(1.0)

    # Test with zero
    if nextfloat(0.0) <= 0.0
        @assert false
    end
    if prevfloat(0.0) >= 0.0
        @assert false
    end

    # Test with negative numbers
    if nextfloat(-1.0) <= -1.0
        @assert false
    end
    if prevfloat(-1.0) >= -1.0
        @assert false
    end

    # Infinity handling
    @assert nextfloat(Inf) == Inf

    # Float64 argument (explicit)
    if nextfloat(1.0) <= 1.0
        @assert false
    end

    @test (true)
end

true  # Test passed
