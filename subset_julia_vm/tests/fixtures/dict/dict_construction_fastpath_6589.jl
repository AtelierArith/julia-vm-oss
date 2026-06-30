# Issue #6619: public Dict construction routes through the pure-Julia
# Dict{K,V} struct methods. The VM still decodes NewDict* for old bytecode/cache
# compatibility, but literals, comprehensions, empty Dict(), and Dict{K,V}()
# should now expose typed Memory-backed Dict structs.

using Test

function public_constructor_routing_ok()
    # literal pairs -> Dict(ps::Pair...)
    d1 = Dict("a" => 1, "b" => 2)
    a1 = typeof(d1) == Dict{String, Int64} &&
         typeof(d1.keys) == Memory{String} &&
         typeof(d1.vals) == Memory{Int64} &&
         length(d1) == 2 && d1["a"] == 1 && d1["b"] == 2

    # comprehension -> Dict(kv)
    d2 = Dict(string(k) => k * 10 for k in 1:3)
    a2 = typeof(d2) == Dict{String, Int64} &&
         typeof(d2.keys) == Memory{String} &&
         typeof(d2.vals) == Memory{Int64} &&
         length(d2) == 3 && d2["1"] == 10 && d2["3"] == 30

    # typed empty -> Dict{K,V}() pure-Julia constructor, then setindex! dispatch
    d3 = Dict{String,Int64}()
    d3["x"] = 5
    a3 = typeof(d3) == Dict{String, Int64} &&
         typeof(d3.keys) == Memory{String} &&
         typeof(d3.vals) == Memory{Int64} &&
         length(d3) == 1 && d3["x"] == 5 &&
         valtype(d3) == Int64 && keytype(d3) == String

    # empty -> Dict() pure-Julia constructor
    d4 = Dict()
    a4 = typeof(d4) == Dict{Any, Any} &&
         typeof(d4.keys) == Memory{Any} &&
         typeof(d4.vals) == Memory{Any} &&
         isempty(d4) && length(d4) == 0
    return a1 && a2 && a3 && a4
end

# Literal and non-literal construction now share the same struct-backed surface.
function literal_matches_iterable_constructor()
    literal = Dict("a" => 1, "b" => 2)
    nonliteral = Dict([("a", 1), ("b", 2)])
    return typeof(literal) == typeof(nonliteral) &&
           typeof(literal.keys) == typeof(nonliteral.keys) &&
           typeof(literal.vals) == typeof(nonliteral.vals) &&
           length(literal) == length(nonliteral) &&
           literal["a"] == nonliteral["a"] &&
           literal["b"] == nonliteral["b"]
end

function getkey_returns_stored_key()
    d = Dict{Any,Int64}(1 => 2)
    stored = getkey(d, 1.0, 0)
    return stored === 1 && !(stored === 1.0)
end

all_ok() = public_constructor_routing_ok() &&
           literal_matches_iterable_constructor() &&
           getkey_returns_stored_key()

@testset "Dict public construction routes to struct methods (#6619)" begin
    @test public_constructor_routing_ok()
    @test literal_matches_iterable_constructor()
    @test getkey_returns_stored_key()
end

all_ok()
