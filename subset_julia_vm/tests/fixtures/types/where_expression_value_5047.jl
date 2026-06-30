# `where`-expression in VALUE/expression position lowers to a UnionAll type
# value (Advances Issues #5047/#5049/#5053 — subtype-engine increment).
#
# Previously `Tuple{T,T} where T` and `Array{T,N} where {T,N}` in expression
# position failed at lowering with
# `UnsupportedFeature{UnsupportedExpression("where_expression")}`. This increment
# desugars `Body where {V...}` into nested `UnionAll(TypeVar(:V), Body)`
# construction, so the result is a first-class `UnionAll` type value: `typeof`
# is `UnionAll`, it `isa UnionAll`/`isa Type`, it displays correctly, and
# `Base.unwrap_unionall`/`rewrap_unionall` round-trip.
#
# OUT OF SCOPE (later increment): subtype SOLVING with `where`
# (e.g. `Tuple{Int,Int} <: (Tuple{T,T} where T)`), which needs the forall/exists
# solver. This fixture only asserts construction/typeof/display/identity/unwrap.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

@testset "where-expression as value lowers to UnionAll (Issue #5047)" begin
    # --- typeof of a where-expression value is UnionAll ---
    @test typeof(Tuple{T,T} where T) == UnionAll
    @test typeof(Array{T,N} where {T,N}) == UnionAll
    @test typeof(Vector{T} where T) == UnionAll

    # --- the value isa UnionAll / isa Type ---
    @test (Vector{T} where T) isa UnionAll
    @test (Tuple{T,T} where T) isa UnionAll
    @test (Tuple{T,T} where T) isa Type

    # --- bounded where in value position is still a UnionAll ---
    @test typeof(Vector{T} where T<:Number) == UnionAll
    @test (Vector{T} where T<:Number) isa UnionAll

    # --- unwrap_unionall peels the UnionAll layer back to the body ---
    # (The body has a free TypeVar, so we introspect it rather than writing a
    # bare `Tuple{T,T}` RHS — that would raise UndefVarError in plain Julia.)
    utt = (Tuple{T,T} where T)
    btt = Base.unwrap_unionall(utt)
    @test typeof(btt) == DataType
    @test btt isa DataType
    @test nameof(btt) == :Tuple
    @test string(btt) == "Tuple{T, T}"
    # rewrap_unionall round-trips: unwrap then rewrap recovers the value
    @test Base.rewrap_unionall(btt, utt) === utt
    uvt = (Vector{T} where T)
    @test Base.rewrap_unionall(Base.unwrap_unionall(uvt), uvt) isa UnionAll

    # --- identity with the canonical builtin UnionAll aliases ---
    @test (Vector{T} where T) === Vector
    @test (Array{T,N} where {T,N}) === Array

    # --- alias-binding form: T1 = Array{T,N} where {T,N}; T1 === Array ---
    T1 = Array{T,N} where {T,N}
    @test T1 === Array
end

# --- MUST STAY WORKING: declaration-position `where` is unaffected ---
f(x::T) where T = x
g(x::T) where T<:Number = x + 1
h(x::Vector{T}) where T = length(x)

@testset "declaration-position where still works (regression guard)" begin
    @test f(3) == 3
    @test g(3) == 4
    @test h([1, 2, 3]) == 3
end

true
