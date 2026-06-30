# Pure Julia dispatch verification for sort/sort! (Issue #3725)
#
# After removing the stale BuiltinId::Sort, public `sort` and `sort!`
# calls must route through Pure Julia in base/sort.jl. This fixture
# exercises the public API to ensure dispatch still works for typical
# kwargs (rev, by, lt) and in-place vs copy semantics.

using Test

@testset "sort: routes to Pure Julia base/sort.jl" begin
    @test sort([3, 1, 2]) == [1, 2, 3]
    @test sort([3, 1, 2]; rev=true) == [3, 2, 1]
    @test sort([-3, 1, -2]; by=abs) == [1, -2, -3]
    @test sort([3, 1, 2]; lt=(a, b) -> a > b) == [3, 2, 1]
end

@testset "sort!: mutates and returns the original array" begin
    arr = [3, 1, 2]
    result = sort!(arr)
    @test result === arr
    @test arr == [1, 2, 3]
end

@testset "sort(strings; by=length)" begin
    @test sort(["aa", "b", "ccc"]; by=length) == ["b", "aa", "ccc"]
end

true
