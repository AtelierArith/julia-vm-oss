# Issue #5048: set-theoretic `typeintersect` — the distributive law over Union,
# element-wise Tuple intersection (length mismatch -> Bottom), strict invariant
# parametric intersection, UnionAll x UnionAll / diagonal variables, plus the
# upstream correctness properties `typeintersect(A,B) <: A & <: B` and
# `A <: B => typeintersect(A,B) == A`. Shares the subtype engine env (#5615).

using Test

@testset "set-theoretic typeintersect (Issue #5048)" begin
    # distributive law over Union
    @test typeintersect(Union{Int,String}, Real) == Int
    @test typeintersect(Union{Int8,Int16,String}, Integer) == Union{Int8,Int16}
    @test typeintersect(Union{Int,String}, Union{String,Float64}) == String

    # element-wise Tuple, length mismatch -> Bottom
    @test typeintersect(Tuple{Int,Real}, Tuple{Integer,Float64}) == Tuple{Int,Float64}
    @test typeintersect(Tuple{Int,Int}, Tuple{Int}) == Union{}
    @test typeintersect(Tuple{Union{Int,Bool},String}, Tuple{Integer,String}) ==
          Tuple{Union{Int,Bool},String}

    # strict invariant parametric intersection
    @test typeintersect(Vector{Int}, Vector{Float64}) == Union{}
    @test typeintersect(Vector{Int}, Vector{Int}) == Vector{Int}
    @test typeintersect(Vector{Int}, AbstractVector{Int}) == Vector{Int}

    # UnionAll x concrete / UnionAll x UnionAll / diagonal variable.
    # For the UnionAll x UnionAll narrowing the result is the tighter bound; check
    # it by mutual subtyping (the built UnionAll is semantically `Vector{<:Integer}`).
    @test typeintersect(Vector, Vector{Int}) == Vector{Int}
    @test typeintersect(Vector{T} where T<:Real, Vector{T} where T<:Integer) <:
          (Vector{T} where T<:Integer)
    @test (Vector{T} where T<:Integer) <:
          typeintersect(Vector{T} where T<:Real, Vector{T} where T<:Integer)
    @test typeintersect(Tuple{T,T} where T, Tuple{Int,Real}) == Tuple{Int,Int}
    @test typeintersect(Tuple{T,T} where T, Tuple{Int,String}) == Union{}
    @test typeintersect(Dict{Int,V} where V, Dict{Int,String}) == Dict{Int,String}
    @test typeintersect(Tuple{Vector{T},T} where T, Tuple{Vector{Int},Int}) ==
          Tuple{Vector{Int},Int}

    # Type{...} and abstract intersections
    @test typeintersect(Type{Int}, DataType) == Type{Int}
    @test typeintersect(Real, Integer) == Integer
    @test typeintersect(Any, Int) == Int
    @test typeintersect(Int, String) == Union{}

    # correctness properties: I <: A and I <: B
    for (A, B) in [(Union{Int,String}, Real),
                   (Vector{T} where T<:Real, Vector{T} where T<:Integer),
                   (Tuple{T,T} where T, Tuple{Int,Real}),
                   (Pair{Int,T} where T, Pair{Int,String})]
        I = typeintersect(A, B)
        @test I <: A
        @test I <: B
    end

    # A <: B  =>  typeintersect(A,B) == A
    for (A, B) in [(Int, Real), (Vector{Int}, AbstractVector{Int}), (Int8, Integer)]
        @test typeintersect(A, B) == A
    end
end

true
