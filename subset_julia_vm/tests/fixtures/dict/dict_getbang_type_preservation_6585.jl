# Issue #6585: get!(d, key, default) on a Value::Dict reached through an
# Any-typed binding must preserve the value type of its result. It was being
# coerced to Float64 — silently wrong for numbers (99 -> 99.0) and a runtime
# error for non-numbers — because a bare `ValueType::Dict` bridged to the
# inference lattice with a `Float64` default value type, and the `get!` tfunc
# returns the dict's value type. Verified against upstream Julia 1.12.

using Test

# new key -> inserts and returns default (type preserved)
new_int(d) = get!(d, "z", 99)
new_str(d) = get!(d, "z", "hi")
# existing key -> returns stored value (type preserved)
ex_int(d) = get!(d, "a", 0)
ex_str(d) = get!(d, "a", "x")

ti() = typeof(new_int(Dict("a" => 7))) == Int64 && new_int(Dict("a" => 7)) == 99
ts() = typeof(new_str(Dict("a" => "q"))) == String && new_str(Dict("a" => "q")) == "hi"
ei() = typeof(ex_int(Dict("a" => 7))) == Int64 && ex_int(Dict("a" => 7)) == 7
es() = typeof(ex_str(Dict("a" => "q"))) == String && ex_str(Dict("a" => "q")) == "q"

# persistence: the inserted entry persists in the bound dict (Issue #5225)
function persists()
    d = Dict("a" => 1)
    r = get!(d, "z", 5)
    return r == 5 && haskey(d, "z") && d["z"] == 5 && typeof(d["z"]) == Int64
end

all_ok() = ti() && ts() && ei() && es() && persists()

@testset "get! return-type preservation through Any binding (#6585)" begin
    @test ti()
    @test ts()
    @test ei()
    @test es()
    @test persists()
end

# Final value gates the in-harness nextest run on correctness, not just no-throw.
all_ok()
