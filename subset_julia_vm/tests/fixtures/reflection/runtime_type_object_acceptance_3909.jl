using Test

# Issue #3909: runtime type-object identity and layout-semantics acceptance
# surface. This fixture consolidates the issue's acceptance criteria into a
# single regression guard, exercising fresh `TypeVar` construction, `UnionAll`
# wrapping/unwrapping, parametric type parameters, builtin/user struct layout
# metadata, and identity-sensitive type comparisons. Every assertion is verified
# field-for-field against upstream Julia 1.12.6.
#
# The former focused follow-up for `Vector`/`Matrix` as `Array{T,N}`
# dimensional aliases is covered separately by `array_dimensional_alias_5593.jl`.

struct Box3909{T}
    value::T
end

mutable struct MBox3909
    x::Int64
end

@testset "fresh TypeVar construction (#3909)" begin
    tv = TypeVar(:T)
    @test tv isa TypeVar
    @test tv.name === :T
    @test tv.lb === Union{}
    @test tv.ub === Any
end

@testset "UnionAll wrapping / unwrapping (#3909)" begin
    @test Vector isa UnionAll
    @test Box3909 isa UnionAll
    # Unwrapping a user parametric type's UnionAll yields the bound body.
    @test Base.unwrap_unionall(Box3909) === Box3909{Base.unwrap_unionall(Box3909).parameters[1]}
    # Rewrapping the unwrapped body with the bound var roundtrips to the alias.
    body = Base.unwrap_unionall(Box3909)
    @test Base.rewrap_unionall(body, Box3909) === Box3909
end

@testset "parametric type parameters (#3909)" begin
    @test Box3909{Int}.parameters == Core.svec(Int64)
    @test Box3909{Int} === Box3909{Int}
    @test Box3909{Int} !== Box3909{Float64}
    @test Box3909{Int} <: Box3909
end

@testset "builtin / user struct layout metadata (#3909)" begin
    @test fieldnames(Box3909{Int}) === (:value,)
    @test fieldtypes(Box3909{Int}) === (Int64,)
    @test isbitstype(Box3909{Int}) === true
    @test isbitstype(MBox3909) === false
    @test ismutabletype(MBox3909) === true
    @test ismutabletype(Box3909{Int}) === false
    @test sizeof(Int64) === 8
end

@testset "identity-sensitive type comparisons (#3909)" begin
    @test typeof(Int64) === DataType
    @test Int64 === Int64
    @test Vector{Int} === Vector{Int}
    @test (Vector{Int} === Vector{Float64}) === false
    @test isa(Box3909{Int}, DataType)
end

true
