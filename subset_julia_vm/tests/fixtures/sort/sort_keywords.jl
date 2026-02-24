# Test sort/sort! with by and rev keyword arguments
# Issue #2011: sort missing by and rev keyword arguments

using Test

@testset "sort with keyword arguments (Issue #2011)" begin
    # Basic sort (no keywords, should still work)
    @test sort([3, 1, 4, 1, 5]) == [1, 1, 3, 4, 5]

    # sort with rev=true
    @test sort([3, 1, 4, 1, 5], rev=true) == [5, 4, 3, 1, 1]

    # sort with by=abs
    @test sort([-3, 1, -2], by=abs) == [1, -2, -3]

    # sort with both by and rev
    @test sort([-3, 1, -2], by=abs, rev=true) == [-3, -2, 1]

    # sort! with rev=true (in-place)
    arr = [5, 3, 1, 4, 2]
    sort!(arr, rev=true)
    @test arr == [5, 4, 3, 2, 1]

    # sort! with by (in-place)
    arr2 = [-5, 2, -1, 4]
    sort!(arr2, by=abs)
    @test arr2 == [-1, 2, 4, -5]

    # sort with rev=false (explicit, should be same as default)
    @test sort([3, 1, 2], rev=false) == [1, 2, 3]

    # Single element
    @test sort([42], rev=true) == [42]
    @test sort([42], by=abs) == [42]
end

true
