# Test: something function - first non-nothing value
# Expected: true

using Test

@testset "something(x, y, ...) - returns first non-nothing value" begin

    @test (something(nothing, 42) == 42)
end

true  # Test passed
