# Issue #7819 (follow-up to #7728): the AbstractArray{T,N} subtype edge must
# survive an EXTRA value-parameter intermediate that re-passes {S,T,N} to its
# parent — the exact shape StaticArrays uses:
#
#   StaticArray7819{S,T,N}    <: AbstractArray{T,N}
#   StaticVecOrMat7819{S,T,N} <: StaticArray7819{S,T,N}        # re-passes {S,T,N}
#   StaticVector7819{N,T}     <: StaticVecOrMat7819{Tuple{N},T,1}
#   Vec7819{N,T}              <: StaticVector7819{N,T}
#
# #7728's fixture only had a single intermediate. With the extra
# StaticVecOrMat7819 layer sjulia collapsed the parametric family entry's
# type-parameter list (a monomorphized instance clobbered it with an empty list),
# so `registered_instantiated_struct_parent_in` could no longer substitute the
# concrete arguments and EVERY edge in the chain went false. All expectations
# below were verified against upstream Julia 1.12.

using Test

abstract type StaticArray7819{S,T,N} <: AbstractArray{T,N} end
abstract type StaticVecOrMat7819{S,T,N} <: StaticArray7819{S,T,N} end
abstract type StaticVector7819{N,T} <: StaticVecOrMat7819{Tuple{N},T,1} end
struct Vec7819{N,T} <: StaticVector7819{N,T}
    data::Tuple
end

@testset "value-param AbstractArray parent chain w/ extra intermediate (Issue #7819)" begin
    @test Vec7819{3,Int64} <: StaticVector7819{3,Int64}
    @test Vec7819{3,Int64} <: StaticVecOrMat7819{Tuple{3},Int64,1}
    @test Vec7819{3,Int64} <: StaticArray7819{Tuple{3},Int64,1}
    @test Vec7819{3,Int64} <: StaticArray7819

    # The bug: the parameterized AbstractArray check must thread T=Int64, N=1
    # through TWO value-parameter intermediates.
    @test Vec7819{3,Int64} <: AbstractArray{Int64,1}
    @test Vec7819{3,Int64} <: AbstractArray

    # Element type / rank are invariant.
    @test !(Vec7819{3,Int64} <: AbstractArray{Float64,1})
    @test !(Vec7819{3,Int64} <: AbstractArray{Int64,2})
end

true
