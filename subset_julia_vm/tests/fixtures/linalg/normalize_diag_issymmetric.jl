# normalize, diag, issymmetric, ishermitian (Issue #1928)

using Test
using LinearAlgebra

@testset "normalize" begin
    # normalize unit vector
    v1 = [3.0, 4.0]
    nv1 = normalize(v1)
    @test abs(norm(nv1) - 1.0) < 1e-10

    # normalize preserves direction
    @test abs(nv1[1] - 0.6) < 1e-10
    @test abs(nv1[2] - 0.8) < 1e-10

    # normalize with p-norm (L1)
    v2 = [1.0, 2.0, 3.0]
    nv2 = normalize(v2, 1)
    expected_l1 = 1.0 + 2.0 + 3.0
    @test abs(nv2[1] - 1.0/expected_l1) < 1e-10
    @test abs(nv2[2] - 2.0/expected_l1) < 1e-10
    @test abs(nv2[3] - 3.0/expected_l1) < 1e-10

    # normalize 3D vector
    v3 = [1.0, 0.0, 0.0]
    nv3 = normalize(v3)
    @test abs(nv3[1] - 1.0) < 1e-10
    @test abs(nv3[2] - 0.0) < 1e-10
    @test abs(nv3[3] - 0.0) < 1e-10
end

@testset "diag" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]

    # Main diagonal
    d0 = diag(A)
    @test length(d0) == 3
    @test abs(d0[1] - 1.0) < 1e-10
    @test abs(d0[2] - 5.0) < 1e-10
    @test abs(d0[3] - 9.0) < 1e-10

    # Super-diagonal (k=1)
    d1 = diag(A, 1)
    @test length(d1) == 2
    @test abs(d1[1] - 2.0) < 1e-10
    @test abs(d1[2] - 6.0) < 1e-10

    # Sub-diagonal (k=-1)
    dm1 = diag(A, -1)
    @test length(dm1) == 2
    @test abs(dm1[1] - 4.0) < 1e-10
    @test abs(dm1[2] - 8.0) < 1e-10

    # k=2 super-diagonal
    d2 = diag(A, 2)
    @test length(d2) == 1
    @test abs(d2[1] - 3.0) < 1e-10

    # k=-2 sub-diagonal
    dm2 = diag(A, -2)
    @test length(dm2) == 1
    @test abs(dm2[1] - 7.0) < 1e-10

    # Non-square matrix
    B = [1.0 2.0 3.0; 4.0 5.0 6.0]
    db = diag(B)
    @test length(db) == 2
    @test abs(db[1] - 1.0) < 1e-10
    @test abs(db[2] - 5.0) < 1e-10
end

@testset "issymmetric" begin
    # Symmetric matrix
    S = [1.0 2.0; 2.0 3.0]
    @test issymmetric(S) == true

    # Non-symmetric matrix
    N = [1.0 2.0; 3.0 4.0]
    @test issymmetric(N) == false

    # 3x3 symmetric
    S3 = [1.0 2.0 3.0; 2.0 5.0 6.0; 3.0 6.0 9.0]
    @test issymmetric(S3) == true

    # Non-square → false
    R = [1.0 2.0 3.0; 4.0 5.0 6.0]
    @test issymmetric(R) == false

    # Identity is symmetric
    I2 = [1.0 0.0; 0.0 1.0]
    @test issymmetric(I2) == true
end

@testset "ishermitian" begin
    # Real symmetric matrix is also Hermitian
    S = [1.0 2.0; 2.0 3.0]
    @test ishermitian(S) == true

    # Non-symmetric real matrix is not Hermitian
    N = [1.0 2.0; 3.0 4.0]
    @test ishermitian(N) == false

    # Identity is Hermitian
    I2 = [1.0 0.0; 0.0 1.0]
    @test ishermitian(I2) == true

    # 3x3 symmetric real → Hermitian
    S3 = [1.0 2.0 3.0; 2.0 5.0 6.0; 3.0 6.0 9.0]
    @test ishermitian(S3) == true
end

true
