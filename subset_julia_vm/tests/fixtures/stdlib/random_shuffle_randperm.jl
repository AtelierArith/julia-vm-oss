# Random module test - shuffle, shuffle!, randperm, randperm!
# Tests permutation properties with deterministic seeding (Issue #1917)

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

# Helper: check array contains exactly the expected elements (sorted)
function has_same_elements(arr, expected)
    if length(arr) != length(expected)
        return false
    end
    s1 = sort(arr)
    s2 = sort(expected)
    for i in 1:length(s1)
        if s1[i] != s2[i]
            return false
        end
    end
    return true
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

@testset "Random shuffle and randperm" begin
    @testset "shuffle! basic properties" begin
        Random.seed!(42)
        a = [1, 2, 3, 4, 5]
        shuffle!(a)
        # After shuffle, array should still contain the same elements
        @test length(a) == 5
        @test is_permutation_of_1_to_n(a, 5)
    end

    @testset "shuffle returns new array" begin
        Random.seed!(123)
        original = [10, 20, 30, 40, 50]
        shuffled = shuffle(original)
        # Original should be unchanged
        @test original[1] == 10
        @test original[5] == 50
        # Shuffled should contain same elements
        @test length(shuffled) == 5
        @test has_same_elements(shuffled, [10, 20, 30, 40, 50])
    end

    @testset "shuffle! single element" begin
        a = [42]
        shuffle!(a)
        @test a[1] == 42
    end

    @testset "shuffle! two elements" begin
        Random.seed!(99)
        a = [1, 2]
        shuffle!(a)
        @test has_same_elements(a, [1, 2])
    end

    @testset "randperm basic properties" begin
        Random.seed!(42)
        p = randperm(5)
        # Should be a permutation of 1:5
        @test length(p) == 5
        @test is_permutation_of_1_to_n(p, 5)
    end

    @testset "randperm! fills array" begin
        Random.seed!(77)
        a = zeros(Int64, 6)
        randperm!(a)
        # Should be a permutation of 1:6
        @test length(a) == 6
        @test is_permutation_of_1_to_n(a, 6)
    end

    @testset "randperm deterministic with seed" begin
        Random.seed!(42)
        p1 = randperm(5)
        Random.seed!(42)
        p2 = randperm(5)
        # Same seed should produce same permutation
        @test arrays_equal(p1, p2)
    end

    @testset "shuffle deterministic with seed" begin
        Random.seed!(42)
        s1 = shuffle([1, 2, 3, 4, 5])
        Random.seed!(42)
        s2 = shuffle([1, 2, 3, 4, 5])
        @test arrays_equal(s1, s2)
    end

    @testset "randperm(0) returns empty" begin
        p = randperm(0)
        @test length(p) == 0
    end

    @testset "randperm(1) returns [1]" begin
        p = randperm(1)
        @test p[1] == 1
        @test length(p) == 1
    end

    @testset "larger randperm" begin
        Random.seed!(42)
        p = randperm(10)
        @test length(p) == 10
        @test is_permutation_of_1_to_n(p, 10)
    end
end

true
