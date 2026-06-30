# Issue #6531: Dict constructors accept non-literal Pair values and iterables
# of pairs, matching upstream Julia's Dict(p::Pair), Dict(ps::Pair...), and
# Dict(kv) constructors.

using Test

@testset "Dict non-literal constructors (#6531)" begin
    p = "a" => 1
    d1 = Dict(p)
    @test d1["a"] == 1
    @test haskey(d1, "a")
    @test length(d1) == 1

    p2 = "b" => 2
    d2 = Dict(p, p2)
    @test d2["a"] == 1
    @test d2["b"] == 2
    @test length(d2) == 2

    pairs_vec = ["a" => 1, "b" => 2]
    d3 = Dict(pairs_vec)
    @test d3["a"] == 1
    @test d3["b"] == 2
    @test length(d3) == 2

    d4 = Dict(zip(["a", "b"], [1, 2]))
    @test d4["a"] == 1
    @test d4["b"] == 2
    @test length(d4) == 2
end

true
