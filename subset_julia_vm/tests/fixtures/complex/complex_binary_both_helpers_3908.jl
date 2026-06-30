# Verify that the binary_both fallback paths still dispatch correctly
# after the Value::Array sites in vm/exec/binary_both.rs were routed
# through the legacy_array_ref_from_value helper (Issue #3908).
#
# Exercises the three refactored fallback branches:
#   1. Complex scalar * real array  (complex-array-mul branch)
#   2. Real scalar * real array      (real-array-mul branch)
#   3. Real array * real array       (array-array-matmul branch)

using Test

@testset "Complex scalar * Real array (binary_both helper)" begin
    z = Complex(2.0, -1.0)
    a = [1.0, 2.0, 3.0]

    r1 = z * a
    @test real(r1[1]) == 2.0
    @test imag(r1[1]) == -1.0
    @test real(r1[2]) == 4.0
    @test imag(r1[2]) == -2.0
    @test real(r1[3]) == 6.0
    @test imag(r1[3]) == -3.0

    r2 = a * z
    @test real(r2[1]) == 2.0
    @test imag(r2[1]) == -1.0
    @test real(r2[3]) == 6.0
    @test imag(r2[3]) == -3.0
end

@testset "Real scalar * Real array (binary_both helper)" begin
    s = 3
    a = [1.0, 2.0, 4.0]

    r1 = s * a
    @test r1[1] == 3.0
    @test r1[2] == 6.0
    @test r1[3] == 12.0

    r2 = a * s
    @test r2[1] == 3.0
    @test r2[2] == 6.0
    @test r2[3] == 12.0

    # F64 * Array
    sf = 2.5
    r3 = sf * a
    @test r3[1] == 2.5
    @test r3[2] == 5.0
    @test r3[3] == 10.0
end

@testset "Real Array * Real Array matmul (binary_both helper)" begin
    # 2x2 * Vector{2} via the array-array matmul fallback
    A = [1.0 2.0; 3.0 4.0]
    v = [5.0, 6.0]
    r = A * v
    @test r[1] == 17.0
    @test r[2] == 39.0

    # Identity matrix * Vector
    I2 = [1.0 0.0; 0.0 1.0]
    r2 = I2 * v
    @test r2[1] == 5.0
    @test r2[2] == 6.0
end

true
