# Mixed Real/Complex array operations
# Tests element-wise operations between real arrays and complex arrays.
# Related: Issue #1605, #1591

using Test

@testset "Real array + Complex array" begin
    a_real = [1.0, 2.0, 3.0]
    a_complex = [Complex(1.0, 2.0), Complex(3.0, 4.0), Complex(5.0, 6.0)]

    result = a_real + a_complex
    # 1.0 + (1+2i) = 2+2i
    @test real(result[1]) == 2.0
    @test imag(result[1]) == 2.0
    # 2.0 + (3+4i) = 5+4i
    @test real(result[2]) == 5.0
    @test imag(result[2]) == 4.0
    # 3.0 + (5+6i) = 8+6i
    @test real(result[3]) == 8.0
    @test imag(result[3]) == 6.0
end

@testset "Complex array + Real array" begin
    a_complex = [Complex(1.0, 2.0), Complex(3.0, 4.0)]
    a_real = [10.0, 20.0]

    result = a_complex + a_real
    @test real(result[1]) == 11.0
    @test imag(result[1]) == 2.0
    @test real(result[2]) == 23.0
    @test imag(result[2]) == 4.0
end

@testset "Real array - Complex array" begin
    a_real = [5.0, 10.0]
    a_complex = [Complex(1.0, 2.0), Complex(3.0, 4.0)]

    result = a_real - a_complex
    # 5.0 - (1+2i) = 4-2i
    @test real(result[1]) == 4.0
    @test imag(result[1]) == -2.0
    # 10.0 - (3+4i) = 7-4i
    @test real(result[2]) == 7.0
    @test imag(result[2]) == -4.0
end

@testset "Complex array - Real array" begin
    a_complex = [Complex(5.0, 3.0), Complex(10.0, 7.0)]
    a_real = [1.0, 2.0]

    result = a_complex - a_real
    @test real(result[1]) == 4.0
    @test imag(result[1]) == 3.0
    @test real(result[2]) == 8.0
    @test imag(result[2]) == 7.0
end

@testset "Real array .* Complex array" begin
    a_real = [2.0, 3.0]
    a_complex = [Complex(1.0, 2.0), Complex(3.0, 4.0)]

    result = a_real .* a_complex
    # 2.0 * (1+2i) = 2+4i
    @test real(result[1]) == 2.0
    @test imag(result[1]) == 4.0
    # 3.0 * (3+4i) = 9+12i
    @test real(result[2]) == 9.0
    @test imag(result[2]) == 12.0
end

true
