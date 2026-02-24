# Test real scalar * complex array multiplication

using Test

@testset "Real scalar * Complex array multiplication" begin
    # Create complex array using explicit Complex constructor
    z1 = Complex(1.0, 2.0)
    z2 = Complex(3.0, 4.0)
    a = [z1, z2]

    # Float64 scalar * Complex array
    c = 2.0
    result = c * a
    @test real(result[1]) == 2.0
    @test imag(result[1]) == 4.0
    @test real(result[2]) == 6.0
    @test imag(result[2]) == 8.0

    # Complex array * Float64 scalar (commutative)
    result2 = a * c
    @test real(result2[1]) == 2.0
    @test imag(result2[1]) == 4.0
    @test real(result2[2]) == 6.0
    @test imag(result2[2]) == 8.0

    # Int64 scalar * Complex array
    n = 3
    result3 = n * a
    @test real(result3[1]) == 3.0
    @test imag(result3[1]) == 6.0
    @test real(result3[2]) == 9.0
    @test imag(result3[2]) == 12.0
end

true
