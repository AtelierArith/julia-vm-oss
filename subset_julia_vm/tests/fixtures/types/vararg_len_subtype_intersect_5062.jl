# Issue #5062: subtype / typeintersect involving the fixed-length value
# parameter `N` of `Vararg{T,N}` (and the synonymous `NTuple{N,T}`).
#
# Before the fix, sjulia handled the *pattern* side of the alias
# (`Tuple{Int,Int,Int} <: NTuple{3,Int}`) but not the *actual* side, so the
# reverse relation `NTuple{3,Int} <: Tuple{Int,Int,Int}` and the equivalence
# direction were rejected, and `typeintersect` did not flatten the alias.
#
# Fix: expand a concrete-length `Vararg{T,N}` element into the flat
# `Tuple{T, ..., T}` shape on both operands during subtype checking and
# intersection, matching upstream Julia's identity
# `Tuple{Vararg{T,N}} === Tuple{T, ..., T}`.

using Test

@testset "NTuple{N,T} <-> Tuple flat form subtyping (Issue #5062)" begin
    # Both directions of the equivalence hold.
    @test NTuple{3,Int} <: Tuple{Int,Int,Int}
    @test Tuple{Int,Int,Int} <: NTuple{3,Int}
    # Length mismatch is rejected in both directions.
    @test !(NTuple{2,Int} <: Tuple{Int,Int,Int})
    @test !(Tuple{Int,Int} <: NTuple{3,Int})
    # Element covariance survives the alias expansion.
    @test NTuple{3,Int} <: Tuple{Real,Real,Real}
    @test !(NTuple{3,Real} <: Tuple{Int,Int,Int})
end

@testset "Vararg{T,N} concrete length subtyping (Issue #5062)" begin
    @test Tuple{Vararg{Int,3}} <: Tuple{Int,Int,Int}
    @test !(Tuple{Vararg{Int,3}} <: Tuple{Int,Int})
    # Two fixed-length varargs: equal length + covariant element.
    @test NTuple{3,Int} <: NTuple{3,Integer}
    @test !(NTuple{3,Int} <: NTuple{2,Int})
end

@testset "typeintersect over the fixed-length vararg alias (Issue #5062)" begin
    @test typeintersect(NTuple{3,Int}, Tuple{Int,Int,Int}) === Tuple{Int,Int,Int}
    @test typeintersect(NTuple{2,Int}, Tuple{Int,Int,Int}) === Union{}
    @test typeintersect(Tuple{Vararg{Int,3}}, Tuple{Int,Int,Int}) === Tuple{Int,Int,Int}
end

true
