# User-defined parametric type invariance (Issue #5052)
#
# Julia parametric DataTypes are invariant in their parameters: `Foo{Int} <:
# Foo{Number}` is false even though `Int <: Number`. This must hold for
# user-defined parametric structs, not only builtins like `Array`.
#
# This fixture also pins the covariant-base relationship through a parametric
# abstract supertype: a parametric struct is a subtype of the *bare* abstract
# supertype (`Box{Int} <: AbstractBox`), while invariance still forbids
# `Box{Int} <: AbstractBox{Number}`. Verified against upstream Julia 1.12.

using Test

abstract type AbstractBox{T} end
struct Box{T} <: AbstractBox{T}
    value::T
end
struct Pair2{A,B}
    a::A
    b::B
end

h(::Box{Number}) = "number"
h(::Box{Int}) = "int"

@testset "parametric invariance + abstract supertype (Issue #5052)" begin
    # Invariance: Foo{Int} <: Foo{Number} is false (the core of #5052)
    @test !(Box{Int} <: Box{Number})
    @test Box{Int} <: Box{Int}
    @test !(Box{Number} <: Box{Int})

    # Multi-parameter invariance
    @test !(Pair2{Int,Int} <: Pair2{Number,Number})
    @test Pair2{Int,Int} <: Pair2{Int,Int}
    @test !(Pair2{Int,Number} <: Pair2{Int,Int})

    # Nested invariance
    @test !(Box{Box{Int}} <: Box{Box{Number}})

    # Parametric struct <: bare parametric abstract supertype
    @test Box{Int} <: AbstractBox
    @test Box <: AbstractBox

    # Invariance still holds against a parametric abstract supertype
    @test Box{Int} <: AbstractBox{Int}
    @test !(Box{Int} <: AbstractBox{Number})

    # Dispatch respects invariance
    b = Box{Int}(3)
    @test h(b) == "int"
end

true
