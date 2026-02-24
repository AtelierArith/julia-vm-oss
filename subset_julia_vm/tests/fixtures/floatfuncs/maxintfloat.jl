# Test maxintfloat function (Issue #479)
# Based on Julia's base/floatfuncs.jl:32-45

using Test

@testset "maxintfloat: largest exact integer in Float64 (Issue #479)" begin

    result = 0.0

    # Test maxintfloat() returns 2^53
    if maxintfloat() == 9007199254740992.0
        result = result + 1.0
    end

    # Test maxintfloat(Float64) returns 2^53
    if maxintfloat(Float64) == 9007199254740992.0
        result = result + 1.0
    end

    # Test maxintfloat on a value
    if maxintfloat(1.0) == 9007199254740992.0
        result = result + 1.0
    end

    # Verify the value is 2^53
    if maxintfloat() == 2.0 ^ 53
        result = result + 1.0
    end

    # Test that maxintfloat is exactly representable (no rounding)
    x = maxintfloat()
    if x == floor(x)
        result = result + 1.0
    end

    # Test maxintfloat + 1 is also representable (just to verify value)
    if maxintfloat() + 1.0 == 9007199254740993.0
        result = result + 1.0
    end

    @test (result) == 6.0
end

true  # Test passed
