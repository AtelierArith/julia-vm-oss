using Test

@testset "container TypeVar subtype CoreType gate (Issues #5615/#5949)" begin
    @test Dict{String,Int64} <: (AbstractDict{String,T} where T)
    @test !(Dict{String,Int64} <: (AbstractDict{Symbol,T} where T))
    @test !(Dict{String,Int64} <: AbstractDict{String,Real})

    @test Set{Int64} <: (AbstractSet{T} where T)
    @test !(Set{String} <: (AbstractSet{T} where T<:Real))
    @test !(Set{Int64} <: AbstractSet{Real})

    @test Base.RefValue{Int64} <: (Ref{T} where T)
    @test !(Base.RefValue{String} <: (Ref{T} where T<:Real))
    @test !(Base.RefValue{Int64} <: Ref{Real})
end

true
