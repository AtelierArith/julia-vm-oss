# Test: Power operator with Any-typed parameter returns correct type
# Julia semantics: Int64^Int64 -> Int64, Float64^Int64 -> Float64

using Test

function pow_any(x::Any)
    x ^ 2
end

@testset "Power operator with Any-typed parameter preserves input type (Int->Int, Float->Float)" begin


    # Test with Int64 argument - should return Int64
    r1 = pow_any(5)
    check1 = r1 isa Int64

    # Test with Float64 argument - should return Float64
    r2 = pow_any(5.0)
    check2 = r2 isa Float64

    # Return 2.0 if both type checks pass
    result = (check1 ? 1 : 0) + (check2 ? 1 : 0)
    @test (result) == 2.0
end

true  # Test passed
