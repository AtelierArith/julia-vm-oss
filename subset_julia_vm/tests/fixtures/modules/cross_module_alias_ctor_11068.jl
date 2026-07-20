# Cross-module const aliases of parametric constructor types (Issue #11068)

using Test

module AliasOwnerA11068
struct X{T}
    tag::Symbol
    X{T}() where {T} = new{T}(:ok)
end
end

module AliasOwnerB11068
using ..AliasOwnerA11068
const Y = AliasOwnerA11068.X
end

const TopAlias11068 = AliasOwnerA11068.X

@testset "cross-module parametric constructor aliases" begin
    @test AliasOwnerB11068.Y{Int}().tag == :ok
    @test TopAlias11068{Int}().tag == :ok
    @test AliasOwnerB11068.Y{String}() isa AliasOwnerA11068.X{String}
end

true
