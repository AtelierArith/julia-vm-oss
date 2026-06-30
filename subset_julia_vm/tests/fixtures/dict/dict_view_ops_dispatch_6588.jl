# Issue #6588: public keys/values/pairs/merge/copy resolve through method
# dispatch first, so a user-defined method on a custom type wins, while the
# `_dict_*` intrinsics remain the Value::Dict representation boundary. For a real
# Value::Dict these names are already served by the bare `::Dict` Pure Julia
# wrappers in base/dict.jl (keys(d::Dict) = _dict_keys(d), etc.). Verified
# against upstream Julia 1.12.

using Test

mutable struct Box
    v::Int
end
Base.keys(b::Box) = "keys:" * string(b.v)
Base.values(b::Box) = "values:" * string(b.v)
Base.copy(b::Box) = Box(b.v + 100)

ck(x) = keys(x)
cv(x) = values(x)
cc(x) = copy(x).v

function user_view_ops_win()
    a1 = keys(Box(5)) == "keys:5"
    a2 = values(Box(5)) == "values:5"
    a3 = copy(Box(5)).v == 105
    a4 = ck(Box(5)) == "keys:5"      # via Any
    a5 = cv(Box(5)) == "values:5"    # via Any
    a6 = cc(Box(5)) == 105           # via Any
    return a1 && a2 && a3 && a4 && a5 && a6
end

# Value::Dict keys/values/pairs/merge/copy stay correct (Pure Julia bare-::Dict
# wrappers over the _dict_* intrinsics).
function dict_view_ops_ok()
    d = Dict("a" => 1, "b" => 2)
    kc = 0
    for k in keys(d); kc += 1; end
    vs = 0
    for v in values(d); vs += v; end
    ps = 0
    for p in pairs(d); ps += p.second; end
    c = copy(d)
    c["a"] = 100
    m = merge(d, Dict("c" => 3))
    return kc == 2 && vs == 3 && ps == 3 && d["a"] == 1 && c["a"] == 100 && length(m) == 3
end

all_ok() = user_view_ops_win() && dict_view_ops_ok()

@testset "user keys/values/copy win; Value::Dict views intact (#6588)" begin
    @test user_view_ops_win()
    @test dict_view_ops_ok()
end

all_ok()
