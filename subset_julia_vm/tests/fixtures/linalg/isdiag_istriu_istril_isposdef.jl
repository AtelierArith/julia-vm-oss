# isdiag, istriu, istril, isposdef (Issue #1938)

using Test
using LinearAlgebra

@testset "isdiag" begin
    # Diagonal matrix
    D = [1.0 0.0; 0.0 2.0]
    @test isdiag(D) == true

    # Non-diagonal matrix
    A = [1.0 2.0; 3.0 4.0]
    @test isdiag(A) == false

    # Identity matrix
    I2 = [1.0 0.0; 0.0 1.0]
    @test isdiag(I2) == true

    # Zero matrix
    Z = [0.0 0.0; 0.0 0.0]
    @test isdiag(Z) == true

    # 3x3 diagonal
    D3 = [1.0 0.0 0.0; 0.0 2.0 0.0; 0.0 0.0 3.0]
    @test isdiag(D3) == true

    # 3x3 with one off-diagonal element
    A3 = [1.0 0.0 0.0; 0.0 2.0 0.0; 0.0 1.0 3.0]
    @test isdiag(A3) == false
end

@testset "istriu" begin
    # Upper triangular matrix
    U = [1.0 2.0; 0.0 3.0]
    @test istriu(U) == true

    # Non-upper-triangular
    A = [1.0 2.0; 3.0 4.0]
    @test istriu(A) == false

    # Diagonal is upper triangular
    D = [1.0 0.0; 0.0 2.0]
    @test istriu(D) == true

    # Identity is upper triangular
    I2 = [1.0 0.0; 0.0 1.0]
    @test istriu(I2) == true

    # 3x3 upper triangular
    U3 = [1.0 2.0 3.0; 0.0 4.0 5.0; 0.0 0.0 6.0]
    @test istriu(U3) == true

    # 3x3 not upper triangular
    A3 = [1.0 2.0 3.0; 0.0 4.0 5.0; 1.0 0.0 6.0]
    @test istriu(A3) == false
end

@testset "istril" begin
    # Lower triangular matrix
    L = [1.0 0.0; 2.0 3.0]
    @test istril(L) == true

    # Non-lower-triangular
    A = [1.0 2.0; 3.0 4.0]
    @test istril(A) == false

    # Diagonal is lower triangular
    D = [1.0 0.0; 0.0 2.0]
    @test istril(D) == true

    # Identity is lower triangular
    I2 = [1.0 0.0; 0.0 1.0]
    @test istril(I2) == true

    # 3x3 lower triangular
    L3 = [1.0 0.0 0.0; 2.0 3.0 0.0; 4.0 5.0 6.0]
    @test istril(L3) == true

    # 3x3 not lower triangular
    A3 = [1.0 0.0 1.0; 2.0 3.0 0.0; 4.0 5.0 6.0]
    @test istril(A3) == false
end

@testset "isposdef" begin
    # Identity is positive definite
    I2 = [1.0 0.0; 0.0 1.0]
    @test isposdef(I2) == true

    # Positive definite symmetric matrix
    P = [2.0 1.0; 1.0 2.0]
    @test isposdef(P) == true

    # Not positive definite (negative eigenvalue)
    N = [1.0 2.0; 2.0 1.0]
    @test isposdef(N) == false

    # Not symmetric
    A = [1.0 2.0; 3.0 4.0]
    @test isposdef(A) == false

    # Zero matrix is not positive definite
    Z = [0.0 0.0; 0.0 0.0]
    @test isposdef(Z) == false

    # 3x3 positive definite
    P3 = [4.0 2.0 1.0; 2.0 5.0 3.0; 1.0 3.0 6.0]
    @test isposdef(P3) == true

    # Diagonal with all positive entries is positive definite
    D = [3.0 0.0; 0.0 5.0]
    @test isposdef(D) == true
end

true
