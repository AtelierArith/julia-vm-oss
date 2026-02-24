# mul!, ldiv!, rdiv! (Issue #1956)

using Test
using LinearAlgebra

@testset "mul! basic" begin
    A = [1.0 2.0; 3.0 4.0]
    B = [5.0 6.0; 7.0 8.0]
    C = zeros(2, 2)
    mul!(C, A, B)
    # C = A * B = [1*5+2*7, 1*6+2*8; 3*5+4*7, 3*6+4*8] = [19, 22; 43, 50]
    @test abs(C[1, 1] - 19.0) < 1e-10
    @test abs(C[1, 2] - 22.0) < 1e-10
    @test abs(C[2, 1] - 43.0) < 1e-10
    @test abs(C[2, 2] - 50.0) < 1e-10
end

@testset "mul! with alpha and beta" begin
    A = [1.0 0.0; 0.0 1.0]  # identity
    B = [2.0 3.0; 4.0 5.0]
    C = [10.0 10.0; 10.0 10.0]
    # C = 2 * I * B + 3 * C = 2*B + 3*C = [4+30, 6+30; 8+30, 10+30] = [34, 36; 38, 40]
    mul!(C, A, B, 2.0, 3.0)
    @test abs(C[1, 1] - 34.0) < 1e-10
    @test abs(C[1, 2] - 36.0) < 1e-10
    @test abs(C[2, 1] - 38.0) < 1e-10
    @test abs(C[2, 2] - 40.0) < 1e-10
end

@testset "mul! identity" begin
    A = [1.0 0.0; 0.0 1.0]
    B = [7.0 8.0; 9.0 10.0]
    C = zeros(2, 2)
    mul!(C, A, B)
    @test abs(C[1, 1] - 7.0) < 1e-10
    @test abs(C[1, 2] - 8.0) < 1e-10
    @test abs(C[2, 1] - 9.0) < 1e-10
    @test abs(C[2, 2] - 10.0) < 1e-10
end

@testset "ldiv! vector" begin
    # Solve Ax = b where A = [2 0; 0 3], b = [4; 9]
    # x = [2; 3]
    A = [2.0 0.0; 0.0 3.0]
    b = [4.0, 9.0]
    ldiv!(A, b)
    @test abs(b[1] - 2.0) < 1e-10
    @test abs(b[2] - 3.0) < 1e-10
end

@testset "ldiv! general" begin
    # Solve Ax = b where A = [1 2; 3 4], b = [5; 11]
    # x = [1; 2]  (since 1*1+2*2=5, 3*1+4*2=11)
    A = [1.0 2.0; 3.0 4.0]
    b = [5.0, 11.0]
    ldiv!(A, b)
    @test abs(b[1] - 1.0) < 1e-8
    @test abs(b[2] - 2.0) < 1e-8
end

@testset "rdiv! basic" begin
    # Solve XB = A where B = [2 0; 0 3], A = [4 9; 6 12]
    # X = [2 3; 3 4]
    A = [4.0 9.0; 6.0 12.0]
    B = [2.0 0.0; 0.0 3.0]
    rdiv!(A, B)
    @test abs(A[1, 1] - 2.0) < 1e-10
    @test abs(A[1, 2] - 3.0) < 1e-10
    @test abs(A[2, 1] - 3.0) < 1e-10
    @test abs(A[2, 2] - 4.0) < 1e-10
end

true
