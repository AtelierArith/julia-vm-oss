# Test svd function (Singular Value Decomposition)
# svd(A) -> (U=..., S=..., V=..., Vt=...)
# Where A = U * Diagonal(S) * Vt

using Test
using LinearAlgebra

@testset "svd: Singular Value Decomposition (Issue #630)" begin


    # Test with a 3x2 matrix
    A = [1.0 2.0; 3.0 4.0; 5.0 6.0]
    F = svd(A)

    # Access SVD components via named tuple field access
    U = F.U
    S = F.S
    V = F.V
    Vt = F.Vt

    # Test 1: Check U has correct shape (3x2 for thin SVD)
    u_shape_ok = (size(U, 1) == 3 && size(U, 2) == 2) ? 1.0 : 0.0

    # Test 2: Check S is a 1D vector with 2 elements (min(3,2) = 2)
    s_shape_ok = (length(S) == 2) ? 1.0 : 0.0

    # Test 3: Check V has correct shape (2x2)
    v_shape_ok = (size(V, 1) == 2 && size(V, 2) == 2) ? 1.0 : 0.0

    # Test 4: Check Vt has correct shape (2x2, transposed)
    vt_shape_ok = (size(Vt, 1) == 2 && size(Vt, 2) == 2) ? 1.0 : 0.0

    # Test 5: Singular values should be positive and in decreasing order
    s_positive = (S[1] > 0.0 && S[2] > 0.0) ? 1.0 : 0.0
    s_decreasing = (S[1] >= S[2]) ? 1.0 : 0.0

    # Sum: 6 tests passed = 6.0
    result = u_shape_ok + s_shape_ok + v_shape_ok + vt_shape_ok + s_positive + s_decreasing
    println(result)
    @test (result) == 6.0
end

true  # Test passed
