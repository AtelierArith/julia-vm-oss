# Test sortperm/sortperm!/issorted with by, rev, lt keyword arguments
# Issue #2015: sortperm and issorted missing keyword argument support

using Test

@testset "sortperm with keyword arguments (Issue #2015)" begin
    # Basic sortperm (no keywords)
    @test sortperm([3, 1, 2]) == [2, 3, 1]

    # sortperm with rev=true
    @test sortperm([3, 1, 2], rev=true) == [1, 3, 2]

    # sortperm with by=abs
    @test sortperm([-3, 1, -2], by=abs) == [2, 3, 1]

    # sortperm with both by and rev
    @test sortperm([-3, 1, -2], by=abs, rev=true) == [1, 3, 2]

    # sortperm! with rev=true
    perm = collect(1:3)
    sortperm!(perm, [3, 1, 2], rev=true)
    @test perm == [1, 3, 2]

    # Single element
    @test sortperm([42], rev=true) == [1]
end

@testset "issorted with keyword arguments (Issue #2015)" begin
    # Basic issorted
    @test issorted([1, 2, 3]) == true
    @test issorted([3, 2, 1]) == false

    # issorted with rev=true
    @test issorted([3, 2, 1], rev=true) == true
    @test issorted([1, 2, 3], rev=true) == false

    # issorted with by=abs
    @test issorted([1, -2, 3], by=abs) == true
    @test issorted([3, -2, 1], by=abs) == false

    # issorted with by and rev combined
    @test issorted([3, -2, 1], by=abs, rev=true) == true

    # Single element
    @test issorted([42], rev=true) == true
end

true
