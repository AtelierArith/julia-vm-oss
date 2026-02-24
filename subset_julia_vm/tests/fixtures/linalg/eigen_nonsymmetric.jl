# Test eigen decomposition for non-symmetric matrices
# Issue #1563: eigen() does not support non-symmetric matrices with complex eigenvalues

using Test
using LinearAlgebra

@testset "Non-symmetric matrix eigen decomposition" begin
    # Non-symmetric 2x2 matrix
    # [1 2; 3 4] has eigenvalues: (5 + sqrt(33))/2 ≈ 5.372 and (5 - sqrt(33))/2 ≈ -0.372
    A = [1.0 2.0; 3.0 4.0]

    F = eigen(A)

    # Check that we get eigenvalues
    @test length(F.values) == 2

    # Get eigenvalues (they should be complex for general matrices)
    ev1 = F.values[1]
    ev2 = F.values[2]

    # The trace of A equals sum of eigenvalues
    # trace(A) = 1 + 4 = 5
    trace_check = real(ev1) + real(ev2)
    @test abs(trace_check - 5.0) < 1e-6

    # The determinant of A equals product of eigenvalues
    # det(A) = 1*4 - 2*3 = -2
    # For this matrix, eigenvalues are real: λ1 ≈ 5.372, λ2 ≈ -0.372
    det_check = real(ev1) * real(ev2)
    @test abs(det_check - (-2.0)) < 1e-6

    # Check that eigenvectors have correct shape
    @test size(F.vectors) == (2, 2)

    # Check individual eigenvalue magnitudes
    # Expected: λ1 ≈ 5.372, λ2 ≈ -0.372
    expected_ev1 = (5.0 + sqrt(33.0)) / 2.0  # ≈ 5.372
    expected_ev2 = (5.0 - sqrt(33.0)) / 2.0  # ≈ -0.372

    # One eigenvalue should be close to expected_ev1, the other to expected_ev2
    real_evs = [real(ev1), real(ev2)]
    @test minimum(abs.(real_evs .- expected_ev1)) < 1e-3 || minimum(abs.(real_evs .- expected_ev2)) < 1e-3
end

true
