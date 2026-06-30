# Test: bare `Ref` / `Base.RefValue` UnionAll type-object predicates (Issue #5223)
#
# Upstream Julia declares `abstract type Ref{T} end`, so the un-instantiated
# `Ref` is a `UnionAll` and every `Ref{T}` instantiation is an abstract
# `DataType`. The concrete box `Base.RefValue{T} <: Ref{T}` is a struct.
# Before #5223 a bare `Ref` resolved to the constructor function (so
# `typeof(Ref)` / type predicates misbehaved) and `Ref{Int}` was wrongly
# classified as a concrete struct.

using Test

@testset "bare Ref / RefValue UnionAll predicates" begin
    # bare `Ref` is `Ref{T} where T`, an abstract UnionAll
    @assert typeof(Ref) === UnionAll
    @assert isconcretetype(Ref) == false
    @assert isabstracttype(Ref) == true
    @assert isstructtype(Ref) == false

    # `Ref{Int}` is an abstract DataType instantiation (Ref is abstract upstream)
    @assert typeof(Ref{Int}) === DataType
    @assert isconcretetype(Ref{Int}) == false
    @assert isabstracttype(Ref{Int}) == true
    @assert isstructtype(Ref{Int}) == false

    # bare `Base.RefValue` is the concrete box struct's UnionAll
    @assert typeof(Base.RefValue) === UnionAll
    @assert isconcretetype(Base.RefValue) == false
    @assert isabstracttype(Base.RefValue) == false
    @assert isstructtype(Base.RefValue) == true

    # `Base.RefValue{Int}` is the concrete box DataType
    @assert typeof(Base.RefValue{Int}) === DataType
    @assert isconcretetype(Base.RefValue{Int}) == true
    @assert isabstracttype(Base.RefValue{Int}) == false
    @assert isstructtype(Base.RefValue{Int}) == true

    # subtyping is unchanged (no regression from #5130)
    @assert Ref{Int} <: Ref
    @assert Base.RefValue{Int} <: Ref
    @assert Base.RefValue{Int} <: Ref{Int}

    # Ref instances and Ref{T} construction still work (no regression from #5130)
    r = Ref(5)
    @assert r[] == 5
    r[] = 7
    @assert r[] == 7
    @assert isa(r, Ref)
    @assert isa(r, Ref{Int})
    @assert typeof(r) === Base.RefValue{Int}
    r2 = Ref{Float64}(1.5)
    @assert r2[] == 1.5

    @test (true)
end

true  # Test passed
