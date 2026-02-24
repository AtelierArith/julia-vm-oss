# Phase 4-4: Set algebra operations in Pure Julia (Issue #2575)
# Tests union, intersect, setdiff, symdiff, issubset, isdisjoint, issetequal

using Test

@testset "Set algebra operations - Pure Julia" begin
    s1 = Set([1, 2, 3])
    s2 = Set([2, 3, 4])

    # union
    u = union(s1, s2)
    @test length(u) == 4
    @test 1 in u
    @test 2 in u
    @test 3 in u
    @test 4 in u

    # intersect
    i = intersect(s1, s2)
    @test length(i) == 2
    @test 2 in i
    @test 3 in i
    @test !(1 in i)
    @test !(4 in i)

    # setdiff
    d = setdiff(s1, s2)
    @test length(d) == 1
    @test 1 in d
    @test !(2 in d)

    # symdiff
    sd = symdiff(s1, s2)
    @test length(sd) == 2
    @test 1 in sd
    @test 4 in sd
    @test !(2 in sd)
    @test !(3 in sd)

    # issubset
    @test issubset(Set([2, 3]), s1)
    @test !issubset(s1, Set([2, 3]))
    @test issubset(Set(), s1)
    @test issubset(s1, s1)

    # isdisjoint
    @test isdisjoint(Set([5, 6]), s1)
    @test !isdisjoint(s1, s2)
    @test isdisjoint(Set(), s1)

    # issetequal
    @test issetequal(s1, Set([3, 1, 2]))
    @test !issetequal(s1, s2)
    @test issetequal(Set(), Set())
end

true
