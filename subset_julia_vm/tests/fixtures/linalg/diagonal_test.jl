# Test Diagonal type from LinearAlgebra
# Tests basic Diagonal functionality: construction, indexing, and multiplication

using LinearAlgebra
using Test

@testset "Diagonal type basic operations" begin
    # Test 1: Create a Diagonal matrix
    D = Diagonal([1.0, 2.0, 3.0])
    @test size(D, 1) == 3
    @test size(D, 2) == 3
    
    # Test 2: Indexing - diagonal elements
    @test D[1, 1] == 1.0
    @test D[2, 2] == 2.0
    @test D[3, 3] == 3.0
    
    # Test 3: Indexing - off-diagonal elements should be zero
    @test D[1, 2] == 0.0
    @test D[2, 1] == 0.0
    @test D[1, 3] == 0.0
    @test D[3, 1] == 0.0
end

@testset "Diagonal matrix multiplication" begin
    D = Diagonal([1.0, 2.0, 3.0])
    
    # Test 4: Diagonal * Matrix
    A = [1.0 2.0; 3.0 4.0; 5.0 6.0]  # 3×2 matrix
    result = D * A
    @test size(result) == (3, 2)
    @test result[1, 1] == 1.0 * 1.0  # D[1,1] * A[1,1]
    @test result[1, 2] == 1.0 * 2.0  # D[1,1] * A[1,2]
    @test result[2, 1] == 2.0 * 3.0  # D[2,2] * A[2,1]
    @test result[2, 2] == 2.0 * 4.0  # D[2,2] * A[2,2]
    @test result[3, 1] == 3.0 * 5.0  # D[3,3] * A[3,1]
    @test result[3, 2] == 3.0 * 6.0  # D[3,3] * A[3,2]
    
    # Test 5: Matrix * Diagonal
    B = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2×3 matrix
    result2 = B * D
    @test size(result2) == (2, 3)
    @test result2[1, 1] == 1.0 * 1.0  # B[1,1] * D[1,1]
    @test result2[1, 2] == 2.0 * 2.0  # B[1,2] * D[2,2]
    @test result2[1, 3] == 3.0 * 3.0  # B[1,3] * D[3,3]
    @test result2[2, 1] == 4.0 * 1.0  # B[2,1] * D[1,1]
    @test result2[2, 2] == 5.0 * 2.0  # B[2,2] * D[2,2]
    @test result2[2, 3] == 6.0 * 3.0  # B[2,3] * D[3,3]
    
    # Test 6: Diagonal * Diagonal
    D1 = Diagonal([1.0, 2.0])
    D2 = Diagonal([3.0, 4.0])
    result3 = D1 * D2
    @test isa(result3, Diagonal)
    @test result3.diag[1] == 1.0 * 3.0
    @test result3.diag[2] == 2.0 * 4.0
end

@testset "Diagonal with SVD" begin
    # Test 7: Diagonal(S) * Vt (as in user's example)
    S = [1.0, 2.0, 3.0]
    Vt = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]  # 3×3 matrix
    DS = Diagonal(S)
    result = DS * Vt
    @test size(result) == (3, 3)
    @test result[1, 1] == 1.0 * 1.0  # S[1] * Vt[1,1]
    @test result[2, 2] == 2.0 * 5.0  # S[2] * Vt[2,2]
    @test result[3, 3] == 3.0 * 9.0  # S[3] * Vt[3,3]
end

true  # Test passed
