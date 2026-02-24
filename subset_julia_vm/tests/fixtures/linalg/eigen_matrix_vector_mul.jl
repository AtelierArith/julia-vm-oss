# Test case from issue #1560: Matrix * complex vector
# This tests matrix multiplication with complex vectors (simplified version)

using Test
using LinearAlgebra

@testset "Matrix * Complex Vector (Issue #1560)" begin
    # Create a simple 2x2 real matrix
    A = [1.0 2.0; 3.0 4.0]

    # Create a complex vector directly (simulating what F.vectors[:, 1] would return)
    z1 = Complex{Float64}(0.5, 0.1)
    z2 = Complex{Float64}(0.8, -0.2)
    v = [z1, z2]

    # Matrix-vector multiplication should work
    result = A * v

    # Check the result length
    @test length(result) == 2

    # Manually compute expected result:
    # result[1] = 1.0*(0.5+0.1i) + 2.0*(0.8-0.2i) = 0.5+0.1i + 1.6-0.4i = 2.1-0.3i
    # result[2] = 3.0*(0.5+0.1i) + 4.0*(0.8-0.2i) = 1.5+0.3i + 3.2-0.8i = 4.7-0.5i
    @test abs(result[1].re - 2.1) < 1e-10
    @test abs(result[1].im - (-0.3)) < 1e-10
    @test abs(result[2].re - 4.7) < 1e-10
    @test abs(result[2].im - (-0.5)) < 1e-10
end

true
