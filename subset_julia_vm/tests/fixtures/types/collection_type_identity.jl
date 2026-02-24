# Test type identity for NamedTuple, Dict, and Set
# Ensures typeof() returns the correct type for collection values
# Regression test for Issue #1894

using Test

@testset "Dict type identity" begin
    d = Dict("a" => 1, "b" => 2)
    @test typeof(d) == Dict{Any, Any}
    @test isa(d, Dict)
end

@testset "Set type identity" begin
    s = Set([1, 2, 3])
    @test typeof(s) == Set{Any}
    @test isa(s, Set)
end

@testset "NamedTuple type identity" begin
    nt = (a=1, b=2)
    @test isa(nt, NamedTuple)
end

true
