# Array / Scalar and Scalar / Array plain division (Issue #1929)
# In Julia, v / n is equivalent to v ./ n (element-wise broadcast)

using Test

@testset "Array / Scalar" begin
    v = [3.0, 4.0]
    n = 5.0

    # Vector / Float64
    r = v / n
    @test abs(r[1] - 0.6) < 1e-10
    @test abs(r[2] - 0.8) < 1e-10

    # Vector / Int64
    r2 = [10.0, 20.0] / 5
    @test abs(r2[1] - 2.0) < 1e-10
    @test abs(r2[2] - 4.0) < 1e-10
end

@testset "Scalar / Array" begin
    v = [2.0, 4.0]

    # Float64 / Vector
    r = 8.0 / v
    @test abs(r[1] - 4.0) < 1e-10
    @test abs(r[2] - 2.0) < 1e-10

    # Int / Vector
    r2 = 12 / [3.0, 4.0]
    @test abs(r2[1] - 4.0) < 1e-10
    @test abs(r2[2] - 3.0) < 1e-10
end

true
