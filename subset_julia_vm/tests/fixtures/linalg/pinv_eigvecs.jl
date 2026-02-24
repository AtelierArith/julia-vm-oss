# LinearAlgebra test - pinv and eigvecs (Issue #1921)

using Test
using LinearAlgebra

@testset "pinv and eigvecs" begin
    @testset "pinv: square matrix" begin
        A = [1.0 2.0; 3.0 4.0]
        Ap = pinv(A)
        # A * pinv(A) * A ≈ A
        @test isapprox(A * Ap * A, A)
        # pinv(A) * A * pinv(A) ≈ pinv(A)
        @test isapprox(Ap * A * Ap, Ap)
    end

    @testset "pinv: tall matrix (overdetermined)" begin
        A = [1.0 2.0; 3.0 4.0; 5.0 6.0]
        Ap = pinv(A)
        # Shape: pinv of 3x2 should be 2x3
        @test size(Ap, 1) == 2
        @test size(Ap, 2) == 3
        # A * pinv(A) * A ≈ A
        @test isapprox(A * Ap * A, A)
    end

    @testset "pinv: wide matrix (underdetermined)" begin
        A = [1.0 2.0 3.0; 4.0 5.0 6.0]
        Ap = pinv(A)
        # Shape: pinv of 2x3 should be 3x2
        @test size(Ap, 1) == 3
        @test size(Ap, 2) == 2
        # A * pinv(A) * A ≈ A
        @test isapprox(A * Ap * A, A)
    end

    @testset "pinv: identity matrix" begin
        I3 = [1.0 0.0 0.0; 0.0 1.0 0.0; 0.0 0.0 1.0]
        Ip = pinv(I3)
        # pinv(I) = I
        @test isapprox(Ip, I3)
    end

    @testset "pinv: invertible matrix matches inv" begin
        A = [2.0 1.0; 1.0 3.0]
        @test isapprox(pinv(A), inv(A))
    end

    @testset "eigvecs: symmetric matrix" begin
        A = [2.0 1.0; 1.0 3.0]
        V = eigvecs(A)
        # Should return a 2x2 matrix
        @test size(V, 1) == 2
        @test size(V, 2) == 2
        # Columns should be unit vectors (for symmetric matrices)
        # Check norm of each column ≈ 1
        col1_norm = sqrt(V[1,1]*V[1,1] + V[2,1]*V[2,1])
        col2_norm = sqrt(V[1,2]*V[1,2] + V[2,2]*V[2,2])
        @test isapprox(col1_norm, 1.0)
        @test isapprox(col2_norm, 1.0)
    end

    @testset "eigvecs: 3x3 matrix" begin
        A = [4.0 1.0 0.0; 1.0 3.0 1.0; 0.0 1.0 2.0]
        V = eigvecs(A)
        @test size(V, 1) == 3
        @test size(V, 2) == 3
    end

    @testset "eigvecs: matches eigen().vectors" begin
        A = [3.0 1.0; 1.0 2.0]
        V1 = eigvecs(A)
        F = eigen(A)
        V2 = F.vectors
        @test isapprox(V1, V2)
    end
end

true
