# checksquare, axpy!, axpby!, rmul!, lmul! (Issue #1950)

using Test
using LinearAlgebra

@testset "checksquare" begin
    # 2x2 matrix
    A = [1.0 2.0; 3.0 4.0]
    @test checksquare(A) == 2

    # 3x3 matrix
    B = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]
    @test checksquare(B) == 3

    # 1x1 matrix
    C = reshape([42.0], 1, 1)
    @test checksquare(C) == 1
end

@testset "axpy!" begin
    # Y = a*X + Y
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0, 6.0]
    axpy!(2.0, x, y)
    @test abs(y[1] - 6.0) < 1e-10   # 2*1 + 4 = 6
    @test abs(y[2] - 9.0) < 1e-10   # 2*2 + 5 = 9
    @test abs(y[3] - 12.0) < 1e-10  # 2*3 + 6 = 12

    # With a = 0 (Y unchanged)
    x2 = [10.0, 20.0]
    y2 = [1.0, 2.0]
    axpy!(0.0, x2, y2)
    @test abs(y2[1] - 1.0) < 1e-10
    @test abs(y2[2] - 2.0) < 1e-10

    # With a = -1 (Y = Y - X)
    x3 = [3.0, 5.0]
    y3 = [10.0, 20.0]
    axpy!(-1.0, x3, y3)
    @test abs(y3[1] - 7.0) < 1e-10   # -1*3 + 10 = 7
    @test abs(y3[2] - 15.0) < 1e-10  # -1*5 + 20 = 15
end

@testset "axpby!" begin
    # Y = a*X + b*Y
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0, 6.0]
    axpby!(2.0, x, 3.0, y)
    @test abs(y[1] - 14.0) < 1e-10  # 2*1 + 3*4 = 14
    @test abs(y[2] - 19.0) < 1e-10  # 2*2 + 3*5 = 19
    @test abs(y[3] - 24.0) < 1e-10  # 2*3 + 3*6 = 24

    # With b = 0 (Y = a*X)
    x2 = [5.0, 10.0]
    y2 = [100.0, 200.0]
    axpby!(3.0, x2, 0.0, y2)
    @test abs(y2[1] - 15.0) < 1e-10  # 3*5 + 0*100 = 15
    @test abs(y2[2] - 30.0) < 1e-10  # 3*10 + 0*200 = 30
end

@testset "rmul!" begin
    # Scale vector by scalar
    v = [1.0, 2.0, 3.0, 4.0]
    rmul!(v, 2.0)
    @test abs(v[1] - 2.0) < 1e-10
    @test abs(v[2] - 4.0) < 1e-10
    @test abs(v[3] - 6.0) < 1e-10
    @test abs(v[4] - 8.0) < 1e-10

    # Scale by 0
    w = [5.0, 10.0]
    rmul!(w, 0.0)
    @test abs(w[1]) < 1e-10
    @test abs(w[2]) < 1e-10

    # Scale by negative
    u = [3.0, -2.0]
    rmul!(u, -1.0)
    @test abs(u[1] - (-3.0)) < 1e-10
    @test abs(u[2] - 2.0) < 1e-10
end

@testset "lmul!" begin
    # Scale vector by scalar (left multiply)
    v = [1.0, 2.0, 3.0]
    lmul!(3.0, v)
    @test abs(v[1] - 3.0) < 1e-10
    @test abs(v[2] - 6.0) < 1e-10
    @test abs(v[3] - 9.0) < 1e-10

    # Scale by 0.5
    w = [10.0, 20.0, 30.0]
    lmul!(0.5, w)
    @test abs(w[1] - 5.0) < 1e-10
    @test abs(w[2] - 10.0) < 1e-10
    @test abs(w[3] - 15.0) < 1e-10
end

true
