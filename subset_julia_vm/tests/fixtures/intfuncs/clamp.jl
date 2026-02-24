# Test clamp function (Issue #481)
# Based on Julia's base/intfuncs.jl:1444

using Test

@testset "clamp: restrict value to specified range (Issue #481)" begin

    result = 0.0

    # Test clamp when value is in range
    if clamp(5, 1, 10) == 5
        result = result + 1.0
    end

    # Test clamp when value is below lo
    if clamp(-5, 0, 10) == 0
        result = result + 1.0
    end

    # Test clamp when value is above hi
    if clamp(15, 0, 10) == 10
        result = result + 1.0
    end

    # Test clamp with floats
    if clamp(3.5, 1.0, 5.0) == 3.5
        result = result + 1.0
    end

    # Test clamp at boundary (equals lo)
    if clamp(0, 0, 10) == 0
        result = result + 1.0
    end

    # Test clamp at boundary (equals hi)
    if clamp(10, 0, 10) == 10
        result = result + 1.0
    end

    # Test clamp with negative range
    if clamp(-5, -10, -1) == -5
        result = result + 1.0
    end

    # Test clamp below negative range
    if clamp(-15, -10, -1) == -10
        result = result + 1.0
    end

    @test (result) == 8.0
end

true  # Test passed
