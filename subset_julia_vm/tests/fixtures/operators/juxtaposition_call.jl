# Juxtaposition with function calls: 2f(x) should be 2 * f(x)

using Test

# Define helper functions outside @testset
f(x) = 2x
g(y) = 2f(y)
h(a, b) = 3f(a) + 2f(b)

@testset "Juxtaposition with function calls" begin
    # Basic: 2f(x) = 2 * f(x) = 2 * 2x = 4x
    @test 2f(3) == 12
    @test 2f(5) == 20

    # Nested: g(y) = 2f(y) = 2 * (2y) = 4y
    @test g(3) == 12
    @test g(5) == 20

    # Complex expression: 3f(a) + 2f(b) = 3*2a + 2*2b = 6a + 4b
    @test h(1, 2) == 6 + 8
    @test h(3, 4) == 18 + 16

    # With float literals
    @test 2.0f(3) == 12.0
    @test 3.5f(2) == 14.0
end

true
