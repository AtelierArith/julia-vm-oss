# Test cholesky() Cholesky decomposition (Issue #643)

using LinearAlgebra
using Test

# Test Cholesky on symmetric positive definite matrix
# A = [4 2; 2 3] is SPD (eigenvalues are positive)
A = [4.0 2.0; 2.0 3.0]
F = cholesky(A)

# Verify dimensions
@test size(F.L, 1) == 2
@test size(F.L, 2) == 2
@test size(F.U, 1) == 2
@test size(F.U, 2) == 2

# Verify L is lower triangular (upper-right element should be zero)
@test isapprox(F.L[1, 2], 0.0, atol=1e-10)

# Verify U is upper triangular (lower-left element should be zero)
@test isapprox(F.U[2, 1], 0.0, atol=1e-10)

# Verify L diagonal is positive (property of Cholesky)
@test F.L[1, 1] > 0.0
@test F.L[2, 2] > 0.0

# For A = [4, 2; 2, 3], the Cholesky factor L should be:
# L = [2, 0; 1, sqrt(2)]
# because L * L' = [4, 2; 2, 3]
@test isapprox(F.L[1, 1], 2.0, atol=1e-10)
@test isapprox(F.L[2, 1], 1.0, atol=1e-10)
@test isapprox(F.L[2, 2], sqrt(2.0), atol=1e-10)

# Return true to indicate success
true
