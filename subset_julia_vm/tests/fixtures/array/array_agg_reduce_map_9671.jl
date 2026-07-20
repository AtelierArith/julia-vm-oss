# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 pilot).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: array/clamp_inplace.jl =====
# Test clamp! - clamp array values in place
# clamp!(a, lo, hi) restricts each element to [lo, hi]


@testset "clamp! function" begin
    # Basic test - clamp values to range
    A = [1.0, 5.0, 10.0, 15.0]
    clamp!(A, 3.0, 12.0)
    @test A[1] == 3.0   # 1.0 clamped up to 3.0
    @test A[2] == 5.0   # 5.0 unchanged (in range)
    @test A[3] == 10.0  # 10.0 unchanged (in range)
    @test A[4] == 12.0  # 15.0 clamped down to 12.0

    # Test with negative values
    B = [-10.0, -5.0, 0.0, 5.0, 10.0]
    clamp!(B, -3.0, 3.0)
    @test B[1] == -3.0
    @test B[2] == -3.0
    @test B[3] == 0.0
    @test B[4] == 3.0
    @test B[5] == 3.0

    # Test with all values in range
    C = [2.0, 3.0, 4.0]
    clamp!(C, 1.0, 5.0)
    @test C[1] == 2.0
    @test C[2] == 3.0
    @test C[3] == 4.0

    # Test with all values below range
    D = [1.0, 2.0, 3.0]
    clamp!(D, 5.0, 10.0)
    @test D[1] == 5.0
    @test D[2] == 5.0
    @test D[3] == 5.0
end

# ===== source: array/cumulative_inplace.jl =====
# Test in-place cumulative functions: cumsum!, cumprod!, accumulate!


@testset "cumsum! basic" begin
    x = [1.0, 2.0, 3.0, 4.0]
    y = zeros(4)
    cumsum!(y, x)
    @test y[1] == 1.0
    @test y[2] == 3.0
    @test y[3] == 6.0
    @test y[4] == 10.0
end

@testset "cumsum! integer values" begin
    x = [1.0, 1.0, 1.0, 1.0, 1.0]
    y = zeros(5)
    cumsum!(y, x)
    @test y[5] == 5.0
end

@testset "cumprod! basic" begin
    x = [1.0, 2.0, 3.0, 4.0]
    y = zeros(4)
    cumprod!(y, x)
    @test y[1] == 1.0
    @test y[2] == 2.0
    @test y[3] == 6.0
    @test y[4] == 24.0
end

@testset "cumprod! with zeros" begin
    x = [2.0, 3.0, 0.0, 5.0]
    y = zeros(4)
    cumprod!(y, x)
    @test y[1] == 2.0
    @test y[2] == 6.0
    @test y[3] == 0.0
    @test y[4] == 0.0
end

@testset "accumulate! with min" begin
    x = [3.0, 1.0, 4.0, 1.0, 5.0]
    y = zeros(5)
    accumulate!(min, y, x)
    @test y[1] == 3.0
    @test y[2] == 1.0
    @test y[3] == 1.0
    @test y[4] == 1.0
    @test y[5] == 1.0
end

@testset "accumulate! with max" begin
    x = [1.0, 3.0, 2.0, 5.0, 4.0]
    y = zeros(5)
    accumulate!(max, y, x)
    @test y[1] == 1.0
    @test y[2] == 3.0
    @test y[3] == 3.0
    @test y[4] == 5.0
    @test y[5] == 5.0
end

# ===== source: array/inplace_reductions.jl =====
# Test in-place reduction functions: sum!, prod!, maximum!, minimum!


@testset "sum! reduce along dim 2 (vector)" begin
    A = [1.0 2.0; 3.0 4.0]
    r = zeros(2)
    sum!(r, A)
    @test r[1] == 3.0  # 1+2
    @test r[2] == 7.0  # 3+4
end

@testset "sum! reduce along dim 1 (row matrix)" begin
    A = [1.0 2.0; 3.0 4.0]
    r = zeros(1, 2)
    sum!(r, A)
    @test r[1, 1] == 4.0  # 1+3
    @test r[1, 2] == 6.0  # 2+4
end

@testset "sum! reduce along dim 2 (column matrix)" begin
    A = [1.0 2.0; 3.0 4.0]
    r = zeros(2, 1)
    sum!(r, A)
    @test r[1, 1] == 3.0  # 1+2
    @test r[2, 1] == 7.0  # 3+4
end

@testset "prod! reduce along dim 2 (vector)" begin
    A = [1.0 2.0; 3.0 4.0]
    r = zeros(2)
    prod!(r, A)
    @test r[1] == 2.0   # 1*2
    @test r[2] == 12.0  # 3*4
end

@testset "prod! reduce along dim 1 (row matrix)" begin
    A = [1.0 2.0; 3.0 4.0]
    r = zeros(1, 2)
    prod!(r, A)
    @test r[1, 1] == 3.0  # 1*3
    @test r[1, 2] == 8.0  # 2*4
end

@testset "maximum! reduce along dim 2 (vector)" begin
    A = [1.0 5.0; 3.0 2.0]
    r = zeros(2)
    maximum!(r, A)
    @test r[1] == 5.0  # max(1, 5)
    @test r[2] == 3.0  # max(3, 2)
end

@testset "maximum! reduce along dim 1 (row matrix)" begin
    A = [1.0 5.0; 3.0 2.0]
    r = zeros(1, 2)
    maximum!(r, A)
    @test r[1, 1] == 3.0  # max(1, 3)
    @test r[1, 2] == 5.0  # max(5, 2)
