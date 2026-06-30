# Issue #7728: a value/type-parameter chain through an `AbstractArray{T,N}`
# parent across an abstract supertype chain must thread the concrete element
# and dimension parameters down to the AbstractArray instantiation.
#
# StaticArrays-shaped hierarchy:
#   StaticArray7728{S,T,N} <: AbstractArray{T,N}
#   StaticVector7728{N,T}  <: StaticArray7728{Tuple{N},T,1}
#   SVector7728{N,T}       <: StaticVector7728{N,T}
#
# sjulia previously dropped the parametric PARENT's parameters when lowering an
# `abstract type ... <: Parent{...}` declaration (only the parent base name was
# kept), so the subtype machinery could not substitute T=Int64, N=1 down the
# abstract chain and `SVector7728{3,Int64} <: AbstractArray{Int64,1}` was
# wrongly false. The direct and abstract-name links already worked.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

abstract type StaticArray7728{S,T,N} <: AbstractArray{T,N} end
abstract type StaticVector7728{N,T} <: StaticArray7728{Tuple{N},T,1} end
struct SVector7728{N,T} <: StaticVector7728{N,T}
    data::Tuple
end

@testset "AbstractArray parent param chain (Issue #7728)" begin
    # Direct and abstract-name links (already worked before the fix).
    @test SVector7728{3,Int64} <: StaticVector7728{3,Int64}
    @test SVector7728{3,Int64} <: StaticArray7728

    # The bug: the parameterized AbstractArray check must thread T=Int64, N=1.
    @test SVector7728{3,Int64} <: AbstractArray{Int64,1}
    @test SVector7728{2,Float64} <: AbstractArray{Float64,1}

    # Element/dimension parameters are invariant: a different element type or
    # rank is NOT a subtype.
    @test !(SVector7728{3,Int64} <: AbstractArray{Float64,1})
    @test !(SVector7728{3,Int64} <: AbstractArray{Int64,2})

    # Intermediate abstract links in the chain also carry their parameters.
    @test SVector7728{3,Int64} <: StaticArray7728{Tuple{3},Int64,1}
    @test !(SVector7728{3,Int64} <: StaticArray7728{Tuple{3},Float64,1})
end

true
