# Issue #6587: public getindex/setindex! resolve through method dispatch first,
# so a user-defined method on a custom type wins, while a primitive Value::Dict
# keeps the fast `CallBuiltin(DictGet/DictSet)` path (the justified primitive
# fallback under the Public Base Routing Rule, Issue #3831 / #3729). Verified
# against upstream Julia 1.12.

using Test

mutable struct Box
    v::Int
end
Base.getindex(b::Box, k) = b.v + k
Base.setindex!(b::Box, val, k) = (b.v = val * 10; b)

# Through Any-typed params so resolution is at runtime.
gi(x) = x[3]
si(x) = (x[1] = 7; x.v)

function user_index_wins()
    b = Box(5)
    a1 = b[3] == 8            # direct getindex
    b[2] = 4
    a2 = b.v == 40           # direct setindex!
    a3 = gi(Box(5)) == 8     # getindex via Any
    a4 = si(Box(0)) == 70    # setindex! via Any
    return a1 && a2 && a3 && a4
end

# Value::Dict keeps its correct fast-path behavior.
function dict_index_ok()
    d = Dict("a" => 1)
    d["b"] = 2
    a1 = d["a"] == 1 && d["b"] == 2
    f(x) = x["a"]            # Any-typed getindex on a Value::Dict
    a2 = f(d) == 1
    return a1 && a2
end

all_ok() = user_index_wins() && dict_index_ok()

@testset "user getindex/setindex! win; Value::Dict fast path intact (#6587)" begin
    @test user_index_wins()
    @test dict_index_ok()
end

all_ok()
