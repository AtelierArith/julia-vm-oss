# User-defined parametric type aliases (Issue #5055)
# MyVec{T} = Vector{T} desugars to a UnionAll-valued binding; MyVec{Int}
# instantiates it to Vector{Int}. Verified for identity, construction, isa,
# subtyping and dispatch against upstream Julia 1.12.

using Test

# Parametric alias of a builtin parametric type
MyVec{T} = Vector{T}
# Multi-parameter parametric alias
MyDict{K,V} = Dict{K,V}
# Non-parametric aliases (plain assignment and const)
IntVec = Vector{Int}
const FloatVec = Vector{Float64}

# Parametric alias of a user-defined parametric struct
struct Box{T}
    value::T
end
BoxAlias{T} = Box{T}

# Dispatch on parametric-alias instantiations of builtin parametric types
f(::MyVec{Int}) = "vec-int"
f(::MyVec{Float64}) = "vec-float"
g(::AbstractVector) = "abstract-vec"

@testset "parametric type alias (Issue #5055)" begin
    # Instantiation identity: alias{Args} === target{Args}
    @test MyVec{Int} === Vector{Int}
    @test MyVec{Float64} === Vector{Float64}
    @test MyDict{String,Int} === Dict{String,Int}
    @test BoxAlias{Int} === Box{Int}

    # Construction through the alias
    v = MyVec{Int}([1, 2, 3])
    @test v == [1, 2, 3]
    @test typeof(v) === Vector{Int}

    iv = IntVec([4, 5, 6])
    @test typeof(iv) === Vector{Int}
    fv = FloatVec([1.0, 2.0])
    @test typeof(fv) === Vector{Float64}

    d = MyDict{String,Int}()
    d["a"] = 1
    @test d["a"] == 1
    @test typeof(d) === Dict{String,Int}

    b = BoxAlias{Int}(7)
    @test b.value == 7
    @test typeof(b) === Box{Int}

    # isa through the alias
    @test [1, 2, 3] isa MyVec{Int}
    @test !([1.0] isa MyVec{Int})
    @test b isa BoxAlias{Int}

    # Subtyping through the alias
    @test MyVec{Int} <: Vector{Int}
    @test MyVec{Int} <: AbstractVector
    @test MyDict{String,Int} <: AbstractDict

    # Dispatch on parametric-alias annotations
    @test f([1, 2, 3]) == "vec-int"
    @test f([1.0, 2.0]) == "vec-float"
    @test g(MyVec{Int}([1, 2, 3])) == "abstract-vec"
end

true
