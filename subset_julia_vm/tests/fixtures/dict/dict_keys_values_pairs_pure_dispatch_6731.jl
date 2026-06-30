# Issue #6731 (slice 1): keys/values/pairs on a Dict route through the pure-Julia
# Dict{K,V} methods (base/dict.jl) with no Value::Dict builtin fallback — their
# BASE_FUNCTION_ROUTES entries are now markers (no BuiltinOp). Verified to work
# both statically and through a dynamic (Any-typed) function barrier, and on
# other keys/values/pairs-supporting types. Values vs julia 1.12.

using Test

@testset "keys/values/pairs on Dict — pure dispatch (Issue #6731)" begin
    d = Dict(:a => 1, :b => 2, :c => 3)
    @test sort(collect(keys(d))) == [:a, :b, :c]
    @test sort(collect(values(d))) == [1, 2, 3]
    @test length(pairs(d)) == 3
    collected = Dict(k => v for (k, v) in pairs(d))   # round-trip via pairs iteration
    @test collected == d
end

@testset "keys/values/pairs through a dynamic (Any) barrier (Issue #6731)" begin
    probe(x) = (sort(collect(keys(x))), sort(collect(values(x))), length(pairs(x)))
    d = Dict("x" => 10, "y" => 20)
    k, v, n = probe(d)
    @test k == ["x", "y"]
    @test v == [10, 20]
    @test n == 2
end

@testset "keys on a NamedTuple still works (Issue #6731)" begin
    nt = (a = 1, b = 2)
    @test keys(nt) == (:a, :b)
    @test values(nt) == (1, 2)
end

true
