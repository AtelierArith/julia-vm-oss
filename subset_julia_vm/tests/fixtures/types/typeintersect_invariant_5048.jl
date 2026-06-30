# Concrete invariant-parametric `typeintersect` (Advances Issue #5048).
#
# Issue #5048 is the full set-theoretic `typeintersect` (TypeVar / UnionAll
# forall-exists). This fixture locks the CONCRETE invariant-parametric cases
# that are now correct after the invariant-subtype fix (Issue #5047 / #5563):
# when two parametric container types share a name but differ in an INVARIANT
# parameter (element type or array dimension), and neither is a subtype of the
# other, their intersection is the empty type `Union{}` — never a bare guess of
# one operand. Covariant cases (a true subtype relationship, union
# distribution, covariant Tuple element intersection) keep returning the
# non-empty intersection.
#
# All expectations below were verified against upstream Julia 1.12.
#
# Later #5564 and #5048 slices cover Dict/Set abstract-supertype parity,
# covariant Tuple element invariance, and diagonal UnionAll narrowing.

using Test

@testset "typeintersect: concrete invariant-parametric cases (Issue #5048)" begin
    # --- Invariant element parameter differs => Union{} (was wrongly the LHS) ---
    @test typeintersect(Vector{Int}, Vector{Real}) === Union{}
    @test typeintersect(Vector{Real}, Vector{Int}) === Union{}
    @test typeintersect(Vector{Int}, AbstractVector{Real}) === Union{}
    @test typeintersect(Vector{Float64}, AbstractVector{Int64}) === Union{}
    @test typeintersect(Matrix{Int}, Matrix{Float64}) === Union{}

    # --- Invariant array dimension differs => Union{} ---
    @test typeintersect(Vector{Int}, AbstractArray{Int,2}) === Union{}

    # --- Invariance is recursive through an invariant container element ---
    @test typeintersect(Vector{Vector{Int}}, Vector{Vector{Real}}) === Union{}

    # --- True subtype relationship => the narrower (subtype) operand stays ---
    @test typeintersect(Int, Real) === Int
    @test typeintersect(Vector{Int}, Vector) === Vector{Int}
    @test typeintersect(Vector{Int}, AbstractVector{Int}) === Vector{Int}
    @test typeintersect(Vector{Int}, AbstractArray{Int,1}) === Vector{Int}
    @test typeintersect(Matrix{Int}, AbstractArray{Int,2}) === Matrix{Int}

    # --- Union distribution still narrows to the intersecting member ---
    @test typeintersect(Union{Int,String}, Real) === Int

    # --- Covariant Tuple element intersection (each element intersected) ---
    @test typeintersect(Tuple{Int,String}, Tuple{Real,AbstractString}) ===
          Tuple{Int,String}

    # --- Disjoint operands => Union{} ---
    @test typeintersect(Int, String) === Union{}
    @test typeintersect(Tuple{Int,String}, Tuple{Real}) === Union{}

    # --- Property: typeintersect(A, B) <: A and <: B for the cases above ---
    @test typeintersect(Vector{Int}, AbstractVector{Int}) <: Vector{Int}
    @test typeintersect(Vector{Int}, AbstractVector{Int}) <: AbstractVector{Int}
    @test typeintersect(Union{Int,String}, Real) <: Union{Int,String}
    @test typeintersect(Union{Int,String}, Real) <: Real
end

true
