# Completeness and mutual consistency of the five type predicates
# (isconcretetype / isabstracttype / isstructtype / ismutabletype /
# isprimitivetype) across the full case matrix: concrete immutable struct,
# mutable struct, abstract type, primitive type, parametric instantiation,
# UnionAll (bare parametric), Union, Tuple, function singleton, and Nothing.
# Each predicate matches upstream Julia 1.12 exactly, and the consistency
# relations between them hold (Issue #5102).

using Test

struct ImmS
    x::Int
    y::Float64
end
mutable struct MutS
    x::Int
end
abstract type AbsT end
struct IBox{T}
    val::T
end
mutable struct MBox{T}
    val::T
end
abstract type AbsP{T} end
f() = 1

@testset "concrete immutable struct" begin
    @test isconcretetype(ImmS) == true
    @test isabstracttype(ImmS) == false
    @test isstructtype(ImmS) == true
    @test ismutabletype(ImmS) == false
    @test isprimitivetype(ImmS) == false
end

@testset "mutable struct" begin
    @test isconcretetype(MutS) == true
    @test isabstracttype(MutS) == false
    @test isstructtype(MutS) == true
    @test ismutabletype(MutS) == true
    @test isprimitivetype(MutS) == false
end

@testset "abstract type" begin
    @test isconcretetype(AbsT) == false
    @test isabstracttype(AbsT) == true
    @test isstructtype(AbsT) == false
    @test ismutabletype(AbsT) == false
    @test isprimitivetype(AbsT) == false
    # Builtin abstract types behave identically.
    @test isabstracttype(Number) == true
    @test isconcretetype(Number) == false
    @test isstructtype(Number) == false
    @test isabstracttype(AbstractFloat) == true
    @test isabstracttype(Any) == true
end

@testset "primitive type" begin
    for T in (Int, Int64, Float64, Bool, Char, UInt8)
        @test isprimitivetype(T) == true
        @test isconcretetype(T) == true
        @test isabstracttype(T) == false
        # isprimitivetype excludes these from isstructtype.
        @test isstructtype(T) == false
        @test ismutabletype(T) == false
    end
    # String is NOT a primitive type (it is a struct type upstream).
    @test isprimitivetype(String) == false
    @test isstructtype(String) == true
    @test ismutabletype(String) == true
end

@testset "concrete parametric instantiation" begin
    @test isconcretetype(IBox{Int}) == true
    @test isabstracttype(IBox{Int}) == false
    @test isstructtype(IBox{Int}) == true
    @test ismutabletype(IBox{Int}) == false
    @test isprimitivetype(IBox{Int}) == false

    # Mutable parametric instantiation stays mutable.
    @test isconcretetype(MBox{Int}) == true
    @test isstructtype(MBox{Int}) == true
    @test ismutabletype(MBox{Int}) == true

    @test isconcretetype(Pair{Int,String}) == true
    @test isstructtype(Pair{Int,String}) == true
    @test isconcretetype(Vector{Int}) == true
    @test isstructtype(Vector{Int}) == true
    @test ismutabletype(Vector{Int}) == true

    # Abstract parametric instantiation keeps the declaration's abstractness.
    @test isabstracttype(AbsP{Int}) == true
    @test isconcretetype(AbsP{Int}) == false
    @test isstructtype(AbsP{Int}) == false
    @test ismutabletype(AbsP{Int}) == false
end

@testset "UnionAll / bare parametric type" begin
    # User parametric struct as a UnionAll.
    @test isconcretetype(IBox) == false
    @test isabstracttype(IBox) == false
    @test isstructtype(IBox) == true
    @test ismutabletype(IBox) == false
    @test isconcretetype(MBox) == false
    @test isstructtype(MBox) == true
    @test ismutabletype(MBox) == true

    # Builtin parametric UnionAll types.
    @test isconcretetype(Vector) == false
    @test isstructtype(Vector) == true
    @test ismutabletype(Vector) == true
    @test isconcretetype(Pair) == false
    @test isstructtype(Pair) == true
    @test ismutabletype(Pair) == false
    @test isconcretetype(UnitRange) == false
    @test isstructtype(UnitRange) == true
    @test isconcretetype(NamedTuple) == false
    @test isstructtype(NamedTuple) == true
    @test isconcretetype(Complex) == false
    @test isstructtype(Complex) == true
    @test isconcretetype(Dict) == false
    @test isstructtype(Dict) == true
    @test ismutabletype(Dict) == true

    # Abstract parametric UnionAll.
    @test isabstracttype(AbsP) == true
    @test isconcretetype(AbsP) == false
    @test isstructtype(AbsP) == false
