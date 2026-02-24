# === (egal) with NaN and -0.0
# NaN === NaN is true (1), -0.0 === 0.0 is false (0)
# Result: 1 + 0 = 1.0

using Test

@testset "=== with NaN and -0.0" begin
    result = 0.0
    result += (NaN === NaN) ? 1.0 : 0.0
    result += (-0.0 === 0.0) ? 1.0 : 0.0
    @test (result) == 1.0
end

true  # Test passed
