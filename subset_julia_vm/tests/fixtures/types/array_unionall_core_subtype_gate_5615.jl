using Test

@testset "array UnionAll subtype CoreType gate (Issue #5615)" begin
    @test Vector{Int64} <: (Array{T} where T)
    @test Matrix{Float64} <: (Array{T} where T)
    @test Array{Bool,3} <: (Array{T} where T)

    @test Vector{Int64} <: Array{<:Real}
    @test !(Vector{String} <: Array{<:Real})
    @test !(Vector{Int64} <: Array{Real})

    @test Array{Float64,1} <: Vector{Float64}
    @test !(Array{Float64,2} <: Vector{Float64})
end

true
