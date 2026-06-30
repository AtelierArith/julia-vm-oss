# isconcretetype: concrete parametric structs, function singletons, and Tuple{}
# are concrete; UnionAll/bare-parametric, Union, abstract types, and tuples with
# abstract/unbounded element types are not (Issue #5203).

using Test

struct ParamS{T} end
struct Box{T} x::T end
struct Point x::Int; y::Int end
abstract type AbsT end
f(x) = x

@testset "isconcretetype concrete cases" begin
    # Concrete parametric struct instantiations.
    @test isconcretetype(ParamS{Int}) == true
    @test isconcretetype(Box{Int}) == true
    @test isconcretetype(Pair{Int,String}) == true
    # Abstract or UnionAll-valued *parameters* keep the type concrete: only a
    # free type variable removes concreteness.
    @test isconcretetype(Box{Number}) == true
    @test isconcretetype(Box{Vector}) == true
    @test isconcretetype(Vector{Vector}) == true

    # Function singleton types.
    @test isconcretetype(typeof(f)) == true
    @test isconcretetype(typeof(println)) == true

    # Empty tuple type and tuples of concrete elements.
    @test isconcretetype(Tuple{}) == true
    @test isconcretetype(Tuple{Int,String}) == true
    @test isconcretetype(Tuple{Box{Int}}) == true
    @test isconcretetype(Tuple{Tuple{}}) == true
    # Bounded Vararg has a definite length.
    @test isconcretetype(NTuple{3,Int}) == true
    @test isconcretetype(Tuple{Vararg{Int,3}}) == true

    # Plain leaf types stay concrete.
    @test isconcretetype(Int) == true
    @test isconcretetype(Float64) == true
    @test isconcretetype(String) == true
    @test isconcretetype(Point) == true
    @test isconcretetype(Vector{Int}) == true
end

@testset "isconcretetype non-concrete cases" begin
    # Abstract types.
    @test isconcretetype(Number) == false
    @test isconcretetype(AbstractFloat) == false
    @test isconcretetype(AbsT) == false

    # UnionAll / bare parametric types.
    @test isconcretetype(Box) == false
    @test isconcretetype(ParamS) == false
    @test isconcretetype(Vector) == false

    # Union.
    @test isconcretetype(Union{Int,String}) == false

    # `Type{T}` kinds are not concrete, even though they have one instance.
    @test isconcretetype(Type{Int}) == false
    @test isconcretetype(Type) == false

    # Bare Tuple is Tuple{Vararg{Any}} (unbounded length).
    @test isconcretetype(Tuple) == false

    # Tuples with abstract / UnionAll / unbounded-Vararg element types.
    @test isconcretetype(Tuple{Number}) == false
    @test isconcretetype(Tuple{Int,Vector}) == false
    @test isconcretetype(Tuple{Vector}) == false
    @test isconcretetype(Tuple{Int,Vararg{Int}}) == false
    @test isconcretetype(Tuple{Vararg{Int}}) == false
end

true  # Test passed
