using Test

# Issue #5644: an ANONYMOUS bounded type variable — the internal placeholder name
# `_`, produced when parsing the covariant shorthand `Vector{<:Integer}` — must
# print with the bound-only shorthand `<:Bound` upstream, never echoing the `_`
# placeholder. sjulia rendered `Vector{_<:Integer}`. This is a display-only
# divergence (it does not affect type identity or `===`); the internal `_<:`
# spelling round-trips through parsing unchanged.

@testset "anonymous covariant bound prints as <:Bound, not _<:Bound (Issue #5644)" begin
    @test string(Vector{<:Integer}) == "Vector{<:Integer}"
    @test string(Set{<:Real}) == "Set{<:Real}"
    @test string(Type{<:Number}) == "Type{<:Number}"
    @test string(Array{<:Real,3}) == "Array{<:Real, 3}"
    @test string(Ref{<:Integer}) == "Ref{<:Integer}"

    # Multiple anonymous bounds in one type, and nested anonymous bounds.
    @test string(Dict{<:Integer,<:AbstractString}) == "Dict{<:Integer, <:AbstractString}"
    @test string(Vector{<:Vector{<:Real}}) == "Vector{<:Vector{<:Real}}"

    # A typeintersect result that carries an anonymous bound renders cleanly.
    @test string(typeintersect(Vector{Int}, Vector{<:Real})) == "Vector{Int64}"
end

@testset "named and unbounded type variables are unchanged (Issue #5644)" begin
    # A NAMED bounded typevar keeps its name; only the anonymous `_` is elided.
    @test string(Vector{T} where T<:Real) == "Vector{T} where T<:Real"
    # Plain concrete parametric types are unaffected.
    @test string(Vector{Int}) == "Vector{Int64}"
    @test string(Dict{String,Int}) == "Dict{String, Int64}"
end

true
