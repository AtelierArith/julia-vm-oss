using Test

# Issue #5640: an abstract array-family type must NOT be reported as a subtype of
# a more concrete array-family type. The directional (abstract <-> concrete)
# relationship between array-family container names was previously ignored, so
# `AbstractVector{Int} <: Vector{Int}` wrongly returned `true`. This is also the
# direct cause of a `typeintersect` parity gap tracked under #5048.

@testset "array-family abstract is not subtype of concrete (Issue #5640)" begin
    # Wrong-direction relations: abstract/dense family is NOT a subtype of a
    # more concrete family.
    @test !(AbstractVector{Int} <: Vector{Int})
    @test !(AbstractVector{Int} <: Vector)
    @test !(DenseArray{Int} <: Array{Int})
    @test !(DenseVector{Int} <: Vector{Int})
    @test !(AbstractMatrix{Int} <: Matrix{Int})
    @test !(AbstractArray{Int} <: DenseArray{Int})
    @test !(AbstractArray{Int} <: Vector{Int})

    # Correct concrete -> abstract relations are preserved.
    @test Vector{Int} <: AbstractVector{Int}
    @test Vector{Int} <: AbstractArray{Int}
    @test Vector{Int} <: AbstractArray
    @test Array{Int} <: DenseArray{Int}
    @test Matrix{Int} <: AbstractMatrix{Int}
    @test DenseVector{Int} <: AbstractVector{Int}
    @test Vector{Int} <: AbstractVector

    # Invariance and rank constraints stay intact.
    @test !(Vector{Int} <: AbstractVector{Real})
    @test !(Vector{Int} <: AbstractMatrix{Int})
    @test !(Vector{Int} <: AbstractMatrix)
end

@testset "typeintersect picks concrete array side (Issue #5640 / #5048)" begin
    @test typeintersect(AbstractVector{Int}, Vector{Int}) === Vector{Int}
    @test typeintersect(Vector{Int}, AbstractVector{Int}) === Vector{Int}
    @test typeintersect(AbstractArray{Int,1}, Vector{Int}) === Vector{Int}
    @test typeintersect(DenseArray{Int}, Array{Int}) === Array{Int}
    @test typeintersect(AbstractMatrix{Int}, Matrix{Int}) === Matrix{Int}
    @test typeintersect(Vector{Int}, Vector{Real}) === Union{}
end

true
