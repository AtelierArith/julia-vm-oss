# Statistics module test - Pure Julia implementations
# Tests for mean, var, std, median, quantile, cov, cor

using Test
using Statistics

@testset "Statistics functions" begin
    @testset "mean" begin
        # Basic mean
        arr = [1.0, 2.0, 3.0, 4.0, 5.0]
        @test mean(arr) == 3.0

        # Single element
        @test mean([42.0]) == 42.0

        # Integer array (promoted to float)
        arr_int = [1, 2, 3, 4, 5]
        @test mean(arr_int) == 3.0
    end

    @testset "var (sample variance)" begin
        # Basic variance with Bessel's correction
        arr = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]
        # Mean = 5, sum of squared deviations = 32
        # Variance = 32 / 7 ≈ 4.571428...
        v = var(arr)
        @test abs(v - 4.571428571428571) < 0.0001
    end

    @testset "varm (variance with known mean)" begin
        arr = [1.0, 2.0, 3.0, 4.0, 5.0]
        m = 3.0  # known mean
        v = varm(arr, m)
        # Sum of squared deviations from 3: (1-3)^2 + (2-3)^2 + (3-3)^2 + (4-3)^2 + (5-3)^2 = 4 + 1 + 0 + 1 + 4 = 10
        # Variance = 10 / 4 = 2.5
        @test v == 2.5
    end

    @testset "std (sample standard deviation)" begin
        arr = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]
        s = std(arr)
        # std = sqrt(var) ≈ sqrt(4.571428...) ≈ 2.138...
        @test abs(s - 2.138089935299395) < 0.0001
    end

    @testset "stdm (std with known mean)" begin
        arr = [1.0, 2.0, 3.0, 4.0, 5.0]
        m = 3.0
        s = stdm(arr, m)
        # sqrt(2.5) ≈ 1.5811
        @test abs(s - sqrt(2.5)) < 0.0001
    end

    @testset "median" begin
        # Odd length - middle element
        arr_odd = [1.0, 3.0, 5.0, 7.0, 9.0]
        @test median(arr_odd) == 5.0

        # Even length - average of two middle elements
        arr_even = [1.0, 3.0, 5.0, 7.0]
        @test median(arr_even) == 4.0

        # Unsorted array
        arr_unsorted = [9.0, 1.0, 5.0, 3.0, 7.0]
        @test median(arr_unsorted) == 5.0

        # Single element
        @test median([42.0]) == 42.0
    end

    @testset "middle" begin
        # Middle of two numbers
        @test middle(2.0, 6.0) == 4.0
        @test middle(0.0, 10.0) == 5.0
        @test middle(-5.0, 5.0) == 0.0
    end

    @testset "quantile" begin
        arr = [1.0, 2.0, 3.0, 4.0, 5.0]

        # Min (0th percentile)
        @test quantile(arr, 0.0) == 1.0

        # Max (100th percentile)
        @test quantile(arr, 1.0) == 5.0

        # Median (50th percentile)
        @test quantile(arr, 0.5) == 3.0

        # 25th percentile (interpolated)
        q25 = quantile(arr, 0.25)
        @test abs(q25 - 2.0) < 0.0001

        # 75th percentile (interpolated)
        q75 = quantile(arr, 0.75)
        @test abs(q75 - 4.0) < 0.0001
    end

    @testset "cov (covariance)" begin
        x = [1.0, 2.0, 3.0, 4.0, 5.0]
        y = [2.0, 4.0, 6.0, 8.0, 10.0]

        # Perfect positive correlation: cov = 2.5 * 2 = 5.0
        # Actually: mean_x = 3, mean_y = 6
        # sum((xi - 3)(yi - 6)) = (-2)(-4) + (-1)(-2) + 0*0 + 1*2 + 2*4 = 8 + 2 + 0 + 2 + 8 = 20
        # cov = 20 / 4 = 5.0
        c = cov(x, y)
        @test c == 5.0

        # Covariance of a single vector (same as variance)
        @test cov(x) == var(x)
    end

    @testset "cor (Pearson correlation)" begin
        x = [1.0, 2.0, 3.0, 4.0, 5.0]
        y = [2.0, 4.0, 6.0, 8.0, 10.0]

        # Perfect positive correlation
        r = cor(x, y)
        @test abs(r - 1.0) < 0.0001

        # Negative correlation
        y_neg = [10.0, 8.0, 6.0, 4.0, 2.0]
        r_neg = cor(x, y_neg)
        @test abs(r_neg - (-1.0)) < 0.0001

        # Correlation of vector with itself is 1
        @test cor(x) == 1.0
    end
end

true
