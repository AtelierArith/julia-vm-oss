using Test
using LinearAlgebra

function _close(a, b)
    return abs(a - b) < 1.0e-8
end

@testset "Sylvester uses array unary minus (Issue #7577)" begin
    @test -[1.0, 2.0] == [-1.0, -2.0]

    A = [1.0 0.0; 0.0 2.0]
    B = [3.0 0.0; 0.0 4.0]
    C = [8.0 15.0; 25.0 36.0]
    X = sylvester(A, B, C)

    @test _close(X[1, 1], -2.0)
    @test _close(X[1, 2], -3.0)
    @test _close(X[2, 1], -5.0)
    @test _close(X[2, 2], -6.0)
    @test A * X + X * B == -C
end

true
