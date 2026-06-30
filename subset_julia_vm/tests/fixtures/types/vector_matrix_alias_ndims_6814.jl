# Bare array aliases `Vector` (= `Array{T,1} where T`) and `Matrix`
# (= `Array{T,2} where T`) must keep their fixed rank in `isa` / `<:`.
#
# sjulia previously short-circuited `struct_params_are_subtype` to `true` for any
# array-family pair whose supertype was written without parameters, so a bare
# alias that PINS a rank (`Vector`/`Matrix`/`AbstractVector`/`AbstractMatrix`)
# was treated as rank-free. That made `Vector <: Matrix`, `Array{Int64,1} <:
# Matrix`, and `[1,2,3] isa Matrix` all spuriously true (Issue #6814).
#
# Only the genuinely rank-free names (`Array`/`AbstractArray`/`DenseArray`/
# `BitArray`) match any rank when written bare. All expectations below were
# verified against upstream Julia 1.12.

using Test

@testset "bare Vector/Matrix aliases keep ndims in isa/<: (Issue #6814)" begin
    # --- isa: a value's rank must match the bare alias's fixed rank ---
    @test [1, 2, 3] isa Vector
    @test !([1, 2, 3] isa Matrix)
    m = [1 2; 3 4]
    @test m isa Matrix
    @test !(m isa Vector)
    # Parameterized forms were already correct; keep them so.
    @test !(m isa Array{Int64,1})
    @test m isa Array{Int64,2}

    # --- type-level <:: bare alias rank is invariant ---
    @test !(Matrix{Int64} <: Vector)
    @test !(Vector <: Matrix)
    @test !(Matrix <: Vector)
    @test !(Array{Int64,1} <: Matrix)
    @test Array{Int64,2} <: Matrix
    @test Vector{Int64} <: Vector
    @test Matrix{Int64} <: Matrix

    # --- rank-free supertypes still match any rank when bare ---
    @test Vector <: Array
    @test Matrix <: Array
    @test Vector <: AbstractArray
    @test Matrix <: AbstractArray
    @test Array <: AbstractArray
    # ...but a rank-free type is NOT a subtype of a rank-pinned bare alias.
    @test !(Array <: Matrix)
    @test !(Array <: Vector)

    # --- abstract rank-pinned aliases keep their rank ---
    @test Vector{Int64} <: AbstractVector
    @test !(Vector{Int64} <: AbstractMatrix)
    @test Matrix{Int64} <: AbstractMatrix
    @test !(Matrix{Int64} <: AbstractVector)
end

true