end

@testset "minimum! reduce along dim 2 (vector)" begin
    A = [1.0 5.0; 3.0 2.0]
    r = zeros(2)
    minimum!(r, A)
    @test r[1] == 1.0  # min(1, 5)
    @test r[2] == 2.0  # min(3, 2)
end

@testset "minimum! reduce along dim 1 (row matrix)" begin
    A = [1.0 5.0; 3.0 2.0]
    r = zeros(1, 2)
    minimum!(r, A)
    @test r[1, 1] == 1.0  # min(1, 3)
    @test r[1, 2] == 2.0  # min(5, 2)
end

# ===== source: array/map_inplace_binary_4019.jl =====

@testset "binary map! for arrays (Issue #4019)" begin
    dest = zeros(Int64, 4)
    a = [1, 2, 3, 4]
    b = [10, 20, 30, 40]
    result = map!((x, y) -> x + y, dest, a, b)
    @test result === dest
    @test dest == [11, 22, 33, 44]

    short_dest = zeros(Int64, 2)
    short_result = map!((x, y) -> x * y, short_dest, [2, 3, 4], [5, 6, 7])
    @test short_result === short_dest
    @test short_dest == [10, 18]

    long_dest = [0, 0, 0, 99]
    map!((x, y) -> x - y, long_dest, [8, 9], [3, 4, 5])
    @test long_dest == [5, 5, 0, 99]

    matrix_dest = zeros(Int64, 2, 2)
    map!((x, y) -> x + y, matrix_dest, [1 2; 3 4], [10 20; 30 40])
    @test matrix_dest == [11 22; 33 44]

    float_dest = zeros(Float64, 3)
    map!((x, y) -> x / y, float_dest, [2, 3, 4], [2, 2, 2])
    @test typeof(float_dest) === Vector{Float64}
    @test float_dest == [1.0, 1.5, 2.0]
end

# ===== source: array/permutedims_inplace.jl =====
# Test permutedims! in-place dimension permutation


@testset "permutedims! 2D transpose" begin
    src = [1.0 2.0; 3.0 4.0]
    dest = zeros(2, 2)
    permutedims!(dest, src, (2, 1))
    @test dest[1, 1] == 1.0
    @test dest[1, 2] == 3.0
    @test dest[2, 1] == 2.0
    @test dest[2, 2] == 4.0
end

@testset "permutedims! 2D identity" begin
    src = [1.0 2.0; 3.0 4.0]
    dest = zeros(2, 2)
    permutedims!(dest, src, (1, 2))
    @test dest[1, 1] == 1.0
    @test dest[1, 2] == 2.0
    @test dest[2, 1] == 3.0
    @test dest[2, 2] == 4.0
end

@testset "permutedims! 2D rectangular" begin
    src = [1.0 2.0 3.0; 4.0 5.0 6.0]
    dest = zeros(3, 2)
    permutedims!(dest, src, (2, 1))
    @test dest[1, 1] == 1.0
    @test dest[2, 1] == 2.0
    @test dest[3, 1] == 3.0
    @test dest[1, 2] == 4.0
    @test dest[2, 2] == 5.0
    @test dest[3, 2] == 6.0
end

@testset "permutedims! 3D array" begin
    src = zeros(2, 3, 4)
    for i in 1:2
        for j in 1:3
            for k in 1:4
                src[i, j, k] = Float64(100 * i + 10 * j + k)
            end
        end
    end
    dest = zeros(3, 2, 4)
    permutedims!(dest, src, (2, 1, 3))
    # src[1,2,3] = 123.0, should be at dest[2,1,3]
    @test dest[2, 1, 3] == 123.0
    # src[2,3,4] = 234.0, should be at dest[3,2,4]
    @test dest[3, 2, 4] == 234.0
end

# ===== source: array/sort_operations.jl =====

@testset "Array sort operations" begin
    @testset "sort (non-mutating)" begin
        # Basic sort
        arr = [3, 1, 4, 1, 5, 9, 2, 6]
        sorted = sort(arr)
        @test sorted == [1, 1, 2, 3, 4, 5, 6, 9]
        @test arr == [3, 1, 4, 1, 5, 9, 2, 6]  # original unchanged

        # Already sorted
        @test sort([1, 2, 3, 4, 5]) == [1, 2, 3, 4, 5]

        # Reverse sorted
        @test sort([5, 4, 3, 2, 1]) == [1, 2, 3, 4, 5]

        # Single element
        @test sort([42]) == [42]

        # Float sort
        arr_f = [3.1, 1.4, 2.7]
        @test sort(arr_f) == [1.4, 2.7, 3.1]
    end

    @testset "sort! (mutating)" begin
        # Basic in-place sort
        arr = [3, 1, 4, 1, 5]
        sort!(arr)
        @test arr == [1, 1, 3, 4, 5]

        # Returns the sorted array (same reference)
        arr2 = [5, 3, 1]
        result = sort!(arr2)
        @test result == [1, 3, 5]
        @test result === arr2
    end

    @testset "sort with rev=true" begin
        arr = [3, 1, 4, 1, 5]
        @test sort(arr, rev=true) == [5, 4, 3, 1, 1]
    end

    @testset "unique" begin
        # Remove duplicates preserving order
        @test unique([1, 2, 1, 3, 2, 4]) == [1, 2, 3, 4]

        # All unique
        @test unique([1, 2, 3]) == [1, 2, 3]

        # All duplicates
        @test unique([5, 5, 5]) == [5]

        # Single element
        @test unique([42]) == [42]
    end
end

true
