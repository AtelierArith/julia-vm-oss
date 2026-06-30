# Forall-left subtype solving: `(B where V...) <: C` where the LHS is a
# UnionAll (Advances Issue #5047, also #5049).
#
# Decides `(B where V...) <: C` by introducing a fresh RIGID variable for each
# bound var (constrained by its declared bounds) and checking `B[rigid] <: C`
# holds for ALL such rigid choices — i.e. the bound var behaves as an opaque
# type confined to its bounds. Combined with the already-merged exists-right
# solver (#5571), the rigid LHS var flowing into a RHS UnionAll pattern yields
# forall-exists ALTERNATION for the common single/diagonal-var cases, e.g.
# `(Tuple{T} where T<:Integer) <: (Tuple{S} where S<:Real)` (∀T<:Integer there
# exists S<:Real, namely S=T).
#
# Previously the LHS `where` clause was dropped: the body's bound var was parsed
# as an UNBOUNDED typevar, so its declared upper bound never flowed into the
# subtype check and alternation cases wrongly returned `false`.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

@testset "forall-left: bare bounded var (Issue #5047)" begin
    @test ((Vector{T} where T) <: AbstractVector) == true       # ∀T: Vector{T}<:AbstractVector
    @test ((Vector{T} where T) <: Vector{Int}) == false
    @test ((Vector{T} where T<:Real) <: AbstractVector) == true
    @test ((Vector{T} where T<:Integer) <: AbstractVector) == true
    @test ((Tuple{T,T} where T) <: Tuple) == true
    @test ((Tuple{T} where T) <: Tuple{Int}) == false          # ∃ a T (e.g. String) breaking it
end

@testset "forall-left: builtin UnionAll aliases (Issue #5047)" begin
    @test (Array <: AbstractArray) == true                      # Array is a UnionAll on the left
    # NOTE: `Vector <: AbstractVector` (true upstream) is NOT exercised here: in
    # this VM bare `Vector` renders/routes as the rank-erased `Array` (string
    # "Array", no rank-1 marker, no `where`), so it never reaches the UnionAll
    # subtype arm — it is a separate name-rendering quirk, out of scope for the
    # forall-left engine increment.
end

@testset "forall-left + exists-right ALTERNATION (Issue #5047/#5049)" begin
    # Representative Issue #5049 shape: both sides carry type variables.
    # ∀T ∃S,U: Tuple{T,T} <: Tuple{S,U} holds by S=T, U=T.
    @test ((Tuple{T,T} where T) <: (Tuple{S,U} where {S,U})) == true
    # Reverse direction fails: not every Tuple{S,U} has equal element types.
    @test ((Tuple{S,U} where {S,U}) <: (Tuple{T,T} where T)) == false
    # ∀T<:Integer ∃S<:Real (S=T): Tuple{T}<:Tuple{S} holds.
    @test ((Tuple{T} where T<:Integer) <: (Tuple{S} where S<:Real)) == true
    # ∀T<:Real: NOT every T admits an S<:Integer with Tuple{T}<:Tuple{S}.
    @test ((Tuple{T} where T<:Real) <: (Tuple{S} where S<:Integer)) == false
    # Invariant element under alternation: S:=T (T<:Integer<:Real) works.
    @test ((Vector{T} where T<:Integer) <: (Vector{S} where S<:Real)) == true
    # Diagonal both sides: T=T forces S=S; S:=T satisfies S<:Real.
    @test ((Tuple{T,T} where T<:Integer) <: (Tuple{S,S} where S<:Real)) == true
end

# --- MUST STAY CORRECT: exists-right (#5571), invariant, and non-where. ---
@testset "regression guard: exists-right + invariant (Issue #5047)" begin
    # Exists-right diagonal/bounds (#5571) must still hold.
    @test (Tuple{Int,Int} <: (Tuple{T,T} where T)) == true
    @test (Tuple{Int,String} <: (Tuple{T,T} where T)) == false
    @test (Vector{Int} <: (Vector{T} where T<:Real)) == true
    @test (Vector{String} <: (Vector{T} where T<:Real)) == false
    # Invariant + plain subtyping.
    @test (Vector{Int} <: Vector{Real}) == false
    @test (Dict{String,Int} <: AbstractDict{String,Int}) == true
    @test (Tuple{Int} <: Tuple{Real}) == true
    @test (Vector{Int} <: AbstractVector{Int}) == true
end

true
