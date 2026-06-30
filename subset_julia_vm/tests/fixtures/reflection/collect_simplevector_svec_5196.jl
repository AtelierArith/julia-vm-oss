# Issue #5196: collect over a heterogeneous Core.SimpleVector (svec).
#
# `collect(<Type>.parameters)` / `collect(Core.svec(...))` must materialize a
# `Vector{Any}` preserving heterogeneous elements (types and/or values), exactly
# like upstream Julia. Previously sjulia inferred a numeric element type and
# tried to coerce type-object elements, failing with
# "expected numeric value, got DataType". Unlike a Tuple, `collect` over an svec
# always yields `Vector{Any}` (never narrows to a concrete element type).

using Test

@testset "collect over Core.SimpleVector (svec) -> Vector{Any} (Issue #5196)" begin
    # --- Heterogeneous type-parameter svecs via <Type>.parameters ---
    a = collect(Tuple{Int,String}.parameters)
    @test a == Any[Int, String]
    @test typeof(a) === Vector{Any}
    @test a[1] === Int
    @test a[2] === String

    b = collect(Dict{String,Int}.parameters)
    @test b == Any[String, Int]
    @test typeof(b) === Vector{Any}

    # Mixed type + integer value parameter (Array dimensionality N).
    c = collect(Vector{Int}.parameters)
    @test c == Any[Int, 1]
    @test typeof(c) === Vector{Any}
    @test c[1] === Int
    @test c[2] === 1

    # --- Core.svec(...) constructor (mixed value + type elements) ---
    d = collect(Core.svec(1, "a", Int))
    @test d == Any[1, "a", Int]
    @test typeof(d) === Vector{Any}

    # --- Homogeneous numeric svec still collects to Vector{Any} (NOT Vector{Int}) ---
    e = collect(Core.svec(1, 2, 3))
    @test e == Any[1, 2, 3]
    @test typeof(e) === Vector{Any}
    @test eltype(e) === Any

    # --- Homogeneous type svec ---
    f = collect(Core.svec(Int, String))
    @test f == Any[Int, String]
    @test typeof(f) === Vector{Any}

    # --- Empty svec ---
    g = collect(Core.svec())
    @test typeof(g) === Vector{Any}
    @test length(g) == 0

    # --- eltype of an svec is always Any (both constructor and .parameters forms) ---
    @test eltype(Core.svec(Int, String)) === Any
    @test eltype(Tuple{Int,String}.parameters) === Any
    @test eltype(Core.svec(1, 2, 3)) === Any

    # --- setindex! of type objects into a Vector{Any} (the root cause) ---
    h = Vector{Any}(undef, 2)
    h[1] = Int
    h[2] = String
    @test h == Any[Int, String]
end

true
