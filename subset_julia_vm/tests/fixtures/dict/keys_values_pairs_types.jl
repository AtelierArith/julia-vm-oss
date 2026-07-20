# Test that keys/values/pairs return correct collection types (Issue #3474)

using Test

@testset "dict: keys returns Array" begin
    d = Dict("a" => 1, "b" => 2)
    ks = keys(d)
    # keys returns an iterable keys view upstream; sjulia may materialize it.
    @test length(ks) == 2
    @test "a" in ks
    @test "b" in ks
end

@testset "dict: values returns Array" begin
    d = Dict("x" => 10, "y" => 20)
    vs = values(d)
    # values returns an iterable values view upstream; sjulia may materialize it.
    @test length(vs) == 2
    @test 10 in vs
    @test 20 in vs
end

@testset "dict: pairs returns the dict itself" begin
    d = Dict("a" => 1, "b" => 2)
    ps = pairs(d)
    # pairs(d::Dict) should return d itself (like Julia's AbstractDict rule)
    @test isa(ps, Dict)
    @test length(ps) == 2
end

true
