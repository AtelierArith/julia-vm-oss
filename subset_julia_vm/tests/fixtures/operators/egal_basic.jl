# === (egal) operator - object identity
# 1 === 1 is true (1), 1 === 1.0 is false (0)
# Result: 1 + 0 = 1.0

using Test

@testset "=== (egal) operator - object identity" begin
    result = 0.0
    result += (1 === 1) ? 1.0 : 0.0
    result += (1 === 1.0) ? 1.0 : 0.0
    @test (result) == 1.0
end

true  # Test passed
