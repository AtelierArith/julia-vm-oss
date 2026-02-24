# Test matrix-vector multiplication

using Test
using LinearAlgebra

@testset "matrix-vector multiplication" begin
    # 2x2 matrix * 2-vector
    @testset "2x2 * 2-vector" begin
        A = [1.0 2.0; 3.0 4.0]
        x = [1.0, 2.0]
        y = A * x
        @test y[1] == 5.0   # 1*1 + 2*2 = 5
        @test y[2] == 11.0  # 3*1 + 4*2 = 11
        @test length(y) == 2
    end

    # 3x3 identity matrix * 3-vector
    @testset "Identity matrix" begin
        I3 = [1.0 0.0 0.0; 0.0 1.0 0.0; 0.0 0.0 1.0]
        x = [5.0, 6.0, 7.0]
        y = I3 * x
        @test y[1] == 5.0
        @test y[2] == 6.0
        @test y[3] == 7.0
    end

    # Non-square: 2x3 * 3-vector -> 2-vector
    @testset "2x3 * 3-vector" begin
        A = [1.0 2.0 3.0; 4.0 5.0 6.0]
        x = [1.0, 1.0, 1.0]
        y = A * x
        @test y[1] == 6.0   # 1+2+3 = 6
        @test y[2] == 15.0  # 4+5+6 = 15
        @test length(y) == 2
    end

    # Non-square: 3x2 * 2-vector -> 3-vector
    @testset "3x2 * 2-vector" begin
        A = [1.0 2.0; 3.0 4.0; 5.0 6.0]
        x = [1.0, 0.0]
        y = A * x
        @test y[1] == 1.0
        @test y[2] == 3.0
        @test y[3] == 5.0
        @test length(y) == 3
    end

    # Zero matrix
    @testset "Zero matrix" begin
        Z = [0.0 0.0; 0.0 0.0]
        x = [1.0, 2.0]
        y = Z * x
        @test y[1] == 0.0
        @test y[2] == 0.0
    end
end

true
