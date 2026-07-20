using Test

struct LowerBoundMWE{T} end
abstract type LowerBoundAbstract end
struct LowerBoundLeaf <: LowerBoundAbstract end
struct UnrelatedBoundType end
struct BoundWrapper{T} end

pick(::Type{SubArray{T}}) where {T>:LowerBoundMWE{Int}} = :bounded
pick(::Type) = :fallback
lower_pick(::Type{BoundWrapper{T}}) where {T>:LowerBoundLeaf} = :bounded
lower_pick(::Type) = :fallback
upper_pick(::Type{BoundWrapper{T}}) where {T<:LowerBoundAbstract} = :bounded
upper_pick(::Type) = :fallback

@testset "parametric lower-bound Type dispatch" begin
    # Parametric lower bound: invalid and exact-valid cases.
    @test pick(SubArray{Int8}) == :fallback
    runtime_pick = pick
    @test runtime_pick(SubArray{Int8}) == :fallback
    @test pick(SubArray{LowerBoundMWE{Int}}) == :bounded
    @test runtime_pick(SubArray{LowerBoundMWE{Int}}) == :bounded

    # User hierarchy lower bound: exact and abstract-supertype cases.
    runtime_lower_pick = lower_pick
    @test lower_pick(BoundWrapper{Int8}) == :fallback
    @test runtime_lower_pick(BoundWrapper{Int8}) == :fallback
    @test lower_pick(BoundWrapper{LowerBoundLeaf}) == :bounded
    @test runtime_lower_pick(BoundWrapper{LowerBoundLeaf}) == :bounded
    @test lower_pick(BoundWrapper{LowerBoundAbstract}) == :bounded
    @test runtime_lower_pick(BoundWrapper{LowerBoundAbstract}) == :bounded

    # Structured upper bound: subtype acceptance and unrelated rejection.
    runtime_upper_pick = upper_pick
    @test upper_pick(BoundWrapper{LowerBoundLeaf}) == :bounded
    @test runtime_upper_pick(BoundWrapper{LowerBoundLeaf}) == :bounded
    @test upper_pick(BoundWrapper{UnrelatedBoundType}) == :fallback
    @test runtime_upper_pick(BoundWrapper{UnrelatedBoundType}) == :fallback
end

true
