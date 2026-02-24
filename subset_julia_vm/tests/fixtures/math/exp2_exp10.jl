# Test exp2 and exp10 functions (Issue #483)
# Based on Julia's base/math.jl:1343-1344

using Test

@testset "exp2 and exp10: exponential base 2 and 10 (Issue #483)" begin

    result = 0.0

    # Test exp2(0) = 1
    if abs(exp2(0.0) - 1.0) < 1e-10
        result = result + 1.0
    end

    # Test exp2(1) = 2
    if abs(exp2(1.0) - 2.0) < 1e-10
        result = result + 1.0
    end

    # Test exp2(3) = 8
    if abs(exp2(3.0) - 8.0) < 1e-10
        result = result + 1.0
    end

    # Test exp2(-1) = 0.5
    if abs(exp2(-1.0) - 0.5) < 1e-10
        result = result + 1.0
    end

    # Test exp10(0) = 1
    if abs(exp10(0.0) - 1.0) < 1e-10
        result = result + 1.0
    end

    # Test exp10(1) = 10
    if abs(exp10(1.0) - 10.0) < 1e-10
        result = result + 1.0
    end

    # Test exp10(2) = 100
    if abs(exp10(2.0) - 100.0) < 1e-10
        result = result + 1.0
    end

    # Test exp10(-1) = 0.1
    if abs(exp10(-1.0) - 0.1) < 1e-10
        result = result + 1.0
    end

    @test (result) == 8.0
end

true  # Test passed
