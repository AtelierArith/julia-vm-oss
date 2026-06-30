# Invariant parametric subtyping for built-in container abstract types
# (Advances Issue #5047 — first increment toward the unified subtype engine).
#
# Julia's parametric DataTypes/abstract array types are INVARIANT in their
# element parameter: `Vector{Float64} <: AbstractVector{Int64}` is false even
# though both are vectors, because the element type must be EQUAL (not merely a
# subtype). sjulia previously dropped the parameter of parametric *abstract*
# names (`AbstractVector{Int64}` was parsed as the bare `AbstractVector`), so
# the invariant parameter was never checked and these all wrongly returned true.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

@testset "invariant parametric subtyping for builtin abstracts (Issue #5047)" begin
    # --- The bug: element parameter differs => false (was wrongly true) ---
    @test !(Vector{Float64} <: AbstractVector{Int64})
    @test !(Vector{Int} <: Vector{Real})
    @test !(AbstractVector{Int} <: AbstractVector{Real})
    @test !(Matrix{Float64} <: AbstractMatrix{Real})
    # Dict was already correct (stays Struct with exact-param equality)
    @test !(Dict{String,Int} <: Dict{String,Real})

    # --- Dimension parameter is also invariant (was wrongly true) ---
    @test !(Vector{Int} <: AbstractArray{Int,2})
    @test !(AbstractVector{Int} <: AbstractArray{Real})
    @test !(Vector{Float64} <: AbstractArray{Real})

    # --- Must stay correct: matching element parameter => true ---
    @test Vector{Int} <: AbstractVector{Int}
    @test Matrix{Int} <: AbstractMatrix{Int}
    @test Vector{Int} <: AbstractArray{Int,1}
    @test Vector{Int} <: AbstractArray{Int}
    @test Matrix{Int} <: AbstractArray{Int,2}
    @test Array{Int,1} <: AbstractVector{Int}
    @test Array{Int,2} <: AbstractMatrix{Int}
    @test AbstractVector{Int} <: AbstractArray{Int,1}
    @test AbstractVector{Int} <: AbstractArray{Int}
    @test AbstractMatrix{Int} <: AbstractArray{Int,2}

    # --- Must stay correct: bare (no-param) abstract supertype is covariant ---
    @test Vector{Int} <: AbstractVector
    @test Vector{Int} <: AbstractArray
    @test Matrix{Float64} <: AbstractArray
    @test Vector{Int} <: Vector{Int}
    @test Int <: Real

    # --- Must stay correct: wrong family => false ---
    @test !(Matrix{Int} <: AbstractVector{Int})
    @test !(Vector{Int} <: AbstractMatrix{Int})
end

true
