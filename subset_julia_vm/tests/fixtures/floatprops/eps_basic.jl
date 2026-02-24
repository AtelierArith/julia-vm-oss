# Test eps() returns machine epsilon for Float64
# eps() == 2.220446049250313e-16
# 1.0 + eps() > 1.0 should be true

using Test

@testset "eps() returns machine epsilon (1.0 + eps() > 1.0)" begin
    result = 1.0 + eps() > 1.0
    @test (result ? 1.0 : 0.0) == 1.0
end

true  # Test passed
