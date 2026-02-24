# Complex scalar * Real array multiplication
# Tests complex scalar multiplied by real number arrays.
# Related: Issue #1601, #1605

using Test

@testset "Complex scalar * Real array" begin
    z = Complex(2.0, 3.0)
    a = [1.0, 2.0, 3.0]

    result = z * a
    # (2+3i) * 1 = 2+3i
    @test real(result[1]) == 2.0
    @test imag(result[1]) == 3.0
    # (2+3i) * 2 = 4+6i
    @test real(result[2]) == 4.0
    @test imag(result[2]) == 6.0
    # (2+3i) * 3 = 6+9i
    @test real(result[3]) == 6.0
    @test imag(result[3]) == 9.0
end

@testset "Real array * Complex scalar" begin
    z = Complex(2.0, 3.0)
    a = [1.0, 2.0, 3.0]

    result = a * z
    @test real(result[1]) == 2.0
    @test imag(result[1]) == 3.0
    @test real(result[2]) == 4.0
    @test imag(result[2]) == 6.0
end

@testset "Pure imaginary scalar * Real array" begin
    z = Complex(0.0, 1.0)
    a = [1.0, 2.0, 3.0]

    result = z * a
    @test real(result[1]) == 0.0
    @test imag(result[1]) == 1.0
    @test real(result[2]) == 0.0
    @test imag(result[2]) == 2.0
end

true
