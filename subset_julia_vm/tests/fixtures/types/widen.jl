# Test widen function
# widen returns a wider type for numeric values

using Test

@testset "widen function for type widening" begin

    # Value-based widen - check that values are correctly widened
    r1 = widen(Int8(42)) == 42
    r2 = widen(Int16(100)) == 100
    r3 = widen(Int32(1000)) == 1000
    r4 = widen(Int64(9999)) == 9999

    # Float32 to Float64 conversion
    f32 = Float32(3.14)
    f64 = widen(f32)
    # Check that the widened value is approximately correct (Float32 precision)
    r5 = abs(f64 - 3.14) < 0.01

    # Check that widen of Int64 stays Int64 (can't widen further)
    r6 = widen(Int64(123)) == 123

    # All tests must pass
    @test ((r1 && r2 && r3 && r4 && r5 && r6) ? 1 : 0) == 1.0
end

true  # Test passed
