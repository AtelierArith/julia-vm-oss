# Test Complex{Float64} explicit type parameter

using Test

@testset "Complex{Float64}(3, 4) - explicit Float64 type parameter" begin
    z = Complex{Float64}(3, 4)
    @test (real(z) + imag(z)) == 7.0
end

true  # Test passed
