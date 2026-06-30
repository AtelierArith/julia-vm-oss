using Test

# Issue #5048 (set-theoretic typeintersect, focused slice): a bare parametric
# container `UnionAll` met with a ground parametric instantiation. The
# concrete↔abstract container relation already worked when neither side was a
# `UnionAll` (`typeintersect(Vector{Int}, AbstractVector{Int}) == Vector{Int}`);
# the gap was a `where`-bound container on one side, which returned `Union{}`.
# Each `where` variable is forced (containers are invariant) to the matching
# positional parameter of the other operand, bound-checked, and the resulting
# concrete body verified `<:` the operand.

@testset "UnionAll ∩ abstract container (Issue #5048)" begin
    # Concrete container UnionAll ∩ abstract container (invariant element).
    @test typeintersect(Vector{T} where T<:Real, AbstractVector{Int}) === Vector{Int}
    @test typeintersect(Vector{T} where T, AbstractVector{Int}) === Vector{Int}
    @test typeintersect(Matrix{T} where T<:Real, AbstractMatrix{Int}) === Matrix{Int}
    @test typeintersect(Vector{T} where T<:Real, AbstractArray{Int,1}) === Vector{Int}

    # Symmetric operand order.
    @test typeintersect(AbstractVector{Int}, Vector{T} where T<:Real) === Vector{Int}

    # The element value flows from the abstract operand, respecting the bound.
    @test typeintersect(Vector{T} where T<:Real, AbstractVector{Float64}) === Vector{Float64}
    @test typeintersect(Vector{T} where T<:Real, AbstractVector{Bool}) === Vector{Bool}
    @test typeintersect(Vector{T} where T<:Integer, AbstractVector{Bool}) === Vector{Bool}

    # Multi-parameter container.
    @test typeintersect(Dict{K,V} where {K,V}, AbstractDict{Int,String}) === Dict{Int,String}

    # Partial instantiation: a fixed parameter is kept, the bound one flows in.
    @test typeintersect(Dict{Int,V} where V, AbstractDict{Int,String}) === Dict{Int,String}

    # Bound violation → empty.
    @test typeintersect(Vector{T} where T<:Real, AbstractVector{String}) === Union{}
    @test typeintersect(Vector{T} where T<:Integer, AbstractVector{Float64}) === Union{}

    # Unrelated family → empty (rejected by the subtype verification).
    @test typeintersect(Vector{T} where T<:Real, AbstractSet{Int}) === Union{}

    # Wrong dimensionality → empty.
    @test typeintersect(Vector{T} where T, AbstractArray{Int,2}) === Union{}

    # Diagonal variable across invariant positions must agree.
    @test typeintersect(Pair{T,T} where T, Pair{Int,Int}) === Pair{Int,Int}
    @test typeintersect(Pair{T,T} where T, Pair{Int,String}) === Union{}
    @test typeintersect(Pair{A,B} where {A,B}, Pair{Int,String}) === Pair{Int,String}
end

true
