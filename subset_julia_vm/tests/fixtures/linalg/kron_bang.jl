# Test kron! (in-place Kronecker product)

using Test
using LinearAlgebra

@testset "kron! matrix-matrix" begin
    A = [1.0 2.0; 3.0 4.0]
    B = [0.0 5.0; 6.0 7.0]
    C = zeros(4, 4)
    kron!(C, A, B)
    # Expected: same as kron(A, B)
    K = kron(A, B)
    for i in 1:4
        for j in 1:4
            @test C[i, j] == K[i, j]
        end
    end
end

@testset "kron! vector-vector" begin
    a = [1.0, 2.0, 3.0]
    b = [4.0, 5.0]
    c = zeros(6)
    kron!(c, a, b)
    k = kron(a, b)
    for i in 1:6
        @test c[i] == k[i]
    end
end

@testset "kron! identity" begin
    I2 = [1.0 0.0; 0.0 1.0]
    A = [2.0 3.0; 4.0 5.0]
    C = zeros(4, 4)
    kron!(C, I2, A)
    # kron(I, A) should be block diagonal with A
    @test C[1, 1] == 2.0
    @test C[1, 2] == 3.0
    @test C[2, 1] == 4.0
    @test C[2, 2] == 5.0
    @test C[3, 3] == 2.0
    @test C[3, 4] == 3.0
    @test C[4, 3] == 4.0
    @test C[4, 4] == 5.0
    @test C[1, 3] == 0.0
    @test C[3, 1] == 0.0
end

true
