using Test

# Issue #5233: collecting non-array container elements (Dict / Pair) into a
# typed result array must keep the heap value verbatim instead of routing it
# through the numeric element-store arm (which failed with
# "expected numeric value, got Dict" or silently kept only a Pair's `.first`).
#
# Pairs are heap-allocated in sjulia, so identity/value comparison against a
# fresh Pair literal (`===` / `==`) is intentionally avoided here — the element
# correctness is asserted through `.first` / `.second` projection instead.

pair_eq(p, a, b) = (p isa Pair) && p.first == a && p.second == b

@testset "map over a named function returning a Dict (#5233)" begin
    f(x) = Dict(x => x * x)
    r = map(f, [1, 2, 3])
    @test length(r) == 3
    @test r[1] == Dict(1 => 1)
    @test r[2] == Dict(2 => 4)
    @test r[3] == Dict(3 => 9)
    @test r[1] isa Dict
end

@testset "filter over an array of Dicts (#5233)" begin
    r = filter(d -> haskey(d, 1), [Dict(1 => 1), Dict(2 => 2)])
    @test length(r) == 1
    @test r[1] == Dict(1 => 1)
end

@testset "map / collect / comprehension producing a Vector of Pairs (#5233)" begin
    g(x) = x => x * x
    r = map(g, [1, 2, 3])
    @test length(r) == 3
    @test pair_eq(r[1], 1, 1)
    @test pair_eq(r[2], 2, 4)
    @test pair_eq(r[3], 3, 9)
    @test all(p -> p isa Pair, r)

    gen = collect(x => x * x for x in [1, 2, 3])
    @test length(gen) == 3
    @test pair_eq(gen[1], 1, 1)
    @test pair_eq(gen[3], 3, 9)

    comp = [k => v for (k, v) in [10 => 100, 20 => 200]]
    @test length(comp) == 2
    @test pair_eq(comp[1], 10, 100)
    @test pair_eq(comp[2], 20, 200)
end

@testset "collect(::Dict) yields a Vector of Pairs (#5233)" begin
    r = collect(Dict(1 => 10))
    @test length(r) == 1
    @test r[1] isa Pair
    @test pair_eq(r[1], 1, 10)
end

@testset "typed Pair / Dict array literals (#5233)" begin
    p = Pair{Int,Int}[1 => 2, 3 => 4]
    @test typeof(p) === Vector{Pair{Int64,Int64}}
    @test length(p) == 2
    @test pair_eq(p[1], 1, 2)
    @test pair_eq(p[2], 3, 4)

    # Bare-`Pair` eltype literal stores Pairs verbatim (no numeric coercion).
    pb = Pair[1 => 2, 3 => 4]
    @test length(pb) == 2
    @test pair_eq(pb[1], 1, 2)
    @test pb[1] isa Pair

    # Untyped Pair vector literal.
    pu = [1 => 2, 3 => 4]
    @test length(pu) == 2
    @test pair_eq(pu[1], 1, 2)
    @test pair_eq(pu[2], 3, 4)

    # Typed Dict array literal keeps each Dict element verbatim.
    dd = Dict{Int,Int}[Dict(1 => 1), Dict(2 => 2)]
    @test length(dd) == 2
    @test dd[1] == Dict(1 => 1)
    @test dd[2] == Dict(2 => 2)
end

true
