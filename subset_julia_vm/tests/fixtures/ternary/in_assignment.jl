# Ternary used in variable assignment

using Test

@testset "Ternary used in variable assignment" begin
    x = 10
    result = x > 5 ? 100.0 : 0.0
    @test (result) == 100.0
end

true  # Test passed
