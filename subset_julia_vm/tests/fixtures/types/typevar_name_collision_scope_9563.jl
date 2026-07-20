# TypeVar resolution is scope-based, not name-shape-based (Issue #9563).

using Test

struct S2 <: Real
    v::Float64
end

struct V2
    x::Int
end

struct P
    x::Int
end

struct T
    x::Int
end

struct N
    x::Int
end

struct A1
    x::Int
end

Base.:(==)(a::S2, b::S2) = a.v == b.v
Base.:(+)(a::S2, b::S2) = S2(a.v + b.v)

scope_type_9563(x::Q) where Q = Q
scope_vector_eltype_9563(xs::Vector{Q}) where Q = Q

@testset "colliding struct names remain nominal" begin
    @test typeof(S2) === DataType
    @test typeof(V2) === DataType
    @test typeof(P) === DataType
    @test typeof(T) === DataType
    @test typeof(N) === DataType
    @test typeof(A1) === DataType

    @test S2(1.0) == S2(1.0)
    @test S2(1.0) + S2(2.0) == S2(3.0)
    @test P(1) isa P
    @test T(1) isa T
    @test N(1) isa N
    @test A1(1) isa A1
end

@testset "colliding names preserve array eltypes" begin
    v2s = Vector{V2}(undef, 2)
    ts = T[T(1), T(2)]
    a1s = A1[A1(3)]

    @test typeof(v2s) === Vector{V2}
    @test typeof(ts) === Vector{T}
    @test typeof(a1s) === Vector{A1}
    @test eltype(v2s) === V2
    @test eltype(ts) === T
    @test eltype(a1s) === A1
end

@testset "where binders shadow colliding globals" begin
    @test scope_type_9563(P(1)) === P
    @test scope_type_9563(T(1)) === T
    @test scope_vector_eltype_9563([N(1), N(2)]) === N

    @test typeof(Vector{Q} where Q) === UnionAll
    @test (Vector{Q} where Q) == Vector
    @test (Vector{Q} where Q) !== Vector
    @test (Tuple{Q, Q} where Q) isa UnionAll
    @test Tuple{T, T} isa DataType
end

true
