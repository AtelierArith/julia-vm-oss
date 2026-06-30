# Exists-right subtype solving: `A <: (B where V...)` where the RHS is a
# UnionAll (Advances Issue #5047, also #5049).
#
# Decides `A <: UnionAll` by finding bindings for the bound var(s) that make
# `A <: B[bindings]`, respecting each var's bounds and the DIAGONAL rule (a var
# appearing in multiple covariant slots must take ONE consistent value).
#
# This builds on the #5569 increment, which lowered a value-position `where`
# expression to a first-class UnionAll value. Previously the runtime `<:` on
# such a value ignored the `where` clause entirely (treated bound vars as `Any`,
# enforcing neither bounds nor the diagonal rule), so e.g.
# `Tuple{Int,String} <: (Tuple{T,T} where T)` wrongly returned `true`.
#
# OUT OF SCOPE (later increments): LHS-UnionAll (forall-left) and full
# forall-exists alternation (both sides UnionAll, #5049). The degenerate
# bare-typevar-BODY `where` collapse (`T where T === Any`) is covered by the
# focused Issue #5570 fixture.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

@testset "exists-right: diagonal rule (Issue #5047)" begin
    # Same var T in two covariant tuple slots must take one consistent value.
    @test (Tuple{Int,Int} <: (Tuple{T,T} where T)) == true   # T=Int
    @test (Tuple{Int,String} <: (Tuple{T,T} where T)) == false # diagonal: T cannot be both
    @test (Tuple{Int,Real} <: (Tuple{T,T} where T)) == false   # Int != Real
    # Distinct vars T,S can take independent values.
    @test (Tuple{Int,Float64} <: (Tuple{T,S} where {T,S})) == true
end

@testset "exists-right: bounds (Issue #5047)" begin
    @test (Vector{Int} <: (Vector{T} where T)) == true
    @test (Vector{Int} <: (Vector{T} where T<:Real)) == true   # Int <: Real
    @test (Vector{String} <: (Vector{T} where T<:Real)) == false
    @test (Tuple{Int,Int} <: (Tuple{T,T} where T<:Integer)) == true
    @test (Tuple{Int,Int} <: (Tuple{T,T} where T<:AbstractString)) == false
end

@testset "exists-right: container shapes (Issue #5047)" begin
    @test (Dict{String,Int} <: (Dict{K,V} where {K,V})) == true
    @test (Tuple{Int,Int,Int} <: (Tuple{Vararg{T}} where T)) == true
end

# NOTE: the degenerate bare-typevar-BODY `where` cases — `Int <: (T where T)`
# (true upstream, since `T where T === Any`) and `String <: (T where T<:Real)`
# (false upstream, since `T where T<:Real === Real`) — are covered by the
# focused Issue #5570 fixture. This fixture stays scoped to exists-right
# UnionAll solver behavior.

# --- MUST STAY CORRECT: non-`where` subtyping, incl. invariant cases. ---
@testset "non-where subtyping regression guard (Issue #5047)" begin
    @test (Vector{Int} <: Vector{Real}) == false
    @test (Dict{String,Int} <: AbstractDict{String,Int}) == true
    @test (Tuple{Int} <: Tuple{Real}) == true
    @test (Vector{Int} <: AbstractVector{Int}) == true
end

true