end

@testset "Union" begin
    @test isconcretetype(Union{Int,String}) == false
    @test isabstracttype(Union{Int,String}) == false
    @test isstructtype(Union{Int,String}) == false
    @test ismutabletype(Union{Int,String}) == false
    @test isprimitivetype(Union{Int,String}) == false
    # Union{} (Bottom).
    @test isconcretetype(Union{}) == false
    @test isabstracttype(Union{}) == false
    @test isstructtype(Union{}) == false
end

@testset "Tuple" begin
    # Bare Tuple is the any-tuple DataType: a struct type but not concrete.
    @test isconcretetype(Tuple) == false
    @test isabstracttype(Tuple) == false
    @test isstructtype(Tuple) == true
    @test ismutabletype(Tuple) == false
    @test isprimitivetype(Tuple) == false

    # Concrete tuple types are concrete struct types.
    @test isconcretetype(Tuple{Int,Float64}) == true
    @test isstructtype(Tuple{Int,Float64}) == true
    @test ismutabletype(Tuple{Int,Float64}) == false
    @test isconcretetype(Tuple{}) == true
    @test isstructtype(Tuple{}) == true

    # Tuple with an abstract element is not concrete.
    @test isconcretetype(Tuple{Number}) == false
    @test isstructtype(Tuple{Number}) == true
end

@testset "function singleton" begin
    @test isconcretetype(typeof(f)) == true
    @test isabstracttype(typeof(f)) == false
    @test isstructtype(typeof(f)) == true
    @test ismutabletype(typeof(f)) == false
    @test isprimitivetype(typeof(f)) == false
end

@testset "Nothing" begin
    @test isconcretetype(Nothing) == true
    @test isabstracttype(Nothing) == false
    @test isstructtype(Nothing) == true
    @test ismutabletype(Nothing) == false
    @test isprimitivetype(Nothing) == false
end

@testset "DataType / Type kinds" begin
    @test isconcretetype(DataType) == true
    @test isstructtype(DataType) == true
    @test ismutabletype(DataType) == true
    # Type{Int} is a kind: abstract upstream, not concrete.
    @test isconcretetype(Type{Int}) == false
    @test isabstracttype(Type{Int}) == true
    @test isstructtype(Type{Int}) == false
end

@testset "mutual consistency relations" begin
    # A concrete type is never abstract.
    for T in (ImmS, MutS, Int, Float64, IBox{Int}, Tuple{Int,Float64}, typeof(f), Nothing)
        @test !(isconcretetype(T) && isabstracttype(T))
    end
    # An abstract type is neither concrete nor a struct type.
    for T in (AbsT, Number, AbstractFloat, AbsP{Int}, Type{Int})
        @test isabstracttype(T) == true
        @test isconcretetype(T) == false
        @test isstructtype(T) == false
    end
    # A primitive type is not a struct type; a struct type is not primitive.
    for T in (Int, Float64, Char, Bool)
        @test isprimitivetype(T) == true
        @test isstructtype(T) == false
    end
    for T in (ImmS, MutS, String, Tuple{Int}, IBox{Int})
        @test isstructtype(T) == true
        @test isprimitivetype(T) == false
    end
    # A mutable type is a struct type (mutable struct keyword) or a builtin
    # mutable container; it is concrete when fully specified.
    @test ismutabletype(MutS) && isstructtype(MutS) && isconcretetype(MutS)
    @test ismutabletype(MBox{Int}) && isstructtype(MBox{Int}) && isconcretetype(MBox{Int})
end

true  # Test passed
