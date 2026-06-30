# Type objects as Dict/Set keys with stable, equality-consistent
# objectid / hash (Issue #5108).
#
# Upstream `objectid`/`hash` integers are NOT portable across versions or
# sessions, so this fixture asserts the OBSERVABLE CONTRACT (which holds
# identically under upstream Julia 1.12), never literal hash values:
#   - equal types hash/objectid equal; distinct types (almost surely) do not
#   - type objects are usable as Dict keys and Set elements
#   - a type key is distinct from instances of that type

using Test

@testset "type objects: hash / objectid consistency (Issue #5108)" begin
    # Equal types hash equal (concrete, parametric, abstract, Union)
    @test hash(Int) === hash(Int)
    @test hash(Float64) === hash(Float64)
    @test hash(Vector{Int}) === hash(Vector{Int})
    @test hash(Pair{Int,String}) === hash(Pair{Int,String})
    @test hash(Number) === hash(Number)

    # Distinct types (almost surely) hash differently
    @test hash(Int) != hash(Float64)
    @test hash(Vector{Int}) != hash(Vector{Float64})
    @test hash(Pair{Int,String}) != hash(Pair{String,Int})

    # objectid mirrors the hash contract
    @test objectid(Int) === objectid(Int)
    @test objectid(Vector{Int}) === objectid(Vector{Int})
    @test objectid(Int) != objectid(Float64)
end

@testset "type objects as Dict keys (Issue #5108)" begin
    # Dict{Type,Int}: insert, lookup, overwrite
    d = Dict{Type,Int}()
    d[Int] = 1
    d[Float64] = 2
    @test d[Int] === 1
    @test d[Float64] === 2
    d[Int] = 10
    @test d[Int] === 10
    @test length(d) == 2

    # Dict literal with type keys
    @test Dict(Int => 1)[Int] === 1
    d2 = Dict(Int => 1, Float64 => 2, String => 3)
    @test d2[String] === 3
    @test length(d2) == 3

    # Parametric / abstract / Union type keys
    dp = Dict{Type,Int}()
    dp[Pair{Int,String}] = 7
    @test dp[Pair{Int,String}] === 7
    @test haskey(dp, Pair{Int,String})
    @test !haskey(dp, Pair{String,Int})

    du = Dict{Type,Int}()
    du[Number] = 1
    du[Union{Int,String}] = 2
    @test du[Number] === 1
    @test du[Union{Int,String}] === 2
    @test haskey(du, Union{Int,String})

    # get (read-only) and delete!
    dg = Dict{Type,Int}(Int => 5, Float64 => 9)
    @test get(dg, Int, -1) === 5
    @test get(dg, String, -1) === -1
    delete!(dg, Int)
    @test !haskey(dg, Int)
    @test length(dg) == 1

    # keys() round-trips the type objects back to usable values
    dk = Dict{Type,Int}(Int => 1, Float64 => 2)
    ks = collect(keys(dk))
    @test Int in ks
    @test Float64 in ks
end

@testset "type objects as Set elements (Issue #5108)" begin
    s = Set{Type}([Int, Float64, Int])
    @test length(s) == 2
    @test Int in s
    @test Float64 in s
    @test !(String in s)
end

@testset "type key distinct from its instances (Issue #5108)" begin
    da = Dict{Any,Int}()
    da[Int] = 100   # the type object as a key
    da[1] = 200     # an instance of that type as a key
    @test da[Int] === 100
    @test da[1] === 200
    @test haskey(da, Int)
    @test length(da) == 2
end

true
