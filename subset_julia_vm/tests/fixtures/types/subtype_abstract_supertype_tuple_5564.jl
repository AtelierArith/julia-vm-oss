# Subtype-engine: parametric struct `<:` its PARAMETRIZED abstract supertype,
# and covariant Tuple matching that honors element INVARIANCE (Issue #5564).
#
# Two gaps surfaced after the array-family invariant-subtype fix (#5563):
#
# Bug 1 — A parametric concrete container is a subtype of its PARAMETRIZED
# abstract supertype when the shared invariant parameters are EQUAL:
#   Dict{String,Int} <: AbstractDict{String,Int}  is true
#   Set{Int}         <: AbstractSet{Int}           is true
# #5563 wired this up for the array family (Vector → AbstractVector) but the
# non-array containers (Dict/AbstractDict, Set/AbstractSet) regressed to false
# once the old param-loss bug stopped masking them.
#
# Bug 2 — Covariant Tuple matching must use the (invariant-aware) element
# subtype check, so an invariant parametric element is compared by equality:
#   Tuple{Vector{Int}} <: Tuple{Vector{Real}}  is false  (Vector is invariant)
# while Tuples stay covariant in directly-related leaves:
#   Tuple{Int} <: Tuple{Real}  is true  (Int <: Real)
#
# All expectations below were verified against upstream Julia 1.12.

using Test

@testset "parametric struct <: parametrized abstract supertype (Issue #5564)" begin
    # --- Bug 1: regression — matching invariant params => true ---
    @test Dict{String,Int} <: AbstractDict{String,Int}
    @test Set{Int} <: AbstractSet{Int}
    # array family was already correct (#5563) — keep it green here too
    @test Vector{Int} <: AbstractVector{Int}

    # --- Must stay correct: differing invariant params => false ---
    @test !(Dict{String,Int} <: Dict{String,Real})
    @test !(Dict{String,Int} <: AbstractDict{String,Real})
    @test !(Set{Int} <: AbstractSet{Real})
    @test !(Vector{Float64} <: AbstractVector{Int64})

    # --- Must stay correct: bare (no-param) abstract supertype is covariant ---
    @test Dict{String,Int} <: AbstractDict
    @test Set{Int} <: AbstractSet

    # --- typeintersect consequences (covariant subtype keeps the subtype) ---
    @test typeintersect(Dict{String,Int}, AbstractDict{String,Int}) == Dict{String,Int}
    @test typeintersect(Set{Int}, AbstractSet{Int}) == Set{Int}
end

@testset "covariant Tuple honors element invariance (Issue #5564)" begin
    # --- Bug 2: invariant element under covariant Tuple => false ---
    @test !(Tuple{Vector{Int}} <: Tuple{Vector{Real}})
    @test !(Tuple{Int,Vector{Int}} <: Tuple{Real,Vector{Real}})

    # --- Must stay correct: Tuples ARE covariant in directly-related leaves ---
    @test Tuple{Int} <: Tuple{Real}
    @test Tuple{Vector{Int}} <: Tuple{Vector{Int}}
    @test Tuple{Int,Vector{Int}} <: Tuple{Real,Vector{Int}}

    # --- typeintersect consequence: invariant mismatch => Union{} ---
    @test typeintersect(Tuple{Vector{Int}}, Tuple{Vector{Real}}) == Union{}
end

# --- Must stay correct: the #5563 invariant array cases (do not re-break) ---
@testset "do not re-break #5563 invariant array subtyping" begin
    @test !(Vector{Int} <: Vector{Real})
    @test !(AbstractVector{Int} <: AbstractVector{Real})
    @test !(Vector{Int} <: AbstractArray{Int,2})
    @test Matrix{Int} <: AbstractMatrix{Int}
    @test Vector{Int} <: AbstractArray{Int}
end

true
