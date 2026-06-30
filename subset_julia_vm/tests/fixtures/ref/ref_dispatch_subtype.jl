# Test: Ref dispatch, isa, and subtyping (Issue #5130)

using Test

isref(x::Ref) = "ref"
isref(x) = "other"

isrefint(x::Ref{Int}) = "ref-int"
isrefint(x) = "other"

@testset "Ref dispatch / isa / subtype" begin
    r = Ref(5)

    # isa
    @assert isa(r, Ref)
    @assert isa(r, Ref{Int})
    @assert isa(r, Base.RefValue{Int})
    @assert !isa(r, Ref{Float64})

    # subtyping
    @assert Base.RefValue{Int} <: Ref
    @assert Base.RefValue{Int} <: Ref{Int}
    @assert Ref{Int} <: Ref

    # multiple dispatch on ::Ref and ::Ref{Int}
    @assert isref(r) == "ref"
    @assert isref(5) == "other"
    @assert isrefint(r) == "ref-int"
    @assert isrefint(5) == "other"

    @test (true)
end

true  # Test passed
