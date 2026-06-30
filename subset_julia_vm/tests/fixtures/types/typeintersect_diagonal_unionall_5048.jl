using Test

@testset "diagonal UnionAll typeintersect narrows repeated TypeVar (Issue #5048)" begin
    diagonal = Tuple{T,T} where T<:Real

    @test typeintersect(diagonal, Tuple{Int64,Real}) === Tuple{Int64,Int64}
    @test typeintersect(Tuple{Int64,Real}, diagonal) === Tuple{Int64,Int64}
    @test typeintersect(diagonal, Tuple{Int64,Integer}) === Tuple{Int64,Int64}
    @test typeintersect(diagonal, Tuple{String,Real}) === Union{}

    @test string(typeintersect(diagonal, Tuple{Integer,Real})) ==
          "Tuple{T, T} where T<:Integer"
    @test string(typeintersect(diagonal, Tuple{Real,Real})) ==
          "Tuple{T, T} where T<:Real"
end

@testset "diagonal UnionAll typeintersect with invariant container occurrence (Issue #5048)" begin
    diagonal = Tuple{T,Vector{T}} where T<:Real

    @test typeintersect(diagonal, Tuple{Int64,Vector{Real}}) ===
          Tuple{Int64,Vector{Real}}
    @test typeintersect(diagonal, Tuple{Real,Vector{Int64}}) ===
          Tuple{Int64,Vector{Int64}}
    @test typeintersect(Tuple{Int64,Vector{Real}}, diagonal) ===
          Tuple{Int64,Vector{Real}}
    @test typeintersect(Tuple{Real,Vector{Int64}}, diagonal) ===
          Tuple{Int64,Vector{Int64}}

    @test typeintersect(diagonal, Tuple{String,Vector{Real}}) === Union{}
    @test typeintersect(diagonal, Tuple{Int64,Vector{String}}) === Union{}
    @test typeintersect(diagonal, Tuple{Integer,Vector{Float64}}) === Union{}
    @test typeintersect(diagonal, Tuple{Float64,Vector{Integer}}) === Union{}
end

true
