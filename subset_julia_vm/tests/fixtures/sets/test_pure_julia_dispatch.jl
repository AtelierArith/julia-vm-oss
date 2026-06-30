# Pure Julia dispatch verification for Set algebra (Issue #3724)
#
# After removing the BuiltinId::Set{Union,Intersect,Setdiff,Symdiff,Issubset,
# Isdisjoint,Issetequal} variants and the corresponding `compile_builtin_set`
# short-circuit, Set algebra calls must be served by Pure Julia methods in
# base/set.jl for Set, Vector, and mixed Set/Vector argument forms.

using Test

@testset "union: Set + Vector + mixed" begin
    @test issetequal(union(Set([1, 2, 3]), Set([2, 3, 4])), Set([1, 2, 3, 4]))
    @test union([1, 2, 3], [3, 4, 5]) == [1, 2, 3, 4, 5]
    @test issetequal(union(Set([1, 2]), [2, 3]), Set([1, 2, 3]))
    @test issetequal(union([1, 2], Set([2, 3])), Set([1, 2, 3]))
end

@testset "intersect / setdiff / symdiff" begin
    @test issetequal(intersect(Set([1, 2, 3]), Set([2, 3, 4])), Set([2, 3]))
    @test intersect([1, 2, 3], [2, 3, 4]) == [2, 3]

    @test issetequal(setdiff(Set([1, 2, 3]), Set([2])), Set([1, 3]))
    @test setdiff([1, 2, 3], [2]) == [1, 3]

    @test issetequal(symdiff(Set([1, 2, 3]), Set([3, 4, 5])), Set([1, 2, 4, 5]))
    @test symdiff([1, 2, 3], [3, 4, 5]) == [1, 2, 4, 5]
end

@testset "issubset / isdisjoint / issetequal" begin
    @test issubset(Set([1, 2]), Set([1, 2, 3]))
    @test issubset([1, 2], [1, 2, 3])
    @test issubset(Set([1, 2]), [1, 2, 3])
    @test issubset([1, 2], Set([1, 2, 3]))

    @test isdisjoint(Set([1, 2]), Set([3, 4]))
    @test !isdisjoint(Set([1, 2]), [2, 3])
    @test !isdisjoint([1, 2], Set([2, 3]))

    @test issetequal(Set([1, 2, 3]), Set([3, 2, 1]))
    @test issetequal([1, 2, 3], [3, 2, 1])
    @test issetequal(Set([1, 2, 3]), [3, 2, 1])
end

@testset "user override wins over Pure Julia (regression detector)" begin
    # If `union(::Set, ::Set)` were still routed to a Rust builtin, this
    # custom method would not be honoured.
    Base.union(s1::Set, s2::Set) = Set([99])
    overridden = union(Set([1, 2]), Set([3, 4]))
    @test 99 in overridden
    @test length(overridden) == 1
end

true
