# Random module test - randcycle, randcycle!, randsubseq, randsubseq!
# Tests cyclic permutation and subsequence sampling (Issue #1919)

using Test
using Random

# Helper: check if sorted array equals 1:n (verifies permutation property)
function is_permutation_of_1_to_n(arr, n)
    if length(arr) != n
        return false
    end
    sorted = sort(arr)
    for i in 1:n
        if sorted[i] != i
            return false
        end
    end
    return true
end

# Helper: check if permutation is a single cycle
# A cyclic permutation has no fixed points (for n > 1) and consists of one cycle
function is_single_cycle(perm)
    n = length(perm)
    if n <= 1
        return true
    end
    # Check no fixed points
    for i in 1:n
        if perm[i] == i
            return false
        end
    end
    # Check single cycle: follow from 1, should visit all n elements
    visited = 0
    pos = 1
    for _ in 1:n
        pos = perm[pos]
        visited = visited + 1
    end
    # After n steps, should return to start
    return pos == 1 && visited == n
end

# Helper: arrays are element-wise equal
function arrays_equal(a, b)
    if length(a) != length(b)
        return false
    end
    for i in 1:length(a)
        if a[i] != b[i]
            return false
        end
    end
    return true
end

# Helper: check all elements of subset are in the original array
function is_subsequence_of(sub, original)
    for i in 1:length(sub)
        found = false
        for j in 1:length(original)
            if sub[i] == original[j]
                found = true
            end
        end
        if !found
            return false
        end
    end
    return true
end

@testset "Random randcycle and randsubseq" begin
    @testset "randcycle basic properties" begin
        Random.seed!(42)
        c = randcycle(5)
        # Should be a permutation of 1:5
        @test length(c) == 5
        @test is_permutation_of_1_to_n(c, 5)
        # Should be a single cycle (no fixed points for n > 1)
        @test is_single_cycle(c)
    end

    @testset "randcycle! fills array" begin
        Random.seed!(77)
        a = zeros(Int64, 6)
        randcycle!(a)
        @test length(a) == 6
        @test is_permutation_of_1_to_n(a, 6)
        @test is_single_cycle(a)
    end

    @testset "randcycle deterministic with seed" begin
        Random.seed!(42)
        c1 = randcycle(5)
        Random.seed!(42)
        c2 = randcycle(5)
        @test arrays_equal(c1, c2)
    end

    @testset "randcycle(0) returns empty" begin
        c = randcycle(0)
        @test length(c) == 0
    end

    @testset "randcycle(1) returns [1]" begin
        c = randcycle(1)
        @test c[1] == 1
        @test length(c) == 1
    end

    @testset "larger randcycle" begin
        Random.seed!(42)
        c = randcycle(10)
        @test length(c) == 10
        @test is_permutation_of_1_to_n(c, 10)
        @test is_single_cycle(c)
    end

    @testset "randsubseq p=0 returns empty" begin
        Random.seed!(42)
        s = randsubseq([1, 2, 3, 4, 5], 0.0)
        @test length(s) == 0
    end

    @testset "randsubseq p=1 returns all elements" begin
        Random.seed!(42)
        original = [10, 20, 30, 40, 50]
        s = randsubseq(original, 1.0)
        @test length(s) == 5
        @test is_subsequence_of(s, original)
    end

    @testset "randsubseq basic properties" begin
        Random.seed!(42)
        original = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
        s = randsubseq(original, 0.5)
        # All elements should come from the original
        @test is_subsequence_of(s, original)
        # Length should be between 0 and n (probabilistic, but seed makes it deterministic)
        @test length(s) >= 0
        @test length(s) <= 10
    end

    @testset "randsubseq! in-place" begin
        Random.seed!(42)
        S = []
        A = [10, 20, 30, 40, 50]
        randsubseq!(S, A, 0.5)
        @test is_subsequence_of(S, A)
    end

    @testset "randsubseq deterministic with seed" begin
        Random.seed!(42)
        s1 = randsubseq([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.5)
        Random.seed!(42)
        s2 = randsubseq([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.5)
        @test arrays_equal(s1, s2)
    end
end

true
