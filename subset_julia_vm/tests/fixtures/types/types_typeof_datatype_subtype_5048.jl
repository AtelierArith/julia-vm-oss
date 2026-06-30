using Test

# Issue #5048 (set-theoretic typeintersect): a `Type{T}` is a subtype of
# `DataType` exactly when `T` is itself a nominal DataType — a concrete or
# abstract type, or a fully-applied parametric type — but NOT when `T` is a
# `Union`, a bare parametric (a `UnionAll`), or a `Type{<:Bound}` (also a
# `UnionAll`). sjulia previously reported every `Type{T} <: DataType` as `false`,
# so `typeintersect(Type{Int}, DataType)` collapsed to `Union{}` instead of the
# concrete `Type{Int}` side.

@testset "Type{T} <: DataType honors T's nominal-DataType shape (Issue #5048)" begin
    # T is a nominal DataType -> Type{T} <: DataType.
    @test Type{Int} <: DataType
    @test Type{Integer} <: DataType
    @test Type{String} <: DataType
    @test Type{Any} <: DataType
    @test Type{Vector{Int}} <: DataType
    @test Type{Vector{Real}} <: DataType

    # T is a Union / bare parametric (UnionAll) / Type{<:Bound} -> not a DataType.
    @test !(Type{Union{Int,Bool}} <: DataType)
    @test !(Type{Vector} <: DataType)
    @test !(Type{<:Real} <: DataType)

    # The reverse never holds.
    @test !(DataType <: Type{Int})

    # `Type{T} <: Type` stays true regardless.
    @test Type{Int} <: Type
    @test Type{<:Real} <: Type
end

@testset "typeintersect(Type{T}, DataType) keeps the concrete Type side (Issue #5048)" begin
    @test typeintersect(Type{Int}, DataType) === Type{Int}
    @test typeintersect(DataType, Type{Int}) === Type{Int}
    @test typeintersect(Type{Float64}, DataType) === Type{Float64}
    @test typeintersect(Type{Integer}, DataType) === Type{Integer}
    @test typeintersect(Type{String}, DataType) === Type{String}
    @test typeintersect(Type{Vector{Int}}, DataType) === Type{Vector{Int}}
    @test typeintersect(Type{Vector{Real}}, DataType) === Type{Vector{Real}}

    # Non-DataType T intersects DataType to Union{}.
    @test typeintersect(Type{Union{Int,Bool}}, DataType) === Union{}
    @test typeintersect(Type{Vector}, DataType) === Union{}
end

true
