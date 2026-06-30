# Issue #7787: a user type whose declared abstract parent chain reaches
# `AbstractArray{T,N}` must be `<:` the BARE, parameter-free `AbstractArray`
# (and the bare `AbstractVector`/`AbstractMatrix` when its rank matches), not
# just the parameterized `AbstractArray{T}` form (which was fixed in #7728).
#
# Before the fix, the bare-abstract array arms of
# `struct_is_subtype_of_abstract_with_lookup` only matched BUILT-IN array-family
# NAMES; they did not walk the user struct/abstract hierarchy up into the array
# family, so `MyArr{Float64} <: AbstractArray` was wrongly `false` while the
# parameterized `MyArr{Float64} <: AbstractArray{Float64}` was already `true`.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

# Rank-2 chain (parent is AbstractArray{T,2}).
abstract type AbsContainer7787{T} <: AbstractArray{T,2} end
struct MyArr7787{T} <: AbsContainer7787{T}
    data::Tuple
end

# Rank-1 chain (parent is AbstractVector{T} == AbstractArray{T,1}).
abstract type AbsVecContainer7787{T} <: AbstractVector{T} end
struct MyVecArr7787{T} <: AbsVecContainer7787{T}
    data::Tuple
end

# DenseArray-rooted rank-1 chain.
abstract type AbsDense7787{T} <: DenseArray{T,1} end
struct MyDense7787{T} <: AbsDense7787{T}
    data::Tuple
end

# A user type that is NOT an array must stay outside the array family.
struct Plain7787
    x::Int
end

@testset "bare AbstractArray over user chain (Issue #7787)" begin
    # The bug: bare, parameter-free AbstractArray over a user chain.
    @test MyArr7787{Float64} <: AbstractArray
    # The parameterized form already worked (Issue #7728); keep it green.
    @test MyArr7787{Float64} <: AbstractArray{Float64}

    # Rank: the parent pins rank 2, so the bare AbstractMatrix matches but the
    # bare AbstractVector does not.
    @test MyArr7787{Float64} <: AbstractMatrix
    @test !(MyArr7787{Float64} <: AbstractVector)
    @test MyArr7787{Float64} <: AbstractMatrix{Float64}
    @test !(MyArr7787{Float64} <: AbstractVector{Float64})

    # DenseArray is more specific than AbstractArray: an AbstractArray-rooted
    # user type is NOT a DenseArray.
    @test !(MyArr7787{Float64} <: DenseArray)
    @test !(MyArr7787{Float64} <: DenseArray{Float64})

    # Rank-1 chain: bare AbstractVector matches, AbstractMatrix does not.
    @test MyVecArr7787{Int} <: AbstractArray
    @test MyVecArr7787{Int} <: AbstractVector
    @test !(MyVecArr7787{Int} <: AbstractMatrix)
    @test !(MyVecArr7787{Int} <: DenseArray)

    # DenseArray-rooted rank-1 chain: DenseArray AND AbstractVector match.
    @test MyDense7787{Int} <: AbstractArray
    @test MyDense7787{Int} <: DenseArray
    @test MyDense7787{Int} <: AbstractVector
    @test !(MyDense7787{Int} <: AbstractMatrix)

    # Non-array user type stays outside the array family.
    @test !(Plain7787 <: AbstractArray)
    @test !(Plain7787 <: DenseArray)
    @test !(Plain7787 <: AbstractVector)
end

true
