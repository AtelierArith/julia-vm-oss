# A plain (non-`const`) parametric type-alias definition whose RHS carries a
# `where` clause (`MyVec{T} = Vector{T} where T<:Real`) must LOWER and bind a
# usable `UnionAll` value, matching upstream Julia (Issue #10372).
#
# Root cause: the compile-time type-alias pre-scan
# (`try_extract_type_alias_from_assignment`) deliberately declines a
# `where`-clause RHS (Issue #5053: it is a runtime `UnionAll` value, not a
# static string alias). With that declined, the `Name{P...} = RHS` assignment
# reached ordinary statement lowering, whose assignment path had no arm for a
# `ParametrizedTypeExpression` LHS and raised `UnsupportedAssignmentTarget` at
# the DEFINITION line (the reported error). The fix adds a runtime arm that
# mirrors upstream `Meta.lower` of a parametric alias: wrap the ordinarily
# lowered RHS value in one `UnionAll(TypeVar(:P), ...)` per LHS parameter,
# outermost-first (leftmost LHS parameter = outermost binder).
#
# Issue #10501 completed the MWE by retaining a runtime type-object binding for
# plain aliases in addition to their compile-time annotation alias. This keeps
# `z = MyVec{Float64}` from being frozen as the string `MyVec{Float64}`.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

# Parametric alias with a `where` RHS (the #10372 MWE definition line).
MyVec10372{T} = Vector{T} where T<:Real

# Multi-parameter parametric alias whose RHS uses the chained single-var `where`
# form from Issue #10371 (exercises BOTH fixes together).
MyDict10372{K,V} = Dict{K,V} where V where K

const AliasG10501{T} = Vector{T}
z10372 = MyVec10372{Float64}
z10501 = AliasG10501{Float64}

@testset "plain-assign parametric alias + where: definition (Issue #10372)" begin
    # The definition lowers (previously: UnsupportedAssignmentTarget) and binds
    # a UnionAll value.
    @test MyVec10372 isa UnionAll
    @test typeof(MyVec10372) == UnionAll
    @test string(MyVec10372) == "Vector{T} where T<:Real"
end

@testset "plain-assign parametric alias + where: direct application (Issue #10372)" begin
    # Direct application resolves to the concrete target type (bound respected
    # in the binder; `Float64<:Real`).
    @test MyVec10372{Float64} == Vector{Float64}
    @test MyVec10372{Float64} === Vector{Float64}
    @test MyVec10372{Int} == Vector{Int}
    @test MyVec10372{Float64} <: AbstractVector
    @test z10372 === Vector{Float64}
    @test z10501 === Vector{Float64}
    @test string(z10372) == "Vector{Float64}"
    @test string(z10501) == "Vector{Float64}"
end

@testset "multi-param parametric alias + chained where (Issues #10372/#10371)" begin
    @test MyDict10372 isa UnionAll
    @test typeof(MyDict10372) == UnionAll
    @test MyDict10372{Int,String} == Dict{Int,String}
    @test MyDict10372{Int,String} === Dict{Int,String}
end

true
