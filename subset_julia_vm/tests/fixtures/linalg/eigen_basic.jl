# Test eigen() eigendecomposition (Issue #???)

using LinearAlgebra
using Test

# Use a symmetric matrix with real eigenvalues.
A = [2.0 1.0; 1.0 2.0]
e = eigen(A)

vals = e.values
vecs = e.vectors

@test length(vals) == 2
@test size(vecs, 1) == 2
@test size(vecs, 2) == 2

# Validate A * v ≈ λ * v for each eigenpair.
for i in 1:2
    v1 = vecs[1, i]
    v2 = vecs[2, i]
    λ = vals[i]

    lhs1 = A[1, 1] * v1 + A[1, 2] * v2
    lhs2 = A[2, 1] * v1 + A[2, 2] * v2

    rhs1 = λ * v1
    rhs2 = λ * v2

    @test isapprox(lhs1, rhs1, atol=1e-8)
    @test isapprox(lhs2, rhs2, atol=1e-8)
end

true
