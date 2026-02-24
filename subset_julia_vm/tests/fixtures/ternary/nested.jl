# Nested ternary operators

using Test

@testset "Nested ternary operators" begin
    x = 3
    y = 5
    @test (x > y ? 1.0 : x == y ? 0.0 : -1.0) == -1.0
end

true  # Test passed
