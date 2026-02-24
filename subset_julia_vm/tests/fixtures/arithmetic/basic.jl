# Basic arithmetic operations

using Test

@testset "Basic arithmetic operations" begin
    a = 5
    b = 10
    @test (a + b) == 15.0
end

true  # Test passed
