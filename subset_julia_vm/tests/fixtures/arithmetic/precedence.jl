# Operator precedence: 2 + 3 * 4 = 2 + 12 = 14

using Test

@testset "Operator precedence" begin
    @test (2 + 3 * 4) == 14.0
end

true  # Test passed
