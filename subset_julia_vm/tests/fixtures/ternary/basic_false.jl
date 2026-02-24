# Basic ternary with false condition

using Test

@testset "Basic ternary with false condition" begin
    x = 3
    y = 5
    @test (x > y ? 1.0 : 0.0) == 0.0
end

true  # Test passed
