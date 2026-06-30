# Characterization test for the linalg result producers flipped off the legacy
# native-array carrier onto the MemoryRef-backed Array{T,N} wrapper (Issue #6807,
# milestone #26). Exercises lu / inv / ldiv / qr / svd / eigen and, critically,
# REUSES each decomposition output downstream (indexing, size, matmul, equality)
# so a regression in the flipped wrapper representation would surface here.

using Test
using LinearAlgebra

@testset "linalg producers return usable Array wrappers (Issue #6807)" begin
    A = [4.0 3.0; 6.0 3.0]

    # --- lu: tuple of three array wrappers, reused downstream ---
    L, U, p = lu(A)
    @test size(L) == (2, 2)
    @test size(U) == (2, 2)
    @test length(p) == 2
    # L is unit lower triangular, U upper triangular
    @test isapprox(L[1, 1], 1.0; atol = 1e-10)
    @test isapprox(L[1, 2], 0.0; atol = 1e-10)
    @test isapprox(U[2, 1], 0.0; atol = 1e-10)
    # Reconstruct: L * U == A[p, :]  (downstream matmul + indexing on wrappers)
    recon = L * U
    Ap = A[p, :]
    @test isapprox(recon[1, 1], Ap[1, 1]; atol = 1e-10)
    @test isapprox(recon[2, 2], Ap[2, 2]; atol = 1e-10)

    # --- inv: single array wrapper, reused in matmul ---
    Ainv = inv(A)
    @test size(Ainv) == (2, 2)
    I2 = A * Ainv
    @test isapprox(I2[1, 1], 1.0; atol = 1e-10)
    @test isapprox(I2[2, 2], 1.0; atol = 1e-10)
    @test isapprox(I2[1, 2], 0.0; atol = 1e-10)

    # --- ldiv (solve): result wrapper used in element access ---
    b = [1.0, 2.0]
    x = A \ b
    @test length(x) == 2
    # A * x == b
    Ax = A * x
    @test isapprox(Ax[1], b[1]; atol = 1e-10)
    @test isapprox(Ax[2], b[2]; atol = 1e-10)

    # --- qr: factor fields are array wrappers ---
    F = qr(A)
    @test size(F.R, 1) == 2
    @test size(F.R, 2) == 2
    @test isapprox(F.R[2, 1], 0.0; atol = 1e-10)

    # --- svd: singular-values field is a wrapper, indexable + ordered ---
    s = svd(A).S
    @test length(s) == 2
    @test s[1] >= s[2]

    # --- eigen on a symmetric matrix: values wrapper indexable ---
    # (eigvals may carry Complex eltype, so read real parts like eigvals_basic)
    S = [2.0 1.0; 1.0 2.0]
    ev = eigvals(S)
    @test length(ev) == 2
    sum_eig = real(ev[1]) + real(ev[2])
    prod_eig = real(ev[1]) * real(ev[2])
    @test isapprox(sum_eig, 4.0; atol = 1e-10)
    @test isapprox(prod_eig, 3.0; atol = 1e-10)
end

true
