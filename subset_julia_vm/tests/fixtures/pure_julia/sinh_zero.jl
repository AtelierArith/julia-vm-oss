# sinh(0) = 0

using Test

@testset "sinh(0) = 0 (Pure Julia implementation)" begin
    @test (sinh(0.0)) == 0.0
end

true  # Test passed
