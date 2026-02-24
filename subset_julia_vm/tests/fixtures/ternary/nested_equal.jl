# Nested ternary with equal values

using Test

@testset "Nested ternary with equal values" begin
    x = 5
    y = 5
    @test (x > y ? 1.0 : x == y ? 0.0 : -1.0) == 0.0
end

true  # Test passed
