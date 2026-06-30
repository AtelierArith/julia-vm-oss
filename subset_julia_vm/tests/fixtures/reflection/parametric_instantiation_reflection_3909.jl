using Test

struct ParamBox3909{T}
    x::T
end

struct ParamPair3909{A, B}
    a::A
    b::B
end

abstract type ParamAbs3909{T} end

struct ParamSub3909{T} <: ParamAbs3909{T}
    v::T
end

struct ParamTwo3909{A, B} <: ParamAbs3909{A}
    a::A
    b::B
end

struct ParamNested3909{T}
    inner::ParamBox3909{T}
    tag::Int
end

struct ParamRepeat3909{T, S}
    a::T
    b::S
    c::T
end

@testset "fieldnames/fieldcount on parametric struct instantiations (Issue #3909)" begin
    @test fieldnames(ParamBox3909{Int}) === (:x,)
    @test fieldcount(ParamBox3909{Int}) == 1
    @test fieldnames(ParamPair3909{Int, String}) === (:a, :b)
    @test fieldcount(ParamPair3909{Int, String}) == 2
    @test fieldnames(ParamNested3909{Float64}) === (:inner, :tag)
    @test fieldcount(ParamNested3909{Float64}) == 2

    # Field names do not depend on the type arguments.
    @test fieldnames(ParamBox3909{Float64}) === fieldnames(ParamBox3909{Int})
end

@testset "fieldtypes substitutes type parameters (Issue #3909)" begin
    @test fieldtypes(ParamBox3909{Int}) === (Int64,)
    @test fieldtypes(ParamBox3909{Float64}) === (Float64,)
    @test fieldtypes(ParamPair3909{Int, String}) === (Int64, String)

    # Repeated type variables resolve to the same concrete argument.
    @test fieldtypes(ParamRepeat3909{Int, String}) === (Int64, String, Int64)

    # Nested parametric field type is reconstructed with the concrete argument.
    @test fieldtypes(ParamNested3909{Float64}) === (ParamBox3909{Float64}, Int64)
end

@testset "supertype of parametric instantiations matches upstream (Issue #3909)" begin
    # No declared parent => Any (not the type itself).
    @test supertype(ParamBox3909{Int}) === Any
    @test supertype(ParamPair3909{Int, String}) === Any
    @test supertype(ParamNested3909{Float64}) === Any

    # Parametric parent is reconstructed with substituted arguments.
    @test supertype(ParamSub3909{Int}) === ParamAbs3909{Int}
    @test supertype(ParamTwo3909{Int, Float64}) === ParamAbs3909{Int}
end

@testset "instantiation kind/identity stays consistent (Issue #3909)" begin
    @test typeof(ParamBox3909{Int}) === DataType
    @test typeof(ParamBox3909) === UnionAll
    @test isa(ParamBox3909{Int}, DataType)
    @test isa(ParamBox3909, UnionAll)
    @test ParamBox3909{Int} === ParamBox3909{Int64}
end

true
