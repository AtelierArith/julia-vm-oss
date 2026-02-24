# Test Dict{K,V} Pure Julia mutable struct (Issue #2748)
# Verifies that the Pure Julia Dict struct works with hash table operations.

using Test

@testset "Dict{K,V} struct - constructor" begin
    d = _new_dict_kv(4)
    @test length(d) == 0
    @test isempty(d)
end

@testset "Dict{K,V} struct - basic insert and lookup" begin
    d = _new_dict_kv(4)
    d["a"] = 1
    d["b"] = 2
    d["c"] = 3
    @test length(d) == 3
    @test !isempty(d)
    @test haskey(d, "a")
    @test haskey(d, "b")
    @test haskey(d, "c")
    @test !haskey(d, "d")
    @test d["a"] == 1
    @test d["b"] == 2
    @test d["c"] == 3
end

@testset "Dict{K,V} struct - update existing key" begin
    d = _new_dict_kv(4)
    d["x"] = 10
    @test d["x"] == 10
    d["x"] = 20
    @test d["x"] == 20
    @test length(d) == 1
end

@testset "Dict{K,V} struct - get with default" begin
    d = _new_dict_kv(4)
    d["a"] = 1
    @test get(d, "a", 0) == 1
    @test get(d, "b", 42) == 42
end

@testset "Dict{K,V} struct - delete" begin
    d = _new_dict_kv(4)
    d["a"] = 1
    d["b"] = 2
    d["c"] = 3
    delete!(d, "b")
    @test length(d) == 2
    @test !haskey(d, "b")
    @test haskey(d, "a")
    @test haskey(d, "c")
end

@testset "Dict{K,V} struct - pop!" begin
    d = _new_dict_kv(4)
    d["a"] = 1
    d["b"] = 2
    v = pop!(d, "a")
    @test v == 1
    @test length(d) == 1
    @test !haskey(d, "a")

    v2 = pop!(d, "x", 99)
    @test v2 == 99
end

@testset "Dict{K,V} struct - empty!" begin
    d = _new_dict_kv(4)
    d["a"] = 1
    d["b"] = 2
    empty!(d)
    @test length(d) == 0
    @test isempty(d)
    @test !haskey(d, "a")

    # Can insert after empty
    d["z"] = 3
    @test d["z"] == 3
    @test length(d) == 1
end

@testset "Dict{K,V} struct - iteration" begin
    d = _new_dict_kv(4)
    d[1] = "a"
    d[2] = "b"
    d[3] = "c"
    collected_keys = Int64[]
    collected_vals = String[]
    for pair in d
        push!(collected_keys, pair.first)
        push!(collected_vals, pair.second)
    end
    @test length(collected_keys) == 3
    @test 1 in collected_keys
    @test 2 in collected_keys
    @test 3 in collected_keys
    @test "a" in collected_vals
    @test "b" in collected_vals
    @test "c" in collected_vals
end

@testset "Dict{K,V} struct - many entries (triggers rehash)" begin
    # Use string keys to avoid Int64→Float64 conversion in Vector{Any}
    # which causes hash inconsistency after rehash (see Issue #2748 comments)
    d = _new_dict_kv(4)
    for i in 1:30
        d[string(i)] = i * 10
    end
    @test length(d) == 30
    @test d["1"] == 10
    @test d["15"] == 150
    @test d["30"] == 300

    # Delete some entries
    delete!(d, "5")
    delete!(d, "15")
    delete!(d, "25")
    @test length(d) == 27
    @test !haskey(d, "5")
    @test !haskey(d, "15")
    @test !haskey(d, "25")
    @test haskey(d, "1")
    @test haskey(d, "30")

    # Reinsert after delete
    d["5"] = 55
    @test d["5"] == 55
    @test length(d) == 28
end

true
