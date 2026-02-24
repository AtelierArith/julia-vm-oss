# triu, tril, diagm, opnorm (Issue #1932)

using Test
using LinearAlgebra

@testset "triu" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]

    # Main diagonal (k=0)
    U = triu(A)
    @test abs(U[1,1] - 1.0) < 1e-10
    @test abs(U[1,2] - 2.0) < 1e-10
    @test abs(U[1,3] - 3.0) < 1e-10
    @test abs(U[2,1] - 0.0) < 1e-10
    @test abs(U[2,2] - 5.0) < 1e-10
    @test abs(U[2,3] - 6.0) < 1e-10
    @test abs(U[3,1] - 0.0) < 1e-10
    @test abs(U[3,2] - 0.0) < 1e-10
    @test abs(U[3,3] - 9.0) < 1e-10

    # First superdiagonal (k=1)
    U1 = triu(A, 1)
    @test abs(U1[1,1] - 0.0) < 1e-10
    @test abs(U1[1,2] - 2.0) < 1e-10
    @test abs(U1[1,3] - 3.0) < 1e-10
    @test abs(U1[2,2] - 0.0) < 1e-10
    @test abs(U1[2,3] - 6.0) < 1e-10
    @test abs(U1[3,3] - 0.0) < 1e-10

    # First subdiagonal (k=-1)
    Um1 = triu(A, -1)
    @test abs(Um1[1,1] - 1.0) < 1e-10
    @test abs(Um1[2,1] - 4.0) < 1e-10
    @test abs(Um1[2,2] - 5.0) < 1e-10
    @test abs(Um1[3,1] - 0.0) < 1e-10
    @test abs(Um1[3,2] - 8.0) < 1e-10
    @test abs(Um1[3,3] - 9.0) < 1e-10
end

@testset "tril" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]

    # Main diagonal (k=0)
    L = tril(A)
    @test abs(L[1,1] - 1.0) < 1e-10
    @test abs(L[1,2] - 0.0) < 1e-10
    @test abs(L[1,3] - 0.0) < 1e-10
    @test abs(L[2,1] - 4.0) < 1e-10
    @test abs(L[2,2] - 5.0) < 1e-10
    @test abs(L[2,3] - 0.0) < 1e-10
    @test abs(L[3,1] - 7.0) < 1e-10
    @test abs(L[3,2] - 8.0) < 1e-10
    @test abs(L[3,3] - 9.0) < 1e-10

    # First subdiagonal (k=-1)
    Lm1 = tril(A, -1)
    @test abs(Lm1[1,1] - 0.0) < 1e-10
    @test abs(Lm1[2,1] - 4.0) < 1e-10
    @test abs(Lm1[2,2] - 0.0) < 1e-10
    @test abs(Lm1[3,1] - 7.0) < 1e-10
    @test abs(Lm1[3,2] - 8.0) < 1e-10
    @test abs(Lm1[3,3] - 0.0) < 1e-10

    # First superdiagonal (k=1)
    L1 = tril(A, 1)
    @test abs(L1[1,1] - 1.0) < 1e-10
    @test abs(L1[1,2] - 2.0) < 1e-10
    @test abs(L1[1,3] - 0.0) < 1e-10
    @test abs(L1[2,1] - 4.0) < 1e-10
    @test abs(L1[2,2] - 5.0) < 1e-10
    @test abs(L1[2,3] - 6.0) < 1e-10
    @test abs(L1[3,3] - 9.0) < 1e-10
end

@testset "diagm" begin
    v = [1.0, 2.0, 3.0]
    D = diagm(v)
    @test size(D) == (3, 3)
    @test abs(D[1,1] - 1.0) < 1e-10
    @test abs(D[2,2] - 2.0) < 1e-10
    @test abs(D[3,3] - 3.0) < 1e-10
    @test abs(D[1,2] - 0.0) < 1e-10
    @test abs(D[2,1] - 0.0) < 1e-10
    @test abs(D[1,3] - 0.0) < 1e-10
    @test abs(D[3,1] - 0.0) < 1e-10

    # diagm is inverse of diag
    v2 = diag(D)
    @test abs(v2[1] - v[1]) < 1e-10
    @test abs(v2[2] - v[2]) < 1e-10
    @test abs(v2[3] - v[3]) < 1e-10

    # Single element
    D1 = diagm([5.0])
    @test size(D1) == (1, 1)
    @test abs(D1[1,1] - 5.0) < 1e-10
end

@testset "opnorm" begin
    # Identity matrix: opnorm = 1.0
    I2 = [1.0 0.0; 0.0 1.0]
    @test abs(opnorm(I2) - 1.0) < 1e-10

    # opnorm with p=2 (default): largest singular value
    A = [3.0 0.0; 0.0 4.0]
    @test abs(opnorm(A) - 4.0) < 1e-10
    @test abs(opnorm(A, 2) - 4.0) < 1e-10

    # opnorm with p=1: max absolute column sum
    B = [1.0 2.0; 3.0 4.0]
    # col1 sum = |1| + |3| = 4, col2 sum = |2| + |4| = 6
    @test abs(opnorm(B, 1) - 6.0) < 1e-10

    # opnorm with p=Inf: max absolute row sum
    # row1 sum = |1| + |2| = 3, row2 sum = |3| + |4| = 7
    @test abs(opnorm(B, Inf) - 7.0) < 1e-10
end

true
