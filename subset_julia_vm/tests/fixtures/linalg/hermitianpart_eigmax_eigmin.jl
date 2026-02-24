# hermitianpart, eigmax, eigmin (Issue #1940)

using Test
using LinearAlgebra

@testset "hermitianpart" begin
    # Symmetric matrix: hermitianpart is itself
    S = [1.0 2.0; 2.0 3.0]
    H = hermitianpart(S)
    @test abs(H[1,1] - 1.0) < 1e-10
    @test abs(H[1,2] - 2.0) < 1e-10
    @test abs(H[2,1] - 2.0) < 1e-10
    @test abs(H[2,2] - 3.0) < 1e-10

    # Non-symmetric matrix: (A + A') / 2
    A = [1.0 4.0; 2.0 3.0]
    H2 = hermitianpart(A)
    # H2 = ([1 4; 2 3] + [1 2; 4 3]) / 2 = [1 3; 3 3]
    @test abs(H2[1,1] - 1.0) < 1e-10
    @test abs(H2[1,2] - 3.0) < 1e-10
    @test abs(H2[2,1] - 3.0) < 1e-10
    @test abs(H2[2,2] - 3.0) < 1e-10

    # Result should be symmetric
    @test issymmetric(H2)

    # Identity: hermitianpart is itself
    I2 = [1.0 0.0; 0.0 1.0]
    HI = hermitianpart(I2)
    @test abs(HI[1,1] - 1.0) < 1e-10
    @test abs(HI[1,2] - 0.0) < 1e-10
    @test abs(HI[2,1] - 0.0) < 1e-10
    @test abs(HI[2,2] - 1.0) < 1e-10

    # 3x3 non-symmetric
    B = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]
    HB = hermitianpart(B)
    # HB[1,2] = (2 + 4) / 2 = 3
    @test abs(HB[1,2] - 3.0) < 1e-10
    @test abs(HB[2,1] - 3.0) < 1e-10
    # HB[1,3] = (3 + 7) / 2 = 5
    @test abs(HB[1,3] - 5.0) < 1e-10
    @test abs(HB[3,1] - 5.0) < 1e-10
    # Diagonal unchanged
    @test abs(HB[1,1] - 1.0) < 1e-10
    @test abs(HB[2,2] - 5.0) < 1e-10
    @test abs(HB[3,3] - 9.0) < 1e-10
    # Result should be symmetric
    @test issymmetric(HB)

    # Idempotent: hermitianpart(hermitianpart(A)) == hermitianpart(A)
    HH = hermitianpart(H2)
    @test abs(HH[1,1] - H2[1,1]) < 1e-10
    @test abs(HH[1,2] - H2[1,2]) < 1e-10
    @test abs(HH[2,1] - H2[2,1]) < 1e-10
    @test abs(HH[2,2] - H2[2,2]) < 1e-10
end

@testset "eigmax" begin
    # Identity: max eigenvalue = 1
    I2 = [1.0 0.0; 0.0 1.0]
    @test abs(eigmax(I2) - 1.0) < 1e-10

    # Diagonal matrix: max eigenvalue = max diagonal
    D = [3.0 0.0; 0.0 1.0]
    @test abs(eigmax(D) - 3.0) < 1e-10

    # Symmetric matrix: [2 1; 1 2] has eigenvalues 1 and 3
    S = [2.0 1.0; 1.0 2.0]
    @test abs(eigmax(S) - 3.0) < 1e-10

    # 3x3 diagonal: max eigenvalue = 5
    D3 = [1.0 0.0 0.0; 0.0 5.0 0.0; 0.0 0.0 3.0]
    @test abs(eigmax(D3) - 5.0) < 1e-10
end

@testset "eigmin" begin
    # Identity: min eigenvalue = 1
    I2 = [1.0 0.0; 0.0 1.0]
    @test abs(eigmin(I2) - 1.0) < 1e-10

    # Diagonal matrix: min eigenvalue = min diagonal
    D = [3.0 0.0; 0.0 1.0]
    @test abs(eigmin(D) - 1.0) < 1e-10

    # Symmetric matrix: [2 1; 1 2] has eigenvalues 1 and 3
    S = [2.0 1.0; 1.0 2.0]
    @test abs(eigmin(S) - 1.0) < 1e-10

    # 3x3 diagonal: min eigenvalue = 1
    D3 = [1.0 0.0 0.0; 0.0 5.0 0.0; 0.0 0.0 3.0]
    @test abs(eigmin(D3) - 1.0) < 1e-10
end

true
