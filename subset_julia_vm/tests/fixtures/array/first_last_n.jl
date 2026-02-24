# Test first(arr, n) and last(arr, n) multi-element variants (Issue #1887)

using Test

@testset "first(arr, n) basic" begin
    arr = [10, 20, 30, 40, 50]
    @test first(arr, 3) == [10, 20, 30]
    @test first(arr, 1) == [10]
    @test first(arr, 5) == [10, 20, 30, 40, 50]
end

@testset "first(arr, n) edge cases" begin
    arr = [10, 20, 30]
    # n > length returns all elements
    @test first(arr, 10) == [10, 20, 30]
    # n == 0 returns empty
    @test length(first(arr, 0)) == 0
end

@testset "first(arr, n) single element" begin
    @test first([42], 1) == [42]
    @test length(first([42], 0)) == 0
end

@testset "first(arr, n) float" begin
    arr = [1.5, 2.5, 3.5, 4.5]
    @test first(arr, 2) == [1.5, 2.5]
end

@testset "last(arr, n) basic" begin
    arr = [10, 20, 30, 40, 50]
    @test last(arr, 3) == [30, 40, 50]
    @test last(arr, 1) == [50]
    @test last(arr, 5) == [10, 20, 30, 40, 50]
end

@testset "last(arr, n) edge cases" begin
    arr = [10, 20, 30]
    # n > length returns all elements
    @test last(arr, 10) == [10, 20, 30]
    # n == 0 returns empty
    @test length(last(arr, 0)) == 0
end

@testset "last(arr, n) single element" begin
    @test last([42], 1) == [42]
    @test length(last([42], 0)) == 0
end

@testset "last(arr, n) float" begin
    arr = [1.5, 2.5, 3.5, 4.5]
    @test last(arr, 2) == [3.5, 4.5]
end

true
