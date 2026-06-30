# Issue #3762/#3763: reflection hierarchy queries should share the same
# direct-supertype/subtype information for builtin and user-defined types.

using Test

abstract type ReflectionHierarchyRoot3762 end
abstract type ReflectionHierarchyMid3762 <: ReflectionHierarchyRoot3762 end

struct ReflectionHierarchyLeaf3762 <: ReflectionHierarchyMid3762
    x::Int64
end

@testset "reflection type hierarchy" begin
    @test supertype(BigInt) === Signed
    @test supertype(BigFloat) === AbstractFloat
    @test supertype(ReflectionHierarchyLeaf3762) === ReflectionHierarchyMid3762
    @test supertype(ReflectionHierarchyMid3762) === ReflectionHierarchyRoot3762

    leaf_chain = Base.supertypes(ReflectionHierarchyLeaf3762)
    @test leaf_chain[1] === ReflectionHierarchyLeaf3762
    @test leaf_chain[2] === ReflectionHierarchyMid3762
    @test leaf_chain[3] === ReflectionHierarchyRoot3762
    @test leaf_chain[4] === Any

    bigint_chain = Base.supertypes(BigInt)
    @test bigint_chain[1] === BigInt
    @test bigint_chain[2] === Signed
    @test bigint_chain[3] === Integer

    @test typeintersect(ReflectionHierarchyLeaf3762, ReflectionHierarchyRoot3762) === ReflectionHierarchyLeaf3762
    @test typeintersect(ReflectionHierarchyRoot3762, ReflectionHierarchyLeaf3762) === ReflectionHierarchyLeaf3762
    @test typeintersect(BigInt, Number) === BigInt
    @test typeintersect(Tuple{Union{Int64, String}, Float64}, Tuple{Integer, Real}) === Tuple{Int64, Float64}
    @test typeintersect(Tuple{String}, Tuple{Real}) === Union{}
    @test typeintersect(Type{Union{Int64, String}}, Type{Integer}) === Union{}
end

true
