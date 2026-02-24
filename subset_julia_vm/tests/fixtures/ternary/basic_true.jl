# Basic ternary with true condition

using Test

@testset "Basic ternary with true condition" begin
    x = 5
    y = 3
    @test (x > y ? 1.0 : 0.0) == 1.0
end

true  # Test passed
