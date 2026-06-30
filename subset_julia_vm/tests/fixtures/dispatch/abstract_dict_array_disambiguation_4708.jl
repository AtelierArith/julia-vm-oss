using Test

@testset "dispatcher disambiguates Array from AbstractString / AbstractDict (Issue #4708)" begin
    # Before #4708, defining all three overloads triggered an
    # `AmbiguousMethod` compile error for `myfn([1, 2, 3])` because the
    # JuliaType::AbstractUser parent-`Any` fallback in is_subtype_of
    # spuriously made Array <: AbstractString and Array <: AbstractDict.
    myfn(s::AbstractString) = "string"
    myfn(d::AbstractDict)   = "dict"
    myfn(a::AbstractArray)  = "array"
    myfn(x)                  = "any"

    @test myfn([1, 2, 3]) == "array"
    @test myfn("hello") == "string"
    @test myfn(Dict("a" => 1)) == "dict"
    @test myfn(42) == "any"
    @test myfn(nothing) == "any"
end

@testset "Dict <: AbstractDict and Set <: AbstractSet stay true (Issue #4708)" begin
    # Verify the CoreType-backed fallback for the `Any`-parent case
    # preserves the built-in container hierarchy.
    @test Dict{String, Int}() isa AbstractDict
    @test Set{Int}() isa AbstractSet
    # And the dispatch agrees:
    container_kind(::AbstractDict) = :dict
    container_kind(::AbstractSet)  = :set
    container_kind(::AbstractArray) = :array
    @test container_kind(Dict("a" => 1)) === :dict
    @test container_kind(Set([1, 2])) === :set
    @test container_kind([1, 2, 3]) === :array
end

true
