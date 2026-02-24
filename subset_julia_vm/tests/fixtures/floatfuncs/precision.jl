# Test precision function (Issue #478)
# Based on Julia's base/float.jl:798-807

using Test

@testset "precision: number of significant bits in Float64 (Issue #478)" begin

    result = 0.0

    # Test precision(Float64) returns 53
    if precision(Float64) == 53
        result = result + 1.0
    end

    # Test precision on a Float64 value
    if precision(1.5) == 53
        result = result + 1.0
    end

    # Test precision on zero
    if precision(0.0) == 53
        result = result + 1.0
    end

    # Verify precision value (52 mantissa bits + 1 implicit bit = 53)
    if precision(Float64) == 52 + 1
        result = result + 1.0
    end

    @test (result) == 4.0
end

true  # Test passed
