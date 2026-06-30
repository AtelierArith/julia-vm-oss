# Issue #5048: a diagonal-UnionAll tuple intersect must recover the diagonal
# variable from an invariant parametric container that holds it in ONE of
# several parameter slots — not only the single-parameter `Vector{T}` shape.
# `typeintersect(Tuple{T, Dict{Symbol,T}} where T<:Real, Tuple{Int64, Dict{Symbol,Real}})`
# previously collapsed to `Union{}` because the per-element invariant candidate
# helper handled only unary containers; it now generalizes to N-ary containers
# (`Dict{Symbol,T}`, `Dict{T,Symbol}`, `Pair{Symbol,T}`, `Dict{T,T}`), requiring
# every non-diagonal slot to be invariantly equal so a mismatched slot still
# yields `Union{}`.

using Test

@testset "N-ary invariant-container diagonal typeintersect (Issue #5048)" begin
    # the diagonal var in the value slot of a 2-param container
    @test typeintersect(Tuple{T,Dict{Symbol,T}} where T<:Real, Tuple{Int64,Dict{Symbol,Real}}) ==
          Tuple{Int64,Dict{Symbol,Real}}
    # the diagonal var in the key slot
    @test typeintersect(Tuple{T,Dict{T,Symbol}} where T<:Real, Tuple{Int64,Dict{Real,Symbol}}) ==
          Tuple{Int64,Dict{Real,Symbol}}
    # Pair value slot
    @test typeintersect(Tuple{T,Pair{Symbol,T}} where T<:Real, Tuple{Int64,Pair{Symbol,Real}}) ==
          Tuple{Int64,Pair{Symbol,Real}}
    # both parameter slots are the diagonal var (Dict{T,T}) — they must agree
    @test typeintersect(Tuple{T,Dict{T,T}} where T<:Real, Tuple{Int64,Dict{Real,Real}}) ==
          Tuple{Int64,Dict{Real,Real}}

    # the unary container shape still works (regression)
    @test typeintersect(Tuple{T,Vector{T}} where T<:Real, Tuple{Int64,Vector{Real}}) ==
          Tuple{Int64,Vector{Real}}

    # a non-diagonal slot that differs makes the whole intersection empty
    @test typeintersect(Tuple{T,Dict{Symbol,T}} where T<:Real, Tuple{Int64,Dict{Int,Real}}) ==
          Union{}
    # the two diagonal slots disagreeing makes it empty
    @test typeintersect(Tuple{T,Dict{T,T}} where T<:Real, Tuple{Int64,Dict{Int,Float64}}) ==
          Union{}
end

true
