# Test matrix * complex vector multiplication
# This should reproduce issue #1560: A * v1 causes a RuntimeError

using Test
using LinearAlgebra

@testset "Matrix * Complex Vector" begin
    # Create a simple 2x2 real matrix
    A = [1.0 2.0; 3.0 4.0]

    # Create a complex vector
    z1 = Complex{Float64}(1.0, 1.0)
    z2 = Complex{Float64}(2.0, -1.0)
    v = [z1, z2]

    # Matrix-vector multiplication should work:
    # A * v = [1*z1 + 2*z2, 3*z1 + 4*z2]
    #       = [1*(1+i) + 2*(2-i), 3*(1+i) + 4*(2-i)]
    #       = [(1+4) + (1-2)*i, (3+8) + (3-4)*i]
    #       = [5 - i, 11 - i]
    result = A * v

    # Check the result length
    @test length(result) == 2

    # Check the first element
    @test abs(result[1].re - 5.0) < 1e-10
    @test abs(result[1].im - (-1.0)) < 1e-10

    # Check the second element
    @test abs(result[2].re - 11.0) < 1e-10
    @test abs(result[2].im - (-1.0)) < 1e-10
end

true
