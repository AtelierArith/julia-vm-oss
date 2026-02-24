# Test QR decomposition (Issue #639)
# Note: SubsetJuliaVM returns thin QR (Q is m x min(m,n), R is min(m,n) x n)
# while Julia's qr() returns compact WY representation with full Q

using LinearAlgebra
using Test

# Test QR on square matrix (where thin and full are the same)
B = [1.0 2.0; 3.0 4.0]
G = qr(B)

# Verify dimensions for square matrix
@test size(G.Q, 1) == 2
@test size(G.Q, 2) == 2
@test size(G.R, 1) == 2
@test size(G.R, 2) == 2

# Verify R is upper triangular
@test isapprox(G.R[2, 1], 0.0, atol=1e-10)

# Verify R diagonal is non-zero (basic sanity check)
@test abs(G.R[1, 1]) > 1e-10
@test abs(G.R[2, 2]) > 1e-10

# Return true to indicate success
true
