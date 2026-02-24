# tanh(0) = 0

using Test

@testset "tanh(0) = 0 (Pure Julia implementation)" begin
    @test (tanh(0.0)) == 0.0
end

true  # Test passed
