# cosh(0) = 1

using Test

@testset "cosh(0) = 1 (Pure Julia implementation)" begin
    @test (cosh(0.0)) == 1.0
end

true  # Test passed
