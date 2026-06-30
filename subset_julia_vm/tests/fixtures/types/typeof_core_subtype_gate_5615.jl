using Test

@testset "runtime Type{T} subtype CoreType gate (Issue #5615)" begin
    @test Type{Int64} <: Type
    @test Type{Int64} <: Type{Int64}
    @test !(Type{Int64} <: Type{Integer})
    @test Type{Int64} <: Type{<:Integer}
    @test !(Type{String} <: Type{<:Integer})

    @test Type{Vector{Int64}} <: Type{<:AbstractVector}
    @test !(Type{Matrix{Int64}} <: Type{<:AbstractVector})
    @test Type{Matrix{Int64}} <: Type{<:AbstractMatrix}
end

true
