# Issue #4722: <DataType>.parameters returns a Core.SimpleVector (svec),
# not a Tuple. Covers svec display, typeof / isa identity, length, indexing,
# iteration, structural ===, == and the Core.svec(...) constructor.

using Test

@testset "Core.SimpleVector (svec) parity for <DataType>.parameters" begin
    # .parameters of a concrete Tuple type is an svec of element types
    p = typeof((1, 2.0, "x")).parameters

    # Display: svec(...)
    @test string(p) == "svec(Int64, Float64, String)"

    # Type identity: typeof(p) === Core.SimpleVector and isa
    @test typeof(p) === Core.SimpleVector
    @test p isa Core.SimpleVector
    @test isa(p, Core.SimpleVector)

    # length
    @test length(p) == 3

    # 1-based indexing returns the element types
    @test p[1] === Int64
    @test p[2] === Float64
    @test p[3] === String

    # iteration yields the elements in order
    collected = []
    for x in p
        push!(collected, x)
    end
    @test length(collected) == 3
    @test collected[1] === Int64
    @test collected[3] === String

    # Dict parameters: svec of (K, V)
    d = Dict{String, Int64}.parameters
    @test string(d) == "svec(String, Int64)"
    @test d[1] === String
    @test d[2] === Int64

    # Structural === (svec has by-content identity in Julia)
    @test (Dict{String, Int64}.parameters === Dict{String, Int64}.parameters)
    @test !(Dict{String, Int64}.parameters === Dict{Int64, String}.parameters)
    @test (typeof((1, 2)).parameters === typeof((3, 4)).parameters)
    @test !(typeof((1, 2)).parameters === typeof((1, 2, 3)).parameters)

    # == compares by content
    @test (typeof((1, 2)).parameters == typeof((3, 4)).parameters)

    # Core.svec(...) constructor (value parameters can be non-type values)
    s = Core.svec(Int64, 2, :sym)
    @test string(s) == "svec(Int64, 2, :sym)"
    @test typeof(s) === Core.SimpleVector
    @test s isa Core.SimpleVector
    @test length(s) == 3
    @test s[2] == 2
    @test (s === Core.svec(Int64, 2, :sym))
    @test (s == Core.svec(Int64, 2, :sym))

    # Empty svec
    e = Core.svec()
    @test string(e) == "svec()"
    @test length(e) == 0
    @test isempty(e)

    # Splat: svec expands like a tuple
    countargs(args...) = length(args)
    @test countargs(p...) == 3

    # The type itself prints fully qualified
    @test string(Core.SimpleVector) == "Core.SimpleVector"
end

true
