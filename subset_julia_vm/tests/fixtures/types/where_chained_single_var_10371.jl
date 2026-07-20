# A value-position `where` expression that chains two single-variable `where`
# clauses, whose combined free variables cover a multi-parameter body
# (`Array{T,N} where N where T`), must lower to the same `UnionAll` as the
# equivalent braced multi-variable form (`Array{T,N} where {T,N}`), matching
# upstream Julia (Issue #10371).
#
# Root cause: the pure-Rust parser's value-position `where`-operator handling
# consumed the constraint via `parse_type_constraint`, which greedily parsed a
# FOLLOWING chained `where` as part of the bound. `Array{T,N} where N where T`
# therefore parsed right-associatively (constraints = `N where T`), a shape
# `parse_value_where_type_constraints` cannot unpack outside the bounded form,
# so lowering raised `UnsupportedFeature("unexpected node in where clause:
# WhereExpression")`. The fix parses the constraint with
# `parse_type_constraint_before_chained_where`, mirroring the type-annotation
# path, so the Pratt loop's own left-associative `where` handling picks up each
# chained `where` and builds the nested `UnionAll` left-to-right.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

@testset "chained single-var where over multi-param body (Issue #10371)" begin
    # MWE from the issue.
    x = Array{T,N} where N where T
    @test typeof(x) == UnionAll

    # The chained single-var form is the SAME type as the braced multi-var form.
    @test (Array{T,N} where N where T) == (Array{T,N} where {T,N})
    @test (Array{T,N} where N where T) === (Array{T,N} where {T,N})

    # String rendering matches the canonical braced form exactly (compared to
    # the braced spelling rather than a hardcoded literal, since upstream
    # normalizes the fully-generic `Array{T,N} where {T,N}` down to `"Array"`).
    @test string(Array{T,N} where N where T) == string(Array{T,N} where {T,N})

    # Concrete instantiation applies both binders (outer = T, inner = N).
    @test (Array{T,N} where N where T){Int64,2} == Array{Int64,2}
    @test (Array{T,N} where N where T){Float64,1} == Vector{Float64}
end

@testset "chained single-var where: bounded and Tuple bodies (Issue #10371)" begin
    # Bounds carry through each single-var clause and match the braced form.
    @test (Array{T,N} where N where T<:Real) == (Array{T,N} where {T<:Real,N})
    @test string(Array{T,N} where N where T<:Real) ==
          string(Array{T,N} where {T<:Real,N})

    # A Tuple body whose free variables are covered by chained single-var where.
    @test (Tuple{T,S} where S where T) == (Tuple{T,S} where {T,S})
    @test string(Tuple{T,S} where S where T) == string(Tuple{T,S} where {T,S})
    @test Tuple{Int64,String} <: (Tuple{T,S} where S where T)

    # Two-clause chain over a Dict body: the leftmost binder is outermost.
    @test (Dict{K,V} where V where K) == (Dict{K,V} where {K,V})
    @test (Dict{K,V} where V where K){Int64,String} == Dict{Int64,String}
end

@testset "chained single-var where: non-regression on existing forms (Issue #10371)" begin
    # The braced multi-var form and the single-clause form still work.
    @test typeof(Array{T,N} where {T,N}) == UnionAll
    @test typeof(Vector{T} where T) == UnionAll
    @test typeof(Vector{T} where T<:Real) == UnionAll

    # A single chained clause with a bound that itself references a sibling
    # binder (`T<:S where S`) is unaffected.
    chained = Vector{T} where T<:S where S<:Real
    @test typeof(chained) == UnionAll
    @test Vector{Int64} <: chained
end

true
