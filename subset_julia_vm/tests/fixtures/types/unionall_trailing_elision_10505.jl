# Issue #10505: upstream `show` elides a TRAILING unbounded `where` binder that
# is exactly the type's last parameter and occurs nowhere else (show_can_elide,
# base/show.jl): `Array{T,N} where {T<:Real,N}` prints `Array{T} where T<:Real`.
# sjulia previously kept the full braced form. Non-elidable shapes (bounded
# innermost binder, same-name binder occurring twice, binder used inside
# another parameter) must keep their existing rendering (#10635 guards).

using Test

@testset "trailing unbounded where-var elision (Issue #10505)" begin
    @test string(Array{T,N} where {T<:Real,N}) == "Array{T} where T<:Real"
    @test string(Array{T,N} where N where T<:Real) == "Array{T} where T<:Real"
    @test string(Dict{K,V} where {K<:AbstractString,V}) == "Dict{K} where K<:AbstractString"
    # Fully generic still collapses to the bare name.
    @test string(Array{T,N} where {T,N}) == "Array"
    # Bounded innermost binder: no elision.
    @test string(Array{T,N} where {T,N<:Integer}) == "Array{T, N} where {T, N<:Integer}"
    # Same-name / reused binders keep their full form (#10635).
    @test string(Pair{T,T} where T) == "Pair{T, T} where T"
    @test string(Pair{Vector{B},B} where B) == "Pair{Vector{B}, B} where B"
    # Single bounded binder unchanged.
    @test string(Vector{T} where T<:Real) == "Vector{T} where T<:Real"
end

true
