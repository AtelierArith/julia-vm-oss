# findnext/findprev with predicate function and array
# Issue #2109

using Test

@testset "findnext with predicate" begin
    a = [2, 3, 4, 5, 6, 7]

    # Find first odd starting from index 1
    @test findnext(isodd, a, 1) == 2

    # Find first odd starting from index 3
    @test findnext(isodd, a, 3) == 4

    # Find first odd starting from index 6
    @test findnext(isodd, a, 6) == 6

    # No match
    @test findnext(x -> x > 100, a, 1) === nothing

    # Lambda predicate
    @test findnext(x -> x > 4, a, 1) == 4
end

@testset "findprev with predicate" begin
    a = [2, 3, 4, 5, 6, 7]

    # Find last odd up to index 6
    @test findprev(isodd, a, 6) == 6

    # Find last odd up to index 5
    @test findprev(isodd, a, 5) == 4

    # Find last odd up to index 3
    @test findprev(isodd, a, 3) == 2

    # Find last odd up to index 1
    @test findprev(isodd, a, 1) === nothing

    # Lambda predicate
    @test findprev(x -> x < 5, a, 6) == 3
end

@testset "findnext/findprev string form still works" begin
    s = "hello world hello"
    @test findnext('o', s, 1) == 5
    @test findnext('o', s, 6) == 8
    @test findprev('o', s, 17) == 8
    @test findprev('o', s, 7) == 5
end

true
