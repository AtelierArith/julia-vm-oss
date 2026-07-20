# Bare-identifier assignment of a parametric alias application (Issue #10501).
#
# `z = SomeAlias{Args}` was mis-detected by the type-alias extraction
# (`extract_type_alias_from_binding`) as a NEW static type alias whose target
# was the verbatim RHS text (`"AliasG{Float64}"`), so `z` displayed the frozen
# alias-application string instead of the resolved type (`Vector{Float64}`).
# Fix: (a) alias-expand the stored target for a bare-identifier LHS so an
# application of a registered static alias freezes the RESOLVED type, and
# (b) decline extraction entirely when the RHS base is a runtime type binding
# (a `where`-clause parametric alias, Issue #10372), lowering the statement as
# an ordinary runtime assignment that applies the `UnionAll` at runtime.
#
# All expectations verified against upstream Julia 1.12.

using Test

# --- const parametric alias, applied and assigned (the #10501 MWE) ---
const AliasG10501{T} = Vector{T}
z1 = AliasG10501{Float64}

# --- plain-assignment parametric alias, applied and assigned ---
AliasP10501{T} = Vector{T}
z2 = AliasP10501{Float64}

# --- where-clause parametric alias (the #10372 MWE line 2) ---
MyVec10501{T} = Vector{T} where T<:Real
z3 = MyVec10501{Float64}

# --- bare-identifier where-RHS binding, then applied ---
W10501 = Vector{T} where T<:Real
z4 = W10501{Float64}

# --- const assignment of an alias application ---
const Z5_10501 = AliasG10501{Int}

# --- chained assignment of a runtime-applied type value ---
B10501 = z3

@testset "alias application assigned to bare identifier (Issue #10501)" begin
    @test z1 === Vector{Float64}
    @test string(z1) == "Vector{Float64}"
    @test z2 === Vector{Float64}
    @test z3 === Vector{Float64}
    @test string(z3) == "Vector{Float64}"
    @test z4 === Vector{Float64}
    @test Z5_10501 === Vector{Int}
    @test B10501 === Vector{Float64}
    # The assigned name behaves as the resolved type value.
    @test [1.0, 2.0] isa z1
    @test z1 <: AbstractVector
    @test z3 <: AbstractVector
end

# --- legitimate bare-identifier alias definitions must keep registering ---
const IntVec10501 = Vector{Int}
fintvec10501(x::IntVec10501) = length(x)

struct Wrap10501{T}
    value::T
end
const WrapInt10501 = Wrap10501{Int}
fwrap10501(::WrapInt10501) = "wrap-int"

@testset "legitimate bare-identifier aliases still register (Issue #10501)" begin
    @test IntVec10501 === Vector{Int}
    @test fintvec10501([1, 2, 3]) == 3
    @test WrapInt10501 === Wrap10501{Int}
    @test fwrap10501(Wrap10501{Int}(7)) == "wrap-int"
end

true
