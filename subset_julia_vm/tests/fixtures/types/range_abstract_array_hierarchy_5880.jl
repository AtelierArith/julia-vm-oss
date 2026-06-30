using Test

import Base: LogRange

@testset "range abstract array hierarchy (Issues #5615/#5880)" begin
    @test AbstractRange <: AbstractVector
    @test AbstractRange <: AbstractArray
    @test AbstractUnitRange <: AbstractRange
    @test AbstractUnitRange <: AbstractVector
    @test AbstractUnitRange <: AbstractArray

    @test AbstractRange{Int64} <: AbstractVector{Int64}
    @test !(AbstractRange{Int64} <: AbstractVector{Integer})
    @test UnitRange{Int64} <: AbstractVector{Int64}
    @test UnitRange{Int64} <: AbstractArray{Int64,1}
    @test !(UnitRange{Int64} <: AbstractVector{Integer})
    @test !(UnitRange{Int64} <: Array{Int64,1})
    @test !(UnitRange{Int64} <: DenseArray{Int64,1})

    @test StepRangeLen{Float64} <: AbstractVector{Float64}
    @test LinRange{Float64} <: AbstractArray{Float64,1}
    @test LogRange{Float64} <: AbstractVector{Float64}
    @test LogRange{Float64} <: AbstractArray{Float64,1}
    @test !(LogRange{Float64} <: AbstractRange)
    @test !(LogRange{Float64} <: AbstractVector{Real})
end

true
