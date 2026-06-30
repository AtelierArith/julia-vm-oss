using Test

# Issue #7941: a generic function that guards a field assignment with `isdefined`
# must COMPILE even when no in-scope struct declares the field (the receiver type
# is a generic `where T`, so the concrete field set is unknown at compile time).
# Previously sjulia rejected this at compile time with "Unknown field: __attrs".
mutable struct A7941
    x::Int
end

mutable struct WithAttrs7941
    __attrs::Dict{Symbol,Any}
end

function ensure_attrs(G::T) where T
    if !isdefined(G, :__attrs)
        G.__attrs = Dict()
    end
    return G.__attrs
end

function bump(G::T) where T
    if isdefined(G, :x)
        G.x = G.x + 1
    end
    return G.x
end

@testset "Issue #7941: guarded generic field assignment" begin
    # Defining `ensure_attrs` must not fail at compile time; calling it on a value
    # that already has `:__attrs` takes the guarded (no-assign) branch.
    w = WithAttrs7941(Dict{Symbol,Any}(:a => 1))
    @test ensure_attrs(w)[:a] == 1

    # guarded assignment to an EXISTING field through a generic receiver (the
    # deferred runtime SetFieldByName path) works.
    a = A7941(10)
    @test bump(a) == 11
    @test a.x == 11
end

true
