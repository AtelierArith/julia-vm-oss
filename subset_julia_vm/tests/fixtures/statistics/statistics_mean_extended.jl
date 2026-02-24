# Statistics module test - mean(f, itr) and mean(arr; dims)
# Tests for extended mean variants (Issue #2151)

using Test
using Statistics

square(x) = x * x

@testset "mean(f, itr) - function argument" begin
    # mean of absolute values
    @test mean(abs, [-1.0, -2.0, 3.0]) == 2.0

    # mean of squares
    @test mean(square, [1.0, 2.0, 3.0]) == (1.0 + 4.0 + 9.0) / 3.0

    # mean with identity function
    @test mean(identity, [1.0, 2.0, 3.0, 4.0, 5.0]) == 3.0

    # mean with lambda
    @test mean(x -> x + 1, [1.0, 2.0, 3.0]) == 3.0

    # single element
    @test mean(abs, [-5.0]) == 5.0
end

@testset "mean(arr; dims) - dimensional reduction" begin
    A = [1.0 2.0; 3.0 4.0]

    # dims=1: column means (mean over rows)
    result1 = mean(A; dims=1)
    @test abs(result1[1, 1] - 2.0) < 1e-10
    @test abs(result1[1, 2] - 3.0) < 1e-10

    # dims=2: row means (mean over columns)
    result2 = mean(A; dims=2)
    @test abs(result2[1, 1] - 1.5) < 1e-10
    @test abs(result2[2, 1] - 3.5) < 1e-10

    # 3x3 matrix
    B = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]

    # dims=1: column means
    r1 = mean(B; dims=1)
    @test abs(r1[1, 1] - 4.0) < 1e-10
    @test abs(r1[1, 2] - 5.0) < 1e-10
    @test abs(r1[1, 3] - 6.0) < 1e-10

    # dims=2: row means
    r2 = mean(B; dims=2)
    @test abs(r2[1, 1] - 2.0) < 1e-10
    @test abs(r2[2, 1] - 5.0) < 1e-10
    @test abs(r2[3, 1] - 8.0) < 1e-10
end

true
