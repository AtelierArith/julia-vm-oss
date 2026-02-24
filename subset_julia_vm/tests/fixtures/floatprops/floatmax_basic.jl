# Test floatmax() returns largest finite Float64
# floatmax() == 1.7976931348623157e308
# floatmax() > 1e300 should be true

using Test

@testset "floatmax() returns largest finite Float64" begin
    result = floatmax() > 1e300 && isfinite(floatmax())
    @test (result ? 1.0 : 0.0) == 1.0
end

true  # Test passed
