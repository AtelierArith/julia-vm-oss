# User-declared subtypes of built-in abstract families derive their membership
# from the declared supertype chain (Issue #8560): `<:`, `isa`, and dispatch
# must all agree with upstream Julia (verified with `julia --startup-file=no`).

using Test

abstract type FamAbstract8560 end
struct FamMine8560 <: FamAbstract8560 end

struct FamSet8560{T} <: AbstractSet{T}
    items::Vector{T}
end

struct FamIO8560 <: IO end

struct FamFunctor8560 <: Function
    offset::Int
end

struct FamStr8560 <: AbstractString end

struct FamRange8560 <: AbstractRange{Int} end
struct FamUnitRange8560 <: AbstractUnitRange{Int} end

# Distributions-style parametric abstract chain.
abstract type FamSampleable8560{F} end
abstract type FamDistribution8560{F} <: FamSampleable8560{F} end
struct FamNormal8560 <: FamDistribution8560{Int} end

kindof8560(::AbstractSet) = "set"
kindof8560(::IO) = "io"
kindof8560(::Function) = "function"
kindof8560(::Any) = "other"

@testset "declared families drive <:, isa, dispatch (Issue #8560)" begin
    # User abstract chain (baseline: already derived).
    @test FamMine8560 <: FamAbstract8560
    @test FamNormal8560 <: FamDistribution8560
    @test FamNormal8560 <: FamSampleable8560

    # AbstractSet family.
    @test FamSet8560 <: AbstractSet
    @test FamSet8560{Int} <: AbstractSet
    @test FamSet8560{Int} <: AbstractSet{Int}
    @test !(FamSet8560 <: AbstractDict)
    # Base's own KeySet is declared `<: AbstractSet` in Julia source.
    @test keys(Dict(1 => 2)) isa AbstractSet

    # IO family.
    @test FamIO8560 <: IO
    @test !(FamIO8560 <: Function)

    # Functor structs are `<: Function`.
    @test FamFunctor8560 <: Function
    @test Base.Fix1 <: Function

    # AbstractString family.
    @test FamStr8560 <: AbstractString
    @test !(FamStr8560 <: AbstractChar)

    # Range families, including the array-family ancestry and the
    # directional lattice.
    @test FamRange8560 <: AbstractRange
    @test FamRange8560 <: AbstractVector
    @test FamRange8560 <: AbstractArray
    @test !(FamRange8560 <: AbstractUnitRange)
    @test !(FamRange8560 <: AbstractMatrix)
    @test FamUnitRange8560 <: AbstractUnitRange
    @test FamUnitRange8560 <: AbstractRange

    # isa follows the same derivation.
    @test FamIO8560() isa IO
    @test FamFunctor8560(1) isa Function
    @test FamSet8560([1, 2]) isa AbstractSet
    @test !(FamMine8560() isa Function)

    # Dispatch selects the family method exactly as upstream.
    @test kindof8560(FamSet8560([1])) == "set"
    @test kindof8560(FamIO8560()) == "io"
    @test kindof8560(FamFunctor8560(0)) == "function"
    @test kindof8560(FamMine8560()) == "other"
end

true  # Test passed
