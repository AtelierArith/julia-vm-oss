# Test Kronecker product

using Test
using LinearAlgebra

@testset "kron: Kronecker product of matrices and vectors (Issue #349)" begin

    # Test 1: Vector Kronecker product
    a = [1, 2]
    b = [3, 4]
    c = kron(a, b)

    # kron([1,2], [3,4]) = [1*3, 1*4, 2*3, 2*4] = [3, 4, 6, 8]
    result = true
    result = result && length(c) == 4
    result = result && c[1] == 3.0
    result = result && c[2] == 4.0
    result = result && c[3] == 6.0
    result = result && c[4] == 8.0

    # Test 2: 2x2 matrix Kronecker product
    A = [1 2; 3 4]
    B = [1 0; 0 1]
    C = kron(A, B)

    # kron([1 2; 3 4], [1 0; 0 1]) should be 4x4
    result = result && size(C, 1) == 4
    result = result && size(C, 2) == 4

    # Verify specific elements
    # C[1,1] = A[1,1]*B[1,1] = 1*1 = 1
    # C[1,3] = A[1,2]*B[1,1] = 2*1 = 2
    # C[3,1] = A[2,1]*B[1,1] = 3*1 = 3
    result = result && C[1, 1] == 1.0
    result = result && C[1, 3] == 2.0
    result = result && C[3, 1] == 3.0

    @test (result)
end

true  # Test passed
