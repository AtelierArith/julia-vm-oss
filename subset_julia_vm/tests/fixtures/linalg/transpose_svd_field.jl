# transpose() on matrices extracted from SVD NamedTuple fields (Issue #1922)
# Previously, transpose(F.U) failed with "expected numeric value, got Array"
# because CallTypedDispatch's pattern matching didn't handle Array subtypes.

using Test
using LinearAlgebra

@testset "transpose on SVD NamedTuple fields" begin
    A = [1.0 2.0; 3.0 4.0]
    F = svd(A)

    # Test: transpose of U field from SVD
    U = F.U
    Ut = transpose(U)
    @test size(Ut) == (2, 2)
    @test size(Ut, 1) == size(U, 2)
    @test size(Ut, 2) == size(U, 1)

    # Test: transpose of V field from SVD
    V = F.V
    Vt = transpose(V)
    @test size(Vt) == (2, 2)
    @test size(Vt, 1) == size(V, 2)
    @test size(Vt, 2) == size(V, 1)

    # Test: Ut[i,j] == U[j,i]
    @test abs(Ut[1,1] - U[1,1]) < 1e-10
    @test abs(Ut[1,2] - U[2,1]) < 1e-10
    @test abs(Ut[2,1] - U[1,2]) < 1e-10
    @test abs(Ut[2,2] - U[2,2]) < 1e-10

    # Test: pinv via SVD using transpose(U) directly
    S = F.S
    S_inv = [1.0/S[1], 1.0/S[2]]
    P = V * Diagonal(S_inv) * transpose(U)
    @test size(P) == (2, 2)

    # Test: pinv function also works (uses transpose internally)
    P2 = pinv(A)
    @test size(P2) == (2, 2)
    # Both should give the same result
    @test abs(P[1,1] - P2[1,1]) < 1e-10
    @test abs(P[1,2] - P2[1,2]) < 1e-10
    @test abs(P[2,1] - P2[2,1]) < 1e-10
    @test abs(P[2,2] - P2[2,2]) < 1e-10
end

@testset "transpose on non-square SVD fields" begin
    # Tall matrix
    B = [1.0 2.0; 3.0 4.0; 5.0 6.0]
    F2 = svd(B)
    U2 = F2.U
    Ut2 = transpose(U2)
    @test size(Ut2, 1) == size(U2, 2)
    @test size(Ut2, 2) == size(U2, 1)

    # Wide matrix
    C = [1.0 2.0 3.0; 4.0 5.0 6.0]
    F3 = svd(C)
    V3 = F3.V
    Vt3 = transpose(V3)
    @test size(Vt3, 1) == size(V3, 2)
    @test size(Vt3, 2) == size(V3, 1)
end

true
