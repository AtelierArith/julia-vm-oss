# Test Complex{Float32} explicit type parameter

using Test

@testset "Complex{Float32} explicit type parameter" begin
    # Create Complex{Float32} using explicit type parameter
    z = Complex{Float32}(Float32(3), Float32(4))

    # Test accessor functions
    @test Float64(real(z)) == 3.0
    @test Float64(imag(z)) == 4.0

    # Test addition
    @test (Float64(real(z)) + Float64(imag(z))) == 7.0
end

true
