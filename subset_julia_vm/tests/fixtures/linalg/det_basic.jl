# Test det function (determinant)
# det(A) computes the determinant of a square matrix

using Test
using LinearAlgebra

@testset "det: matrix determinant using LU decomposition (Issue #590)" begin


    # Test 2x2 determinant: det([a b; c d]) = ad - bc
    A = [1.0 2.0; 3.0 4.0]
    d1 = det(A)
    # det = 1*4 - 2*3 = -2

    # Test 3x3 identity matrix determinant = 1
    I3 = [1.0 0.0 0.0; 0.0 1.0 0.0; 0.0 0.0 1.0]
    d2 = det(I3)

    # Test 3x3 upper triangular: det = product of diagonal
    U = [2.0 1.0 3.0; 0.0 4.0 5.0; 0.0 0.0 6.0]
    d3 = det(U)
    # det = 2 * 4 * 6 = 48

    # Sum of tests: -2 + 1 + 48 = 47
    result = d1 + d2 + d3
    println(result)
    @test (result) == 47.0
end

true  # Test passed
