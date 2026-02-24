# Test complex array broadcast operations (Issue #1591)

using Test

@testset "Complex array broadcast operations" begin
    # Create complex arrays using explicit Complex constructor
    z1 = Complex(1.0, 2.0)
    z2 = Complex(3.0, 4.0)
    a = [z1, z2]

    w1 = Complex(0.5, 1.0)
    w2 = Complex(1.5, 2.0)
    b = [w1, w2]

    # Complex array + Complex array
    result_add = a + b
    @test real(result_add[1]) == 1.5
    @test imag(result_add[1]) == 3.0
    @test real(result_add[2]) == 4.5
    @test imag(result_add[2]) == 6.0

    # Complex array - Complex array
    result_sub = a - b
    @test real(result_sub[1]) == 0.5
    @test imag(result_sub[1]) == 1.0
    @test real(result_sub[2]) == 1.5
    @test imag(result_sub[2]) == 2.0

    # Complex array * Complex array (element-wise)
    # (1+2i) * (0.5+1i) = 0.5 + 1i + 1i + 2i^2 = 0.5 + 2i - 2 = -1.5 + 2i
    # (3+4i) * (1.5+2i) = 4.5 + 6i + 6i + 8i^2 = 4.5 + 12i - 8 = -3.5 + 12i
    result_mul = a .* b
    @test real(result_mul[1]) == -1.5
    @test imag(result_mul[1]) == 2.0
    @test real(result_mul[2]) == -3.5
    @test imag(result_mul[2]) == 12.0
end

true
