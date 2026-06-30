# Dict string/symbol-key reads via the borrowed-key probe (Issue #5187).
#
# Routing reads through DictValue::get_by_value(_checked) must be observationally
# identical to the previous owned-DictKey path: the borrowed `&str` key must hash
# and compare to the exact same probe bucket as the owned key, for getindex, get,
# get-with-default, getkey, and haskey, across both String and Symbol keys, and
# after enough inserts to force a hash-table rehash.

using Test

# Untyped (Any) parameter forces the IndexLoad runtime path (array_index.rs).
get_via_index(d, key) = d[key]
get_via_index_typed(d::Dict, key) = d[key]

@testset "Dict string-key borrow probe (Issue #5187)" begin
    d = Dict("a" => 1, "bb" => 2, "ccc" => 3)

    # getindex through Any-typed and Dict-typed params
    @test get_via_index(d, "a") == 1
    @test get_via_index(d, "bb") == 2
    @test get_via_index_typed(d, "ccc") == 3
    @test d["a"] == 1

    # get / get-with-default
    @test get(d, "a", -1) == 1
    @test get(d, "zzz", -1) == -1
    @test get(d, "bb", 0) == 2

    # getkey returns the stored key on hit, default on miss
    @test getkey(d, "a", "fallback") == "a"
    @test getkey(d, "nope", "fallback") == "fallback"

    # haskey for present / absent keys
    @test haskey(d, "a")
    @test haskey(d, "ccc")
    @test !haskey(d, "missing")

    # Missing-key getindex raises KeyError (cold error path still works)
    @test_throws KeyError d["missing"]

    # Symbol keys must not collide with same-text String keys.
    ds = Dict(:alpha => 10, :beta => 20)
    @test ds[:alpha] == 10
    @test get(ds, :beta, 0) == 20
    @test haskey(ds, :alpha)
    @test !haskey(ds, :gamma)
    @test getkey(ds, :alpha, :none) == :alpha
end

@testset "Dict string-key borrow probe after rehash (Issue #5187)" begin
    # Insert enough string keys to force at least one rehash, then read every
    # key back through the borrowed probe.
    d = Dict{String,Int}()
    for i in 1:200
        d["key$i"] = i
    end
    @test length(d) == 200
    ok = true
    for i in 1:200
        if d["key$i"] != i
            ok = false
        end
        if get(d, "key$i", -1) != i
            ok = false
        end
        if !haskey(d, "key$i")
            ok = false
        end
    end
    @test ok
    @test !haskey(d, "key201")
    @test get(d, "absent", -7) == -7
end

true
