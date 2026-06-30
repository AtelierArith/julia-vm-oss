using Test

import Base: LogRange, OneTo

@testset "range subtype CoreType gate (Issues #5615/#5875)" begin
    @test UnitRange <: AbstractUnitRange
    @test OneTo <: AbstractUnitRange
    @test UnitRange <: AbstractRange
    @test StepRange <: AbstractRange
    @test StepRangeLen <: AbstractRange
    @test LinRange <: AbstractRange

    @test UnitRange{Int64} <: AbstractUnitRange
    @test UnitRange{Int64} <: AbstractRange
    @test StepRange{Int64,Int64} <: AbstractRange
    @test StepRangeLen{Float64} <: AbstractRange
    @test LinRange{Float64} <: AbstractRange

    @test !(LogRange <: AbstractRange)
    @test !(LogRange{Float64} <: AbstractRange)
    @test !(LogRange{Float64} <: AbstractRange{Float64})
end

true
