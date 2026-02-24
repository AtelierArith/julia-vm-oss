# ComplexF32 (Complex{Float32}) support test
# Tests Float32 complex arithmetic operations

using Test

@testset "Complex{Float32} basic operations" begin
    # Test constructors
    z1 = Complex{Float32}(Float32(1), Float32(2))
    z2 = Complex{Float32}(Float32(3), Float32(4))

    @test Float64(real(z1)) == 1.0
    @test Float64(imag(z1)) == 2.0
    @test Float64(real(z2)) == 3.0
    @test Float64(imag(z2)) == 4.0

    # Test addition
    z3 = z1 + z2
    @test Float64(real(z3)) == 4.0
    @test Float64(imag(z3)) == 6.0

    # Test subtraction
    z4 = z2 - z1
    @test Float64(real(z4)) == 2.0
    @test Float64(imag(z4)) == 2.0

    # Test multiplication
    # (1+2i) * (3+4i) = 3 + 4i + 6i + 8i^2 = 3 + 10i - 8 = -5 + 10i
    z5 = z1 * z2
    @test Float64(real(z5)) == -5.0
    @test Float64(imag(z5)) == 10.0

    # Test with Float32 scalars
    z6 = z1 + Float32(1)
    @test Float64(real(z6)) == 2.0
    @test Float64(imag(z6)) == 2.0

    z7 = Float32(2) * z1
    @test Float64(real(z7)) == 2.0
    @test Float64(imag(z7)) == 4.0

    # Test zero and one
    z_zero = zero(Complex{Float32})
    @test Float64(real(z_zero)) == 0.0
    @test Float64(imag(z_zero)) == 0.0

    z_one = one(Complex{Float32})
    @test Float64(real(z_one)) == 1.0
    @test Float64(imag(z_one)) == 0.0
end

true
