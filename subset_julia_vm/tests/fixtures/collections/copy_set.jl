# Test copy(Set) returns a Set with same elements

using Test

@testset "copy(Set) returns Set" begin
    s = Set([1, 2, 3])
    cs = copy(s)
    @test length(cs) == 3
    @test 1 in cs
    @test 2 in cs
    @test 3 in cs

    # copy of empty set
    empty_s = Set{Int64}()
    empty_cs = copy(empty_s)
    @test length(empty_cs) == 0
end

true
