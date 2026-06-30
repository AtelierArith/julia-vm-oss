# A struct's own declared type parameters are lexically scoped to the struct, so
# they must SHADOW any same-named top-level global/alias when the declared parent
# type is lowered (Issue #7840).
#
# Before the fix, a top-level `T = Int64` registered a non-parametric type alias
# `T -> Int64`. When lowering `struct Wrap{T} <: AbstractVector{T}`, sjulia
# substituted the global's VALUE into the parametric parent template, freezing it
# to `AbstractVector{Int64}`. That corrupted the subtype relation so
# `Wrap{Float64} <: AbstractVector{Float64}` wrongly returned `false`.
#
# Upstream Julia keeps the struct's type parameters scoped to the struct, so the
# same-named global is irrelevant. The fix excludes the struct's own param names
# from alias/global substitution when expanding the parent annotation, keeping
# `AbstractVector{T}` parametric.

using Test

# Case 1: direct MWE from the issue — a global `T` shadows the struct's param `T`.
T = Int64
struct Wrap{T} <: AbstractVector{T}
    data::Tuple
end

# Case 2: the StaticArray-shaped chain (also from the issue) with a global `T`.
abstract type StaticArray{S,T,N} <: AbstractArray{T,N} end
abstract type StaticVecOrMat{S,T,N} <: StaticArray{S,T,N} end
abstract type StaticVector{N,T} <: StaticVecOrMat{Tuple{N},T,1} end
struct SVector{N,T} <: StaticVector{N,T}
    data::Tuple
end
T = SVector{3,Int64}  # rebind the global `T` again

# Case 3 (regression): a parametric struct with NO shadowing global must still
# keep its parent parametric.
struct Bar{S} <: AbstractVector{S}
    data::Tuple
end

@testset "Struct type params shadow same-named globals in parent (Issue #7840)" begin
    # The parent template stays parametric, so any instantiation matches.
    @test Wrap{Float64} <: AbstractVector{Float64}
    @test Wrap{Int64} <: AbstractVector{Int64}
    @test !(Wrap{Float64} <: AbstractVector{Int64})

    # The StaticArray chain resolves through the parametric parents.
    @test SVector{3,Int64} <: AbstractArray{Int64,1}
    @test SVector{3,Float64} <: AbstractArray{Float64,1}

    # No-shadow parametric struct is unaffected.
    @test Bar{Float64} <: AbstractVector{Float64}
    @test Bar{Int64} <: AbstractVector{Int64}
end

true
