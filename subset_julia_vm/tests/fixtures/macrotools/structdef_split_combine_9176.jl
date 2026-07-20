# Issue #9176: `using MacroTools` must load. Its `structdef.jl` defines
# `combinestructdef` with a do-block quote `:($fieldname::$typ)`, which previously
# failed to lower ("unsupported operator: $") and stopped the whole package from
# loading (reddening 6 fixture chunks on main). This exercises the struct
# split/combine helpers that depend on that code, end to end.
using Test
using MacroTools

@testset "MacroTools splitstructdef / combinestructdef (Issue #9176)" begin
    ex = :(struct Foo
        x::Int
        y::Float64
    end)

    d = MacroTools.splitstructdef(ex)
    @test d[:name] == :Foo
    @test d[:mutable] == false
    @test d[:fields] == [(:x, :Int), (:y, :Float64)]

    # combinestructdef round-trips the split dict back into a `struct` Expr.
    rebuilt = MacroTools.combinestructdef(d)
    @test rebuilt isa Expr
    @test rebuilt.head == :struct

    # A mutable struct with a parametric supertype-free header.
    mex = :(mutable struct Bar
        z::String
    end)
    md = MacroTools.splitstructdef(mex)
    @test md[:mutable] == true
    @test md[:fields] == [(:z, :String)]
end

@testset "MacroTools @capture still matches non-struct patterns (Issue #9176)" begin
    fex = :(f(x) = x + 1)
    matched = @capture(fex, f_(args__) = body_)
    @test matched
    @test f == :f
    @test args == [:x]
end

true
