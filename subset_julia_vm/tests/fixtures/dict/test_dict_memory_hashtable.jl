# Test Dict with Memory-based hash table storage (Issue #2763)
# Verifies that Dict operations work correctly with the open-addressing
# hash table backend (slots::Memory{UInt8}, keys::Memory{K}, vals::Memory{V}).

using Test

@testset "Dict hash table - basic operations" begin
    # Create and populate
    d = Dict("a" => 1, "b" => 2, "c" => 3)
    @test length(d) == 3
    @test haskey(d, "a")
    @test haskey(d, "b")
    @test haskey(d, "c")
    @test !haskey(d, "d")
    @test d["a"] == 1
    @test d["b"] == 2
    @test d["c"] == 3

    # Update existing key
    d["a"] = 100
    @test d["a"] == 100
    @test length(d) == 3

    # Add new key
    d["d"] = 4
    @test d["d"] == 4
    @test length(d) == 4
end

@testset "Dict hash table - delete and rehash" begin
    d = Dict{Int64,Int64}()
    # Insert many entries to trigger rehash
    for i in 1:20
        d[i] = i * 10
    end
    @test length(d) == 20
    @test d[1] == 10
    @test d[10] == 100
    @test d[20] == 200

    # Delete some entries
    delete!(d, 5)
    delete!(d, 10)
    delete!(d, 15)
    @test length(d) == 17
    @test !haskey(d, 5)
    @test !haskey(d, 10)
    @test !haskey(d, 15)
    @test haskey(d, 1)
    @test haskey(d, 20)

    # Insert after delete (may reuse deleted slots)
    d[5] = 55
    @test d[5] == 55
    @test length(d) == 18
end

@testset "Dict hash table - empty and clear" begin
    d = Dict("x" => 1, "y" => 2)
    @test length(d) == 2
    empty!(d)
    @test length(d) == 0
    @test !haskey(d, "x")

    # Can insert after empty
    d["z"] = 3
    @test length(d) == 1
    @test d["z"] == 3
end

@testset "Dict hash table - iteration" begin
    d = Dict(1 => "a", 2 => "b", 3 => "c")
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

@testset "Dict hash table - merge operations" begin
    d1 = Dict("a" => 1, "b" => 2)
    d2 = Dict("b" => 20, "c" => 30)
    d3 = merge(d1, d2)
    @test length(d3) == 3
    @test d3["a"] == 1
    @test d3["b"] == 20
    @test d3["c"] == 30

    # Original dicts unchanged
    @test d1["b"] == 2
    @test length(d1) == 2
end

@testset "Dict hash table - get with default" begin
    d = Dict("a" => 1)
    @test get(d, "a", 0) == 1
    @test get(d, "b", 42) == 42
end

@testset "Dict hash table - pop!" begin
    d = Dict("a" => 1, "b" => 2, "c" => 3)
    v = pop!(d, "b")
    @test v == 2
    @test length(d) == 2
    @test !haskey(d, "b")

    # pop! with default
    v2 = pop!(d, "x", 99)
    @test v2 == 99
    @test length(d) == 2
end

@testset "Dict hash table - copy" begin
    d = Dict("a" => 1, "b" => 2)
    d2 = copy(d)
    d2["c"] = 3
    @test length(d) == 2
    @test length(d2) == 3
    @test !haskey(d, "c")
    @test haskey(d2, "c")
end

true
