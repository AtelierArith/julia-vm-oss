# nullspace, logdet, logabsdet, adjoint (Issue #1934)

using Test
using LinearAlgebra

@testset "nullspace" begin
    # Rank-deficient matrix: [[1,0],[0,0]] has null space span([0,1])
    A = [1.0 0.0; 0.0 0.0]
    N = nullspace(A)
    @test size(N, 1) == 2
    @test size(N, 2) == 1
    # The null vector should be [0, 1] or [0, -1]
    @test abs(abs(N[2, 1]) - 1.0) < 1e-10
    @test abs(N[1, 1]) < 1e-10

    # Full rank matrix: no null space
    B = [1.0 0.0; 0.0 1.0]
    N2 = nullspace(B)
    @test size(N2, 2) == 0

    # Rank-1 3x3 matrix: null space dimension = 2
    C = [1.0 2.0 3.0; 2.0 4.0 6.0; 3.0 6.0 9.0]
    N3 = nullspace(C)
    @test size(N3, 1) == 3
    @test size(N3, 2) == 2
    # Verify A * N ≈ 0
    R = C * N3
    for i in 1:3
        for j in 1:2
            @test abs(R[i, j]) < 1e-8
        end
    end
end

@testset "logdet" begin
    # logdet of identity = 0
    I2 = [1.0 0.0; 0.0 1.0]
    @test abs(logdet(I2) - 0.0) < 1e-10

    # logdet of diagonal matrix
    D = [2.0 0.0; 0.0 3.0]
    @test abs(logdet(D) - log(6.0)) < 1e-10

    # logdet of known matrix: det([1 2; 3 4]) = -2, so logdet returns NaN
    A = [1.0 2.0; 3.0 4.0]
    @test isnan(logdet(A))
end

@testset "logabsdet" begin
    # logabsdet of identity
    I2 = [1.0 0.0; 0.0 1.0]
    result = logabsdet(I2)
    @test abs(result[1] - 0.0) < 1e-10
    @test abs(result[2] - 1.0) < 1e-10

    # logabsdet of positive determinant matrix
    D = [2.0 0.0; 0.0 3.0]
    result2 = logabsdet(D)
    @test abs(result2[1] - log(6.0)) < 1e-10
    @test abs(result2[2] - 1.0) < 1e-10

    # logabsdet of negative determinant matrix: det([1 2; 3 4]) = -2
    A = [1.0 2.0; 3.0 4.0]
    result3 = logabsdet(A)
    @test abs(result3[1] - log(2.0)) < 1e-10
    @test abs(result3[2] - (-1.0)) < 1e-10
end

@testset "adjoint" begin
    # Real matrix: adjoint = transpose
    A = [1.0 2.0; 3.0 4.0]
    B = adjoint(A)
    @test abs(B[1,1] - 1.0) < 1e-10
    @test abs(B[1,2] - 3.0) < 1e-10
    @test abs(B[2,1] - 2.0) < 1e-10
    @test abs(B[2,2] - 4.0) < 1e-10

    # Verify adjoint dimensions
    C = [1.0 2.0 3.0; 4.0 5.0 6.0]
    D = adjoint(C)
    @test size(D) == (3, 2)
    @test abs(D[1,1] - 1.0) < 1e-10
    @test abs(D[2,1] - 2.0) < 1e-10
    @test abs(D[3,1] - 3.0) < 1e-10
    @test abs(D[1,2] - 4.0) < 1e-10
    @test abs(D[2,2] - 5.0) < 1e-10
    @test abs(D[3,2] - 6.0) < 1e-10

    # Square symmetric: adjoint = transpose = self
    S = [1.0 2.0; 2.0 1.0]
    SA = adjoint(S)
    @test abs(SA[1,1] - 1.0) < 1e-10
    @test abs(SA[1,2] - 2.0) < 1e-10
    @test abs(SA[2,1] - 2.0) < 1e-10
    @test abs(SA[2,2] - 1.0) < 1e-10
end

true
