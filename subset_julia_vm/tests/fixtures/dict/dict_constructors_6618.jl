using Test

@testset "Dict constructors narrow pair variables to typed Memory storage" begin
    p = "a" => 1
    d = Dict(p)
    @test typeof(d) == Dict{String, Int64}
    @test typeof(d.slots) == Memory{UInt8}
    @test typeof(d.keys) == Memory{String}
    @test typeof(d.vals) == Memory{Int64}
    @test d["a"] == 1
end

@testset "Dict pair splat constructors typejoin key and value types" begin
    p1 = "a" => Int8(1)
    p2 = "b" => Int16(2)
    d = Dict(p1, p2)
    @test typeof(d) == Dict{String, Signed}
    @test typeof(d.keys) == Memory{String}
    @test typeof(d.vals) == Memory{Signed}
    @test typeof(d["a"]) == Int8
    @test typeof(d["b"]) == Int16

    q1 = :a => 1
    q2 = "b" => 2
    mixed = Dict(q1, q2)
    @test typeof(mixed) == Dict{Any, Int64}
    @test typeof(mixed.keys) == Memory{Any}
    @test mixed[:a] == 1
    @test mixed["b"] == 2
end

@testset "Dict iterable constructors narrow from tuple and zip entries" begin
    tuple_entries = [("a", 1), ("b", 2)]
    from_tuples = Dict(tuple_entries)
    @test typeof(from_tuples) == Dict{String, Int64}
    @test typeof(from_tuples.keys) == Memory{String}
    @test typeof(from_tuples.vals) == Memory{Int64}
    @test from_tuples["a"] == 1
    @test from_tuples["b"] == 2

    ks = ["a", "b"]
    vs = [Int16(1), Int16(2)]
    from_zip = Dict(zip(ks, vs))
    @test typeof(from_zip) == Dict{String, Int16}
    @test typeof(from_zip.keys) == Memory{String}
    @test typeof(from_zip.vals) == Memory{Int16}
    @test from_zip["a"] == Int16(1)
    @test from_zip["b"] == Int16(2)
end

true
