# Test: euler constant
# Expected: true

using Test

@testset "exp(1) matches Euler's number" begin

    @test (abs(exp(1.0) - 2.718281828459045) < 1e-10)
end

true  # Test passed
