# Ternary with arithmetic expressions

using Test

@testset "Ternary with arithmetic expressions" begin
    x = 10
    @test (x > 5 ? x * 2 : x / 2) == 20.0
end

true  # Test passed
